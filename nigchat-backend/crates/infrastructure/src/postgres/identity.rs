//! Users, devices, sessions, OTP challenges and the security log.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nigchat_domain::entities::*;
use nigchat_domain::ids::*;
use nigchat_domain::ports::*;
use nigchat_domain::values::{PhoneNumber, Username};
use nigchat_domain::{DomainError, DomainResult};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::map_sqlx;

// --- row types ------------------------------------------------------------

#[derive(FromRow)]
struct UserRow {
    id: Uuid,
    phone_e164: String,
    username: Option<String>,
    display_name: String,
    about: Option<String>,
    avatar_media_id: Option<Uuid>,
    two_step_enabled_at: Option<DateTime<Utc>>,
    is_active: bool,
    last_seen_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl UserRow {
    fn into_entity(self) -> DomainResult<User> {
        Ok(User {
            id: UserId::from(self.id),
            phone: PhoneNumber::parse(&self.phone_e164)?,
            username: self.username.as_deref().and_then(|u| Username::parse(u).ok()),
            display_name: self.display_name,
            about: self.about,
            avatar_media_id: self.avatar_media_id.map(MediaId::from),
            two_step_enabled: self.two_step_enabled_at.is_some(),
            is_active: self.is_active,
            last_seen_at: self.last_seen_at,
            created_at: self.created_at,
        })
    }
}

const USER_COLUMNS: &str = "id, phone_e164, username, display_name, about, avatar_media_id, \
     two_step_enabled_at, is_active, last_seen_at, created_at";

#[derive(FromRow)]
struct DeviceRow {
    id: Uuid,
    user_id: Uuid,
    platform: String,
    device_name: Option<String>,
    app_version: Option<String>,
    is_primary: bool,
    linked_at: DateTime<Utc>,
    last_active_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
}

impl DeviceRow {
    fn into_entity(self) -> DomainResult<Device> {
        Ok(Device {
            id: DeviceId::from(self.id),
            user_id: UserId::from(self.user_id),
            platform: Platform::parse(&self.platform)?,
            device_name: self.device_name,
            app_version: self.app_version,
            is_primary: self.is_primary,
            linked_at: self.linked_at,
            last_active_at: self.last_active_at,
            revoked_at: self.revoked_at,
        })
    }
}

const DEVICE_COLUMNS: &str = "id, user_id, platform, device_name, app_version, is_primary, \
     linked_at, last_active_at, revoked_at";

// --- users ----------------------------------------------------------------

pub struct PgUserRepository {
    pool: PgPool,
}

impl PgUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepository for PgUserRepository {
    async fn find_by_id(&self, id: UserId) -> DomainResult<Option<User>> {
        let row = sqlx::query_as::<_, UserRow>(&format!(
            "SELECT {USER_COLUMNS} FROM users WHERE id = $1"
        ))
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        row.map(UserRow::into_entity).transpose()
    }

    async fn find_by_phone(&self, phone: &PhoneNumber) -> DomainResult<Option<User>> {
        let row = sqlx::query_as::<_, UserRow>(&format!(
            "SELECT {USER_COLUMNS} FROM users WHERE phone_e164 = $1"
        ))
        .bind(phone.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        row.map(UserRow::into_entity).transpose()
    }

    async fn find_by_username(&self, username: &Username) -> DomainResult<Option<User>> {
        let row = sqlx::query_as::<_, UserRow>(&format!(
            "SELECT {USER_COLUMNS} FROM users WHERE username = $1"
        ))
        .bind(username.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        row.map(UserRow::into_entity).transpose()
    }

    /// Idempotent registration: a retried OTP verification must not create a
    /// second account. `ON CONFLICT` makes that true even across instances.
    async fn upsert_by_phone(
        &self,
        phone: &PhoneNumber,
        phone_hash: &str,
        display_name: &str,
    ) -> DomainResult<User> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;

        let row = sqlx::query_as::<_, UserRow>(&format!(
            r#"
            INSERT INTO users (id, phone_e164, phone_hash, display_name)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (phone_e164) DO UPDATE
                SET phone_hash = EXCLUDED.phone_hash, updated_at = now()
            RETURNING {USER_COLUMNS}
            "#
        ))
        .bind(Uuid::now_v7())
        .bind(phone.as_str())
        .bind(phone_hash)
        .bind(display_name)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        // Defaults must exist before the first message arrives, otherwise the
        // notification policy has nothing to read.
        sqlx::query("INSERT INTO user_privacy_settings (user_id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(row.id)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;

        sqlx::query(
            r#"
            INSERT INTO notification_preferences
                (user_id, message_tone_id, group_tone_id, call_ringtone_id)
            VALUES ($1, 'tone.message.default', 'tone.group.default', 'tone.call.default')
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(row.id)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        tx.commit().await.map_err(map_sqlx)?;
        row.into_entity()
    }

    /// COALESCE means a field absent from the request is left alone, so two
    /// clients editing different fields cannot clobber each other.
    async fn update_profile(
        &self,
        id: UserId,
        display_name: Option<&str>,
        about: Option<&str>,
        username: Option<&Username>,
        avatar_media_id: Option<MediaId>,
    ) -> DomainResult<User> {
        let row = sqlx::query_as::<_, UserRow>(&format!(
            r#"
            UPDATE users
            SET display_name    = COALESCE($2, display_name),
                about           = COALESCE($3, about),
                username        = COALESCE($4, username),
                avatar_media_id = COALESCE($5, avatar_media_id),
                updated_at      = now()
            WHERE id = $1
            RETURNING {USER_COLUMNS}
            "#
        ))
        .bind(id.as_uuid())
        .bind(display_name)
        .bind(about)
        .bind(username.map(Username::as_str))
        .bind(avatar_media_id.map(MediaId::as_uuid))
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx)?;

        row.into_entity()
    }

    async fn touch_last_seen(&self, id: UserId) -> DomainResult<()> {
        sqlx::query("UPDATE users SET last_seen_at = now() WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;
        Ok(())
    }

    /// Takes hashed numbers so the server never receives, and never logs, the
    /// raw numbers of people who are not users.
    async fn find_by_phone_hashes(&self, hashes: &[String]) -> DomainResult<Vec<User>> {
        let rows = sqlx::query_as::<_, UserRow>(&format!(
            r#"
            SELECT {USER_COLUMNS} FROM users
            WHERE is_active AND phone_hash = ANY($1)
            "#
        ))
        .bind(hashes)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        rows.into_iter().map(UserRow::into_entity).collect()
    }

    async fn privacy_settings(&self, id: UserId) -> DomainResult<PrivacySettings> {
        #[derive(FromRow)]
        struct Row {
            last_seen_visibility: String,
            profile_photo_visibility: String,
            about_visibility: String,
            status_visibility: String,
            read_receipts_enabled: bool,
            typing_indicators_enabled: bool,
            who_can_add_to_groups: String,
            who_can_call: String,
            silence_unknown_callers: bool,
            strict_privacy_mode: bool,
        }

        let row = sqlx::query_as::<_, Row>(
            r#"
            SELECT last_seen_visibility, profile_photo_visibility, about_visibility,
                   status_visibility, read_receipts_enabled, typing_indicators_enabled,
                   who_can_add_to_groups, who_can_call, silence_unknown_callers,
                   strict_privacy_mode
            FROM user_privacy_settings WHERE user_id = $1
            "#,
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        // A missing row means defaults, not an error: privacy must fail closed
        // but must never break a request.
        let Some(row) = row else {
            return Ok(PrivacySettings {
                user_id: id,
                last_seen: Visibility::Contacts,
                profile_photo: Visibility::Contacts,
                about: Visibility::Contacts,
                status: Visibility::Contacts,
                read_receipts_enabled: true,
                typing_indicators_enabled: true,
                who_can_add_to_groups: Visibility::Contacts,
                who_can_call: Visibility::Everyone,
                silence_unknown_callers: false,
                strict_privacy_mode: false,
            });
        };

        Ok(PrivacySettings {
            user_id: id,
            last_seen: Visibility::parse(&row.last_seen_visibility),
            profile_photo: Visibility::parse(&row.profile_photo_visibility),
            about: Visibility::parse(&row.about_visibility),
            status: Visibility::parse(&row.status_visibility),
            read_receipts_enabled: row.read_receipts_enabled,
            typing_indicators_enabled: row.typing_indicators_enabled,
            who_can_add_to_groups: Visibility::parse(&row.who_can_add_to_groups),
            who_can_call: Visibility::parse(&row.who_can_call),
            silence_unknown_callers: row.silence_unknown_callers,
            strict_privacy_mode: row.strict_privacy_mode,
        })
    }

    /// COALESCE per column, so an absent field keeps its stored value. The
    /// alternative — read, merge in Rust, write back — loses a concurrent
    /// change made from another device between the read and the write.
    async fn update_privacy_settings(
        &self,
        id: UserId,
        update: PrivacyUpdate,
    ) -> DomainResult<PrivacySettings> {
        sqlx::query(
            r#"
            INSERT INTO user_privacy_settings (user_id) VALUES ($1)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;

        sqlx::query(
            r#"
            UPDATE user_privacy_settings
            SET last_seen_visibility        = COALESCE($2, last_seen_visibility),
                profile_photo_visibility    = COALESCE($3, profile_photo_visibility),
                about_visibility            = COALESCE($4, about_visibility),
                status_visibility           = COALESCE($5, status_visibility),
                read_receipts_enabled       = COALESCE($6, read_receipts_enabled),
                typing_indicators_enabled   = COALESCE($7, typing_indicators_enabled),
                who_can_add_to_groups       = COALESCE($8, who_can_add_to_groups),
                who_can_call                = COALESCE($9, who_can_call),
                silence_unknown_callers     = COALESCE($10, silence_unknown_callers),
                updated_at                  = now()
            WHERE user_id = $1
            "#,
        )
        .bind(id.as_uuid())
        .bind(update.last_seen.map(|v| v.as_str()))
        .bind(update.profile_photo.map(|v| v.as_str()))
        .bind(update.about.map(|v| v.as_str()))
        .bind(update.status.map(|v| v.as_str()))
        .bind(update.read_receipts_enabled)
        .bind(update.typing_indicators_enabled)
        .bind(update.who_can_add_to_groups.map(|v| v.as_str()))
        .bind(update.who_can_call.map(|v| v.as_str()))
        .bind(update.silence_unknown_callers)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;

        self.privacy_settings(id).await
    }

    async fn set_two_step_pin(&self, id: UserId, pin_hash: Option<&str>) -> DomainResult<()> {
        sqlx::query(
            r#"
            UPDATE users
            SET two_step_pin_hash   = $2,
                two_step_enabled_at = CASE WHEN $2 IS NULL THEN NULL ELSE now() END,
                updated_at          = now()
            WHERE id = $1
            "#,
        )
        .bind(id.as_uuid())
        .bind(pin_hash)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    async fn two_step_pin_hash(&self, id: UserId) -> DomainResult<Option<String>> {
        sqlx::query_scalar::<_, Option<String>>("SELECT two_step_pin_hash FROM users WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)
            .map(Option::flatten)
    }

    async fn is_blocked(&self, blocker: UserId, blocked: UserId) -> DomainResult<bool> {
        let exists: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM blocks WHERE blocker_id = $1 AND blocked_id = $2",
        )
        .bind(blocker.as_uuid())
        .bind(blocked.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(exists.is_some())
    }

    /// One query for the whole audience. Per-recipient checks would turn a
    /// large group fan-out into hundreds of round trips.
    async fn blocked_by_any(
        &self,
        user: UserId,
        candidates: &[UserId],
    ) -> DomainResult<Vec<UserId>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<Uuid> = candidates.iter().copied().map(UserId::as_uuid).collect();

        let rows: Vec<Uuid> = sqlx::query_scalar(
            "SELECT blocker_id FROM blocks WHERE blocked_id = $1 AND blocker_id = ANY($2)",
        )
        .bind(user.as_uuid())
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(rows.into_iter().map(UserId::from).collect())
    }

    async fn block(&self, blocker: UserId, blocked: UserId) -> DomainResult<()> {
        sqlx::query(
            "INSERT INTO blocks (blocker_id, blocked_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(blocker.as_uuid())
        .bind(blocked.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    async fn unblock(&self, blocker: UserId, blocked: UserId) -> DomainResult<()> {
        sqlx::query("DELETE FROM blocks WHERE blocker_id = $1 AND blocked_id = $2")
            .bind(blocker.as_uuid())
            .bind(blocked.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;
        Ok(())
    }
}

// --- devices --------------------------------------------------------------

pub struct PgDeviceRepository {
    pool: PgPool,
}

impl PgDeviceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DeviceRepository for PgDeviceRepository {
    async fn find_by_id(&self, id: DeviceId) -> DomainResult<Option<Device>> {
        let row = sqlx::query_as::<_, DeviceRow>(&format!(
            "SELECT {DEVICE_COLUMNS} FROM devices WHERE id = $1"
        ))
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        row.map(DeviceRow::into_entity).transpose()
    }

    async fn list_active(&self, user_id: UserId) -> DomainResult<Vec<Device>> {
        let rows = sqlx::query_as::<_, DeviceRow>(&format!(
            r#"
            SELECT {DEVICE_COLUMNS} FROM devices
            WHERE user_id = $1 AND revoked_at IS NULL
            ORDER BY last_active_at DESC NULLS LAST
            "#
        ))
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        rows.into_iter().map(DeviceRow::into_entity).collect()
    }

    async fn register(
        &self,
        user_id: UserId,
        platform: Platform,
        device_name: Option<&str>,
        app_version: Option<&str>,
        is_primary: bool,
    ) -> DomainResult<Device> {
        let row = sqlx::query_as::<_, DeviceRow>(&format!(
            r#"
            INSERT INTO devices
                (id, user_id, platform, device_name, app_version, is_primary, last_active_at)
            VALUES ($1, $2, $3, $4, $5, $6, now())
            RETURNING {DEVICE_COLUMNS}
            "#
        ))
        .bind(Uuid::now_v7())
        .bind(user_id.as_uuid())
        .bind(platform.as_str())
        .bind(device_name)
        .bind(app_version)
        .bind(is_primary)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx)?;

        row.into_entity()
    }

    async fn touch_active(&self, id: DeviceId, ip_hash: Option<&str>) -> DomainResult<()> {
        sqlx::query(
            "UPDATE devices SET last_active_at = now(), last_ip_hash = COALESCE($2, last_ip_hash) WHERE id = $1",
        )
        .bind(id.as_uuid())
        .bind(ip_hash)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    /// One transaction. Revoking the device and killing its sessions
    /// separately would leave a window in which a revoked device can still
    /// refresh its access token.
    async fn revoke(&self, id: DeviceId, reason: &str) -> DomainResult<()> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;

        let affected = sqlx::query(
            "UPDATE devices SET revoked_at = now(), revoked_reason = $2 WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id.as_uuid())
        .bind(reason)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        if affected.rows_affected() == 0 {
            return Err(DomainError::not_found("device"));
        }

        sqlx::query(
            "UPDATE device_sessions SET revoked_at = now() WHERE device_id = $1 AND revoked_at IS NULL",
        )
        .bind(id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        // Its push tokens are dead too — pushing to a revoked device would
        // notify whoever now holds it.
        sqlx::query(
            "UPDATE notification_tokens SET invalidated_at = now() WHERE device_id = $1",
        )
        .bind(id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        tx.commit().await.map_err(map_sqlx)?;
        Ok(())
    }
}

// --- sessions -------------------------------------------------------------

pub struct PgSessionRepository {
    pool: PgPool,
}

impl PgSessionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SessionRepository for PgSessionRepository {
    async fn create(
        &self,
        user_id: UserId,
        device_id: DeviceId,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> DomainResult<SessionId> {
        let id = Uuid::now_v7();
        sqlx::query(
            r#"
            INSERT INTO device_sessions (id, user_id, device_id, token_hash, expires_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(id)
        .bind(user_id.as_uuid())
        .bind(device_id.as_uuid())
        .bind(token_hash)
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(SessionId::from(id))
    }

    async fn find_by_token_hash(&self, token_hash: &str) -> DomainResult<Option<StoredSession>> {
        #[derive(FromRow)]
        struct Row {
            id: Uuid,
            user_id: Uuid,
            device_id: Uuid,
            expires_at: DateTime<Utc>,
            revoked_at: Option<DateTime<Utc>>,
        }

        let row = sqlx::query_as::<_, Row>(
            "SELECT id, user_id, device_id, expires_at, revoked_at FROM device_sessions WHERE token_hash = $1",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(row.map(|row| StoredSession {
            id: SessionId::from(row.id),
            user_id: UserId::from(row.user_id),
            device_id: DeviceId::from(row.device_id),
            expires_at: row.expires_at,
            revoked_at: row.revoked_at,
        }))
    }

    async fn rotate(&self, old: SessionId, new: SessionId) -> DomainResult<()> {
        sqlx::query(
            "UPDATE device_sessions SET revoked_at = now(), replaced_by = $2 WHERE id = $1",
        )
        .bind(old.as_uuid())
        .bind(new.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    async fn revoke_session(&self, id: SessionId) -> DomainResult<()> {
        sqlx::query("UPDATE device_sessions SET revoked_at = now() WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;
        Ok(())
    }

    async fn revoke_all_for_device(&self, device_id: DeviceId) -> DomainResult<u64> {
        let result = sqlx::query(
            "UPDATE device_sessions SET revoked_at = now() WHERE device_id = $1 AND revoked_at IS NULL",
        )
        .bind(device_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(result.rows_affected())
    }
}

// --- OTP challenges -------------------------------------------------------

pub struct PgChallengeRepository {
    pool: PgPool,
}

impl PgChallengeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuthChallengeRepository for PgChallengeRepository {
    async fn create(
        &self,
        phone: &PhoneNumber,
        code_hash: &str,
        expires_at: DateTime<Utc>,
        ip_hash: Option<&str>,
    ) -> DomainResult<ChallengeId> {
        let id = Uuid::now_v7();
        sqlx::query(
            r#"
            INSERT INTO auth_challenges (id, phone_e164, code_hash, expires_at, ip_hash)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(id)
        .bind(phone.as_str())
        .bind(code_hash)
        .bind(expires_at)
        .bind(ip_hash)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(ChallengeId::from(id))
    }

    async fn latest_active(&self, phone: &PhoneNumber) -> DomainResult<Option<StoredChallenge>> {
        #[derive(FromRow)]
        struct Row {
            id: Uuid,
            code_hash: String,
            attempts: i32,
            expires_at: DateTime<Utc>,
        }

        let row = sqlx::query_as::<_, Row>(
            r#"
            SELECT id, code_hash, attempts, expires_at
            FROM auth_challenges
            WHERE phone_e164 = $1 AND consumed_at IS NULL AND expires_at > now()
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(phone.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(row.map(|row| StoredChallenge {
            id: ChallengeId::from(row.id),
            code_hash: row.code_hash,
            attempts: row.attempts,
            expires_at: row.expires_at,
        }))
    }

    async fn increment_attempts(&self, id: ChallengeId) -> DomainResult<i32> {
        sqlx::query_scalar::<_, i32>(
            "UPDATE auth_challenges SET attempts = attempts + 1 WHERE id = $1 RETURNING attempts",
        )
        .bind(id.as_uuid())
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx)
    }

    /// Conditional update: whoever lands the write wins, and a concurrent
    /// verification of the same code gets `false`.
    async fn consume(&self, id: ChallengeId) -> DomainResult<bool> {
        let result = sqlx::query(
            "UPDATE auth_challenges SET consumed_at = now() WHERE id = $1 AND consumed_at IS NULL",
        )
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(result.rows_affected() == 1)
    }
}

// --- security log ---------------------------------------------------------

pub struct PgSecurityRepository {
    pool: PgPool,
}

impl PgSecurityRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SecurityRepository for PgSecurityRepository {
    async fn record_event(&self, event: SecurityEvent) -> DomainResult<()> {
        sqlx::query(
            r#"
            INSERT INTO security_events
                (user_id, device_id, event_type, severity, ip_hash, user_agent, metadata)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(event.user_id.as_uuid())
        .bind(event.device_id.map(DeviceId::as_uuid))
        .bind(event.event_type.as_str())
        .bind(event.event_type.severity())
        .bind(&event.ip_hash)
        .bind(&event.user_agent)
        .bind(&event.metadata)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    async fn recent_events(&self, user_id: UserId, limit: i64) -> DomainResult<Vec<SecurityEvent>> {
        #[derive(FromRow)]
        struct Row {
            device_id: Option<Uuid>,
            event_type: String,
            ip_hash: Option<String>,
            user_agent: Option<String>,
            metadata: serde_json::Value,
            created_at: DateTime<Utc>,
        }

        let rows = sqlx::query_as::<_, Row>(
            r#"
            SELECT device_id, event_type, ip_hash, user_agent, metadata, created_at
            FROM security_events
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(user_id.as_uuid())
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(rows
            .into_iter()
            .map(|row| SecurityEvent {
                user_id,
                device_id: row.device_id.map(DeviceId::from),
                event_type: parse_event_type(&row.event_type),
                ip_hash: row.ip_hash,
                user_agent: row.user_agent,
                metadata: row.metadata,
                created_at: row.created_at,
            })
            .collect())
    }
}

fn parse_event_type(value: &str) -> SecurityEventType {
    use SecurityEventType::*;
    match value {
        "logout" => Logout,
        "device_linked" => DeviceLinked,
        "device_revoked" => DeviceRevoked,
        "key_changed" => KeyChanged,
        "pin_changed" => PinChanged,
        "pin_failed" => PinFailed,
        "passkey_added" => PasskeyAdded,
        "passkey_removed" => PasskeyRemoved,
        "session_reuse_detected" => SessionReuseDetected,
        "suspicious_login" => SuspiciousLogin,
        "account_deactivated" => AccountDeactivated,
        "two_step_enabled" => TwoStepEnabled,
        "two_step_disabled" => TwoStepDisabled,
        "backup_created" => BackupCreated,
        _ => Login,
    }
}
