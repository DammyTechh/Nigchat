//! Domain errors.
//!
//! These describe what went wrong in *business* terms. Mapping them to HTTP
//! status codes is the API layer's job — the domain does not know HTTP exists.

use thiserror::Error;

pub type DomainResult<T> = Result<T, DomainError>;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("{0}")]
    Validation(String),

    #[error("{entity} not found")]
    NotFound { entity: &'static str },

    #[error("{0}")]
    Conflict(String),

    /// The caller is authenticated but not allowed to do this.
    #[error("not permitted")]
    Forbidden,

    /// The caller is not authenticated, or the credential is bad.
    #[error("authentication failed")]
    Unauthenticated,

    #[error("verification code is incorrect or expired")]
    InvalidCredentials,

    #[error("too many attempts; retry in {retry_after_seconds}s")]
    RateLimited { retry_after_seconds: u64 },

    /// The recipient (or sender) has blocked the other party.
    #[error("blocked")]
    Blocked,

    /// A dependency failed. Carries no user-facing detail on purpose: the
    /// message is logged, not returned.
    #[error("infrastructure failure: {0}")]
    Infrastructure(String),
}

impl DomainError {
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub fn not_found(entity: &'static str) -> Self {
        Self::NotFound { entity }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    pub fn infrastructure(message: impl Into<String>) -> Self {
        Self::Infrastructure(message.into())
    }

    /// True when the caller retrying the exact same request could succeed.
    /// Used by clients to decide whether to back off or give up.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimited { .. } | Self::Infrastructure(_))
    }
}
