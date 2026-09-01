//! `/v1/media` — upload tickets and download URLs.

use axum::extract::{Path, State};
use axum::Json;
use nigchat_application::media::{MediaPurpose, MediaService};
use nigchat_domain::ids::MediaId;
use nigchat_domain::DomainError;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::extract::CurrentUser;
use crate::state::ApiState;

#[derive(Deserialize, ToSchema)]
pub struct UploadRequest {
    /// `avatar` or `attachment`.
    #[schema(example = "avatar")]
    pub purpose: String,
    #[schema(example = "image/jpeg")]
    pub mime_type: String,
    /// Bytes. Checked against the limit for the purpose.
    pub byte_size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_ms: Option<i32>,
}

#[derive(Serialize, ToSchema)]
pub struct UploadResponse {
    pub media_id: String,
    /// PUT the bytes here directly. They never pass through this API.
    pub upload_url: String,
    pub method: String,
    /// Send these headers with the PUT, verbatim.
    pub headers: Vec<[String; 2]>,
    pub expires_in: i64,
}

#[derive(Deserialize, ToSchema)]
pub struct CompleteRequest {
    /// The size actually uploaded.
    pub byte_size: i64,
}

#[derive(Serialize, ToSchema)]
pub struct MediaResponse {
    pub id: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_ms: Option<i32>,
    pub url: String,
}

/// Request an upload URL
///
/// Returns a short-lived signed URL. The client PUTs the bytes straight to
/// storage — routing a video through an API worker would block it for the whole
/// upload while looking idle to the autoscaler.
#[utoipa::path(
    post, path = "/v1/media/uploads", tag = "media", security(("bearer" = [])),
    request_body = UploadRequest,
    responses(
        (status = 200, body = UploadResponse),
        (status = 400, description = "Unsupported type or file too large", body = crate::error::ErrorResponse),
        (status = 429, body = crate::error::ErrorResponse),
        (status = 500, description = "Storage is not configured", body = crate::error::ErrorResponse),
    )
)]
pub async fn request_upload(
    State(state): State<ApiState>,
    user: CurrentUser,
    Json(body): Json<UploadRequest>,
) -> ApiResult<Json<UploadResponse>> {
    let purpose = match body.purpose.as_str() {
        "avatar" => MediaPurpose::Avatar,
        "attachment" => MediaPurpose::Attachment,
        other => {
            return Err(ApiError(DomainError::validation(format!(
                "unknown purpose '{other}' — expected avatar or attachment"
            ))))
        }
    };

    let service = MediaService::new(state.services.clone());
    let ticket = service
        .request_upload(
            user.user_id,
            purpose,
            &body.mime_type,
            body.byte_size,
            body.width,
            body.height,
            body.duration_ms,
        )
        .await
        .map_err(ApiError)?;

    Ok(Json(UploadResponse {
        media_id: ticket.media_id.to_string(),
        upload_url: ticket.upload.url,
        method: ticket.upload.method,
        headers: ticket
            .upload
            .headers
            .into_iter()
            .map(|(k, v)| [k, v])
            .collect(),
        expires_in: ticket.upload.expires_in_seconds,
    }))
}

/// Confirm an upload finished
///
/// Until this is called the record stays `pending` and the sweeper will delete
/// it — which is what stops abandoned uploads accumulating as storage cost.
#[utoipa::path(
    post, path = "/v1/media/{media_id}/complete", tag = "media",
    security(("bearer" = [])), params(("media_id" = Uuid, Path,)),
    request_body = CompleteRequest,
    responses(
        (status = 200, body = MediaResponse),
        (status = 404, description = "Unknown, not yours, or already completed", body = crate::error::ErrorResponse),
    )
)]
pub async fn complete(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path(media_id): Path<Uuid>,
    Json(body): Json<CompleteRequest>,
) -> ApiResult<Json<MediaResponse>> {
    let service = MediaService::new(state.services.clone());
    let id = MediaId::from(media_id);

    let asset = service
        .complete(user.user_id, id, body.byte_size)
        .await
        .map_err(ApiError)?;

    let url = service.download_url(id).await.map_err(ApiError)?;

    Ok(Json(MediaResponse {
        id: asset.id.to_string(),
        mime_type: asset.mime_type,
        byte_size: asset.byte_size,
        width: asset.width,
        height: asset.height,
        duration_ms: asset.duration_ms,
        url,
    }))
}

/// Get a download URL
///
/// Avatars resolve to a public path. Everything else gets a signed link that
/// expires, so fetch it when you need it rather than caching the URL.
#[utoipa::path(
    get, path = "/v1/media/{media_id}", tag = "media",
    security(("bearer" = [])), params(("media_id" = Uuid, Path,)),
    responses(
        (status = 200, body = MediaResponse),
        (status = 404, body = crate::error::ErrorResponse),
    )
)]
pub async fn get(
    State(state): State<ApiState>,
    _user: CurrentUser,
    Path(media_id): Path<Uuid>,
) -> ApiResult<Json<MediaResponse>> {
    let service = MediaService::new(state.services.clone());
    let id = MediaId::from(media_id);

    let asset = state
        .services
        .media
        .find(id)
        .await
        .map_err(ApiError)?
        .ok_or(ApiError(DomainError::not_found("media")))?;

    let url = service.download_url(id).await.map_err(ApiError)?;

    Ok(Json(MediaResponse {
        id: asset.id.to_string(),
        mime_type: asset.mime_type,
        byte_size: asset.byte_size,
        width: asset.width,
        height: asset.height,
        duration_ms: asset.duration_ms,
        url,
    }))
}
