//! Device and notification-settings use cases (spec §11, §16, §26.2).

use nigchat_domain::entities::{
    ConversationNotificationSettings, Device, NotificationPreferences, NotificationTone,
    PushProvider, SecurityEvent, SecurityEventType,
};
use nigchat_domain::events::{DeviceEventKind, EventEnvelope, ServerEvent};
use nigchat_domain::ids::{ConversationId, DeviceId, UserId};
use nigchat_domain::values::{PreviewMode, QuietHours, Vibration};
use nigchat_domain::{DomainError, DomainResult};

use crate::services::Services;

pub struct DeviceService {
    services: Services,
}

pub struct RegisterPushTokenCommand {
    pub user_id: UserId,
    pub device_id: DeviceId,
    pub provider: PushProvider,
    pub token: String,
    pub is_voip: bool,
    pub sandbox: bool,
}

/// Partial update. `None` leaves a field alone, so two clients editing
/// different settings cannot clobber each other.
#[derive(Default)]
pub struct UpdateNotificationPreferences {
    pub messages_enabled: Option<bool>,
    pub groups_enabled: Option<bool>,
    pub calls_enabled: Option<bool>,
    pub status_enabled: Option<bool>,
    pub channels_enabled: Option<bool>,
    pub reactions_enabled: Option<bool>,
    pub preview_mode: Option<PreviewMode>,
    pub message_tone_id: Option<String>,
    pub group_tone_id: Option<String>,
    pub call_ringtone_id: Option<String>,
    pub vibration: Option<Vibration>,
    pub in_app_sounds: Option<bool>,
    pub quiet_hours: Option<Option<QuietHours>>,
}

#[derive(Default)]
pub struct UpdateConversationNotifications {
    pub notify_on_mention: Option<bool>,
    pub tone_id: Option<Option<String>>,
    pub call_ringtone_id: Option<Option<String>>,
    pub vibration: Option<Option<Vibration>>,
    pub preview_mode: Option<Option<PreviewMode>>,
}

impl DeviceService {
    pub fn new(services: Services) -> Self {
        Self { services }
    }

    pub async fn list(&self, user_id: UserId) -> DomainResult<Vec<Device>> {
        self.services.devices.list_active(user_id).await
    }

    /// Revoking a device kills its sessions in the same transaction, then tells
    /// that device to sign itself out. Order matters: the session dies first,
    /// so a device that ignores the event still cannot refresh.
    pub async fn revoke(
        &self,
        user_id: UserId,
        device_id: DeviceId,
        reason: &str,
    ) -> DomainResult<()> {
        let device = self
            .services
            .devices
            .find_by_id(device_id)
            .await?
            .ok_or(DomainError::not_found("device"))?;

        // Ownership check: device ids come from the client and are untrusted.
        if device.user_id != user_id {
            return Err(DomainError::Forbidden);
        }

        self.services.devices.revoke(device_id, reason).await?;

        self.services
            .security
            .record_event(
                SecurityEvent::new(user_id, SecurityEventType::DeviceRevoked)
                    .with_device(device_id)
                    .with_metadata(serde_json::json!({ "reason": reason })),
            )
            .await
            .ok();

        self.services
            .events
            .publish(EventEnvelope::to_device(
                user_id,
                device_id,
                ServerEvent::DeviceEvent {
                    device_id,
                    event: DeviceEventKind::Revoked,
                },
            ))
            .await
            .ok();

        Ok(())
    }

    /// Push tokens rotate frequently — on reinstall, on OS update, on APNs
    /// whim. Registration is an upsert keyed on `(provider, token)`, so the
    /// client can call it on every launch without accumulating rows.
    pub async fn register_push_token(
        &self,
        command: RegisterPushTokenCommand,
    ) -> DomainResult<()> {
        if command.token.trim().is_empty() || command.token.len() > 4_096 {
            return Err(DomainError::validation("invalid push token"));
        }

        let device = self
            .services
            .devices
            .find_by_id(command.device_id)
            .await?
            .ok_or(DomainError::not_found("device"))?;

        if device.user_id != command.user_id {
            return Err(DomainError::Forbidden);
        }

        self.services
            .notifications
            .register_token(
                command.user_id,
                command.device_id,
                command.provider,
                &command.token,
                command.is_voip,
                command.sandbox,
            )
            .await?;

        Ok(())
    }

    pub async fn list_tones(&self) -> DomainResult<Vec<NotificationTone>> {
        self.services.notifications.list_tones().await
    }

    pub async fn notification_preferences(
        &self,
        user_id: UserId,
    ) -> DomainResult<NotificationPreferences> {
        self.services.notifications.preferences(user_id).await
    }

    pub async fn update_notification_preferences(
        &self,
        user_id: UserId,
        update: UpdateNotificationPreferences,
    ) -> DomainResult<NotificationPreferences> {
        let mut prefs = self.services.notifications.preferences(user_id).await?;

        // Tone ids are client-supplied strings. Validating them against the
        // catalogue here means an unknown id is a 400 now, rather than a
        // silently silent notification three weeks later.
        for tone_id in [
            update.message_tone_id.as_deref(),
            update.group_tone_id.as_deref(),
            update.call_ringtone_id.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if !self.services.notifications.tone_exists(tone_id).await? {
                return Err(DomainError::validation(format!(
                    "unknown notification tone '{tone_id}'"
                )));
            }
        }

        apply(&mut prefs.messages_enabled, update.messages_enabled);
        apply(&mut prefs.groups_enabled, update.groups_enabled);
        apply(&mut prefs.calls_enabled, update.calls_enabled);
        apply(&mut prefs.status_enabled, update.status_enabled);
        apply(&mut prefs.channels_enabled, update.channels_enabled);
        apply(&mut prefs.reactions_enabled, update.reactions_enabled);
        apply(&mut prefs.preview_mode, update.preview_mode);
        apply(&mut prefs.vibration, update.vibration);
        apply(&mut prefs.in_app_sounds, update.in_app_sounds);

        if let Some(tone) = update.message_tone_id {
            prefs.message_tone_id = Some(tone);
        }
        if let Some(tone) = update.group_tone_id {
            prefs.group_tone_id = Some(tone);
        }
        if let Some(tone) = update.call_ringtone_id {
            prefs.call_ringtone_id = Some(tone);
        }
        // Double option: outer None means "not supplied", inner None means
        // "switch quiet hours off".
        if let Some(quiet) = update.quiet_hours {
            prefs.quiet_hours = quiet;
        }

        self.services.notifications.save_preferences(&prefs).await?;
        Ok(prefs)
    }

    pub async fn conversation_notifications(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
    ) -> DomainResult<ConversationNotificationSettings> {
        self.assert_member(conversation_id, user_id).await?;
        self.services
            .notifications
            .conversation_settings(conversation_id, user_id)
            .await
    }

    /// Per-conversation custom sound and mention behaviour (spec §16).
    pub async fn update_conversation_notifications(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
        update: UpdateConversationNotifications,
    ) -> DomainResult<ConversationNotificationSettings> {
        self.assert_member(conversation_id, user_id).await?;

        let mut settings = self
            .services
            .notifications
            .conversation_settings(conversation_id, user_id)
            .await?;

        for tone_id in [
            update.tone_id.as_ref().and_then(|t| t.as_deref()),
            update.call_ringtone_id.as_ref().and_then(|t| t.as_deref()),
        ]
        .into_iter()
        .flatten()
        {
            if !self.services.notifications.tone_exists(tone_id).await? {
                return Err(DomainError::validation(format!(
                    "unknown notification tone '{tone_id}'"
                )));
            }
        }

        apply(&mut settings.notify_on_mention, update.notify_on_mention);
        if let Some(tone) = update.tone_id {
            settings.tone_id = tone;
        }
        if let Some(tone) = update.call_ringtone_id {
            settings.call_ringtone_id = tone;
        }
        if let Some(vibration) = update.vibration {
            settings.vibration = vibration;
        }
        if let Some(preview) = update.preview_mode {
            settings.preview_mode = preview;
        }

        self.services
            .notifications
            .save_conversation_settings(&settings)
            .await?;

        Ok(settings)
    }

    /// Set or change the two-step verification PIN (spec §14).
    ///
    /// This is the control that stops a SIM-swap attacker from taking an
    /// account with nothing but a hijacked SMS. Argon2id, so a database leak
    /// does not yield usable PINs, and changing an existing PIN requires the
    /// current one.
    pub async fn set_two_step_pin(
        &self,
        user_id: UserId,
        new_pin: &str,
        current_pin: Option<&str>,
    ) -> DomainResult<()> {
        // 6–12 digits. Longer is fine, but a PIN is not a password and the
        // real protection is the attempt limit below, not the entropy.
        let valid = (6..=12).contains(&new_pin.len())
            && new_pin.chars().all(|c| c.is_ascii_digit());
        if !valid {
            return Err(DomainError::validation(
                "PIN must be between 6 and 12 digits",
            ));
        }

        // Reject the obvious ones outright rather than letting a user pick a
        // PIN an attacker would try first.
        if is_trivial_pin(new_pin) {
            return Err(DomainError::validation(
                "choose a less predictable PIN",
            ));
        }

        let existing = self.services.users.two_step_pin_hash(user_id).await?;
        if let Some(existing_hash) = existing {
            let supplied = current_pin.ok_or_else(|| {
                DomainError::validation("the current PIN is required to change it")
            })?;
            self.verify_pin_attempt(user_id, supplied, &existing_hash)
                .await?;
        }

        let hash = self.services.hasher.hash_secret(new_pin)?;
        self.services
            .users
            .set_two_step_pin(user_id, Some(&hash))
            .await?;

        self.services
            .security
            .record_event(SecurityEvent::new(user_id, SecurityEventType::PinChanged))
            .await
            .ok();

        Ok(())
    }

    /// Disabling requires the current PIN — otherwise a stolen access token
    /// could switch off the very control protecting the account.
    pub async fn disable_two_step(&self, user_id: UserId, current_pin: &str) -> DomainResult<()> {
        let existing = self
            .services
            .users
            .two_step_pin_hash(user_id)
            .await?
            .ok_or_else(|| DomainError::validation("two-step verification is not enabled"))?;

        self.verify_pin_attempt(user_id, current_pin, &existing)
            .await?;

        self.services.users.set_two_step_pin(user_id, None).await?;

        self.services
            .security
            .record_event(SecurityEvent::new(
                user_id,
                SecurityEventType::TwoStepDisabled,
            ))
            .await
            .ok();

        Ok(())
    }

    pub async fn verify_two_step(&self, user_id: UserId, pin: &str) -> DomainResult<bool> {
        let Some(hash) = self.services.users.two_step_pin_hash(user_id).await? else {
            return Ok(true); // not enabled
        };
        self.verify_pin_attempt(user_id, pin, &hash).await?;
        Ok(true)
    }

    /// Rate limited and audited. A six-digit PIN is only safe because guessing
    /// is throttled — five attempts an hour, and every failure is recorded on
    /// the user's own security timeline so a guessing campaign is visible.
    async fn verify_pin_attempt(
        &self,
        user_id: UserId,
        supplied: &str,
        hash: &str,
    ) -> DomainResult<()> {
        self.services
            .rate_limiter
            .check(&format!("pin:verify:{user_id}"), 5, 3_600)
            .await?;

        if !self.services.hasher.verify_secret(supplied, hash)? {
            self.services
                .security
                .record_event(SecurityEvent::new(user_id, SecurityEventType::PinFailed))
                .await
                .ok();
            return Err(DomainError::InvalidCredentials);
        }

        // A correct PIN clears the budget, so one mistyped digit does not lock
        // the user out for the rest of the hour.
        self.services
            .rate_limiter
            .reset(&format!("pin:verify:{user_id}"))
            .await
            .ok();

        Ok(())
    }

    /// The user-visible security timeline (spec §14).
    pub async fn security_events(
        &self,
        user_id: UserId,
        limit: i64,
    ) -> DomainResult<Vec<SecurityEvent>> {
        self.services
            .security
            .recent_events(user_id, limit.clamp(1, 200))
            .await
    }

    async fn assert_member(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
    ) -> DomainResult<()> {
        self.services
            .conversations
            .membership(conversation_id, user_id)
            .await?
            .filter(|member| member.is_active())
            .map(|_| ())
            .ok_or(DomainError::Forbidden)
    }
}

/// Repeated digits, and ascending or descending runs. These are what an
/// attacker tries in the handful of guesses the rate limit allows.
fn is_trivial_pin(pin: &str) -> bool {
    let digits: Vec<u8> = pin.bytes().collect();

    if digits.windows(2).all(|pair| pair[0] == pair[1]) {
        return true;
    }
    if digits.windows(2).all(|pair| pair[1] == pair[0] + 1) {
        return true;
    }
    if digits.windows(2).all(|pair| pair[0] == pair[1] + 1) {
        return true;
    }
    false
}

fn apply<T>(target: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *target = value;
    }
}

#[cfg(test)]
mod tests {
    use super::is_trivial_pin;

    #[test]
    fn rejects_predictable_pins() {
        assert!(is_trivial_pin("111111"));
        assert!(is_trivial_pin("123456"));
        assert!(is_trivial_pin("654321"));
    }

    #[test]
    fn accepts_ordinary_pins() {
        assert!(!is_trivial_pin("194837"));
        assert!(!is_trivial_pin("903212"));
    }
}
