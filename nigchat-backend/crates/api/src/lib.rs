//! # NigChat API
//!
//! HTTP and WebSocket delivery. Handlers do three things and no more:
//! deserialise, call a use case, serialise. Any `if` that expresses a product
//! rule belongs in `application` or `domain`, not here.

pub mod error;
pub mod extract;
pub mod openapi;
pub mod router;
pub mod routes;
pub mod state;
pub mod ws;

pub use router::{build_router, RouterConfig};
pub use state::ApiState;
