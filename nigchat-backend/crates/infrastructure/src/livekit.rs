//! LiveKit access tokens.
//!
//! LiveKit is the SFU — the server that receives each participant's audio and
//! video and forwards it to the others. It is why calls do not need this
//! service to carry media, and why they scale past two people.
//!
//! Authentication is a JWT signed with the API secret. There is no network call
//! to mint one, so issuing a token is free and instant.
//!
//! Claims that matter:
//!   `iss`         the API key
//!   `sub`         the participant's identity, which is the user id
//!   `video.room`  the single room this token is valid for
//!   `exp`         short — a token is for joining, not for staying

use jsonwebtoken::{encode, EncodingKey, Header};
use nigchat_domain::ports::MediaServerTokens;
use nigchat_domain::{DomainError, DomainResult};
use serde::Serialize;

/// Long enough to survive a slow connect, short enough that a leaked token is
/// not a standing invitation into someone's call.
const TOKEN_TTL_SECONDS: i64 = 600;

#[derive(Serialize)]
struct VideoGrant {
    room: String,
    #[serde(rename = "roomJoin")]
    room_join: bool,
    #[serde(rename = "canPublish")]
    can_publish: bool,
    #[serde(rename = "canSubscribe")]
    can_subscribe: bool,
    #[serde(rename = "canPublishData")]
    can_publish_data: bool,
}

#[derive(Serialize)]
struct Claims {
    iss: String,
    sub: String,
    /// LiveKit shows this to the other participants.
    name: String,
    nbf: i64,
    exp: i64,
    video: VideoGrant,
}

pub struct LiveKitTokens {
    api_key: String,
    api_secret: String,
    server_url: String,
}

impl LiveKitTokens {
    pub fn new(api_key: String, api_secret: String, server_url: String) -> Self {
        Self {
            api_key,
            api_secret,
            server_url,
        }
    }
}

impl MediaServerTokens for LiveKitTokens {
    fn issue(&self, room: &str, identity: &str, display_name: &str) -> DomainResult<String> {
        let now = chrono::Utc::now().timestamp();

        let claims = Claims {
            iss: self.api_key.clone(),
            sub: identity.to_string(),
            name: display_name.to_string(),
            // A few seconds of slack for clock skew between us and LiveKit.
            nbf: now - 10,
            exp: now + TOKEN_TTL_SECONDS,
            video: VideoGrant {
                // Scoped to one room. A token for this call cannot open another.
                room: room.to_string(),
                room_join: true,
                can_publish: true,
                can_subscribe: true,
                // Used for in-call signals: mute state, hand raise, reactions.
                can_publish_data: true,
            },
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.api_secret.as_bytes()),
        )
        .map_err(|err| {
            tracing::error!(?err, "failed to mint a LiveKit token");
            DomainError::infrastructure("could not start the call")
        })
    }

    fn server_url(&self) -> &str {
        &self.server_url
    }
}
