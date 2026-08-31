//! `/v1/messages` and `/v1/conversations/{id}/messages`.

use axum::extract::{Path, Query, State};
use axum::Json;
use nigchat_application::messaging::SendMessageCommand;
use nigchat_domain::entities::MessageKind;
use nigchat_domain::ids::{ClientMessageId, ConversationId, MediaId, MessageId, UserId};
use nigchat_domain::values::Cursor;
use uuid::Uuid;

use super::dto::*;
use crate::error::{ApiError, ApiResult};
use crate::extract::CurrentUser;
use crate::state::ApiState;

/// Send a message
///
/// **Always supply `client_message_id`**, generated on the device before the
/// request. Retrying with the same value returns the original message rather
/// than creating a duplicate, which is what makes a send safe to retry on a
/// dropped connection.
///
/// The body is ciphertext produced on the device. The server orders and routes
/// it; it cannot read it.
#[utoipa::path(
    post, path = "/v1/messages", tag = "messages", security(("bearer" = [])),
    request_body = SendMessageRequest,
    responses(
        (status = 200, description = "Sent, or the original returned for a replay", body = MessageResponse),
        (status = 400, description = "Invalid payload", body = crate::error::ErrorResponse),
        (status = 403, description = "Not a member, or posting is restricted to admins", body = crate::error::ErrorResponse),
        (status = 429, body = crate::error::ErrorResponse),
    )
)]
pub async fn send(
    State(state): State<ApiState>,
    user: CurrentUser,
    Json(body): Json<SendMessageRequest>,
) -> ApiResult<Json<MessageResponse>> {
    let kind = MessageKind::parse(&body.kind).map_err(ApiError)?;
    let ciphertext = decode_b64(&body.ciphertext).map_err(ApiError)?;

    let result = state
        .messaging
        .send(SendMessageCommand {
            conversation_id: ConversationId::from(body.conversation_id),
            sender_id: user.user_id,
            sender_device_id: user.device_id,
            client_message_id: ClientMessageId::from(body.client_message_id),
            kind,
            ciphertext,
            envelope_version: body.envelope_version,
            metadata: body.metadata,
            reply_to_id: body.reply_to_id.map(MessageId::from),
            mentions: body.mentions.into_iter().map(UserId::from).collect(),
            media_ids: body.media_ids.into_iter().map(MediaId::from).collect(),
        })
        .await
        .map_err(ApiError)?;

    Ok(Json(result.message.into()))
}

/// List messages
///
/// Keyset pagination over `seq` — never offset. Use `before_seq` to scroll
/// into history and `after_seq` to catch up after being offline.
#[utoipa::path(
    get, path = "/v1/conversations/{conversation_id}/messages", tag = "messages",
    security(("bearer" = [])),
    params(("conversation_id" = Uuid, Path,), ListMessagesQuery),
    responses(
        (status = 200, body = MessagePage),
        (status = 403, body = crate::error::ErrorResponse),
    )
)]
pub async fn list(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path(conversation_id): Path<Uuid>,
    Query(query): Query<ListMessagesQuery>,
) -> ApiResult<Json<Page<MessageResponse>>> {
    let cursor = Cursor::new(query.before_seq, query.after_seq, query.limit);

    let (messages, has_more) = state
        .messaging
        .page(ConversationId::from(conversation_id), user.user_id, cursor)
        .await
        .map_err(ApiError)?;

    let next_cursor = messages.last().map(|m| m.seq.value()).filter(|_| has_more);

    Ok(Json(Page {
        items: messages.into_iter().map(MessageResponse::from).collect(),
        has_more,
        next_cursor,
    }))
}

/// Edit a message
///
/// Author only, text only, within 15 minutes of sending. The previous version
/// is retained as an encrypted revision.
#[utoipa::path(
    patch, path = "/v1/messages/{message_id}", tag = "messages", security(("bearer" = [])),
    params(("message_id" = Uuid, Path,)), request_body = EditMessageRequest,
    responses(
        (status = 200, body = MessageResponse),
        (status = 403, description = "Not the author, or the edit window has closed", body = crate::error::ErrorResponse),
    )
)]
pub async fn edit(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path(message_id): Path<Uuid>,
    Json(body): Json<EditMessageRequest>,
) -> ApiResult<Json<MessageResponse>> {
    let ciphertext = decode_b64(&body.ciphertext).map_err(ApiError)?;

    let message = state
        .messaging
        .edit(MessageId::from(message_id), user.user_id, ciphertext)
        .await
        .map_err(ApiError)?;

    Ok(Json(message.into()))
}

/// Delete a message
///
/// Soft delete: the sequence number survives so other devices learn the
/// message is gone instead of finding a hole. `for_everyone` requires
/// authorship or admin rights.
#[utoipa::path(
    delete, path = "/v1/messages/{message_id}", tag = "messages", security(("bearer" = [])),
    params(
        ("message_id" = Uuid, Path,),
        ("for_everyone" = Option<bool>, Query,),
    ),
    responses(
        (status = 200, body = SeqResponse),
        (status = 403, body = crate::error::ErrorResponse),
    )
)]
pub async fn delete(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path(message_id): Path<Uuid>,
    Query(query): Query<DeleteQuery>,
) -> ApiResult<Json<SeqResponse>> {
    let seq = state
        .messaging
        .delete(
            MessageId::from(message_id),
            user.user_id,
            query.for_everyone.unwrap_or(false),
        )
        .await
        .map_err(ApiError)?;

    Ok(Json(SeqResponse { seq: seq.value() }))
}

#[derive(serde::Deserialize)]
pub struct DeleteQuery {
    pub for_everyone: Option<bool>,
}

/// Add or remove a reaction
#[utoipa::path(
    post, path = "/v1/messages/{message_id}/reactions", tag = "messages",
    security(("bearer" = [])), params(("message_id" = Uuid, Path,)),
    request_body = ReactionRequest,
    responses(
        (status = 200, body = OkResponse),
        (status = 403, body = crate::error::ErrorResponse),
    )
)]
pub async fn react(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path(message_id): Path<Uuid>,
    Json(body): Json<ReactionRequest>,
) -> ApiResult<Json<OkResponse>> {
    state
        .messaging
        .set_reaction(
            MessageId::from(message_id),
            user.user_id,
            &body.emoji,
            body.removed,
        )
        .await
        .map_err(ApiError)?;

    Ok(Json(OkResponse { ok: true }))
}
