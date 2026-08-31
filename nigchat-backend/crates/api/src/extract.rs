//! Request extractors.
//!
//! `CurrentUser` is the only way a handler learns who is calling. A handler
//! therefore cannot accidentally trust a user id from a request body, which is
//! the most common authorization bug in chat backends.

use axum::async_trait;
use axum::extract::FromRequestParts;
use axum::http::header::{AUTHORIZATION, USER_AGENT};
use axum::http::request::Parts;
use nigchat_domain::ids::{DeviceId, UserId};
use nigchat_domain::DomainError;

use crate::error::ApiError;
use crate::state::ApiState;

#[derive(Debug, Clone, Copy)]
pub struct CurrentUser {
    pub user_id: UserId,
    pub device_id: DeviceId,
}

#[async_trait]
impl FromRequestParts<ApiState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ApiState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(str::trim)
            .ok_or(ApiError(DomainError::Unauthenticated))?;

        let claims = state.auth.verify_access_token(token).map_err(ApiError)?;

        Ok(CurrentUser {
            user_id: claims.user_id,
            device_id: claims.device_id,
        })
    }
}

/// Client metadata for audit rows. Absent behind some proxies, so every field
/// is optional and nothing depends on it being present.
#[derive(Debug, Clone, Default)]
pub struct ClientContext {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

#[async_trait]
impl FromRequestParts<ApiState> for ClientContext {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ApiState,
    ) -> Result<Self, Self::Rejection> {
        // Forwarding headers are only honoured when the deployment says it sits
        // behind a proxy that sets them (TRUST_PROXY_HEADERS). On a directly
        // reachable server, trusting a client-supplied X-Forwarded-For would let
        // an attacker rotate their apparent IP at will and walk straight past
        // the per-IP OTP limit — the limit that stops SMS-pumping fraud.
        if !state.trust_proxy_headers {
            return Ok(ClientContext {
                ip: None,
                user_agent: user_agent(parts),
            });
        }

        let ip = parts
            .headers
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .map(|value| value.trim().to_string())
            .or_else(|| {
                parts
                    .headers
                    .get("x-real-ip")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string)
            });

        Ok(ClientContext {
            ip,
            user_agent: user_agent(parts),
        })
    }
}

/// Truncated: a header is attacker-controlled and must not be able to bloat an
/// audit row.
fn user_agent(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get(USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.chars().take(256).collect())
}
