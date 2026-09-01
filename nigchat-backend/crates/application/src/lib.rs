//! # NigChat application layer
//!
//! Use cases. Each type here answers one question of the form "what happens
//! when a user does X", and answers it by composing domain rules with the
//! ports declared in `nigchat-domain::ports`.
//!
//! Two rules hold throughout:
//!
//! 1. **Nothing here knows what a database or an HTTP request is.** Everything
//!    arrives through a port trait, so a use case can be tested with fakes.
//! 2. **Authorization happens here, before any write.** A repository trusts
//!    its caller; the use case is the caller that must not be trusted blindly.

pub mod auth;
pub mod calls;
pub mod conversations;
pub mod device_links;
pub mod devices;
pub mod keys;
pub mod media;
pub mod messaging;
pub mod notifications;
pub mod services;

pub use services::Services;
