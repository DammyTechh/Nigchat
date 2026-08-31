//! `/v1/keys` — the E2EE key directory (spec §28).
//!
//! The server stores public material and hands out bundles. It performs no
//! cryptography and holds nothing that would let it read a message.

use axum::extract::{Path, State};
use axum::Json;
use nigchat_application::keys::PublishKeysCommand;
use nigchat_domain::ids::UserId;
use uuid::Uuid;

use super::dto::*;
use crate::error::{ApiError, ApiResult};
use crate::extract::CurrentUser;
use crate::state::ApiState;

/// Publish this device's keys
///
/// Called once when a device is linked, and again on rotation. Republishing an
/// identity key bumps its version and notifies every peer, because that is
/// also what a server-side impersonation attempt would look like.
#[utoipa::path(
    post, path = "/v1/keys", tag = "encryption", security(("bearer" = [])),
    request_body = PublishKeysRequest,
    responses(
        (status = 200, description = "New key version", body = i32),
        (status = 400, body = crate::error::ErrorResponse),
    )
)]
pub async fn publish(
    State(state): State<ApiState>,
    user: CurrentUser,
    Json(body): Json<PublishKeysRequest>,
) -> ApiResult<Json<i32>> {
    let mut one_time = Vec::with_capacity(body.one_time_prekeys.len());
    for key in &body.one_time_prekeys {
        one_time.push((key.key_id, decode_b64(&key.public_key).map_err(ApiError)?));
    }

    let version = state
        .keys
        .publish(PublishKeysCommand {
            user_id: user.user_id,
            device_id: user.device_id,
            registration_id: body.registration_id,
            identity_public_key: decode_b64(&body.identity_public_key).map_err(ApiError)?,
            signed_prekey_id: body.signed_prekey_id,
            signed_prekey_public: decode_b64(&body.signed_prekey_public).map_err(ApiError)?,
            signed_prekey_signature: decode_b64(&body.signed_prekey_signature)
                .map_err(ApiError)?,
            one_time_prekeys: one_time,
        })
        .await
        .map_err(ApiError)?;

    Ok(Json(version))
}

/// Fetch prekey bundles for a user
///
/// Returns one bundle per active device and **consumes** the one-time prekey
/// it hands out. A bundle without `one_time_prekey_id` means that device has
/// run out and should top up.
#[utoipa::path(
    get, path = "/v1/keys/{user_id}", tag = "encryption", security(("bearer" = [])),
    params(("user_id" = Uuid, Path,)),
    responses(
        (status = 200, body = Vec<PreKeyBundleResponse>),
        (status = 404, description = "No published keys for that user", body = crate::error::ErrorResponse),
    )
)]
pub async fn bundles(
    State(state): State<ApiState>,
    _user: CurrentUser,
    Path(user_id): Path<Uuid>,
) -> ApiResult<Json<Vec<PreKeyBundleResponse>>> {
    let bundles = state
        .keys
        .bundles_for(UserId::from(user_id))
        .await
        .map_err(ApiError)?;

    Ok(Json(
        bundles.into_iter().map(PreKeyBundleResponse::from).collect(),
    ))
}

/// How many one-time prekeys remain
///
/// Poll this and upload more when `needs_top_up` is true.
#[utoipa::path(
    get, path = "/v1/keys/count", tag = "encryption", security(("bearer" = [])),
    responses((status = 200, body = PreKeyCountResponse))
)]
pub async fn count(
    State(state): State<ApiState>,
    user: CurrentUser,
) -> ApiResult<Json<PreKeyCountResponse>> {
    let (remaining, needs_top_up) = state
        .keys
        .remaining_prekeys(user.user_id, user.device_id)
        .await
        .map_err(ApiError)?;

    Ok(Json(PreKeyCountResponse {
        remaining,
        needs_top_up,
    }))
}
