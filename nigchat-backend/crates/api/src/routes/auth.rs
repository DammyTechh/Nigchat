//! `/v1/auth` — registration, sign-in and session lifecycle.

use axum::extract::State;
use axum::Json;
use nigchat_application::auth::VerifyOtpCommand;
use nigchat_domain::entities::Platform;
use nigchat_domain::ids::DeviceId;
use nigchat_domain::values::PhoneNumber;

use super::dto::*;
use crate::error::{ApiError, ApiResult};
use crate::extract::{ClientContext, CurrentUser};
use crate::state::ApiState;

/// Request a verification code
///
/// Sends a 6-digit code by SMS. Rate limited three ways: one per minute and
/// five per hour per number, plus twenty per hour per IP — the last stops an
/// attacker pumping SMS charges across many different numbers.
#[utoipa::path(
    post,
    path = "/v1/auth/request-otp",
    tag = "auth",
    request_body = RequestOtpRequest,
    responses(
        (status = 200, description = "Code sent", body = RequestOtpResponse),
        (status = 400, description = "Malformed phone number", body = crate::error::ErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::ErrorResponse),
    )
)]
pub async fn request_otp(
    State(state): State<ApiState>,
    client: ClientContext,
    Json(body): Json<RequestOtpRequest>,
) -> ApiResult<Json<RequestOtpResponse>> {
    let phone = PhoneNumber::parse(&body.phone_e164).map_err(ApiError)?;

    let result = state
        .auth
        .request_otp(phone, client.ip.as_deref())
        .await
        .map_err(ApiError)?;

    Ok(Json(RequestOtpResponse {
        challenge_sent: true,
        expires_in: result.expires_in_seconds,
        debug_code: result.debug_code,
    }))
}

/// Verify a code and start a session
///
/// Creates the account if the number is new. Returns an access token (15
/// minutes) and a refresh token (90 days, rotated on every use).
#[utoipa::path(
    post,
    path = "/v1/auth/verify-otp",
    tag = "auth",
    request_body = VerifyOtpRequest,
    responses(
        (status = 200, description = "Authenticated", body = TokenPairResponse),
        (status = 401, description = "Wrong or expired code", body = crate::error::ErrorResponse),
        (status = 429, description = "Too many attempts", body = crate::error::ErrorResponse),
    )
)]
pub async fn verify_otp(
    State(state): State<ApiState>,
    client: ClientContext,
    Json(body): Json<VerifyOtpRequest>,
) -> ApiResult<Json<TokenPairResponse>> {
    let phone = PhoneNumber::parse(&body.phone_e164).map_err(ApiError)?;
    let platform = Platform::parse(&body.platform).map_err(ApiError)?;

    let session = state
        .auth
        .verify_otp(VerifyOtpCommand {
            phone,
            code: body.code,
            display_name: body.display_name,
            platform,
            device_name: body.device_name,
            app_version: body.app_version,
            existing_device_id: body.device_id.map(DeviceId::from),
            ip: client.ip,
            user_agent: client.user_agent,
        })
        .await
        .map_err(ApiError)?;

    Ok(Json(TokenPairResponse {
        access_token: session.access_token,
        refresh_token: session.refresh_token,
        expires_in: session.expires_in_seconds,
        user_id: session.user.id.as_uuid(),
        device_id: session.device.id.as_uuid(),
        is_new_account: session.is_new_account,
    }))
}

/// Rotate the refresh token
///
/// The presented token is dead once this returns. Presenting it again is
/// treated as theft and revokes every session on the device.
#[utoipa::path(
    post,
    path = "/v1/auth/refresh",
    tag = "auth",
    request_body = RefreshRequest,
    responses(
        (status = 200, description = "New token pair", body = TokenPairResponse),
        (status = 401, description = "Invalid, expired or reused token", body = crate::error::ErrorResponse),
    )
)]
pub async fn refresh(
    State(state): State<ApiState>,
    Json(body): Json<RefreshRequest>,
) -> ApiResult<Json<TokenPairResponse>> {
    let session = state
        .auth
        .refresh(&body.refresh_token)
        .await
        .map_err(ApiError)?;

    Ok(Json(TokenPairResponse {
        access_token: session.access_token,
        refresh_token: session.refresh_token,
        expires_in: session.expires_in_seconds,
        user_id: session.user.id.as_uuid(),
        device_id: session.device.id.as_uuid(),
        is_new_account: false,
    }))
}

/// Sign out this device
///
/// Revokes the refresh tokens. The current access token remains valid until it
/// expires — at most 15 minutes.
#[utoipa::path(
    post,
    path = "/v1/auth/logout",
    tag = "auth",
    security(("bearer" = [])),
    responses((status = 200, description = "Signed out", body = OkResponse))
)]
pub async fn logout(
    State(state): State<ApiState>,
    user: CurrentUser,
) -> ApiResult<Json<OkResponse>> {
    state
        .auth
        .logout(user.user_id, user.device_id)
        .await
        .map_err(ApiError)?;
    Ok(Json(OkResponse { ok: true }))
}
