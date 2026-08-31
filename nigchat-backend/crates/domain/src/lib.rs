//! # NigChat domain
//!
//! Entities, value objects, domain errors and **ports** — the traits that the
//! outside world must implement to be usable by the application layer.
//!
//! Nothing here talks to a database, an HTTP client or a message broker. That
//! is deliberate: every rule in this crate can be unit-tested with no fixtures,
//! no containers and no network.

pub mod entities;
pub mod error;
pub mod events;
pub mod ids;
pub mod notifications;
pub mod ports;
pub mod values;

pub use error::{DomainError, DomainResult};
