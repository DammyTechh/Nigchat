//! Device-link requests (spec §11).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nigchat_domain::ids::{DeviceId, UserId};
use nigchat_domain::ports::{ApprovedLink, DeviceLinkRepository, PendingLink};
use nigchat_domain::DomainResult;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::map_sqlx;

pub struct PgDeviceLinkRepository {
    pool: PgPool,
}

impl PgDeviceLinkRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DeviceLinkRepository for PgDeviceLinkRepository {
    async fn create(
        &self,
        code_hash: &str,
        platform: &str,
        device_name: Option<&str>,
        expires_at: DateTime<Utc>,
    ) -> DomainResult<()> {
        sqlx::query(
            r#"
            INSERT INTO device_link_requests (id, code_hash, platform, device_name, expires_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(code_hash)
        .bind(platform)
        .bind(device_name)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    async fn find_pending(&self, code_hash: &str) -> DomainResult<Option<PendingLink>> {
        #[derive(FromRow)]
        struct Row {
            platform: String,
            device_name: Option<String>,
            expires_at: DateTime<Utc>,
            approved: bool,
        }

        let row = sqlx::query_as::<_, Row>(
            r#"
            SELECT platform, device_name, expires_at, (approved_at IS NOT NULL) AS approved
            FROM device_link_requests
            WHERE code_hash = $1
            "#,
        )
        .bind(code_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(row.map(|row| PendingLink {
            platform: row.platform,
            device_name: row.device_name,
            expires_at: row.expires_at,
            approved: row.approved,
        }))
    }

    /// The `approved_at IS NULL AND expires_at > now()` predicate is what makes
    /// this safe: two phones approving the same code race on one row, and only
    /// the first `UPDATE` matches.
    async fn approve(
        &self,
        code_hash: &str,
        user_id: UserId,
        device_id: DeviceId,
    ) -> DomainResult<bool> {
        let result = sqlx::query(
            r#"
            UPDATE device_link_requests
            SET approved_at = now(), user_id = $2, claimed_by = $3
            WHERE code_hash = $1 AND approved_at IS NULL AND expires_at > now()
            "#,
        )
        .bind(code_hash)
        .bind(user_id.as_uuid())
        .bind(device_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(result.rows_affected() == 1)
    }

    /// `DELETE ... RETURNING` so the token pair can be handed out exactly once.
    /// A replayed poll finds nothing.
    async fn consume(&self, code_hash: &str) -> DomainResult<Option<ApprovedLink>> {
        #[derive(FromRow)]
        struct Row {
            user_id: Option<Uuid>,
            claimed_by: Option<Uuid>,
        }

        let row = sqlx::query_as::<_, Row>(
            r#"
            DELETE FROM device_link_requests
            WHERE code_hash = $1 AND approved_at IS NOT NULL
            RETURNING user_id, claimed_by
            "#,
        )
        .bind(code_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(row.and_then(|row| {
            Some(ApprovedLink {
                user_id: UserId::from(row.user_id?),
                device_id: DeviceId::from(row.claimed_by?),
            })
        }))
    }

    async fn purge_expired(&self) -> DomainResult<u64> {
        let result = sqlx::query("DELETE FROM device_link_requests WHERE expires_at < now()")
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;
        Ok(result.rows_affected())
    }
}
