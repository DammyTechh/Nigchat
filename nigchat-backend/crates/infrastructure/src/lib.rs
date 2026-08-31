//! # NigChat infrastructure
//!
//! Concrete adapters for the domain ports. Nothing outside this crate names
//! PostgreSQL, Redis, FCM or APNs.

pub mod crypto;
pub mod postgres;
pub mod push;
pub mod realtime;
pub mod sms;

pub use crypto::{Argon2Hasher, JwtTokenService, SystemClock};
pub use postgres::PostgresRepositories;
pub use realtime::{RedisEventPublisher, RedisPresence, RedisRateLimiter};

use nigchat_domain::DomainError;

/// Database failures become `Infrastructure` errors, which the API layer maps
/// to a 500 with no detail leaked to the caller. Two exceptions are translated
/// into meaningful domain errors so callers can react correctly.
pub(crate) fn map_sqlx(err: sqlx::Error) -> DomainError {
    match &err {
        sqlx::Error::RowNotFound => DomainError::not_found("record"),
        sqlx::Error::Database(db) if db.code().as_deref() == Some("23505") => {
            DomainError::conflict("that value is already taken")
        }
        _ => {
            tracing::error!(?err, "database error");
            DomainError::infrastructure("database error")
        }
    }
}

pub(crate) fn map_redis(err: redis::RedisError) -> DomainError {
    tracing::error!(?err, "redis error");
    DomainError::infrastructure("cache error")
}
