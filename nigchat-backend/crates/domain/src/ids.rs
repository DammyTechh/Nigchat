//! Typed identifiers.
//!
//! Every id is a UUID, but they are *different* UUIDs. A `UserId` where a
//! `ConversationId` belongs is a compile error rather than a 3am production
//! incident, which is worth the small amount of boilerplate below.
//!
//! v7 is used for generation: time-ordered, so inserts append to the right of
//! the B-tree instead of fragmenting it the way v4 does.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            /// Time-ordered id for a new record.
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            /// Takes `self`, not `&self`: these ids are `Copy`, and an
            /// owned receiver lets `Option::map(Id::as_uuid)` and
            /// `iter().copied().map(Id::as_uuid)` be written directly
            /// instead of wrapping each one in a closure.
            pub fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl From<Uuid> for $name {
            fn from(id: Uuid) -> Self {
                Self(id)
            }
        }

        impl From<$name> for Uuid {
            fn from(id: $name) -> Uuid {
                id.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

typed_id!(UserId);
typed_id!(DeviceId);
typed_id!(SessionId);
typed_id!(ConversationId);
typed_id!(MessageId);
typed_id!(MediaId);
typed_id!(CommunityId);
typed_id!(StatusId);
typed_id!(CallId);
typed_id!(ReportId);
typed_id!(ChallengeId);
typed_id!(NotificationTokenId);

/// Client-generated id used for send idempotency. Distinct from `MessageId`
/// because the client mints it before the server has assigned anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClientMessageId(pub Uuid);

impl From<Uuid> for ClientMessageId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl fmt::Display for ClientMessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
