//! `/v1/conversations` — direct chats, groups and channels.

use axum::extract::{Path, State};
use axum::Json;
use nigchat_domain::entities::MemberRole;
use nigchat_domain::ids::{ConversationId, UserId};
use nigchat_domain::values::{MuteDuration, Seq};
use nigchat_domain::DomainError;
use uuid::Uuid;

use super::dto::*;
use crate::error::{ApiError, ApiResult};
use crate::extract::CurrentUser;
use crate::state::ApiState;

/// List my conversations
///
/// One query: unread counts, last-message metadata and mute state are all
/// included, so rendering the list needs no follow-up requests.
#[utoipa::path(
    get, path = "/v1/conversations", tag = "conversations", security(("bearer" = [])),
    responses((status = 200, body = Vec<ConversationSummaryResponse>))
)]
pub async fn list(
    State(state): State<ApiState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<ConversationSummaryResponse>>> {
    let conversations = state
        .conversations
        .list(user.user_id)
        .await
        .map_err(ApiError)?;

    Ok(Json(
        conversations
            .into_iter()
            .map(ConversationSummaryResponse::from)
            .collect(),
    ))
}

/// Open a direct conversation
///
/// Idempotent: calling twice for the same pair returns the same conversation,
/// even when both users tap at the same moment on different servers.
#[utoipa::path(
    post, path = "/v1/conversations/direct", tag = "conversations", security(("bearer" = [])),
    request_body = CreateDirectRequest,
    responses(
        (status = 200, body = ConversationResponse),
        (status = 403, description = "Blocked by that user", body = crate::error::ErrorResponse),
        (status = 404, body = crate::error::ErrorResponse),
    )
)]
pub async fn create_direct(
    State(state): State<ApiState>,
    user: CurrentUser,
    Json(body): Json<CreateDirectRequest>,
) -> ApiResult<Json<ConversationResponse>> {
    let conversation = state
        .conversations
        .open_direct(user.user_id, UserId::from(body.peer_user_id))
        .await
        .map_err(ApiError)?;
    Ok(Json(conversation.into()))
}

/// Create a group
#[utoipa::path(
    post, path = "/v1/conversations/group", tag = "conversations", security(("bearer" = [])),
    request_body = CreateGroupRequest,
    responses(
        (status = 200, body = ConversationResponse),
        (status = 400, description = "Invalid title", body = crate::error::ErrorResponse),
        (status = 429, body = crate::error::ErrorResponse),
    )
)]
pub async fn create_group(
    State(state): State<ApiState>,
    user: CurrentUser,
    Json(body): Json<CreateGroupRequest>,
) -> ApiResult<Json<ConversationResponse>> {
    let members: Vec<UserId> = body.member_ids.into_iter().map(UserId::from).collect();

    let conversation = state
        .conversations
        .create_group(
            user.user_id,
            &body.title,
            body.description.as_deref(),
            &members,
        )
        .await
        .map_err(ApiError)?;

    Ok(Json(conversation.into()))
}

/// Get a conversation
#[utoipa::path(
    get, path = "/v1/conversations/{conversation_id}", tag = "conversations",
    security(("bearer" = [])), params(("conversation_id" = Uuid, Path,)),
    responses(
        (status = 200, body = ConversationResponse),
        (status = 403, description = "Not a member", body = crate::error::ErrorResponse),
    )
)]
pub async fn get(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path(conversation_id): Path<Uuid>,
) -> ApiResult<Json<ConversationResponse>> {
    let conversation = state
        .conversations
        .get(ConversationId::from(conversation_id), user.user_id)
        .await
        .map_err(ApiError)?;
    Ok(Json(conversation.into()))
}

/// Add members
#[utoipa::path(
    post, path = "/v1/conversations/{conversation_id}/members", tag = "conversations",
    security(("bearer" = [])), params(("conversation_id" = Uuid, Path,)),
    request_body = AddMembersRequest,
    responses(
        (status = 200, description = "Ids actually added", body = Vec<Uuid>),
        (status = 403, description = "Admin rights required", body = crate::error::ErrorResponse),
    )
)]
pub async fn add_members(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path(conversation_id): Path<Uuid>,
    Json(body): Json<AddMembersRequest>,
) -> ApiResult<Json<Vec<Uuid>>> {
    let members: Vec<UserId> = body.member_ids.into_iter().map(UserId::from).collect();

    let added = state
        .conversations
        .add_members(ConversationId::from(conversation_id), user.user_id, &members)
        .await
        .map_err(ApiError)?;

    Ok(Json(added.into_iter().map(|id| id.as_uuid()).collect()))
}

/// Remove a member, or leave
///
/// Removing yourself is how "leave group" is expressed. Removing anyone else
/// requires admin rights, and an owner can only be removed by themselves.
#[utoipa::path(
    delete, path = "/v1/conversations/{conversation_id}/members/{user_id}", tag = "conversations",
    security(("bearer" = [])),
    params(("conversation_id" = Uuid, Path,), ("user_id" = Uuid, Path,)),
    responses(
        (status = 200, body = OkResponse),
        (status = 403, body = crate::error::ErrorResponse),
    )
)]
pub async fn remove_member(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path((conversation_id, target_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<OkResponse>> {
    state
        .conversations
        .remove_member(
            ConversationId::from(conversation_id),
            user.user_id,
            UserId::from(target_id),
        )
        .await
        .map_err(ApiError)?;
    Ok(Json(OkResponse { ok: true }))
}

/// Change a member's role
#[utoipa::path(
    put, path = "/v1/conversations/{conversation_id}/members/{user_id}/role", tag = "conversations",
    security(("bearer" = [])),
    params(("conversation_id" = Uuid, Path,), ("user_id" = Uuid, Path,)),
    request_body = SetRoleRequest,
    responses(
        (status = 200, body = OkResponse),
        (status = 403, body = crate::error::ErrorResponse),
    )
)]
pub async fn set_role(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path((conversation_id, target_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<SetRoleRequest>,
) -> ApiResult<Json<OkResponse>> {
    let role = match body.role.as_str() {
        "owner" => MemberRole::Owner,
        "admin" => MemberRole::Admin,
        "member" => MemberRole::Member,
        other => {
            return Err(ApiError(DomainError::validation(format!(
                "unknown role '{other}'"
            ))))
        }
    };

    state
        .conversations
        .set_role(
            ConversationId::from(conversation_id),
            user.user_id,
            UserId::from(target_id),
            role,
        )
        .await
        .map_err(ApiError)?;

    Ok(Json(OkResponse { ok: true }))
}

/// Mute or unmute
///
/// While muted, an @mention still notifies unless `notify_on_mention` is
/// switched off for this conversation.
#[utoipa::path(
    post, path = "/v1/conversations/{conversation_id}/mute", tag = "notifications",
    security(("bearer" = [])), params(("conversation_id" = Uuid, Path,)),
    request_body = MuteRequest,
    responses((status = 200, body = ConversationNotificationResponse))
)]
pub async fn mute(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path(conversation_id): Path<Uuid>,
    Json(body): Json<MuteRequest>,
) -> ApiResult<Json<ConversationNotificationResponse>> {
    let duration = match body.duration.as_deref() {
        None => None,
        Some("eight_hours") => Some(MuteDuration::EightHours),
        Some("one_week") => Some(MuteDuration::OneWeek),
        Some("always") => Some(MuteDuration::Always),
        Some(other) => {
            return Err(ApiError(DomainError::validation(format!(
                "unknown mute duration '{other}'"
            ))))
        }
    };

    let conversation_id = ConversationId::from(conversation_id);

    state
        .conversations
        .mute(conversation_id, user.user_id, duration)
        .await
        .map_err(ApiError)?;

    let settings = state
        .devices
        .conversation_notifications(conversation_id, user.user_id)
        .await
        .map_err(ApiError)?;

    Ok(Json(ConversationNotificationResponse {
        muted_until: settings.mute.muted_until,
        notify_on_mention: settings.notify_on_mention,
        tone_id: settings.tone_id,
        call_ringtone_id: settings.call_ringtone_id,
        vibration: settings.vibration.map(|v| v.as_str().to_string()),
        preview_mode: settings.preview_mode.map(|p| p.as_str().to_string()),
    }))
}

/// Mark as delivered
///
/// The first tick. Call as soon as a device receives a message, whether or not
/// the user has opened the conversation — the two ticks mean different things
/// to the sender.
#[utoipa::path(
    post, path = "/v1/conversations/{conversation_id}/delivered", tag = "messages",
    security(("bearer" = [])), params(("conversation_id" = Uuid, Path,)),
    request_body = MarkDeliveredRequest,
    responses((status = 200, body = SeqResponse))
)]
pub async fn mark_delivered(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path(conversation_id): Path<Uuid>,
    Json(body): Json<MarkDeliveredRequest>,
) -> ApiResult<Json<SeqResponse>> {
    let marker = state
        .messaging
        .mark_delivered(
            ConversationId::from(conversation_id),
            user.user_id,
            Seq(body.last_delivered_seq),
        )
        .await
        .map_err(ApiError)?;

    Ok(Json(SeqResponse {
        seq: marker.value(),
    }))
}

/// Mark as read
///
/// A high-water mark that only moves forward, so one call covers every message
/// up to `last_read_seq` and a large group emits one event, not hundreds.
#[utoipa::path(
    post, path = "/v1/conversations/{conversation_id}/read", tag = "messages",
    security(("bearer" = [])), params(("conversation_id" = Uuid, Path,)),
    request_body = MarkReadRequest,
    responses((status = 200, body = SeqResponse))
)]
pub async fn mark_read(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path(conversation_id): Path<Uuid>,
    Json(body): Json<MarkReadRequest>,
) -> ApiResult<Json<SeqResponse>> {
    let marker = state
        .messaging
        .mark_read(
            ConversationId::from(conversation_id),
            user.user_id,
            Seq(body.last_read_seq),
        )
        .await
        .map_err(ApiError)?;

    Ok(Json(SeqResponse {
        seq: marker.value(),
    }))
}
