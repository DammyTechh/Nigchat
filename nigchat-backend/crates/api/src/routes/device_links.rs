//! `/v1/devices/link-requests` — QR pairing for the web and desktop clients.

use axum::extract::{Path, State};
use axum::Json;
use nigchat_application::device_links::{DeviceLinkService, LinkStatus};
use nigchat_domain::entities::Platform;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::{ApiError, ApiResult};
use crate::extract::{ClientContext, CurrentUser};
use crate::state::ApiState;

#[derive(Deserialize, ToSchema)]
pub struct CreateLinkRequest {
    /// web, windows, macos or linux.
    #[schema(example = "web")]
    pub platform: String,
    /// Shown to the user on the phone before they approve.
    #[schema(example = "Chrome on Windows")]
    pub device_name: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CreateLinkResponse {
    /// Render this as a QR. Opaque, single-use, and short-lived.
    pub code: String,
    #[schema(example = 60)]
    pub expires_in: i64,
}

#[derive(Serialize, ToSchema)]
pub struct LinkStatusResponse {
    /// `pending`, `approved` or `gone`.
    pub status: String,
    /// Present exactly once, on the poll that observes the approval.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct ApproveLinkResponse {
    pub linked: bool,
    pub device_id: String,
}

/// Request a pairing code
///
/// Unauthenticated — the browser has no session yet, which is the point. The
/// code lives 60 seconds and is single-use.
#[utoipa::path(
    post, path = "/v1/devices/link-requests", tag = "devices",
    request_body = CreateLinkRequest,
    responses(
        (status = 200, body = CreateLinkResponse),
        (status = 429, description = "Too many codes from this address", body = crate::error::ErrorResponse),
    )
)]
pub async fn create(
    State(state): State<ApiState>,
    client: ClientContext,
    Json(body): Json<CreateLinkRequest>,
) -> ApiResult<Json<CreateLinkResponse>> {
    let platform = Platform::parse(&body.platform).map_err(ApiError)?;

    let service = DeviceLinkService::new(state.services.clone());
    let request = service
        .request(platform, body.device_name.as_deref(), client.ip.as_deref())
        .await
        .map_err(ApiError)?;

    Ok(Json(CreateLinkResponse {
        code: request.code,
        expires_in: request.expires_in_seconds,
    }))
}

/// Poll a pairing code
///
/// The browser calls this until it returns `approved`, which carries the token
/// pair. That happens once — the request is deleted as it is read, so a
/// replayed poll returns `gone`.
#[utoipa::path(
    get, path = "/v1/devices/link-requests/{code}", tag = "devices",
    params(("code" = String, Path,)),
    responses((status = 200, body = LinkStatusResponse))
)]
pub async fn poll(
    State(state): State<ApiState>,
    Path(code): Path<String>,
) -> ApiResult<Json<LinkStatusResponse>> {
    let service = DeviceLinkService::new(state.services.clone());

    let status = service.poll(&code).await.map_err(ApiError)?;

    Ok(Json(match status {
        LinkStatus::Pending => LinkStatusResponse {
            status: "pending".into(),
            access_token: None,
            refresh_token: None,
            user_id: None,
            device_id: None,
            expires_in: None,
        },
        LinkStatus::Gone => LinkStatusResponse {
            status: "gone".into(),
            access_token: None,
            refresh_token: None,
            user_id: None,
            device_id: None,
            expires_in: None,
        },
        LinkStatus::Approved {
            user_id,
            device_id,
            access_token,
            refresh_token,
            expires_in_seconds,
        } => LinkStatusResponse {
            status: "approved".into(),
            access_token: Some(access_token),
            refresh_token: Some(refresh_token),
            user_id: Some(user_id.to_string()),
            device_id: Some(device_id.to_string()),
            expires_in: Some(expires_in_seconds),
        },
    }))
}

/// Approve a scanned code
///
/// Called by the phone. The caller's own session is what authorises the new
/// device — that is the entire trust model, and why the browser never needs a
/// password.
#[utoipa::path(
    post, path = "/v1/devices/link-requests/{code}/approve", tag = "devices",
    security(("bearer" = [])), params(("code" = String, Path,)),
    responses(
        (status = 200, body = ApproveLinkResponse),
        (status = 400, description = "Expired or already used", body = crate::error::ErrorResponse),
        (status = 429, body = crate::error::ErrorResponse),
    )
)]
pub async fn approve(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path(code): Path<String>,
) -> ApiResult<Json<ApproveLinkResponse>> {
    let service = DeviceLinkService::new(state.services.clone());

    let device_id = service.approve(user.user_id, &code).await.map_err(ApiError)?;

    Ok(Json(ApproveLinkResponse {
        linked: true,
        device_id: device_id.to_string(),
    }))
}
