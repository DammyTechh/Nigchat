//! Entities — things with an identity that persists across changes.
//!
//! These are storage-agnostic. A repository translates between these and rows;
//! nothing here knows a column name.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::*;
use crate::values::{MuteState, PhoneNumber, PreviewMode, QuietHours, Seq, Username, Vibration};

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub phone: PhoneNumber,
    pub username: Option<Username>,
    pub display_name: String,
    pub about: Option<String>,
    pub avatar_media_id: Option<MediaId>,
    pub two_step_enabled: bool,
    pub is_active: bool,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl User {
    /// Whether this account may currently authenticate or send.
    pub fn can_transact(&self) -> bool {
        self.is_active
    }
}

/// What another user is allowed to see. Resolved server-side — never trust a
/// client to hide something it was sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Everyone,
    Contacts,
    Nobody,
}

impl Visibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Everyone => "everyone",
            Self::Contacts => "contacts",
            Self::Nobody => "nobody",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "everyone" => Self::Everyone,
            "nobody" => Self::Nobody,
            _ => Self::Contacts,
        }
    }

    pub fn allows(&self, viewer_is_contact: bool) -> bool {
        match self {
            Self::Everyone => true,
            Self::Contacts => viewer_is_contact,
            Self::Nobody => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacySettings {
    pub user_id: UserId,
    pub last_seen: Visibility,
    pub profile_photo: Visibility,
    pub about: Visibility,
    pub status: Visibility,
    pub read_receipts_enabled: bool,
    pub typing_indicators_enabled: bool,
    pub who_can_add_to_groups: Visibility,
    pub who_can_call: Visibility,
    pub silence_unknown_callers: bool,
    pub strict_privacy_mode: bool,
}

// ---------------------------------------------------------------------------
// Devices
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Ios,
    IpadOs,
    Android,
    AndroidTablet,
    Web,
    Windows,
    MacOs,
    Linux,
}

impl Platform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ios => "ios",
            Self::IpadOs => "ipados",
            Self::Android => "android",
            Self::AndroidTablet => "android_tablet",
            Self::Web => "web",
            Self::Windows => "windows",
            Self::MacOs => "macos",
            Self::Linux => "linux",
        }
    }

    pub fn parse(value: &str) -> crate::DomainResult<Self> {
        Ok(match value {
            "ios" => Self::Ios,
            "ipados" => Self::IpadOs,
            "android" => Self::Android,
            "android_tablet" => Self::AndroidTablet,
            "web" => Self::Web,
            "windows" => Self::Windows,
            "macos" => Self::MacOs,
            "linux" => Self::Linux,
            other => {
                return Err(crate::DomainError::validation(format!(
                    "unsupported platform '{other}'"
                )))
            }
        })
    }

    /// Which push provider this platform uses by default.
    pub fn default_push_provider(&self) -> PushProvider {
        match self {
            Self::Ios | Self::IpadOs | Self::MacOs => PushProvider::Apns,
            Self::Android | Self::AndroidTablet => PushProvider::Fcm,
            Self::Web | Self::Windows | Self::Linux => PushProvider::WebPush,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: DeviceId,
    pub user_id: UserId,
    pub platform: Platform,
    pub device_name: Option<String>,
    pub app_version: Option<String>,
    pub is_primary: bool,
    pub linked_at: DateTime<Utc>,
    pub last_active_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl Device {
    pub fn is_active(&self) -> bool {
        self.revoked_at.is_none()
    }
}

/// The public half of a device's E2EE identity (spec §28). The server holds
/// public material only — it is a key directory, not a key holder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceIdentityKey {
    pub device_id: DeviceId,
    pub user_id: UserId,
    pub identity_public_key: Vec<u8>,
    pub key_version: i32,
    pub rotated_at: Option<DateTime<Utc>>,
}

/// One bundle hands a sender everything needed to open a session with one
/// device. The one-time prekey is consumed on handout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreKeyBundle {
    pub user_id: UserId,
    pub device_id: DeviceId,
    pub registration_id: i32,
    pub identity_public_key: Vec<u8>,
    pub signed_prekey_id: i32,
    pub signed_prekey_public: Vec<u8>,
    pub signed_prekey_signature: Vec<u8>,
    /// `None` when the device has exhausted its one-time keys. Sessions can
    /// still be established, with weaker forward-secrecy properties, so this
    /// must trigger a top-up push to that device.
    pub one_time_prekey_id: Option<i32>,
    pub one_time_prekey_public: Option<Vec<u8>>,
}

// ---------------------------------------------------------------------------
// Conversations
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationKind {
    Direct,
    Group,
    Channel,
}

impl ConversationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Group => "group",
            Self::Channel => "channel",
        }
    }

    pub fn parse(value: &str) -> crate::DomainResult<Self> {
        Ok(match value {
            "direct" => Self::Direct,
            "group" => Self::Group,
            "channel" => Self::Channel,
            other => {
                return Err(crate::DomainError::validation(format!(
                    "unknown conversation kind '{other}'"
                )))
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    Member,
    Admin,
    Owner,
}

impl MemberRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Member => "member",
            Self::Admin => "admin",
            Self::Owner => "owner",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "owner" => Self::Owner,
            "admin" => Self::Admin,
            _ => Self::Member,
        }
    }

    pub fn can_administer(&self) -> bool {
        matches!(self, Self::Admin | Self::Owner)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: ConversationId,
    pub kind: ConversationKind,
    pub community_id: Option<CommunityId>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub avatar_media_id: Option<MediaId>,
    pub created_by: Option<UserId>,
    pub only_admins_can_post: bool,
    pub disappearing_seconds: Option<i32>,
    pub max_members: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Conversation {
    /// Central authorization rule for sending. Called by the messaging use
    /// case before anything is written.
    pub fn can_post(&self, role: MemberRole) -> bool {
        match self.kind {
            ConversationKind::Direct => true,
            ConversationKind::Group => !self.only_admins_can_post || role.can_administer(),
            // Channels are broadcast-only: followers read, admins write.
            ConversationKind::Channel => role.can_administer(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMember {
    pub conversation_id: ConversationId,
    pub user_id: UserId,
    pub role: MemberRole,
    pub last_read_seq: Seq,
    pub last_delivered_seq: Seq,
    pub is_pinned: bool,
    pub is_archived: bool,
    pub is_locked: bool,
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,
}

impl ConversationMember {
    pub fn is_active(&self) -> bool {
        self.left_at.is_none()
    }

    pub fn unread_count(&self, head: Seq) -> i64 {
        head.distance_from(self.last_read_seq)
    }
}

/// A row in the conversation list. Denormalised on read so rendering the list
/// is one query rather than one query per row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: ConversationId,
    pub kind: ConversationKind,
    pub title: Option<String>,
    pub avatar_media_id: Option<MediaId>,
    pub head_seq: Seq,
    pub last_read_seq: Seq,
    pub unread_count: i64,
    pub last_message_at: Option<DateTime<Utc>>,
    pub last_message_kind: Option<String>,
    pub is_pinned: bool,
    pub is_archived: bool,
    pub is_locked: bool,
    pub mute: MuteState,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Text,
    Image,
    Video,
    Audio,
    VoiceNote,
    Document,
    Sticker,
    Gif,
    Location,
    Contact,
    Poll,
    System,
    CallEvent,
}

impl MessageKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::VoiceNote => "voice_note",
            Self::Document => "document",
            Self::Sticker => "sticker",
            Self::Gif => "gif",
            Self::Location => "location",
            Self::Contact => "contact",
            Self::Poll => "poll",
            Self::System => "system",
            Self::CallEvent => "call_event",
        }
    }

    pub fn parse(value: &str) -> crate::DomainResult<Self> {
        Ok(match value {
            "text" => Self::Text,
            "image" => Self::Image,
            "video" => Self::Video,
            "audio" => Self::Audio,
            "voice_note" => Self::VoiceNote,
            "document" => Self::Document,
            "sticker" => Self::Sticker,
            "gif" => Self::Gif,
            "location" => Self::Location,
            "contact" => Self::Contact,
            "poll" => Self::Poll,
            "system" => Self::System,
            "call_event" => Self::CallEvent,
            other => {
                return Err(crate::DomainError::validation(format!(
                    "unsupported message kind '{other}'"
                )))
            }
        })
    }

    /// Clients may not author system messages; only the server may.
    pub fn is_client_authorable(&self) -> bool {
        !matches!(self, Self::System)
    }

    /// The word shown in a notification when previews are off, and in the
    /// conversation list.
    pub fn notification_label(&self) -> &'static str {
        match self {
            Self::Text => "Message",
            Self::Image => "Photo",
            Self::Video => "Video",
            Self::Audio | Self::VoiceNote => "Voice message",
            Self::Document => "Document",
            Self::Sticker => "Sticker",
            Self::Gif => "GIF",
            Self::Location => "Location",
            Self::Contact => "Contact",
            Self::Poll => "Poll",
            Self::System => "Update",
            Self::CallEvent => "Call",
        }
    }
}

/// A message as the server knows it.
///
/// `ciphertext` is opaque: the server routes and orders, it does not read
/// (spec §28). There is no plaintext field for user messages at all, which
/// makes "never log message bodies" a property of the type rather than a rule
/// someone has to remember.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub conversation_id: ConversationId,
    pub seq: Seq,
    pub sender_id: Option<UserId>,
    pub sender_device_id: Option<DeviceId>,
    pub client_message_id: ClientMessageId,
    pub kind: MessageKind,
    pub ciphertext: Option<Vec<u8>>,
    pub envelope_version: i16,
    /// Populated only for server-authored system messages.
    pub system_text: Option<String>,
    /// Routing and UI metadata. Never content.
    pub metadata: serde_json::Value,
    pub reply_to_id: Option<MessageId>,
    pub forward_score: i16,
    pub expires_at: Option<DateTime<Utc>>,
    pub edited_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    /// Populated when a message is read back. Empty on the write path.
    #[serde(default)]
    pub attachments: Vec<MessageAttachment>,
}

impl Message {
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }

    /// Only the author may edit, only within the window, and never a deleted
    /// message.
    pub fn can_be_edited_by(&self, user_id: UserId, now: DateTime<Utc>) -> bool {
        const EDIT_WINDOW_MINUTES: i64 = 15;
        self.sender_id == Some(user_id)
            && !self.is_deleted()
            && self.kind == MessageKind::Text
            && (now - self.created_at) < chrono::Duration::minutes(EDIT_WINDOW_MINUTES)
    }
}

/// A file hanging off a message. Metadata only — the bytes live in object
/// storage and the client resolves a URL from the id when it renders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageAttachment {
    pub media_id: MediaId,
    pub mime_type: String,
    pub byte_size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_ms: Option<i32>,
    pub position: i16,
}

impl MessageAttachment {
    /// Drives how the bubble renders it: inline, as a player, or as a file row.
    pub fn category(&self) -> &'static str {
        if self.mime_type.starts_with("image/") {
            "image"
        } else if self.mime_type.starts_with("video/") {
            "video"
        } else if self.mime_type.starts_with("audio/") {
            "audio"
        } else {
            "file"
        }
    }
}

/// Everything needed to write a message, validated before it reaches a
/// repository.
#[derive(Debug, Clone)]
pub struct NewMessage {
    pub conversation_id: ConversationId,
    pub sender_id: UserId,
    pub sender_device_id: DeviceId,
    pub client_message_id: ClientMessageId,
    pub kind: MessageKind,
    pub ciphertext: Vec<u8>,
    pub envelope_version: i16,
    pub metadata: serde_json::Value,
    pub reply_to_id: Option<MessageId>,
    pub mentions: Vec<UserId>,
    pub media_ids: Vec<MediaId>,
    pub expires_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Notifications (spec §16)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PushProvider {
    Fcm,
    Apns,
    WebPush,
}

impl PushProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fcm => "fcm",
            Self::Apns => "apns",
            Self::WebPush => "web_push",
        }
    }

    pub fn parse(value: &str) -> crate::DomainResult<Self> {
        Ok(match value {
            "fcm" => Self::Fcm,
            "apns" => Self::Apns,
            "web_push" => Self::WebPush,
            other => {
                return Err(crate::DomainError::validation(format!(
                    "unknown push provider '{other}'"
                )))
            }
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationToken {
    pub id: NotificationTokenId,
    pub user_id: UserId,
    pub device_id: DeviceId,
    pub provider: PushProvider,
    pub token: String,
    pub is_voip: bool,
    pub sandbox: bool,
    pub failure_count: i16,
    pub invalidated_at: Option<DateTime<Utc>>,
}

impl NotificationToken {
    pub fn is_usable(&self) -> bool {
        self.invalidated_at.is_none()
    }
}

/// A selectable sound. The audio ships with the client; the server stores the
/// identifier, so adding a tone is a migration row and not an app release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationTone {
    pub id: String,
    pub display_name: String,
    pub category: String,
    pub asset_name: String,
    pub is_default: bool,
}

/// Account-wide notification rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPreferences {
    pub user_id: UserId,
    pub messages_enabled: bool,
    pub groups_enabled: bool,
    pub calls_enabled: bool,
    pub status_enabled: bool,
    pub channels_enabled: bool,
    pub reactions_enabled: bool,
    pub security_alerts_enabled: bool,
    pub preview_mode: PreviewMode,
    pub message_tone_id: Option<String>,
    pub group_tone_id: Option<String>,
    pub call_ringtone_id: Option<String>,
    pub vibration: Vibration,
    pub in_app_sounds: bool,
    pub high_priority: bool,
    pub quiet_hours: Option<QuietHours>,
}

impl NotificationPreferences {
    /// Sensible defaults for a new account: messages and calls on, status off.
    pub fn defaults_for(user_id: UserId) -> Self {
        Self {
            user_id,
            messages_enabled: true,
            groups_enabled: true,
            calls_enabled: true,
            status_enabled: false,
            channels_enabled: true,
            reactions_enabled: true,
            security_alerts_enabled: true,
            preview_mode: PreviewMode::Full,
            message_tone_id: Some("tone.message.default".into()),
            group_tone_id: Some("tone.group.default".into()),
            call_ringtone_id: Some("tone.call.default".into()),
            vibration: Vibration::Default,
            in_app_sounds: true,
            high_priority: false,
            quiet_hours: None,
        }
    }
}

/// Per-conversation overrides. Every `None` falls back to the account default,
/// which is why these are options rather than duplicated values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationNotificationSettings {
    pub conversation_id: ConversationId,
    pub user_id: UserId,
    pub mute: MuteState,
    pub notify_on_mention: bool,
    pub tone_id: Option<String>,
    pub call_ringtone_id: Option<String>,
    pub vibration: Option<Vibration>,
    pub preview_mode: Option<PreviewMode>,
}

impl ConversationNotificationSettings {
    pub fn defaults_for(conversation_id: ConversationId, user_id: UserId) -> Self {
        Self {
            conversation_id,
            user_id,
            mute: MuteState::unmuted(),
            notify_on_mention: true,
            tone_id: None,
            call_ringtone_id: None,
            vibration: None,
            preview_mode: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Security
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityEventType {
    Login,
    Logout,
    DeviceLinked,
    DeviceRevoked,
    KeyChanged,
    PinChanged,
    PinFailed,
    PasskeyAdded,
    PasskeyRemoved,
    SessionReuseDetected,
    SuspiciousLogin,
    AccountDeactivated,
    TwoStepEnabled,
    TwoStepDisabled,
    BackupCreated,
}

impl SecurityEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Logout => "logout",
            Self::DeviceLinked => "device_linked",
            Self::DeviceRevoked => "device_revoked",
            Self::KeyChanged => "key_changed",
            Self::PinChanged => "pin_changed",
            Self::PinFailed => "pin_failed",
            Self::PasskeyAdded => "passkey_added",
            Self::PasskeyRemoved => "passkey_removed",
            Self::SessionReuseDetected => "session_reuse_detected",
            Self::SuspiciousLogin => "suspicious_login",
            Self::AccountDeactivated => "account_deactivated",
            Self::TwoStepEnabled => "two_step_enabled",
            Self::TwoStepDisabled => "two_step_disabled",
            Self::BackupCreated => "backup_created",
        }
    }

    /// Events the user is actively alerted about rather than merely being able
    /// to find in their security log.
    pub fn should_alert_user(&self) -> bool {
        matches!(
            self,
            Self::DeviceLinked
                | Self::KeyChanged
                | Self::SessionReuseDetected
                | Self::SuspiciousLogin
                | Self::TwoStepDisabled
        )
    }

    pub fn severity(&self) -> &'static str {
        match self {
            Self::SessionReuseDetected | Self::SuspiciousLogin => "critical",
            Self::DeviceLinked | Self::KeyChanged | Self::TwoStepDisabled | Self::PinFailed => {
                "warning"
            }
            _ => "info",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEvent {
    pub user_id: UserId,
    pub device_id: Option<DeviceId>,
    pub event_type: SecurityEventType,
    pub ip_hash: Option<String>,
    pub user_agent: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl SecurityEvent {
    pub fn new(user_id: UserId, event_type: SecurityEventType) -> Self {
        Self {
            user_id,
            device_id: None,
            event_type,
            ip_hash: None,
            user_agent: None,
            metadata: serde_json::Value::Object(Default::default()),
            created_at: Utc::now(),
        }
    }

    pub fn with_device(mut self, device_id: DeviceId) -> Self {
        self.device_id = Some(device_id);
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}
