//! Wire types.
//!
//! Kept separate from domain entities on purpose. The API contract and the
//! internal model change for different reasons and at different speeds; if a
//! handler serialised entities directly, renaming an internal field would
//! break eight shipped clients.
//!
//! Binary values (ciphertext, keys) are base64 in JSON.

use base64::Engine;
use chrono::{DateTime, Utc};
use nigchat_domain::entities as domain;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

pub fn encode_b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn decode_b64(value: &str) -> Result<Vec<u8>, nigchat_domain::DomainError> {
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| nigchat_domain::DomainError::validation("value must be base64"))
}

// --- auth -----------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct RequestOtpRequest {
    /// E.164, including the country code.
    #[schema(example = "+2348012345678")]
    pub phone_e164: String,
}

#[derive(Serialize, ToSchema)]
pub struct RequestOtpResponse {
    pub challenge_sent: bool,
    #[schema(example = 300)]
    pub expires_in: i64,
    /// Development only. Never populated when `ENVIRONMENT` is not
    /// `development` — the server refuses to boot in that configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_code: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct VerifyOtpRequest {
    #[schema(example = "+2348012345678")]
    pub phone_e164: String,
    #[schema(example = "123456")]
    pub code: String,
    /// Required on first registration, ignored afterwards.
    pub display_name: Option<String>,
    /// One of: ios, ipados, android, android_tablet, web, windows, macos, linux
    #[schema(example = "android")]
    pub platform: String,
    pub device_name: Option<String>,
    pub app_version: Option<String>,
    /// Send the id of an existing install so re-authentication reuses the
    /// device row instead of creating a duplicate.
    pub device_id: Option<Uuid>,
}

#[derive(Deserialize, ToSchema)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Serialize, ToSchema)]
pub struct TokenPairResponse {
    pub access_token: String,
    /// Rotated on every refresh. The previous value is dead immediately;
    /// presenting it again revokes every session on the device.
    pub refresh_token: String,
    #[schema(example = 900)]
    pub expires_in: i64,
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub is_new_account: bool,
}

// --- users ----------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct MeResponse {
    pub id: Uuid,
    pub phone_e164: String,
    pub username: Option<String>,
    pub display_name: String,
    pub about: Option<String>,
    pub avatar_media_id: Option<Uuid>,
    pub two_step_enabled: bool,
    pub created_at: DateTime<Utc>,
}

impl From<domain::User> for MeResponse {
    fn from(user: domain::User) -> Self {
        Self {
            id: user.id.as_uuid(),
            phone_e164: user.phone.as_str().to_string(),
            username: user.username.map(|u| u.as_str().to_string()),
            display_name: user.display_name,
            about: user.about,
            avatar_media_id: user.avatar_media_id.map(|id| id.as_uuid()),
            two_step_enabled: user.two_step_enabled,
            created_at: user.created_at,
        }
    }
}

/// Another user's profile. Deliberately excludes the phone number.
#[derive(Serialize, ToSchema)]
pub struct PublicUserResponse {
    pub id: Uuid,
    pub username: Option<String>,
    pub display_name: String,
    pub about: Option<String>,
    pub avatar_media_id: Option<Uuid>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

impl From<domain::User> for PublicUserResponse {
    fn from(user: domain::User) -> Self {
        Self {
            id: user.id.as_uuid(),
            username: user.username.map(|u| u.as_str().to_string()),
            display_name: user.display_name,
            about: user.about,
            avatar_media_id: user.avatar_media_id.map(|id| id.as_uuid()),
            last_seen_at: user.last_seen_at,
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateProfileRequest {
    pub display_name: Option<String>,
    pub about: Option<String>,
    pub username: Option<String>,
    pub avatar_media_id: Option<Uuid>,
}

#[derive(Deserialize, ToSchema)]
pub struct ContactSyncRequest {
    /// Peppered hashes, not raw numbers: the server must not learn the phone
    /// numbers of people who are not users.
    #[schema(max_items = 2000)]
    pub phone_hashes: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct SetTwoStepPinRequest {
    /// 6–12 digits. Repeated digits and simple runs are rejected.
    #[schema(example = "194837")]
    pub pin: String,
    /// Required when changing an existing PIN.
    pub current_pin: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct VerifyPinRequest {
    pub pin: String,
}

#[derive(Deserialize, ToSchema)]
pub struct BlockRequest {
    pub user_id: Uuid,
}

// --- devices --------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct DeviceResponse {
    pub id: Uuid,
    pub platform: String,
    pub device_name: Option<String>,
    pub app_version: Option<String>,
    pub is_primary: bool,
    pub linked_at: DateTime<Utc>,
    pub last_active_at: Option<DateTime<Utc>>,
}

impl From<domain::Device> for DeviceResponse {
    fn from(device: domain::Device) -> Self {
        Self {
            id: device.id.as_uuid(),
            platform: device.platform.as_str().to_string(),
            device_name: device.device_name,
            app_version: device.app_version,
            is_primary: device.is_primary,
            linked_at: device.linked_at,
            last_active_at: device.last_active_at,
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct RegisterPushTokenRequest {
    /// One of: fcm, apns, web_push
    #[schema(example = "fcm")]
    pub provider: String,
    pub token: String,
    /// iOS PushKit token, used for incoming calls only.
    #[serde(default)]
    pub is_voip: bool,
    /// APNs sandbox gateway. Must be false for App Store builds.
    #[serde(default)]
    pub sandbox: bool,
}

// --- conversations --------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct ConversationResponse {
    pub id: Uuid,
    pub kind: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub avatar_media_id: Option<Uuid>,
    pub only_admins_can_post: bool,
    pub disappearing_seconds: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<domain::Conversation> for ConversationResponse {
    fn from(conversation: domain::Conversation) -> Self {
        Self {
            id: conversation.id.as_uuid(),
            kind: conversation.kind.as_str().to_string(),
            title: conversation.title,
            description: conversation.description,
            avatar_media_id: conversation.avatar_media_id.map(|id| id.as_uuid()),
            only_admins_can_post: conversation.only_admins_can_post,
            disappearing_seconds: conversation.disappearing_seconds,
            created_at: conversation.created_at,
            updated_at: conversation.updated_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct ConversationSummaryResponse {
    pub id: Uuid,
    pub kind: String,
    pub title: Option<String>,
    pub avatar_media_id: Option<Uuid>,
    /// Highest sequence number in the conversation. Compare with
    /// `last_read_seq` to know what to fetch.
    pub head_seq: i64,
    pub last_read_seq: i64,
    pub unread_count: i64,
    pub last_message_at: Option<DateTime<Utc>>,
    pub last_message_kind: Option<String>,
    pub is_pinned: bool,
    pub is_archived: bool,
    pub is_locked: bool,
    pub muted_until: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

impl From<domain::ConversationSummary> for ConversationSummaryResponse {
    fn from(summary: domain::ConversationSummary) -> Self {
        Self {
            id: summary.id.as_uuid(),
            kind: summary.kind.as_str().to_string(),
            title: summary.title,
            avatar_media_id: summary.avatar_media_id.map(|id| id.as_uuid()),
            head_seq: summary.head_seq.value(),
            last_read_seq: summary.last_read_seq.value(),
            unread_count: summary.unread_count,
            last_message_at: summary.last_message_at,
            last_message_kind: summary.last_message_kind,
            is_pinned: summary.is_pinned,
            is_archived: summary.is_archived,
            is_locked: summary.is_locked,
            muted_until: summary.mute.muted_until,
            updated_at: summary.updated_at,
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CreateDirectRequest {
    pub peer_user_id: Uuid,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateGroupRequest {
    #[schema(example = "Lagos Devs")]
    pub title: String,
    pub description: Option<String>,
    #[serde(default)]
    pub member_ids: Vec<Uuid>,
}

#[derive(Deserialize, ToSchema)]
pub struct AddMembersRequest {
    pub member_ids: Vec<Uuid>,
}

#[derive(Deserialize, ToSchema)]
pub struct SetRoleRequest {
    /// One of: member, admin, owner
    pub role: String,
}

#[derive(Deserialize, ToSchema)]
pub struct MuteRequest {
    /// One of: eight_hours, one_week, always. Omit to unmute.
    pub duration: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct MarkDeliveredRequest {
    /// Highest sequence number this device has received.
    pub last_delivered_seq: i64,
}

#[derive(Deserialize, ToSchema)]
pub struct MarkReadRequest {
    /// Highest sequence number the user has seen. Only ever moves forward.
    pub last_read_seq: i64,
}

// --- messages -------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct MessageResponse {
    pub id: Uuid,
    pub conversation_id: Uuid,
    /// Ordering key. Use this for pagination and sync, never `created_at`.
    pub seq: i64,
    pub sender_id: Option<Uuid>,
    pub client_message_id: Uuid,
    pub kind: String,
    /// Base64 ciphertext. The server cannot read it; decrypt on the device.
    pub ciphertext: Option<String>,
    pub envelope_version: i16,
    /// Set only for server-authored system messages.
    pub system_text: Option<String>,
    pub metadata: serde_json::Value,
    pub reply_to_id: Option<Uuid>,
    pub expires_at: Option<DateTime<Utc>>,
    pub edited_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<domain::Message> for MessageResponse {
    fn from(message: domain::Message) -> Self {
        Self {
            id: message.id.as_uuid(),
            conversation_id: message.conversation_id.as_uuid(),
            seq: message.seq.value(),
            sender_id: message.sender_id.map(|id| id.as_uuid()),
            client_message_id: message.client_message_id.0,
            kind: message.kind.as_str().to_string(),
            ciphertext: message.ciphertext.as_deref().map(encode_b64),
            envelope_version: message.envelope_version,
            system_text: message.system_text,
            metadata: message.metadata,
            reply_to_id: message.reply_to_id.map(|id| id.as_uuid()),
            expires_at: message.expires_at,
            edited_at: message.edited_at,
            deleted_at: message.deleted_at,
            created_at: message.created_at,
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct SendMessageRequest {
    pub conversation_id: Uuid,
    /// Generate on the device **before** sending. Retrying with the same value
    /// returns the original message instead of creating a duplicate — this is
    /// what makes a send safe to retry on a dropped connection.
    pub client_message_id: Uuid,
    /// text, image, video, audio, voice_note, document, sticker, gif,
    /// location, contact or poll.
    #[serde(default = "default_kind")]
    pub kind: String,
    /// Base64 ciphertext, encrypted on the device.
    pub ciphertext: String,
    #[serde(default = "default_envelope_version")]
    pub envelope_version: i16,
    /// Routing and UI metadata only. Never plaintext content.
    #[serde(default)]
    pub metadata: serde_json::Value,
    pub reply_to_id: Option<Uuid>,
    #[serde(default)]
    pub mentions: Vec<Uuid>,
    #[serde(default)]
    pub media_ids: Vec<Uuid>,
}

fn default_kind() -> String {
    "text".to_string()
}

fn default_envelope_version() -> i16 {
    1
}

/// Distinguishes "field absent" (`None`) from "field explicitly null"
/// (`Some(None)`). Without this, a client could never turn a setting off — a
/// missing field and an explicit null would look identical.
fn double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    serde::Deserialize::deserialize(deserializer).map(Some)
}

#[derive(Deserialize, ToSchema)]
pub struct EditMessageRequest {
    pub ciphertext: String,
}

#[derive(Deserialize, ToSchema)]
pub struct ReactionRequest {
    #[schema(example = "👍")]
    pub emoji: String,
    #[serde(default)]
    pub removed: bool,
}

#[derive(Deserialize, IntoParams)]
pub struct ListMessagesQuery {
    /// Scroll into history: messages with a lower `seq`.
    pub before_seq: Option<i64>,
    /// Catch up after being offline: messages with a higher `seq`.
    pub after_seq: Option<i64>,
    /// 1–200, default 50.
    pub limit: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[aliases(MessagePage = Page<MessageResponse>)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub has_more: bool,
    /// Pass back as `before_seq` (or `after_seq`) to continue.
    pub next_cursor: Option<i64>,
}

// --- notifications --------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct NotificationToneResponse {
    #[schema(example = "tone.message.pulse")]
    pub id: String,
    pub display_name: String,
    /// message, group, call, status or system
    pub category: String,
    /// File the client bundles for this tone.
    pub asset_name: String,
    pub is_default: bool,
}

#[derive(Serialize, ToSchema)]
pub struct NotificationPreferencesResponse {
    pub messages_enabled: bool,
    pub groups_enabled: bool,
    pub calls_enabled: bool,
    pub status_enabled: bool,
    pub channels_enabled: bool,
    pub reactions_enabled: bool,
    /// Security alerts cannot be disabled.
    pub security_alerts_enabled: bool,
    /// full, name_only or hidden
    pub preview_mode: String,
    pub message_tone_id: Option<String>,
    pub group_tone_id: Option<String>,
    pub call_ringtone_id: Option<String>,
    pub vibration: String,
    pub in_app_sounds: bool,
    pub quiet_hours: Option<QuietHoursDto>,
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct QuietHoursDto {
    /// Minutes past local midnight, 0–1439. 1320 is 22:00.
    #[schema(example = 1320)]
    pub start_minute: u16,
    /// 420 is 07:00. A window may cross midnight.
    #[schema(example = 420)]
    pub end_minute: u16,
    /// IANA name, so the window survives travel and DST.
    #[schema(example = "Africa/Lagos")]
    pub timezone: String,
    /// Let calls ring through. Messages never do.
    pub allow_calls: bool,
}

#[derive(Deserialize, ToSchema, Default)]
pub struct UpdateNotificationPreferencesRequest {
    pub messages_enabled: Option<bool>,
    pub groups_enabled: Option<bool>,
    pub calls_enabled: Option<bool>,
    pub status_enabled: Option<bool>,
    pub channels_enabled: Option<bool>,
    pub reactions_enabled: Option<bool>,
    pub preview_mode: Option<String>,
    pub message_tone_id: Option<String>,
    pub group_tone_id: Option<String>,
    pub call_ringtone_id: Option<String>,
    pub vibration: Option<String>,
    pub in_app_sounds: Option<bool>,
    /// Send `null` to switch quiet hours off; omit the field to leave it
    /// unchanged. The nested option is what distinguishes those two cases.
    #[serde(default, deserialize_with = "double_option")]
    pub quiet_hours: Option<Option<QuietHoursDto>>,
}

#[derive(Serialize, ToSchema)]
pub struct ConversationNotificationResponse {
    pub muted_until: Option<DateTime<Utc>>,
    /// When muted, an @mention still notifies unless this is false.
    pub notify_on_mention: bool,
    /// Overrides the account tone for this conversation.
    pub tone_id: Option<String>,
    pub call_ringtone_id: Option<String>,
    pub vibration: Option<String>,
    pub preview_mode: Option<String>,
}

#[derive(Deserialize, ToSchema, Default)]
pub struct UpdateConversationNotificationsRequest {
    pub notify_on_mention: Option<bool>,
    pub tone_id: Option<String>,
    pub call_ringtone_id: Option<String>,
    pub vibration: Option<String>,
    pub preview_mode: Option<String>,
}

// --- E2EE keys ------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct PublishKeysRequest {
    pub registration_id: i32,
    /// Base64 public identity key.
    pub identity_public_key: String,
    pub signed_prekey_id: i32,
    pub signed_prekey_public: String,
    pub signed_prekey_signature: String,
    /// Upload in batches of up to 200; each is consumed by one session.
    #[serde(default)]
    pub one_time_prekeys: Vec<OneTimePreKeyDto>,
}

#[derive(Deserialize, ToSchema)]
pub struct OneTimePreKeyDto {
    pub key_id: i32,
    pub public_key: String,
}

#[derive(Serialize, ToSchema)]
pub struct PreKeyBundleResponse {
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub registration_id: i32,
    pub identity_public_key: String,
    pub signed_prekey_id: i32,
    pub signed_prekey_public: String,
    pub signed_prekey_signature: String,
    /// Absent when the device has exhausted its one-time keys — the session is
    /// still establishable, with weaker forward secrecy.
    pub one_time_prekey_id: Option<i32>,
    pub one_time_prekey_public: Option<String>,
}

impl From<domain::PreKeyBundle> for PreKeyBundleResponse {
    fn from(bundle: domain::PreKeyBundle) -> Self {
        Self {
            user_id: bundle.user_id.as_uuid(),
            device_id: bundle.device_id.as_uuid(),
            registration_id: bundle.registration_id,
            identity_public_key: encode_b64(&bundle.identity_public_key),
            signed_prekey_id: bundle.signed_prekey_id,
            signed_prekey_public: encode_b64(&bundle.signed_prekey_public),
            signed_prekey_signature: encode_b64(&bundle.signed_prekey_signature),
            one_time_prekey_id: bundle.one_time_prekey_id,
            one_time_prekey_public: bundle.one_time_prekey_public.as_deref().map(encode_b64),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct PreKeyCountResponse {
    pub remaining: i64,
    /// True when the device should upload more.
    pub needs_top_up: bool,
}

// --- security -------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct SecurityEventResponse {
    pub event_type: String,
    pub severity: String,
    pub device_id: Option<Uuid>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl From<domain::SecurityEvent> for SecurityEventResponse {
    fn from(event: domain::SecurityEvent) -> Self {
        Self {
            event_type: event.event_type.as_str().to_string(),
            severity: event.event_type.severity().to_string(),
            device_id: event.device_id.map(|id| id.as_uuid()),
            metadata: event.metadata,
            created_at: event.created_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct OkResponse {
    pub ok: bool,
}

#[derive(Serialize, ToSchema)]
pub struct SeqResponse {
    pub seq: i64,
}
