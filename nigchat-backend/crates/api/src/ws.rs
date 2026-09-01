//! WebSocket delivery.
//!
//! A socket lives on exactly one instance. The message that must reach it can
//! be written by any instance, so instances never talk to each other: they
//! publish to the shared bus, and each delivers to whichever sockets it owns.
//!
//! The socket carries only ephemeral signals upward (typing, heartbeat).
//! Anything that mutates durable state goes over REST, so a dropped socket can
//! never lose a message.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use nigchat_domain::events::{EventEnvelope, TypingState};
use nigchat_domain::ids::{ConversationId, DeviceId, UserId};
use serde::Deserialize;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::state::ApiState;

/// Also refreshes the presence TTL, so a crashed instance's entries age out
/// rather than marking a user online forever.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// Per user, per instance. A client that reconnects in a loop without closing
/// would otherwise pin unbounded memory and file descriptors on one server.
const MAX_SOCKETS_PER_USER: usize = 12;

type ConnectionId = Uuid;

/// Registry of sockets owned by this process. Local by design.
#[derive(Default)]
pub struct Hub {
    connections: RwLock<HashMap<UserId, HashMap<ConnectionId, Connection>>>,
}

struct Connection {
    device_id: DeviceId,
    sender: mpsc::UnboundedSender<Arc<str>>,
}

impl Hub {
    pub fn new() -> Self {
        Self::default()
    }

    async fn register(
        &self,
        user_id: UserId,
        device_id: DeviceId,
        connection_id: ConnectionId,
    ) -> Option<mpsc::UnboundedReceiver<Arc<str>>> {
        let (sender, receiver) = mpsc::unbounded_channel();
        let mut guard = self.connections.write().await;
        let sockets = guard.entry(user_id).or_default();

        if sockets.len() >= MAX_SOCKETS_PER_USER {
            tracing::warn!(%user_id, "socket limit reached; rejecting connection");
            return None;
        }

        sockets.insert(connection_id, Connection { device_id, sender });
        Some(receiver)
    }

    async fn unregister(&self, user_id: UserId, connection_id: ConnectionId) {
        let mut guard = self.connections.write().await;
        if let Some(sockets) = guard.get_mut(&user_id) {
            sockets.remove(&connection_id);
            if sockets.is_empty() {
                guard.remove(&user_id);
            }
        }
    }

    pub async fn local_connection_count(&self) -> usize {
        self.connections
            .read()
            .await
            .values()
            .map(|sockets| sockets.len())
            .sum()
    }

    /// Serialise once, hand the same `Arc<str>` to every socket. At fan-out
    /// widths of several hundred this is the difference between one allocation
    /// and hundreds.
    pub async fn deliver(&self, envelope: &EventEnvelope) {
        let payload: Arc<str> = match serde_json::to_string(&envelope.event) {
            Ok(json) => Arc::from(json.as_str()),
            Err(err) => {
                tracing::error!(?err, "failed to serialise server event");
                return;
            }
        };

        let guard = self.connections.read().await;
        for user_id in &envelope.recipients {
            let Some(sockets) = guard.get(user_id) else {
                continue;
            };
            for connection in sockets.values() {
                // Device-scoped events (revocation) reach exactly one socket.
                if let Some(target) = envelope.target_device {
                    if connection.device_id != target {
                        continue;
                    }
                }
                // A closed receiver means the socket died between lookup and
                // send; the reader task cleans it up.
                let _ = connection.sender.send(payload.clone());
            }
        }
    }
}

#[derive(Deserialize)]
pub struct WsQuery {
    /// Browsers cannot set headers on a WebSocket handshake, so the access
    /// token travels in the query string. It is short-lived and the connection
    /// is TLS-only in production.
    token: String,
}

/// Frames a client may send. Nothing here writes to the database.
#[derive(Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
enum ClientFrame {
    Ping,
    Typing {
        conversation_id: ConversationId,
        #[serde(default)]
        state: Option<TypingStateDto>,
    },
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum TypingStateDto {
    Typing,
    Recording,
    Stopped,
}

impl From<TypingStateDto> for TypingState {
    fn from(value: TypingStateDto) -> Self {
        match value {
            TypingStateDto::Typing => TypingState::Typing,
            TypingStateDto::Recording => TypingState::Recording,
            TypingStateDto::Stopped => TypingState::Stopped,
        }
    }
}

pub async fn handler(
    State(state): State<ApiState>,
    Query(query): Query<WsQuery>,
    upgrade: WebSocketUpgrade,
) -> ApiResult<impl IntoResponse> {
    let claims = state
        .auth
        .verify_access_token(&query.token)
        .map_err(ApiError)?;

    let user_id = claims.user_id;
    let device_id = claims.device_id;

    Ok(upgrade.on_upgrade(move |socket| async move {
        run_socket(state, socket, user_id, device_id).await;
    }))
}

async fn run_socket(state: ApiState, socket: WebSocket, user_id: UserId, device_id: DeviceId) {
    let connection_id = Uuid::new_v4();
    let (mut sink, mut stream) = socket.split();

    let Some(mut outbound) = state.hub.register(user_id, device_id, connection_id).await else {
        // Close cleanly rather than dropping silently, so the client knows to
        // back off instead of retrying immediately.
        let _ = sink.send(WsMessage::Close(None)).await;
        return;
    };

    state
        .services
        .presence
        .mark_online(user_id, device_id)
        .await
        .ok();

    tracing::info!(%user_id, %device_id, "websocket connected");

    let presence = state.services.presence.clone();
    let writer = tokio::spawn(async move {
        let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
        loop {
            tokio::select! {
                event = outbound.recv() => {
                    match event {
                        Some(payload) => {
                            if sink.send(WsMessage::Text(payload.to_string())).await.is_err() {
                                break;
                            }
                        }
                        None => break,
                    }
                }
                _ = heartbeat.tick() => {
                    // Ping surfaces dead NAT entries and sleeping mobile radios
                    // as a closed socket instead of a silent black hole.
                    if sink.send(WsMessage::Ping(Vec::new())).await.is_err() {
                        break;
                    }
                    presence.mark_online(user_id, device_id).await.ok();
                }
            }
        }
    });

    while let Some(Ok(frame)) = stream.next().await {
        match frame {
            WsMessage::Text(text) => match serde_json::from_str::<ClientFrame>(&text) {
                Ok(ClientFrame::Ping) => {}
                Ok(ClientFrame::Typing {
                    conversation_id,
                    state: typing_state,
                }) => {
                    let typing = typing_state
                        .map(TypingState::from)
                        .unwrap_or(TypingState::Typing);
                    if let Err(err) = state
                        .messaging
                        .typing(conversation_id, user_id, typing)
                        .await
                    {
                        tracing::debug!(?err, "typing indicator rejected");
                    }
                }
                Err(err) => tracing::debug!(?err, "ignoring malformed client frame"),
            },
            WsMessage::Close(_) => break,
            _ => {}
        }
    }

    writer.abort();
    state.hub.unregister(user_id, connection_id).await;
    state
        .services
        .presence
        .mark_offline(user_id, device_id)
        .await
        .ok();
    state.services.users.touch_last_seen(user_id).await.ok();

    tracing::info!(%user_id, %device_id, "websocket disconnected");
}

/// Subscribes this instance to the shared bus for the process lifetime.
///
/// Reconnects on failure: losing this task silently would leave the instance
/// looking healthy while delivering nothing to its sockets.
pub fn spawn_bus_listener(state: ApiState, redis_url: String) {
    tokio::spawn(async move {
        loop {
            if let Err(err) = listen_once(&state, &redis_url).await {
                tracing::error!(?err, "event bus subscription dropped; retrying in 2s");
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

// `get_async_connection` is deprecated in favour of the multiplexed
// connection, but a multiplexed connection cannot enter pub/sub mode — the
// dedicated `get_async_pubsub` helper only exists from redis 0.27. Revisit when
// the crate is upgraded.
#[allow(deprecated)]
async fn listen_once(state: &ApiState, redis_url: &str) -> anyhow::Result<()> {
    // A dedicated connection: Redis pub/sub puts the socket into subscriber
    // mode, where normal commands are rejected, so it cannot be shared with
    // the pooled connection manager.
    let client = redis::Client::open(redis_url)?;
    let connection = client.get_async_connection().await?;
    let mut pubsub = connection.into_pubsub();
    pubsub.subscribe("nigchat.events").await?;

    tracing::info!("subscribed to the realtime event bus");

    let mut stream = pubsub.on_message();
    while let Some(message) = stream.next().await {
        let payload: String = match message.get_payload() {
            Ok(payload) => payload,
            Err(err) => {
                tracing::warn!(?err, "unreadable bus payload");
                continue;
            }
        };

        match serde_json::from_str::<EventEnvelope>(&payload) {
            Ok(envelope) => state.hub.deliver(&envelope).await,
            Err(err) => tracing::warn!(?err, "undecodable bus envelope"),
        }
    }

    anyhow::bail!("pub/sub stream ended")
}
