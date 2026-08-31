//! Notification tokens, preferences, tones and the delivery ledger (spec §16).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nigchat_domain::entities::*;
use nigchat_domain::ids::*;
use nigchat_domain::ports::NotificationRepository;
use nigchat_domain::values::{MuteState, PreviewMode, QuietHours, Seq, Vibration};
use nigchat_domain::DomainResult;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::map_sqlx;

pub struct PgNotificationRepository {
    pool: PgPool,
}

impl PgNotificationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl NotificationRepository for PgNotificationRepository {
    /// Upsert on `(provider, token)`. Tokens rotate constantly — reinstall, OS
    /// update, APNs whim — so the client calls this on every launch and must
    /// not accumulate rows. Re-registering also clears a previous
    /// invalidation, which is how a resurrected token recovers.
    async fn register_token(
        &self,
        user_id: UserId,
        device_id: DeviceId,
        provider: PushProvider,
        token: &str,
        is_voip: bool,
        sandbox: bool,
    ) -> DomainResult<NotificationTokenId> {
        let id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO notification_tokens
                (id, user_id, device_id, provider, token, is_voip, environment)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (provider, token) DO UPDATE
                SET user_id        = EXCLUDED.user_id,
                    device_id      = EXCLUDED.device_id,
                    is_voip        = EXCLUDED.is_voip,
                    environment    = EXCLUDED.environment,
                    invalidated_at = NULL,
                    failure_count  = 0,
                    updated_at     = now()
            RETURNING id
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(user_id.as_uuid())
        .bind(device_id.as_uuid())
        .bind(provider.as_str())
        .bind(token)
        .bind(is_voip)
        .bind(if sandbox { "sandbox" } else { "production" })
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(NotificationTokenId::from(id))
    }

    async fn active_tokens(&self, user_id: UserId) -> DomainResult<Vec<NotificationToken>> {
        #[derive(FromRow)]
        struct Row {
            id: Uuid,
            device_id: Uuid,
            provider: String,
            token: String,
            is_voip: bool,
            environment: String,
            failure_count: i16,
            invalidated_at: Option<DateTime<Utc>>,
        }

        let rows = sqlx::query_as::<_, Row>(
            r#"
            SELECT nt.id, nt.device_id, nt.provider, nt.token, nt.is_voip,
                   nt.environment, nt.failure_count, nt.invalidated_at
            FROM notification_tokens nt
            JOIN devices d ON d.id = nt.device_id AND d.revoked_at IS NULL
            WHERE nt.user_id = $1 AND nt.invalidated_at IS NULL
            "#,
        )
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        rows.into_iter()
            .map(|row| {
                Ok(NotificationToken {
                    id: NotificationTokenId::from(row.id),
                    user_id,
                    device_id: DeviceId::from(row.device_id),
                    provider: PushProvider::parse(&row.provider)?,
                    token: row.token,
                    is_voip: row.is_voip,
                    sandbox: row.environment == "sandbox",
                    failure_count: row.failure_count,
                    invalidated_at: row.invalidated_at,
                })
            })
            .collect()
    }

    /// Marked, not deleted, so the failure rate stays measurable.
    async fn invalidate_token(&self, token: &str) -> DomainResult<()> {
        sqlx::query("UPDATE notification_tokens SET invalidated_at = now() WHERE token = $1")
            .bind(token)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;
        Ok(())
    }

    /// Repeated transient failures eventually retire a token on their own: a
    /// token failing 10 times in a row is dead even if the provider never
    /// says so explicitly.
    async fn record_token_failure(&self, token: &str) -> DomainResult<()> {
        sqlx::query(
            r#"
            UPDATE notification_tokens
            SET failure_count  = failure_count + 1,
                invalidated_at = CASE WHEN failure_count + 1 >= 10 THEN now() ELSE invalidated_at END
            WHERE token = $1
            "#,
        )
        .bind(token)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    async fn preferences(&self, user_id: UserId) -> DomainResult<NotificationPreferences> {
        #[derive(FromRow)]
        struct Row {
            messages_enabled: bool,
            groups_enabled: bool,
            calls_enabled: bool,
            status_enabled: bool,
            channels_enabled: bool,
            reactions_enabled: bool,
            security_alerts_enabled: bool,
            preview_mode: String,
            message_tone_id: Option<String>,
            group_tone_id: Option<String>,
            call_ringtone_id: Option<String>,
            vibration: String,
            in_app_sounds: bool,
            high_priority: bool,
            quiet_hours_enabled: bool,
            quiet_hours_start_min: Option<i16>,
            quiet_hours_end_min: Option<i16>,
            quiet_hours_timezone: String,
            quiet_hours_allow_calls: bool,
        }

        let row = sqlx::query_as::<_, Row>(
            r#"
            SELECT messages_enabled, groups_enabled, calls_enabled, status_enabled,
                   channels_enabled, reactions_enabled, security_alerts_enabled,
                   preview_mode, message_tone_id, group_tone_id, call_ringtone_id,
                   vibration, in_app_sounds, high_priority, quiet_hours_enabled,
                   quiet_hours_start_min, quiet_hours_end_min, quiet_hours_timezone,
                   quiet_hours_allow_calls
            FROM notification_preferences WHERE user_id = $1
            "#,
        )
        .bind(user_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        // A missing row means defaults. Notification dispatch must never fail
        // because a settings row was not created.
        let Some(row) = row else {
            return Ok(NotificationPreferences::defaults_for(user_id));
        };

        let quiet_hours = match (
            row.quiet_hours_enabled,
            row.quiet_hours_start_min,
            row.quiet_hours_end_min,
        ) {
            (true, Some(start), Some(end)) => QuietHours::new(
                start as u16,
                end as u16,
                row.quiet_hours_timezone,
                row.quiet_hours_allow_calls,
            )
            .ok(),
            _ => None,
        };

        Ok(NotificationPreferences {
            user_id,
            messages_enabled: row.messages_enabled,
            groups_enabled: row.groups_enabled,
            calls_enabled: row.calls_enabled,
            status_enabled: row.status_enabled,
            channels_enabled: row.channels_enabled,
            reactions_enabled: row.reactions_enabled,
            security_alerts_enabled: row.security_alerts_enabled,
            preview_mode: PreviewMode::parse(&row.preview_mode).unwrap_or(PreviewMode::Full),
            message_tone_id: row.message_tone_id,
            group_tone_id: row.group_tone_id,
            call_ringtone_id: row.call_ringtone_id,
            vibration: Vibration::parse(&row.vibration).unwrap_or(Vibration::Default),
            in_app_sounds: row.in_app_sounds,
            high_priority: row.high_priority,
            quiet_hours,
        })
    }

    async fn save_preferences(&self, prefs: &NotificationPreferences) -> DomainResult<()> {
        sqlx::query(
            r#"
            INSERT INTO notification_preferences
                (user_id, messages_enabled, groups_enabled, calls_enabled, status_enabled,
                 channels_enabled, reactions_enabled, preview_mode, message_tone_id,
                 group_tone_id, call_ringtone_id, vibration, in_app_sounds, high_priority,
                 quiet_hours_enabled, quiet_hours_start_min, quiet_hours_end_min,
                 quiet_hours_timezone, quiet_hours_allow_calls, updated_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19, now())
            ON CONFLICT (user_id) DO UPDATE SET
                messages_enabled       = EXCLUDED.messages_enabled,
                groups_enabled         = EXCLUDED.groups_enabled,
                calls_enabled          = EXCLUDED.calls_enabled,
                status_enabled         = EXCLUDED.status_enabled,
                channels_enabled       = EXCLUDED.channels_enabled,
                reactions_enabled      = EXCLUDED.reactions_enabled,
                preview_mode           = EXCLUDED.preview_mode,
                message_tone_id        = EXCLUDED.message_tone_id,
                group_tone_id          = EXCLUDED.group_tone_id,
                call_ringtone_id       = EXCLUDED.call_ringtone_id,
                vibration              = EXCLUDED.vibration,
                in_app_sounds          = EXCLUDED.in_app_sounds,
                high_priority          = EXCLUDED.high_priority,
                quiet_hours_enabled    = EXCLUDED.quiet_hours_enabled,
                quiet_hours_start_min  = EXCLUDED.quiet_hours_start_min,
                quiet_hours_end_min    = EXCLUDED.quiet_hours_end_min,
                quiet_hours_timezone   = EXCLUDED.quiet_hours_timezone,
                quiet_hours_allow_calls = EXCLUDED.quiet_hours_allow_calls,
                updated_at             = now()
            "#,
        )
        .bind(prefs.user_id.as_uuid())
        .bind(prefs.messages_enabled)
        .bind(prefs.groups_enabled)
        .bind(prefs.calls_enabled)
        .bind(prefs.status_enabled)
        .bind(prefs.channels_enabled)
        .bind(prefs.reactions_enabled)
        .bind(prefs.preview_mode.as_str())
        .bind(&prefs.message_tone_id)
        .bind(&prefs.group_tone_id)
        .bind(&prefs.call_ringtone_id)
        .bind(prefs.vibration.as_str())
        .bind(prefs.in_app_sounds)
        .bind(prefs.high_priority)
        .bind(prefs.quiet_hours.is_some())
        .bind(prefs.quiet_hours.as_ref().map(|q| q.start_minute as i16))
        .bind(prefs.quiet_hours.as_ref().map(|q| q.end_minute as i16))
        .bind(
            prefs
                .quiet_hours
                .as_ref()
                .map(|q| q.timezone.clone())
                .unwrap_or_else(|| "UTC".into()),
        )
        .bind(
            prefs
                .quiet_hours
                .as_ref()
                .map(|q| q.allow_calls)
                .unwrap_or(true),
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    async fn conversation_settings(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
    ) -> DomainResult<ConversationNotificationSettings> {
        #[derive(FromRow)]
        struct Row {
            muted_until: Option<DateTime<Utc>>,
            notify_on_mention: bool,
            tone_id: Option<String>,
            call_ringtone_id: Option<String>,
            vibration: Option<String>,
            preview_mode: Option<String>,
        }

        let row = sqlx::query_as::<_, Row>(
            r#"
            SELECT muted_until, notify_on_mention, tone_id, call_ringtone_id,
                   vibration, preview_mode
            FROM conversation_notification_settings
            WHERE conversation_id = $1 AND user_id = $2
            "#,
        )
        .bind(conversation_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        let Some(row) = row else {
            return Ok(ConversationNotificationSettings::defaults_for(
                conversation_id,
                user_id,
            ));
        };

        Ok(ConversationNotificationSettings {
            conversation_id,
            user_id,
            mute: MuteState {
                muted_until: row.muted_until,
            },
            notify_on_mention: row.notify_on_mention,
            tone_id: row.tone_id,
            call_ringtone_id: row.call_ringtone_id,
            vibration: row.vibration.as_deref().and_then(|v| Vibration::parse(v).ok()),
            preview_mode: row
                .preview_mode
                .as_deref()
                .and_then(|p| PreviewMode::parse(p).ok()),
        })
    }

    async fn save_conversation_settings(
        &self,
        settings: &ConversationNotificationSettings,
    ) -> DomainResult<()> {
        sqlx::query(
            r#"
            INSERT INTO conversation_notification_settings
                (conversation_id, user_id, muted_until, notify_on_mention, tone_id,
                 call_ringtone_id, vibration, preview_mode, updated_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8, now())
            ON CONFLICT (conversation_id, user_id) DO UPDATE SET
                muted_until       = EXCLUDED.muted_until,
                notify_on_mention = EXCLUDED.notify_on_mention,
                tone_id           = EXCLUDED.tone_id,
                call_ringtone_id  = EXCLUDED.call_ringtone_id,
                vibration         = EXCLUDED.vibration,
                preview_mode      = EXCLUDED.preview_mode,
                updated_at        = now()
            "#,
        )
        .bind(settings.conversation_id.as_uuid())
        .bind(settings.user_id.as_uuid())
        .bind(settings.mute.muted_until)
        .bind(settings.notify_on_mention)
        .bind(&settings.tone_id)
        .bind(&settings.call_ringtone_id)
        .bind(settings.vibration.map(|v| v.as_str()))
        .bind(settings.preview_mode.map(|p| p.as_str()))
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    async fn list_tones(&self) -> DomainResult<Vec<NotificationTone>> {
        #[derive(FromRow)]
        struct Row {
            id: String,
            display_name: String,
            category: String,
            asset_name: String,
            is_default: bool,
        }

        let rows = sqlx::query_as::<_, Row>(
            r#"
            SELECT id, display_name, category, asset_name, is_default
            FROM notification_tones
            ORDER BY category, sort_order
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(rows
            .into_iter()
            .map(|row| NotificationTone {
                id: row.id,
                display_name: row.display_name,
                category: row.category,
                asset_name: row.asset_name,
                is_default: row.is_default,
            })
            .collect())
    }

    async fn tone_exists(&self, tone_id: &str) -> DomainResult<bool> {
        let found: Option<String> =
            sqlx::query_scalar("SELECT id FROM notification_tones WHERE id = $1")
                .bind(tone_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(map_sqlx)?;
        Ok(found.is_some())
    }

    /// `ON CONFLICT DO NOTHING` on the ledger's unique constraint is what makes
    /// push idempotent: a retried dispatch returns `false` and the caller
    /// skips it, so a phone cannot buzz twice for one message.
    #[allow(clippy::too_many_arguments)]
    async fn record_delivery(
        &self,
        user_id: UserId,
        device_id: Option<DeviceId>,
        conversation_id: Option<ConversationId>,
        message_seq: Option<Seq>,
        category: &str,
        status: &str,
        suppressed_reason: Option<&str>,
        provider: Option<&str>,
        error: Option<&str>,
    ) -> DomainResult<bool> {
        let result = sqlx::query(
            r#"
            INSERT INTO notification_deliveries
                (user_id, device_id, conversation_id, message_seq, category, status,
                 suppressed_reason, provider, error)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(user_id.as_uuid())
        .bind(device_id.map(DeviceId::as_uuid))
        .bind(conversation_id.map(ConversationId::as_uuid))
        .bind(message_seq.map(|seq| seq.value()))
        .bind(category)
        .bind(status)
        .bind(suppressed_reason)
        .bind(provider)
        .bind(error)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(result.rows_affected() == 1)
    }
}
