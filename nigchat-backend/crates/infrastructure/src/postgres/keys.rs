//! E2EE key directory (spec §28). Public material only — nothing stored here
//! would let the server read a message.

use async_trait::async_trait;
use nigchat_domain::entities::{DeviceIdentityKey, PreKeyBundle};
use nigchat_domain::ids::{DeviceId, UserId};
use nigchat_domain::ports::KeyRepository;
use nigchat_domain::DomainResult;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::map_sqlx;

pub struct PgKeyRepository {
    pool: PgPool,
}

impl PgKeyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl KeyRepository for PgKeyRepository {
    /// Republishing bumps `key_version`. Peers compare that value to decide
    /// whether to raise a "security code changed" warning, so the increment is
    /// the security-relevant part of this write.
    async fn publish_identity_key(
        &self,
        device_id: DeviceId,
        user_id: UserId,
        identity_public_key: &[u8],
        registration_id: i32,
    ) -> DomainResult<i32> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;

        let version: i32 = sqlx::query_scalar(
            r#"
            INSERT INTO device_identity_keys (device_id, user_id, identity_public_key)
            VALUES ($1, $2, $3)
            ON CONFLICT (device_id) DO UPDATE
                SET identity_public_key = EXCLUDED.identity_public_key,
                    key_version = device_identity_keys.key_version + 1,
                    rotated_at  = now()
            RETURNING key_version
            "#,
        )
        .bind(device_id.as_uuid())
        .bind(user_id.as_uuid())
        .bind(identity_public_key)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        sqlx::query("UPDATE devices SET registration_id = $2 WHERE id = $1")
            .bind(device_id.as_uuid())
            .bind(registration_id)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;

        tx.commit().await.map_err(map_sqlx)?;
        Ok(version)
    }

    async fn publish_signed_prekey(
        &self,
        device_id: DeviceId,
        key_id: i32,
        public_key: &[u8],
        signature: &[u8],
    ) -> DomainResult<()> {
        sqlx::query(
            r#"
            INSERT INTO device_signed_prekeys (device_id, key_id, public_key, signature)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (device_id, key_id) DO UPDATE
                SET public_key = EXCLUDED.public_key, signature = EXCLUDED.signature
            "#,
        )
        .bind(device_id.as_uuid())
        .bind(key_id)
        .bind(public_key)
        .bind(signature)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    async fn upload_one_time_prekeys(
        &self,
        device_id: DeviceId,
        keys: &[(i32, Vec<u8>)],
    ) -> DomainResult<u64> {
        if keys.is_empty() {
            return Ok(0);
        }

        // One statement via UNNEST rather than a loop: a device tops up 100
        // keys at a time and 100 round trips would be absurd.
        let ids: Vec<i32> = keys.iter().map(|(id, _)| *id).collect();
        let publics: Vec<Vec<u8>> = keys.iter().map(|(_, key)| key.clone()).collect();

        let result = sqlx::query(
            r#"
            INSERT INTO device_one_time_prekeys (device_id, key_id, public_key)
            SELECT $1, k.key_id, k.public_key
            FROM UNNEST($2::int[], $3::bytea[]) AS k(key_id, public_key)
            ON CONFLICT (device_id, key_id) DO NOTHING
            "#,
        )
        .bind(device_id.as_uuid())
        .bind(&ids)
        .bind(&publics)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(result.rows_affected())
    }

    /// Hands out one bundle per active device and **consumes** the one-time
    /// prekey it returns — that is the whole point of a one-time key. The
    /// DELETE and the SELECT are one statement so two senders cannot be handed
    /// the same key.
    async fn take_prekey_bundles(&self, user_id: UserId) -> DomainResult<Vec<PreKeyBundle>> {
        #[derive(FromRow)]
        struct Row {
            device_id: Uuid,
            registration_id: Option<i32>,
            identity_public_key: Vec<u8>,
            signed_prekey_id: i32,
            signed_prekey_public: Vec<u8>,
            signed_prekey_signature: Vec<u8>,
            one_time_prekey_id: Option<i32>,
            one_time_prekey_public: Option<Vec<u8>>,
        }

        let rows = sqlx::query_as::<_, Row>(
            r#"
            WITH devices_with_keys AS (
                SELECT d.id AS device_id,
                       d.registration_id,
                       ik.identity_public_key,
                       sp.key_id     AS signed_prekey_id,
                       sp.public_key AS signed_prekey_public,
                       sp.signature  AS signed_prekey_signature
                FROM devices d
                JOIN device_identity_keys ik ON ik.device_id = d.id
                JOIN LATERAL (
                    SELECT key_id, public_key, signature
                    FROM device_signed_prekeys
                    WHERE device_id = d.id
                    ORDER BY created_at DESC
                    LIMIT 1
                ) sp ON TRUE
                WHERE d.user_id = $1 AND d.revoked_at IS NULL
            ),
            -- Lowest unused key per device. DISTINCT ON rather than a lateral
            -- inside USING, because a DELETE ... USING clause may not
            -- reference its own target table.
            candidate AS (
                SELECT DISTINCT ON (otp.device_id)
                       otp.device_id, otp.key_id
                FROM device_one_time_prekeys otp
                WHERE otp.device_id IN (SELECT device_id FROM devices_with_keys)
                ORDER BY otp.device_id, otp.key_id
            ),
            -- RETURNING only yields rows this statement actually removed, so
            -- two concurrent senders can never be handed the same key: the
            -- loser simply gets no one-time key for that device.
            claimed AS (
                DELETE FROM device_one_time_prekeys otp
                USING candidate c
                WHERE otp.device_id = c.device_id AND otp.key_id = c.key_id
                RETURNING otp.device_id, otp.key_id, otp.public_key
            )
            SELECT d.device_id,
                   d.registration_id,
                   d.identity_public_key,
                   d.signed_prekey_id,
                   d.signed_prekey_public,
                   d.signed_prekey_signature,
                   c.key_id     AS one_time_prekey_id,
                   c.public_key AS one_time_prekey_public
            FROM devices_with_keys d
            LEFT JOIN claimed c ON c.device_id = d.device_id
            "#,
        )
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(rows
            .into_iter()
            .map(|row| PreKeyBundle {
                user_id,
                device_id: DeviceId::from(row.device_id),
                registration_id: row.registration_id.unwrap_or(0),
                identity_public_key: row.identity_public_key,
                signed_prekey_id: row.signed_prekey_id,
                signed_prekey_public: row.signed_prekey_public,
                signed_prekey_signature: row.signed_prekey_signature,
                one_time_prekey_id: row.one_time_prekey_id,
                one_time_prekey_public: row.one_time_prekey_public,
            })
            .collect())
    }

    async fn one_time_prekey_count(&self, device_id: DeviceId) -> DomainResult<i64> {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM device_one_time_prekeys WHERE device_id = $1",
        )
        .bind(device_id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx)
    }

    async fn identity_keys_for(&self, user_id: UserId) -> DomainResult<Vec<DeviceIdentityKey>> {
        #[derive(FromRow)]
        struct Row {
            device_id: Uuid,
            identity_public_key: Vec<u8>,
            key_version: i32,
            rotated_at: Option<chrono::DateTime<chrono::Utc>>,
        }

        let rows = sqlx::query_as::<_, Row>(
            r#"
            SELECT ik.device_id, ik.identity_public_key, ik.key_version, ik.rotated_at
            FROM device_identity_keys ik
            JOIN devices d ON d.id = ik.device_id AND d.revoked_at IS NULL
            WHERE ik.user_id = $1
            "#,
        )
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(rows
            .into_iter()
            .map(|row| DeviceIdentityKey {
                device_id: DeviceId::from(row.device_id),
                user_id,
                identity_public_key: row.identity_public_key,
                key_version: row.key_version,
                rotated_at: row.rotated_at,
            })
            .collect())
    }
}
