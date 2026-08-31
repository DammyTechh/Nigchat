//! Domain errors → HTTP.
//!
//! Two invariants:
//!   1. Every client-visible error carries a stable machine-readable `code`.
//!      Clients branch on the code, never on the message text.
//!   2. Infrastructure failures are logged server-side and returned as a
//!      generic 500. A database error message must never reach a caller — it
//!      leaks schema details and sometimes data.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use nigchat_domain::DomainError;
use serde::Serialize;
use utoipa::ToSchema;

pub struct ApiError(pub DomainError);

impl From<DomainError> for ApiError {
    fn from(err: DomainError) -> Self {
        Self(err)
    }
}

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorDetail {
    /// Stable identifier, e.g. `rate_limited`. Branch on this.
    #[schema(example = "forbidden")]
    pub code: String,
    #[schema(example = "not permitted")]
    pub message: String,
    /// Present only on `rate_limited`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u64>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        use DomainError::*;

        let (status, code, message, retry_after) = match &self.0 {
            Validation(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg.clone(), None),
            NotFound { entity } => (
                StatusCode::NOT_FOUND,
                "not_found",
                format!("{entity} not found"),
                None,
            ),
            Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg.clone(), None),
            Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "not permitted".into(),
                None,
            ),
            Unauthenticated => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "authentication required".into(),
                None,
            ),
            InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "verification code is incorrect or expired".into(),
                None,
            ),
            RateLimited {
                retry_after_seconds,
            } => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "too many requests".into(),
                Some(*retry_after_seconds),
            ),
            Blocked => (
                StatusCode::FORBIDDEN,
                "blocked",
                "message cannot be delivered".into(),
                None,
            ),
            Infrastructure(detail) => {
                tracing::error!(detail, "infrastructure failure");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "an unexpected error occurred".into(),
                    None,
                )
            }
        };

        let mut response = (
            status,
            Json(ErrorResponse {
                error: ErrorDetail {
                    code: code.to_string(),
                    message,
                    retry_after_seconds: retry_after,
                },
            }),
        )
            .into_response();

        // Clients and proxies both honour Retry-After; sending it prevents a
        // rate-limited client from hammering the endpoint further.
        if let Some(seconds) = retry_after {
            if let Ok(value) = seconds.to_string().parse() {
                response.headers_mut().insert("retry-after", value);
            }
        }

        response
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
