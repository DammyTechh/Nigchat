//! Notification policy (spec §16).
//!
//! This is the single place that decides **whether** to notify, **how loudly**
//! and **with which sound**. It is pure: no database, no clock of its own, no
//! push SDK. Everything it needs is passed in, which means every rule below is
//! covered by a unit test that runs in microseconds.
//!
//! Keeping this out of the push adapter is deliberate. Notification rules are
//! the part of a messaging app users complain about most — a group that pings
//! at 3am, a muted chat that still buzzes, a custom tone that never plays. Any
//! rule expressed here can be verified; a rule buried inside an FCM client
//! cannot.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::entities::{
    ConversationKind, ConversationNotificationSettings, MessageKind, NotificationPreferences,
};
use crate::values::{PreviewMode, Vibration};

/// Why a notification was not sent. Recorded on the delivery row so support
/// can answer "why didn't I get notified?" with a fact instead of a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuppressionReason {
    /// The user has this category switched off entirely.
    CategoryDisabled,
    /// The conversation is muted and this message did not mention them.
    Muted,
    /// Inside the user's quiet-hours window.
    QuietHours,
    /// The user has an active socket; the in-app path already delivered it.
    RecipientOnline,
    /// The sender is blocked.
    SenderBlocked,
    /// Nothing to send to — no valid token on any device.
    NoValidToken,
    /// The user sent it themselves, from another device.
    OwnMessage,
}

impl SuppressionReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CategoryDisabled => "category_disabled",
            Self::Muted => "muted",
            Self::QuietHours => "quiet_hours",
            Self::RecipientOnline => "online",
            Self::SenderBlocked => "blocked",
            Self::NoValidToken => "no_valid_token",
            Self::OwnMessage => "own_message",
        }
    }
}

/// What kind of thing happened. Drives which preference switch and which tone
/// category apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationCategory {
    Message,
    Mention,
    Reply,
    Group,
    Call,
    MissedCall,
    Status,
    Channel,
    Reaction,
    DeviceLink,
    Security,
}

impl NotificationCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::Mention => "mention",
            Self::Reply => "reply",
            Self::Group => "group",
            Self::Call => "call",
            Self::MissedCall => "missed_call",
            Self::Status => "status",
            Self::Channel => "channel",
            Self::Reaction => "reaction",
            Self::DeviceLink => "device_link",
            Self::Security => "security",
        }
    }

    /// Categories that must reach the user even when they have asked for
    /// quiet. A security alert the user never sees is worse than useless, and
    /// a missed call is time-critical by definition.
    pub fn bypasses_quiet_hours(&self) -> bool {
        matches!(self, Self::Security | Self::DeviceLink)
    }

    /// Categories that must reach the user even when the conversation is
    /// muted. Being @mentioned is the classic case: users mute a busy group
    /// but still expect to hear when someone addresses them directly.
    pub fn bypasses_mute(&self) -> bool {
        matches!(self, Self::Mention | Self::Call | Self::Security)
    }
}

/// The facts the policy needs. Assembled by the application layer from the
/// database; the policy itself performs no lookups.
#[derive(Debug, Clone)]
pub struct NotificationContext {
    pub category: NotificationCategory,
    pub conversation_kind: ConversationKind,
    pub message_kind: Option<MessageKind>,
    /// True when this recipient is @mentioned in the message.
    pub is_mention: bool,
    /// True when the message replies to one of this recipient's messages.
    pub is_reply_to_recipient: bool,
    /// True when the recipient has a live WebSocket on any device.
    pub recipient_online: bool,
    pub sender_blocked: bool,
    /// The recipient is also the sender, on another device.
    pub is_own_message: bool,
    pub has_valid_token: bool,
    /// Current time already converted into the recipient's local zone. The
    /// conversion belongs to infrastructure; the window logic belongs here.
    pub recipient_local_time: DateTime<chrono_tz_shim::FixedOffsetLike>,
    pub now_utc: DateTime<Utc>,
}

/// A minimal stand-in so the domain does not depend on a timezone crate.
/// Infrastructure converts UTC into the user's zone and hands the result over
/// as a fixed-offset timestamp.
pub mod chrono_tz_shim {
    pub type FixedOffsetLike = chrono::FixedOffset;
}

/// The resolved instruction handed to the push adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationPlan {
    pub category: NotificationCategory,
    /// Tone identifier, resolved through the override chain. `None` means
    /// silent.
    pub tone_id: Option<String>,
    pub vibration: Vibration,
    pub preview_mode: PreviewMode,
    /// High priority wakes a sleeping device. Reserved for calls and
    /// security, because abusing it drains batteries and gets an app
    /// throttled by the platform.
    pub high_priority: bool,
    /// Collapse key: notifications sharing one are replaced rather than
    /// stacked, which keeps a busy group from filling the shade.
    pub collapse_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationDecision {
    Send(Box<NotificationPlan>),
    Suppress(SuppressionReason),
}

impl NotificationDecision {
    pub fn is_send(&self) -> bool {
        matches!(self, Self::Send(_))
    }

    pub fn plan(&self) -> Option<&NotificationPlan> {
        match self {
            Self::Send(plan) => Some(plan),
            Self::Suppress(_) => None,
        }
    }

    pub fn suppression_reason(&self) -> Option<SuppressionReason> {
        match self {
            Self::Suppress(reason) => Some(*reason),
            Self::Send(_) => None,
        }
    }
}

pub struct NotificationPolicy;

impl NotificationPolicy {
    /// Decide what to do with one (recipient, event) pair.
    ///
    /// Checks run cheapest-and-most-absolute first, so an obviously suppressed
    /// notification costs almost nothing on a fan-out to a 500-member group.
    pub fn decide(
        context: &NotificationContext,
        preferences: &NotificationPreferences,
        conversation: &ConversationNotificationSettings,
    ) -> NotificationDecision {
        use NotificationDecision::Suppress;

        // 1. Absolutes. No preference can override these.
        if context.is_own_message {
            return Suppress(SuppressionReason::OwnMessage);
        }
        if context.sender_blocked {
            return Suppress(SuppressionReason::SenderBlocked);
        }
        if !context.has_valid_token {
            return Suppress(SuppressionReason::NoValidToken);
        }

        // 2. Already delivered in-app. Calls are the exception: a ringing call
        //    must produce a system-level alert even with the app open, because
        //    the user may not be looking at the screen.
        if context.recipient_online && !matches!(context.category, NotificationCategory::Call) {
            return Suppress(SuppressionReason::RecipientOnline);
        }

        // 3. Category switches.
        if !Self::category_enabled(context, preferences) {
            return Suppress(SuppressionReason::CategoryDisabled);
        }

        // 4. Mute. A mention or a call cuts through, unless the user has
        //    explicitly turned mention notifications off for this chat.
        let effective_category = Self::effective_category(context);
        let muted = conversation.mute.is_muted_at(context.now_utc);
        if muted {
            let cuts_through = effective_category.bypasses_mute()
                && (!context.is_mention || conversation.notify_on_mention);
            if !cuts_through {
                return Suppress(SuppressionReason::Muted);
            }
        }

        // 5. Quiet hours, evaluated in the recipient's local time.
        if let Some(quiet) = &preferences.quiet_hours {
            if quiet.contains(context.recipient_local_time)
                && !effective_category.bypasses_quiet_hours()
            {
                let call_allowed = quiet.allow_calls
                    && matches!(
                        effective_category,
                        NotificationCategory::Call | NotificationCategory::MissedCall
                    );
                if !call_allowed {
                    return Suppress(SuppressionReason::QuietHours);
                }
            }
        }

        NotificationDecision::Send(Box::new(NotificationPlan {
            category: effective_category,
            tone_id: Self::resolve_tone(effective_category, preferences, conversation),
            vibration: conversation.vibration.unwrap_or(preferences.vibration),
            preview_mode: conversation.preview_mode.unwrap_or(preferences.preview_mode),
            high_priority: Self::is_high_priority(effective_category, preferences),
            collapse_key: Self::collapse_key(context),
        }))
    }

    /// A mention or a reply is more specific than a plain message, and the
    /// more specific category wins — that is what makes mute-with-mentions
    /// work.
    fn effective_category(context: &NotificationContext) -> NotificationCategory {
        if context.is_mention {
            NotificationCategory::Mention
        } else if context.is_reply_to_recipient {
            NotificationCategory::Reply
        } else if matches!(context.category, NotificationCategory::Message)
            && matches!(context.conversation_kind, ConversationKind::Group)
        {
            NotificationCategory::Group
        } else {
            context.category
        }
    }

    fn category_enabled(
        context: &NotificationContext,
        preferences: &NotificationPreferences,
    ) -> bool {
        match context.category {
            NotificationCategory::Message
            | NotificationCategory::Mention
            | NotificationCategory::Reply => match context.conversation_kind {
                ConversationKind::Group => preferences.groups_enabled,
                ConversationKind::Channel => preferences.channels_enabled,
                ConversationKind::Direct => preferences.messages_enabled,
            },
            NotificationCategory::Group => preferences.groups_enabled,
            NotificationCategory::Channel => preferences.channels_enabled,
            NotificationCategory::Call | NotificationCategory::MissedCall => {
                preferences.calls_enabled
            }
            NotificationCategory::Status => preferences.status_enabled,
            NotificationCategory::Reaction => preferences.reactions_enabled,
            // Security and device-link alerts are not user-disableable: an
            // attacker who can mute the alerts can take the account silently.
            NotificationCategory::Security | NotificationCategory::DeviceLink => true,
        }
    }

    /// Tone override chain, most specific first:
    ///   per-conversation tone → account tone for that category → platform
    ///   default (`None`, meaning the client's own default sound).
    fn resolve_tone(
        category: NotificationCategory,
        preferences: &NotificationPreferences,
        conversation: &ConversationNotificationSettings,
    ) -> Option<String> {
        match category {
            NotificationCategory::Call | NotificationCategory::MissedCall => conversation
                .call_ringtone_id
                .clone()
                .or_else(|| preferences.call_ringtone_id.clone()),
            NotificationCategory::Group => conversation
                .tone_id
                .clone()
                .or_else(|| preferences.group_tone_id.clone()),
            NotificationCategory::Security => Some("tone.system.security".to_string()),
            NotificationCategory::Status => Some("tone.status.default".to_string()),
            _ => conversation
                .tone_id
                .clone()
                .or_else(|| preferences.message_tone_id.clone()),
        }
    }

    fn is_high_priority(
        category: NotificationCategory,
        preferences: &NotificationPreferences,
    ) -> bool {
        matches!(
            category,
            NotificationCategory::Call | NotificationCategory::Security
        ) || preferences.high_priority
    }

    /// One key per conversation so a burst of messages replaces rather than
    /// stacks. Calls are never collapsed — each is a distinct event.
    fn collapse_key(context: &NotificationContext) -> Option<String> {
        match context.category {
            NotificationCategory::Call | NotificationCategory::Security => None,
            _ => Some(format!("conv:{}", context.conversation_kind.as_str())),
        }
    }

    /// The text a device shows, given the resolved preview mode.
    ///
    /// Returns a *template*, not decrypted content — the server cannot read
    /// the message (spec §28). With `Full`, the device decrypts locally and
    /// substitutes the body itself.
    pub fn notification_title(
        plan: &NotificationPlan,
        sender_display_name: Option<&str>,
        conversation_title: Option<&str>,
        conversation_kind: ConversationKind,
    ) -> String {
        match plan.preview_mode {
            PreviewMode::Hidden => "NigChat".to_string(),
            PreviewMode::NameOnly | PreviewMode::Full => match conversation_kind {
                ConversationKind::Direct => {
                    sender_display_name.unwrap_or("New message").to_string()
                }
                _ => conversation_title
                    .or(sender_display_name)
                    .unwrap_or("New message")
                    .to_string(),
            },
        }
    }

    pub fn notification_body(plan: &NotificationPlan, message_kind: Option<MessageKind>) -> String {
        match plan.preview_mode {
            PreviewMode::Hidden => "You have a new message".to_string(),
            // With Full, the client replaces this once it decrypts. This is the
            // fallback shown if decryption is not possible (for example on a
            // device that has not yet established a session).
            PreviewMode::Full | PreviewMode::NameOnly => message_kind
                .map(|kind| kind.notification_label().to_string())
                .unwrap_or_else(|| "New message".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ConversationId, UserId};
    use crate::values::{MuteState, QuietHours};
    use chrono::{FixedOffset, TimeZone};

    fn local(hour: u32, minute: u32) -> DateTime<FixedOffset> {
        FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(2026, 8, 29, hour, minute, 0)
            .unwrap()
    }

    fn context() -> NotificationContext {
        NotificationContext {
            category: NotificationCategory::Message,
            conversation_kind: ConversationKind::Direct,
            message_kind: Some(MessageKind::Text),
            is_mention: false,
            is_reply_to_recipient: false,
            recipient_online: false,
            sender_blocked: false,
            is_own_message: false,
            has_valid_token: true,
            recipient_local_time: local(12, 0),
            now_utc: Utc::now(),
        }
    }

    fn preferences() -> NotificationPreferences {
        NotificationPreferences::defaults_for(UserId::new())
    }

    fn conversation_settings() -> ConversationNotificationSettings {
        ConversationNotificationSettings::defaults_for(ConversationId::new(), UserId::new())
    }

    #[test]
    fn sends_a_plain_direct_message() {
        let decision =
            NotificationPolicy::decide(&context(), &preferences(), &conversation_settings());
        let plan = decision.plan().expect("should send");
        assert_eq!(plan.tone_id.as_deref(), Some("tone.message.default"));
        assert_eq!(plan.preview_mode, PreviewMode::Full);
        assert!(!plan.high_priority);
    }

    #[test]
    fn suppresses_when_recipient_has_a_live_socket() {
        let mut ctx = context();
        ctx.recipient_online = true;
        let decision = NotificationPolicy::decide(&ctx, &preferences(), &conversation_settings());
        assert_eq!(
            decision.suppression_reason(),
            Some(SuppressionReason::RecipientOnline)
        );
    }

    #[test]
    fn a_ringing_call_still_alerts_an_online_user() {
        let mut ctx = context();
        ctx.recipient_online = true;
        ctx.category = NotificationCategory::Call;
        let decision = NotificationPolicy::decide(&ctx, &preferences(), &conversation_settings());
        assert!(decision.is_send());
        assert!(decision.plan().unwrap().high_priority);
    }

    #[test]
    fn muted_conversation_is_silent() {
        let mut settings = conversation_settings();
        settings.mute = MuteState {
            muted_until: Some(Utc::now() + chrono::Duration::hours(4)),
        };
        let decision = NotificationPolicy::decide(&context(), &preferences(), &settings);
        assert_eq!(decision.suppression_reason(), Some(SuppressionReason::Muted));
    }

    #[test]
    fn a_mention_cuts_through_mute() {
        let mut ctx = context();
        ctx.is_mention = true;
        ctx.conversation_kind = ConversationKind::Group;

        let mut settings = conversation_settings();
        settings.mute = MuteState {
            muted_until: Some(Utc::now() + chrono::Duration::hours(4)),
        };

        let decision = NotificationPolicy::decide(&ctx, &preferences(), &settings);
        let plan = decision.plan().expect("mention should cut through mute");
        assert_eq!(plan.category, NotificationCategory::Mention);
    }

    #[test]
    fn mention_can_be_silenced_per_conversation() {
        let mut ctx = context();
        ctx.is_mention = true;

        let mut settings = conversation_settings();
        settings.mute = MuteState {
            muted_until: Some(Utc::now() + chrono::Duration::hours(4)),
        };
        settings.notify_on_mention = false;

        let decision = NotificationPolicy::decide(&ctx, &preferences(), &settings);
        assert_eq!(decision.suppression_reason(), Some(SuppressionReason::Muted));
    }

    #[test]
    fn quiet_hours_silence_messages_across_midnight() {
        let mut prefs = preferences();
        prefs.quiet_hours = Some(QuietHours::new(22 * 60, 7 * 60, "Africa/Lagos", true).unwrap());

        let mut ctx = context();
        ctx.recipient_local_time = local(23, 30);

        let decision = NotificationPolicy::decide(&ctx, &prefs, &conversation_settings());
        assert_eq!(
            decision.suppression_reason(),
            Some(SuppressionReason::QuietHours)
        );
    }

    #[test]
    fn quiet_hours_can_still_allow_calls() {
        let mut prefs = preferences();
        prefs.quiet_hours = Some(QuietHours::new(22 * 60, 7 * 60, "Africa/Lagos", true).unwrap());

        let mut ctx = context();
        ctx.recipient_local_time = local(2, 0);
        ctx.category = NotificationCategory::Call;

        let decision = NotificationPolicy::decide(&ctx, &prefs, &conversation_settings());
        assert!(decision.is_send());
    }

    #[test]
    fn security_alerts_ignore_quiet_hours_and_cannot_be_disabled() {
        let mut prefs = preferences();
        prefs.messages_enabled = false;
        prefs.quiet_hours = Some(QuietHours::new(0, 1439, "UTC", false).unwrap());

        let mut ctx = context();
        ctx.category = NotificationCategory::Security;
        ctx.recipient_local_time = local(3, 0);

        let decision = NotificationPolicy::decide(&ctx, &prefs, &conversation_settings());
        let plan = decision.plan().expect("security alerts always send");
        assert_eq!(plan.tone_id.as_deref(), Some("tone.system.security"));
        assert!(plan.high_priority);
    }

    #[test]
    fn per_conversation_tone_beats_the_account_tone() {
        let mut settings = conversation_settings();
        settings.tone_id = Some("tone.message.pulse".into());

        let decision = NotificationPolicy::decide(&context(), &preferences(), &settings);
        assert_eq!(
            decision.plan().unwrap().tone_id.as_deref(),
            Some("tone.message.pulse")
        );
    }

    #[test]
    fn group_messages_use_the_group_tone() {
        let mut ctx = context();
        ctx.conversation_kind = ConversationKind::Group;

        let decision = NotificationPolicy::decide(&ctx, &preferences(), &conversation_settings());
        let plan = decision.plan().unwrap();
        assert_eq!(plan.category, NotificationCategory::Group);
        assert_eq!(plan.tone_id.as_deref(), Some("tone.group.default"));
    }

    #[test]
    fn hidden_preview_reveals_nothing() {
        let mut prefs = preferences();
        prefs.preview_mode = PreviewMode::Hidden;

        let decision = NotificationPolicy::decide(&context(), &prefs, &conversation_settings());
        let plan = decision.plan().unwrap();

        let title = NotificationPolicy::notification_title(
            plan,
            Some("Ada Obi"),
            None,
            ConversationKind::Direct,
        );
        let body = NotificationPolicy::notification_body(plan, Some(MessageKind::Image));

        assert_eq!(title, "NigChat");
        assert!(!body.contains("Photo"));
        assert!(!title.contains("Ada"));
    }

    #[test]
    fn own_messages_never_notify_other_devices_with_an_alert() {
        let mut ctx = context();
        ctx.is_own_message = true;
        let decision = NotificationPolicy::decide(&ctx, &preferences(), &conversation_settings());
        assert_eq!(
            decision.suppression_reason(),
            Some(SuppressionReason::OwnMessage)
        );
    }

    #[test]
    fn blocked_senders_cannot_notify() {
        let mut ctx = context();
        ctx.sender_blocked = true;
        let decision = NotificationPolicy::decide(&ctx, &preferences(), &conversation_settings());
        assert_eq!(
            decision.suppression_reason(),
            Some(SuppressionReason::SenderBlocked)
        );
    }
}
