//! `/v1/me`, `/v1/users` — profile, contact discovery, blocking, devices,
//! security log.

use axum::extract::{Path, State};
use axum::Json;
use nigchat_application::devices::RegisterPushTokenCommand;
use nigchat_domain::entities::PushProvider;
use nigchat_domain::ids::{DeviceId, MediaId, UserId};
use nigchat_domain::values::Username;
use nigchat_domain::DomainError;
use uuid::Uuid;

use super::dto::*;
use crate::error::{ApiError, ApiResult};
use crate::extract::CurrentUser;
use crate::state::ApiState;

/// Get my profile
#[utoipa::path(
    get, path = "/v1/me", tag = "users", security(("bearer" = [])),
    responses((status = 200, body = MeResponse))
)]
pub async fn me(State(state): State<ApiState>, user: CurrentUser) -> ApiResult<Json<MeResponse>> {
    let found = state
        .services
        .users
        .find_by_id(user.user_id)
        .await
        .map_err(ApiError)?
        .ok_or(ApiError(DomainError::not_found("user")))?;
    Ok(Json(found.into()))
}

/// Update my profile
///
/// Omitted fields are left unchanged, so two clients editing different fields
/// cannot clobber each other.
#[utoipa::path(
    patch, path = "/v1/me", tag = "users", security(("bearer" = [])),
    request_body = UpdateProfileRequest,
    responses(
        (status = 200, body = MeResponse),
        (status = 400, description = "Invalid username or display name", body = crate::error::ErrorResponse),
        (status = 409, description = "Username taken", body = crate::error::ErrorResponse),
    )
)]
pub async fn update_me(
    State(state): State<ApiState>,
    user: CurrentUser,
    Json(body): Json<UpdateProfileRequest>,
) -> ApiResult<Json<MeResponse>> {
    if let Some(name) = &body.display_name {
        let length = name.trim().chars().count();
        if length == 0 || length > 64 {
            return Err(ApiError(DomainError::validation(
                "display_name must be between 1 and 64 characters",
            )));
        }
    }

    let username = body
        .username
        .as_deref()
        .map(Username::parse)
        .transpose()
        .map_err(ApiError)?;

    let updated = state
        .services
        .users
        .update_profile(
            user.user_id,
            body.display_name.as_deref().map(str::trim),
            body.about.as_deref(),
            username.as_ref(),
            body.avatar_media_id.map(MediaId::from),
        )
        .await
        .map_err(ApiError)?;

    Ok(Json(updated.into()))
}

/// Get another user's profile
///
/// Never includes a phone number.
#[utoipa::path(
    get, path = "/v1/users/{user_id}", tag = "users", security(("bearer" = [])),
    params(("user_id" = Uuid, Path,)),
    responses(
        (status = 200, body = PublicUserResponse),
        (status = 404, body = crate::error::ErrorResponse),
    )
)]
pub async fn get_user(
    State(state): State<ApiState>,
    _user: CurrentUser,
    Path(user_id): Path<Uuid>,
) -> ApiResult<Json<PublicUserResponse>> {
    let found = state
        .services
        .users
        .find_by_id(UserId::from(user_id))
        .await
        .map_err(ApiError)?
        .filter(|user| user.is_active)
        .ok_or(ApiError(DomainError::not_found("user")))?;

    Ok(Json(found.into()))
}

/// Contact discovery
///
/// Send peppered hashes, not raw numbers — the server must not learn the phone
/// numbers of people who are not users. Limited to 2,000 per request and 10
/// requests per hour, because this endpoint is how an attacker would enumerate
/// who is on the platform.
#[utoipa::path(
    post, path = "/v1/users/sync-contacts", tag = "users", security(("bearer" = [])),
    request_body = ContactSyncRequest,
    responses(
        (status = 200, body = Vec<PublicUserResponse>),
        (status = 429, body = crate::error::ErrorResponse),
    )
)]
pub async fn sync_contacts(
    State(state): State<ApiState>,
    user: CurrentUser,
    Json(body): Json<ContactSyncRequest>,
) -> ApiResult<Json<Vec<PublicUserResponse>>> {
    if body.phone_hashes.len() > 2_000 {
        return Err(ApiError(DomainError::validation(
            "at most 2000 contacts per request",
        )));
    }

    state
        .services
        .rate_limiter
        .check(&format!("contacts:sync:{}", user.user_id), 10, 3_600)
        .await
        .map_err(ApiError)?;

    let found = state
        .services
        .users
        .find_by_phone_hashes(&body.phone_hashes)
        .await
        .map_err(ApiError)?;

    Ok(Json(
        found
            .into_iter()
            .filter(|candidate| candidate.id != user.user_id)
            .map(PublicUserResponse::from)
            .collect(),
    ))
}

/// Block a user
#[utoipa::path(
    post, path = "/v1/me/blocks", tag = "users", security(("bearer" = [])),
    request_body = BlockRequest,
    responses((status = 200, body = OkResponse))
)]
pub async fn block(
    State(state): State<ApiState>,
    user: CurrentUser,
    Json(body): Json<BlockRequest>,
) -> ApiResult<Json<OkResponse>> {
    let target = UserId::from(body.user_id);
    if target == user.user_id {
        return Err(ApiError(DomainError::validation("cannot block yourself")));
    }

    state
        .services
        .users
        .block(user.user_id, target)
        .await
        .map_err(ApiError)?;

    Ok(Json(OkResponse { ok: true }))
}

/// Unblock a user
#[utoipa::path(
    delete, path = "/v1/me/blocks/{user_id}", tag = "users", security(("bearer" = [])),
    params(("user_id" = Uuid, Path,)),
    responses((status = 200, body = OkResponse))
)]
pub async fn unblock(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path(user_id): Path<Uuid>,
) -> ApiResult<Json<OkResponse>> {
    state
        .services
        .users
        .unblock(user.user_id, UserId::from(user_id))
        .await
        .map_err(ApiError)?;
    Ok(Json(OkResponse { ok: true }))
}

/// List my linked devices
#[utoipa::path(
    get, path = "/v1/me/devices", tag = "devices", security(("bearer" = [])),
    responses((status = 200, body = Vec<DeviceResponse>))
)]
pub async fn list_devices(
    State(state): State<ApiState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<DeviceResponse>>> {
    let devices = state
        .devices
        .list(user.user_id)
        .await
        .map_err(ApiError)?;
    Ok(Json(devices.into_iter().map(DeviceResponse::from).collect()))
}

/// Revoke a device
///
/// Ends its sessions and retires its push tokens in one transaction, then
/// tells the device to sign itself out.
#[utoipa::path(
    delete, path = "/v1/me/devices/{device_id}", tag = "devices", security(("bearer" = [])),
    params(("device_id" = Uuid, Path,)),
    responses(
        (status = 200, body = OkResponse),
        (status = 403, description = "Not your device", body = crate::error::ErrorResponse),
    )
)]
pub async fn revoke_device(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path(device_id): Path<Uuid>,
) -> ApiResult<Json<OkResponse>> {
    state
        .devices
        .revoke(user.user_id, DeviceId::from(device_id), "user_revoked")
        .await
        .map_err(ApiError)?;
    Ok(Json(OkResponse { ok: true }))
}

/// Register a push token
///
/// Call on every launch: tokens rotate on reinstall and OS update. The write
/// is an upsert, so repeated calls do not accumulate rows.
#[utoipa::path(
    post, path = "/v1/me/devices/push-token", tag = "notifications", security(("bearer" = [])),
    request_body = RegisterPushTokenRequest,
    responses(
        (status = 200, body = OkResponse),
        (status = 400, description = "Unknown provider or malformed token", body = crate::error::ErrorResponse),
    )
)]
pub async fn register_push_token(
    State(state): State<ApiState>,
    user: CurrentUser,
    Json(body): Json<RegisterPushTokenRequest>,
) -> ApiResult<Json<OkResponse>> {
    let provider = PushProvider::parse(&body.provider).map_err(ApiError)?;

    state
        .devices
        .register_push_token(RegisterPushTokenCommand {
            user_id: user.user_id,
            device_id: user.device_id,
            provider,
            token: body.token,
            is_voip: body.is_voip,
            sandbox: body.sandbox,
        })
        .await
        .map_err(ApiError)?;

    Ok(Json(OkResponse { ok: true }))
}

/// Enable or change two-step verification
///
/// The control that stops a SIM-swap attacker taking an account with nothing
/// but a hijacked SMS. Changing an existing PIN requires the current one.
#[utoipa::path(
    post, path = "/v1/me/two-step", tag = "security", security(("bearer" = [])),
    request_body = SetTwoStepPinRequest,
    responses(
        (status = 200, body = OkResponse),
        (status = 400, description = "PIN too short, non-numeric or predictable", body = crate::error::ErrorResponse),
        (status = 401, description = "Current PIN incorrect", body = crate::error::ErrorResponse),
        (status = 429, description = "Too many attempts", body = crate::error::ErrorResponse),
    )
)]
pub async fn set_two_step_pin(
    State(state): State<ApiState>,
    user: CurrentUser,
    Json(body): Json<SetTwoStepPinRequest>,
) -> ApiResult<Json<OkResponse>> {
    state
        .devices
        .set_two_step_pin(user.user_id, &body.pin, body.current_pin.as_deref())
        .await
        .map_err(ApiError)?;
    Ok(Json(OkResponse { ok: true }))
}

/// Disable two-step verification
///
/// Requires the current PIN: otherwise a stolen access token could switch off
/// the control protecting the account.
#[utoipa::path(
    delete, path = "/v1/me/two-step", tag = "security", security(("bearer" = [])),
    request_body = VerifyPinRequest,
    responses(
        (status = 200, body = OkResponse),
        (status = 401, body = crate::error::ErrorResponse),
    )
)]
pub async fn disable_two_step(
    State(state): State<ApiState>,
    user: CurrentUser,
    Json(body): Json<VerifyPinRequest>,
) -> ApiResult<Json<OkResponse>> {
    state
        .devices
        .disable_two_step(user.user_id, &body.pin)
        .await
        .map_err(ApiError)?;
    Ok(Json(OkResponse { ok: true }))
}

/// Verify the two-step PIN
#[utoipa::path(
    post, path = "/v1/me/two-step/verify", tag = "security", security(("bearer" = [])),
    request_body = VerifyPinRequest,
    responses(
        (status = 200, body = OkResponse),
        (status = 401, body = crate::error::ErrorResponse),
        (status = 429, body = crate::error::ErrorResponse),
    )
)]
pub async fn verify_two_step(
    State(state): State<ApiState>,
    user: CurrentUser,
    Json(body): Json<VerifyPinRequest>,
) -> ApiResult<Json<OkResponse>> {
    let ok = state
        .devices
        .verify_two_step(user.user_id, &body.pin)
        .await
        .map_err(ApiError)?;
    Ok(Json(OkResponse { ok }))
}

/// Look up a user by username
#[utoipa::path(
    get, path = "/v1/users/by-username/{username}", tag = "users", security(("bearer" = [])),
    params(("username" = String, Path,)),
    responses(
        (status = 200, body = PublicUserResponse),
        (status = 404, body = crate::error::ErrorResponse),
    )
)]
pub async fn get_by_username(
    State(state): State<ApiState>,
    user: CurrentUser,
    Path(username): Path<String>,
) -> ApiResult<Json<PublicUserResponse>> {
    // Handle lookup is an enumeration surface: an unlimited endpoint lets an
    // attacker harvest the whole username space.
    state
        .services
        .rate_limiter
        .check(&format!("username:lookup:{}", user.user_id), 60, 3_600)
        .await
        .map_err(ApiError)?;

    let parsed = Username::parse(&username).map_err(ApiError)?;

    let found = state
        .services
        .users
        .find_by_username(&parsed)
        .await
        .map_err(ApiError)?
        .filter(|found| found.is_active)
        .ok_or(ApiError(DomainError::not_found("user")))?;

    Ok(Json(found.into()))
}

/// My security timeline
///
/// New device linked, identity key changed, session reuse detected, and so on.
/// Contains no secrets and no message content.
#[utoipa::path(
    get, path = "/v1/me/security-events", tag = "security", security(("bearer" = [])),
    responses((status = 200, body = Vec<SecurityEventResponse>))
)]
pub async fn security_events(
    State(state): State<ApiState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<SecurityEventResponse>>> {
    let events = state
        .devices
        .security_events(user.user_id, 100)
        .await
        .map_err(ApiError)?;

    Ok(Json(
        events.into_iter().map(SecurityEventResponse::from).collect(),
    ))
}
