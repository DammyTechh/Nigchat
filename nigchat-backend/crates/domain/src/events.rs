//! Realtime events (spec §27).
//!
//! One envelope type crosses the process boundary: the publisher resolves the
//! recipients once, and every instance delivers to whichever sockets it owns.
//! Receiving instances never re-query membership — that would turn one fan-out
//! into N database round trips.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::*;
use crate::values::Seq;

/// What a client receives on its socket.
///
/// `tag`/`data` shape keeps decoding simple in TypeScript, Kotlin and Swift.
/// Adding a variant is backward compatible; renaming one is not, so treat these
/// names as part of the public contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ServerEvent {
    /// A new message. The body is ciphertext; the receiving device decrypts.
    MessageCreated {
        conversation_id: ConversationId,
        message_id: MessageId,
        seq: Seq,
        sender_id: Option<UserId>,
        kind: String,
        ciphertext: Option<Vec<u8>>,
        created_at: DateTime<Utc>,
    },
    MessageEdited {
        conversation_id: ConversationId,
        seq: Seq,
        ciphertext: Option<Vec<u8>>,
        edited_at: DateTime<Utc>,
    },
    MessageDeleted {
        conversation_id: ConversationId,
        seq: Seq,
        for_everyone: bool,
    },
    ReactionChanged {
        conversation_id: ConversationId,
        message_id: MessageId,
        user_id: UserId,
        emoji: String,
        removed: bool,
    },
    /// High-water mark moved. Sending the mark rather than per-message
    /// receipts keeps a 500-member group from emitting 500 events per read.
    ReadReceipt {
        conversation_id: ConversationId,
        user_id: UserId,
        last_read_seq: Seq,
    },
    DeliveryReceipt {
        conversation_id: ConversationId,
        user_id: UserId,
        last_delivered_seq: Seq,
    },
    Typing {
        conversation_id: ConversationId,
        user_id: UserId,
        state: TypingState,
    },
    Presence {
        user_id: UserId,
        online: bool,
        last_seen_at: Option<DateTime<Utc>>,
    },
    ConversationCreated {
        conversation_id: ConversationId,
        kind: String,
    },
    ConversationUpdated {
        conversation_id: ConversationId,
    },
    MembershipChanged {
        conversation_id: ConversationId,
        user_id: UserId,
        change: MembershipChange,
    },
    /// Call signalling. Media never touches this service.
    CallSignal {
        call_id: CallId,
        conversation_id: Option<ConversationId>,
        signal: CallSignalKind,
        payload: serde_json::Value,
    },
    /// A device was linked or revoked — the client should refresh its device
    /// list and, if it is the revoked device, sign out.
    DeviceEvent {
        device_id: DeviceId,
        event: DeviceEventKind,
    },
    /// A peer's identity key changed. The client shows a security warning and
    /// re-verifies (spec §28).
    KeyChanged {
        user_id: UserId,
        device_id: DeviceId,
        key_version: i32,
    },
    /// "You are too far behind; stop replaying and re-sync."
    ///
    /// Emitted when a client's cursor is older than what the realtime layer
    /// can reconstruct. Without this, a long-offline device would silently
    /// hold an incomplete view.
    SyncRequired {
        conversation_id: Option<ConversationId>,
        reason: String,
    },
    /// Server heartbeat carrying a monotonically increasing id, so a client
    /// can detect that it missed events and trigger a re-sync.
    Heartbeat { server_event_id: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypingState {
    Typing,
    Recording,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipChange {
    Joined,
    Left,
    Removed,
    RoleChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallSignalKind {
    Offer,
    Answer,
    IceCandidate,
    Ringing,
    Accepted,
    Declined,
    Ended,
    ParticipantJoined,
    ParticipantLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceEventKind {
    Linked,
    Revoked,
}

/// An event plus its addressing.
///
/// Delivery is at-least-once by design (Appendix 3). Every consumer must be
/// idempotent: a duplicate `MessageCreated` for a `seq` the client already has
/// is discarded on the client, not prevented on the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub recipients: Vec<UserId>,
    /// When set, only this device receives it — used for device-scoped events
    /// such as revocation.
    pub target_device: Option<DeviceId>,
    pub event: ServerEvent,
    /// Instance that published, for tracing fan-out in production.
    pub origin: Option<String>,
    pub published_at: DateTime<Utc>,
}

impl EventEnvelope {
    pub fn broadcast(recipients: Vec<UserId>, event: ServerEvent) -> Self {
        Self {
            recipients,
            target_device: None,
            event,
            origin: None,
            published_at: Utc::now(),
        }
    }

    pub fn to_device(user_id: UserId, device_id: DeviceId, event: ServerEvent) -> Self {
        Self {
            recipients: vec![user_id],
            target_device: Some(device_id),
            event,
            origin: None,
            published_at: Utc::now(),
        }
    }

    pub fn excluding(mut self, user_id: UserId) -> Self {
        self.recipients.retain(|id| *id != user_id);
        self
    }
}
