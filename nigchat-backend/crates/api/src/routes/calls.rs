//! `/v1/calls` — signalling. Audio and video go through the SFU, not here.

use axum::extract::{Path, State};
use axum::Json;
use nigchat_application::calls::CallService;
use nigchat_domain::ids::{CallId, ConversationId};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::extract::CurrentUser;
use crate::state::ApiState;

#[derive(Deserialize, ToSchema)]
pub struct StartCallRequest {
    pub conversation_id: Uuid,
    /// False for a voice call.
    #[serde(default)]
    pub video: bool,
}

#[derive(Serialize, ToSchema)]
pub struct CallTicketResponse {
    pub call_id: String,
    pub room: String,
    /// audio or video
    pub kind: String,
    pub is_group: bool,
    /// LiveKit access token — scoped to this room, valid ten minutes.
    pub token: String,
    /// Where the client connects, e.g. wss://project.livekit.cloud
    pub server_url: String,
}

#[derive(Deserialize, ToSchema)]
pub struct EndCallRequest {
    /// completed, declined, missed, cancelled, busy or left.
    #[schema(example = "completed")]
    pub reason: String,
}

#[derive(Serialize, ToSchema)]
pub struct CallHistoryEntry {
    pub id: String,
    pub conversation_id: Option<String>,
    pub initiator_id: Option<String>,
    pub kind: String,
    pub is_group: bool,
    pub started_at: String,
    pub answered_at: Option<String>,
    pub ended_at: Option<String>,
    pub end_reason: Option<String>,
}

/// Start a call
///
/// Creates the session, rings everyone else in the conversation over the
/// socket and by push, and returns a token for the media server. The client
/// then connects to `server_url` with `token` — audio and video never touch
/// this API.
#[utoipa::path(
    post, path = "/v1/calls", tag = "calls", security(("bearer" = [])),
    request_body = StartCallRequest,
    responses(
        (status = 200, body = CallTicketResponse),
        (status = 403, description = "Not a member, blocked, or they do not accept calls", body = crate::error::ErrorResponse),
        (status = 429, body = crate::error::ErrorResponse),
        (status = 500, description = "Calling is not configured", body = crate::error::ErrorResponse),
    )
)]
pub async fn start(
    State(state): State<ApiState>,
    user: CurrentUser,
    Json(body): Json<StartCallRequest>,
) -> ApiResult<Json<CallTicketResponse>> {
    let service = CallService::new(state.services.clone());

    let ticket = service
        .start(
            user.user_id,
            ConversationId::from(body.conversation_id),
            body.video,
        )
        .await
        .map_err(ApiError)?;

    Ok(Json(CallTicketResponse {
        call_id: ticket.call.id.to_string(),
        room: ticket.call.room.clone(),
        kind: ticket.call.kind.clone(),
        is_group: ticket.call.is_group,
        token: ticket.token,
        server_url: ticket.server_url,
    }))
}

/// Answer a call
///
/// Returns a token for the same room. Being on the participant list is the
/// authorisation — knowing a call id is not enough.
#[utoipa::path(
    post, path = "/v1/calls/{call_id}/join", tag = "calls",
    security(("bearer" = [])), params(("call_id" = Uuid, Path,)),
    responses(
        (status = 200, body = CallTicketResponse),
        (status = 400, description = "The call has ended", body = crate::error::ErrorResponse),
        (status = 403, description = "Not invited to this call", body = crate::error::ErrorResponse),
    )
)]
pub async fn join(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path(call_id): Path<Uuid>,
) -> ApiResult<Json<CallTicketResponse>> {
    let service = CallService::new(state.services.clone());

    let ticket = service
        .join(user.user_id, CallId::from(call_id))
        .await
        .map_err(ApiError)?;

    Ok(Json(CallTicketResponse {
        call_id: ticket.call.id.to_string(),
        room: ticket.call.room.clone(),
        kind: ticket.call.kind.clone(),
        is_group: ticket.call.is_group,
        token: ticket.token,
        server_url: ticket.server_url,
    }))
}

/// End, decline or leave a call
///
/// In a group, `left` removes one participant; anything else ends the call for
/// everyone and stops every device ringing.
#[utoipa::path(
    post, path = "/v1/calls/{call_id}/end", tag = "calls",
    security(("bearer" = [])), params(("call_id" = Uuid, Path,)),
    request_body = EndCallRequest,
    responses(
        (status = 200, description = "Ended"),
        (status = 403, body = crate::error::ErrorResponse),
    )
)]
pub async fn end(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path(call_id): Path<Uuid>,
    Json(body): Json<EndCallRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let service = CallService::new(state.services.clone());

    service
        .end(user.user_id, CallId::from(call_id), &body.reason)
        .await
        .map_err(ApiError)?;

    Ok(Json(serde_json::json!({ "ended": true })))
}

/// Call history
#[utoipa::path(
    get, path = "/v1/calls", tag = "calls", security(("bearer" = [])),
    responses((status = 200, body = Vec<CallHistoryEntry>))
)]
pub async fn history(
    State(state): State<ApiState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<CallHistoryEntry>>> {
    let service = CallService::new(state.services.clone());

    let calls = service.history(user.user_id, 100).await.map_err(ApiError)?;

    Ok(Json(
        calls
            .into_iter()
            .map(|call| CallHistoryEntry {
                id: call.id.to_string(),
                conversation_id: call.conversation_id.map(|id| id.to_string()),
                initiator_id: call.initiator_id.map(|id| id.to_string()),
                kind: call.kind,
                is_group: call.is_group,
                started_at: call.started_at.to_rfc3339(),
                answered_at: call.answered_at.map(|at| at.to_rfc3339()),
                ended_at: call.ended_at.map(|at| at.to_rfc3339()),
                end_reason: call.end_reason,
            })
            .collect(),
    ))
}
