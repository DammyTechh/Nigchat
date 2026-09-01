//! Ports.
//!
//! The application layer depends on these traits, never on PostgreSQL, Redis
//! or a push SDK. Infrastructure implements them; `server` wires the concrete
//! types together at startup.
//!
//! Two payoffs that justify the indirection:
//!   * use cases are testable with in-memory fakes, no containers
//!   * swapping Redis Pub/Sub for Redpanda, or FCM for another provider,
//!     changes one adapter and zero business rules

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::entities::*;
use crate::error::DomainResult;
use crate::events::EventEnvelope;
use crate::ids::*;
use crate::notifications::NotificationPlan;
use crate::values::{Cursor, PhoneNumber, Seq, Username};

// ===========================================================================
// Repositories
// ===========================================================================

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_id(&self, id: UserId) -> DomainResult<Option<User>>;
    async fn find_by_phone(&self, phone: &PhoneNumber) -> DomainResult<Option<User>>;
    async fn find_by_username(&self, username: &Username) -> DomainResult<Option<User>>;

    /// Creates the account if the phone is new, otherwise returns the existing
    /// one. Registration must be idempotent: a retried OTP verification cannot
    /// create a second account.
    async fn upsert_by_phone(
        &self,
        phone: &PhoneNumber,
        phone_hash: &str,
        display_name: &str,
    ) -> DomainResult<User>;

    async fn update_profile(
        &self,
        id: UserId,
        display_name: Option<&str>,
        about: Option<&str>,
        username: Option<&Username>,
        avatar_media_id: Option<MediaId>,
    ) -> DomainResult<User>;

    async fn touch_last_seen(&self, id: UserId) -> DomainResult<()>;

    /// Contact discovery. Takes hashed numbers so the caller never sends, and
    /// the server never logs, raw numbers of people who are not users.
    async fn find_by_phone_hashes(&self, hashes: &[String]) -> DomainResult<Vec<User>>;

    async fn privacy_settings(&self, id: UserId) -> DomainResult<PrivacySettings>;

    /// Partial update — `None` leaves a field alone, so two devices changing
    /// different settings do not clobber each other.
    async fn update_privacy_settings(
        &self,
        id: UserId,
        update: PrivacyUpdate,
    ) -> DomainResult<PrivacySettings>;

    async fn set_two_step_pin(&self, id: UserId, pin_hash: Option<&str>) -> DomainResult<()>;
    async fn two_step_pin_hash(&self, id: UserId) -> DomainResult<Option<String>>;

    async fn is_blocked(&self, blocker: UserId, blocked: UserId) -> DomainResult<bool>;
    async fn blocked_by_any(&self, user: UserId, candidates: &[UserId]) -> DomainResult<Vec<UserId>>;
    async fn block(&self, blocker: UserId, blocked: UserId) -> DomainResult<()>;
    async fn unblock(&self, blocker: UserId, blocked: UserId) -> DomainResult<()>;
}

/// Every field optional: absent means "unchanged".
#[derive(Debug, Clone, Default)]
pub struct PrivacyUpdate {
    pub last_seen: Option<Visibility>,
    pub profile_photo: Option<Visibility>,
    pub about: Option<Visibility>,
    pub status: Option<Visibility>,
    pub read_receipts_enabled: Option<bool>,
    pub typing_indicators_enabled: Option<bool>,
    pub who_can_add_to_groups: Option<Visibility>,
    pub who_can_call: Option<Visibility>,
    pub silence_unknown_callers: Option<bool>,
}

#[async_trait]
pub trait DeviceRepository: Send + Sync {
    async fn find_by_id(&self, id: DeviceId) -> DomainResult<Option<Device>>;
    async fn list_active(&self, user_id: UserId) -> DomainResult<Vec<Device>>;

    async fn register(
        &self,
        user_id: UserId,
        platform: Platform,
        device_name: Option<&str>,
        app_version: Option<&str>,
        is_primary: bool,
    ) -> DomainResult<Device>;

    async fn touch_active(&self, id: DeviceId, ip_hash: Option<&str>) -> DomainResult<()>;

    /// Revokes the device and every session bound to it, in one transaction.
    /// Doing these separately would leave a window where a revoked device can
    /// still refresh.
    async fn revoke(&self, id: DeviceId, reason: &str) -> DomainResult<()>;
}

/// Refresh-token sessions. Access tokens are stateless and never stored.
#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn create(
        &self,
        user_id: UserId,
        device_id: DeviceId,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> DomainResult<SessionId>;

    async fn find_by_token_hash(&self, token_hash: &str) -> DomainResult<Option<StoredSession>>;

    /// Atomically revokes `old` and links it to `new`, so the rotation chain
    /// stays auditable and reuse of a spent token is detectable.
    async fn rotate(&self, old: SessionId, new: SessionId) -> DomainResult<()>;

    async fn revoke_session(&self, id: SessionId) -> DomainResult<()>;

    /// Called when a spent refresh token is presented again. That means the
    /// value leaked, so every session on the device dies.
    async fn revoke_all_for_device(&self, device_id: DeviceId) -> DomainResult<u64>;
}

#[derive(Debug, Clone)]
pub struct StoredSession {
    pub id: SessionId,
    pub user_id: UserId,
    pub device_id: DeviceId,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl StoredSession {
    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }

    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at <= now
    }
}

#[async_trait]
pub trait AuthChallengeRepository: Send + Sync {
    async fn create(
        &self,
        phone: &PhoneNumber,
        code_hash: &str,
        expires_at: DateTime<Utc>,
        ip_hash: Option<&str>,
    ) -> DomainResult<ChallengeId>;

    async fn latest_active(&self, phone: &PhoneNumber) -> DomainResult<Option<StoredChallenge>>;

    async fn increment_attempts(&self, id: ChallengeId) -> DomainResult<i32>;

    /// Returns false when the challenge was already consumed — that is how
    /// two concurrent verifications of the same code are resolved.
    async fn consume(&self, id: ChallengeId) -> DomainResult<bool>;
}

#[derive(Debug, Clone)]
pub struct StoredChallenge {
    pub id: ChallengeId,
    pub code_hash: String,
    pub attempts: i32,
    pub expires_at: DateTime<Utc>,
}

/// E2EE key directory (spec §28). Public material only.
#[async_trait]
pub trait KeyRepository: Send + Sync {
    async fn publish_identity_key(
        &self,
        device_id: DeviceId,
        user_id: UserId,
        identity_public_key: &[u8],
        registration_id: i32,
    ) -> DomainResult<i32>;

    async fn publish_signed_prekey(
        &self,
        device_id: DeviceId,
        key_id: i32,
        public_key: &[u8],
        signature: &[u8],
    ) -> DomainResult<()>;

    async fn upload_one_time_prekeys(
        &self,
        device_id: DeviceId,
        keys: &[(i32, Vec<u8>)],
    ) -> DomainResult<u64>;

    /// Consumes one one-time prekey per device. A device with none left still
    /// returns a bundle, with `one_time_prekey_id: None`.
    async fn take_prekey_bundles(&self, user_id: UserId) -> DomainResult<Vec<PreKeyBundle>>;

    /// Drives the top-up push: a device below the threshold must upload more
    /// before it runs out entirely.
    async fn one_time_prekey_count(&self, device_id: DeviceId) -> DomainResult<i64>;

    async fn identity_keys_for(&self, user_id: UserId) -> DomainResult<Vec<DeviceIdentityKey>>;
}

#[async_trait]
pub trait ConversationRepository: Send + Sync {
    async fn find_by_id(&self, id: ConversationId) -> DomainResult<Option<Conversation>>;

    /// Idempotent. Two users tapping "message" simultaneously on different
    /// instances must end up in one conversation, not two.
    async fn get_or_create_direct(
        &self,
        a: UserId,
        b: UserId,
    ) -> DomainResult<Conversation>;

    async fn create_group(
        &self,
        creator: UserId,
        title: &str,
        description: Option<&str>,
        members: &[UserId],
    ) -> DomainResult<Conversation>;

    async fn list_for_user(&self, user_id: UserId) -> DomainResult<Vec<ConversationSummary>>;

    async fn membership(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
    ) -> DomainResult<Option<ConversationMember>>;

    async fn active_member_ids(&self, conversation_id: ConversationId)
        -> DomainResult<Vec<UserId>>;

    async fn add_members(
        &self,
        conversation_id: ConversationId,
        actor: UserId,
        members: &[UserId],
    ) -> DomainResult<Vec<UserId>>;

    async fn remove_member(
        &self,
        conversation_id: ConversationId,
        actor: UserId,
        target: UserId,
    ) -> DomainResult<()>;

    async fn set_role(
        &self,
        conversation_id: ConversationId,
        target: UserId,
        role: MemberRole,
    ) -> DomainResult<()>;

    /// Monotonic: the mark only moves forward, so a late request from a laggy
    /// device cannot resurrect old unread counts.
    async fn advance_read_marker(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
        seq: Seq,
    ) -> DomainResult<Seq>;

    async fn advance_delivery_marker(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
        seq: Seq,
    ) -> DomainResult<Seq>;

    async fn head_seq(&self, conversation_id: ConversationId) -> DomainResult<Seq>;
}

#[async_trait]
pub trait MessageRepository: Send + Sync {
    /// Allocates the sequence number, writes the message, its mentions and
    /// attachments, and the outbox row — all in one transaction.
    ///
    /// Returns `(message, was_created)`. `false` means this was an idempotent
    /// replay of a `client_message_id` we already had, and the caller must not
    /// fan out a second time.
    async fn append(&self, message: NewMessage) -> DomainResult<(Message, bool)>;

    async fn find_by_id(&self, id: MessageId) -> DomainResult<Option<Message>>;

    async fn find_by_client_id(
        &self,
        conversation_id: ConversationId,
        sender_id: UserId,
        client_message_id: ClientMessageId,
    ) -> DomainResult<Option<Message>>;

    /// Keyset pagination over `seq`. Never OFFSET.
    async fn page(
        &self,
        conversation_id: ConversationId,
        cursor: Cursor,
    ) -> DomainResult<(Vec<Message>, bool)>;

    async fn edit(
        &self,
        id: MessageId,
        editor: UserId,
        ciphertext: &[u8],
    ) -> DomainResult<Message>;

    /// Soft delete. The row and its `seq` survive so other devices learn the
    /// message is gone rather than finding a hole in the sequence.
    async fn soft_delete(
        &self,
        id: MessageId,
        actor: UserId,
        for_everyone: bool,
    ) -> DomainResult<Seq>;

    async fn set_reaction(
        &self,
        message_id: MessageId,
        user_id: UserId,
        emoji: &str,
        removed: bool,
    ) -> DomainResult<()>;

    async fn mentioned_users(&self, message_id: MessageId) -> DomainResult<Vec<UserId>>;

    /// Attachments for a batch of messages, in one query. Fetching per message
    /// would turn a 50-message page into 51 round trips.
    async fn attachments_for(
        &self,
        message_ids: &[MessageId],
    ) -> DomainResult<Vec<(MessageId, MessageAttachment)>>;
}

#[async_trait]
pub trait NotificationRepository: Send + Sync {
    async fn register_token(
        &self,
        user_id: UserId,
        device_id: DeviceId,
        provider: PushProvider,
        token: &str,
        is_voip: bool,
        sandbox: bool,
    ) -> DomainResult<NotificationTokenId>;

    async fn active_tokens(&self, user_id: UserId) -> DomainResult<Vec<NotificationToken>>;

    /// Providers report dead tokens. Marking rather than deleting keeps the
    /// failure rate measurable (spec §16: invalid-token cleanup).
    async fn invalidate_token(&self, token: &str) -> DomainResult<()>;
    async fn record_token_failure(&self, token: &str) -> DomainResult<()>;

    async fn preferences(&self, user_id: UserId) -> DomainResult<NotificationPreferences>;
    async fn save_preferences(&self, prefs: &NotificationPreferences) -> DomainResult<()>;

    async fn conversation_settings(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
    ) -> DomainResult<ConversationNotificationSettings>;

    async fn save_conversation_settings(
        &self,
        settings: &ConversationNotificationSettings,
    ) -> DomainResult<()>;

    async fn list_tones(&self) -> DomainResult<Vec<NotificationTone>>;
    async fn tone_exists(&self, tone_id: &str) -> DomainResult<bool>;

    /// Idempotency for push: returns false when this exact notification was
    /// already recorded, so a retried dispatch does not double-buzz a phone.
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
    ) -> DomainResult<bool>;
}

/// QR device linking (spec §11).
///
/// The web client cannot sign itself in — there is no password to type. It asks
/// for a short-lived code, shows it as a QR, and waits. The phone scans it and
/// approves, which is what mints the browser's session.
#[async_trait]
pub trait DeviceLinkRepository: Send + Sync {
    /// Stores a hash of the code, never the code itself: a database leak must
    /// not let anyone claim a pending link.
    async fn create(
        &self,
        code_hash: &str,
        platform: &str,
        device_name: Option<&str>,
        expires_at: DateTime<Utc>,
    ) -> DomainResult<()>;

    async fn find_pending(&self, code_hash: &str) -> DomainResult<Option<PendingLink>>;

    /// Marks the request approved and records which user approved it.
    /// Returns false if it was already claimed or has expired — the check and
    /// the write are one statement, so two phones cannot both approve.
    async fn approve(&self, code_hash: &str, user_id: UserId, device_id: DeviceId)
        -> DomainResult<bool>;

    /// Consumed by the waiting browser exactly once, then deleted.
    async fn consume(&self, code_hash: &str) -> DomainResult<Option<ApprovedLink>>;

    /// Housekeeping for abandoned codes.
    async fn purge_expired(&self) -> DomainResult<u64>;
}

#[derive(Debug, Clone)]
pub struct PendingLink {
    pub platform: String,
    pub device_name: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub approved: bool,
}

#[derive(Debug, Clone)]
pub struct ApprovedLink {
    pub user_id: UserId,
    pub device_id: DeviceId,
}

/// Object storage.
///
/// Bytes never pass through the API. The client is handed a short-lived signed
/// URL and uploads straight to storage — otherwise every photo would occupy an
/// API worker for the duration of a mobile upload, and autoscaling stops
/// meaning anything.
#[async_trait]
pub trait ObjectStorage: Send + Sync {
    /// A URL the client can `PUT` to, valid for a few minutes.
    async fn signed_upload(&self, key: &str, content_type: &str) -> DomainResult<SignedUpload>;

    /// Where to read it back from. Public for avatars; signed and expiring for
    /// anything in a conversation.
    async fn download_url(&self, key: &str, public: bool) -> DomainResult<String>;

    async fn delete(&self, key: &str) -> DomainResult<()>;
}

#[derive(Debug, Clone)]
pub struct SignedUpload {
    pub url: String,
    pub method: String,
    /// Headers the client must send with the PUT, verbatim.
    pub headers: Vec<(String, String)>,
    pub expires_in_seconds: i64,
}

#[async_trait]
pub trait MediaRepository: Send + Sync {
    async fn create_pending(&self, media: NewMedia) -> DomainResult<MediaAsset>;
    async fn find(&self, id: MediaId) -> DomainResult<Option<MediaAsset>>;

    /// Flips `pending` to `complete`. Returns false if the row is gone or was
    /// already completed, so a replayed call is a no-op rather than a reset.
    async fn mark_complete(&self, id: MediaId, owner: UserId, byte_size: i64)
        -> DomainResult<bool>;

    /// Uploads that were started and never finished. A sweeper deletes them;
    /// without this, every abandoned upload is storage you pay for forever.
    async fn stale_pending(&self, older_than_minutes: i64) -> DomainResult<Vec<MediaAsset>>;
    async fn delete(&self, id: MediaId) -> DomainResult<()>;
}

#[derive(Debug, Clone)]
pub struct NewMedia {
    pub owner_id: UserId,
    pub bucket: String,
    pub key: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_ms: Option<i32>,
    /// Avatars are stored unencrypted in a public bucket; conversation media is
    /// encrypted on the device before upload.
    pub is_encrypted: bool,
}

#[derive(Debug, Clone)]
pub struct MediaAsset {
    pub id: MediaId,
    pub owner_id: Option<UserId>,
    pub bucket: String,
    pub key: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_ms: Option<i32>,
    pub is_encrypted: bool,
    pub upload_status: String,
    pub created_at: DateTime<Utc>,
}

impl MediaAsset {
    pub fn is_complete(&self) -> bool {
        self.upload_status == "complete"
    }
}

/// Calls (spec §29).
///
/// This service stores *who called whom and when*. It never touches audio or
/// video — that goes through an SFU, which is a separate piece of
/// infrastructure the same way Termii is for SMS.
#[async_trait]
pub trait CallRepository: Send + Sync {
    async fn start(
        &self,
        conversation_id: Option<ConversationId>,
        initiator: UserId,
        kind: &str,
        is_group: bool,
        room: &str,
        participants: &[UserId],
    ) -> DomainResult<CallSession>;

    async fn find(&self, id: CallId) -> DomainResult<Option<CallSession>>;

    /// Marks a participant joined. Returns false when they were not invited,
    /// which is what stops someone joining a room they merely learned the name
    /// of.
    async fn mark_joined(&self, call_id: CallId, user_id: UserId) -> DomainResult<bool>;

    async fn mark_left(&self, call_id: CallId, user_id: UserId) -> DomainResult<()>;

    /// Ends the call and returns the participants, so the caller can tell every
    /// device to stop ringing.
    async fn end(&self, call_id: CallId, reason: &str) -> DomainResult<Vec<UserId>>;

    async fn history(&self, user_id: UserId, limit: i64) -> DomainResult<Vec<CallSession>>;
}

#[derive(Debug, Clone)]
pub struct CallSession {
    pub id: CallId,
    pub conversation_id: Option<ConversationId>,
    pub initiator_id: Option<UserId>,
    pub kind: String,
    pub is_group: bool,
    /// Room name on the media server.
    pub room: String,
    pub started_at: DateTime<Utc>,
    pub answered_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub end_reason: Option<String>,
    pub participants: Vec<UserId>,
}

impl CallSession {
    pub fn is_active(&self) -> bool {
        self.ended_at.is_none()
    }
}

/// Mints access tokens for the media server.
///
/// The token is what authorises a device to publish and subscribe in one room.
/// It is short-lived and scoped to a single room, so a leaked one is useless
/// elsewhere and expires quickly.
pub trait MediaServerTokens: Send + Sync {
    fn issue(&self, room: &str, identity: &str, display_name: &str) -> DomainResult<String>;
    /// Where the client connects, e.g. wss://your-project.livekit.cloud
    fn server_url(&self) -> &str;
}

#[async_trait]
pub trait SecurityRepository: Send + Sync {
    async fn record_event(&self, event: SecurityEvent) -> DomainResult<()>;
    async fn recent_events(&self, user_id: UserId, limit: i64) -> DomainResult<Vec<SecurityEvent>>;
}

// ===========================================================================
// Service ports
// ===========================================================================

/// Injected rather than calling `Utc::now()` directly, so time-dependent rules
/// (quiet hours, token expiry, mute windows) are testable without sleeping.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// Consumes one unit. `Err(RateLimited)` when the budget is spent.
    async fn check(&self, key: &str, limit: u32, window_seconds: u64) -> DomainResult<()>;

    /// Clears a counter after a legitimate success, so one bad password
    /// attempt does not count against a user for the rest of the window.
    async fn reset(&self, key: &str) -> DomainResult<()>;
}

/// Cross-instance fan-out. Redis Pub/Sub today, Redpanda later — this trait is
/// the seam that makes that swap a one-crate change.
#[async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(&self, envelope: EventEnvelope) -> DomainResult<()>;
}

/// Who is currently connected, across the whole fleet. Backed by Redis so any
/// instance can answer, which is what the notification policy needs to know
/// before deciding to send a push.
#[async_trait]
pub trait PresenceRegistry: Send + Sync {
    async fn mark_online(&self, user_id: UserId, device_id: DeviceId) -> DomainResult<()>;
    async fn mark_offline(&self, user_id: UserId, device_id: DeviceId) -> DomainResult<()>;
    async fn is_online(&self, user_id: UserId) -> DomainResult<bool>;
    async fn online_subset(&self, user_ids: &[UserId]) -> DomainResult<Vec<UserId>>;
}

#[async_trait]
pub trait SmsSender: Send + Sync {
    /// Sends the verification code. Implementations must never log the code
    /// or the full number.
    async fn send_verification_code(&self, phone: &PhoneNumber, code: &str) -> DomainResult<()>;
}

/// One push, already decided on. The adapter's job is transport only — every
/// policy question was answered before it was called.
#[derive(Debug, Clone)]
pub struct PushMessage {
    pub token: String,
    pub provider: PushProvider,
    pub title: String,
    pub body: String,
    pub plan: NotificationPlan,
    /// Deep link target, e.g. `nigchat://conversation/{id}` (spec §16).
    pub deep_link: Option<String>,
    /// Silent payload the client uses to render the real content locally.
    pub data: serde_json::Value,
    /// Badge count for iOS.
    pub badge: Option<i64>,
    pub is_voip: bool,
    pub sandbox: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushOutcome {
    Delivered { provider_message_id: Option<String> },
    /// The token is dead and must be invalidated — the user uninstalled or
    /// the token rotated.
    TokenInvalid,
    /// Transient. Safe to retry.
    Retryable(String),
    Failed(String),
}

#[async_trait]
pub trait PushSender: Send + Sync {
    fn provider(&self) -> PushProvider;
    async fn send(&self, message: PushMessage) -> DomainResult<PushOutcome>;
}

/// Hashing for secrets at rest.
///
/// Two algorithms on purpose: Argon2id for anything a human chooses (PINs),
/// HMAC-SHA256 for high-entropy machine-generated values (refresh tokens,
/// OTPs, phone hashes) where a slow KDF buys nothing and costs latency on the
/// hot path.
pub trait Hasher: Send + Sync {
    /// Argon2id. Slow by design.
    fn hash_secret(&self, plaintext: &str) -> DomainResult<String>;
    fn verify_secret(&self, plaintext: &str, hash: &str) -> DomainResult<bool>;

    /// Keyed HMAC under a server-side pepper. Deterministic, so it can be
    /// looked up by index.
    fn hash_token(&self, plaintext: &str) -> String;

    /// Peppered hash of a phone number for contact discovery.
    fn hash_phone(&self, phone: &PhoneNumber) -> String;

    /// For IP addresses in audit rows: enables anomaly detection without
    /// retaining the address itself.
    fn hash_ip(&self, ip: &str) -> String;
}

/// Access-token issuing and verification.
pub trait TokenService: Send + Sync {
    fn issue_access_token(&self, user_id: UserId, device_id: DeviceId) -> DomainResult<String>;
    fn verify_access_token(&self, token: &str) -> DomainResult<AccessClaims>;
    fn generate_refresh_token(&self) -> String;
    fn access_token_ttl_seconds(&self) -> i64;
}

#[derive(Debug, Clone, Copy)]
pub struct AccessClaims {
    pub user_id: UserId,
    pub device_id: DeviceId,
    pub expires_at: i64,
}
