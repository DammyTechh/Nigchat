//! Liveness and readiness.
//!
//! The distinction matters to the orchestrator:
//!   * `/healthz` — the process is alive. Touches no dependency, so a Redis
//!     blip does not make Kubernetes restart every pod at once.
//!   * `/readyz` — this instance can serve. Checks dependencies, so a sick
//!     instance leaves the load-balancer pool without being killed.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;

use crate::state::ApiState;

#[utoipa::path(
    get,
    path = "/healthz",
    tag = "system",
    responses((status = 200, description = "Process is alive"))
)]
pub async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "service": "nigchat-api" }))
}

#[utoipa::path(
    get,
    path = "/readyz",
    tag = "system",
    responses(
        (status = 200, description = "Ready to serve traffic"),
        (status = 503, description = "A dependency is unavailable"),
    )
)]
pub async fn readyz(State(state): State<ApiState>) -> (StatusCode, Json<serde_json::Value>) {
    let database_ok = state.services.users.find_by_id(nigchat_domain::ids::UserId(
        uuid::Uuid::nil(),
    ))
    .await
    .is_ok();

    let redis_ok = state
        .services
        .presence
        .is_online(nigchat_domain::ids::UserId(uuid::Uuid::nil()))
        .await
        .is_ok();

    let ready = database_ok && redis_ok;

    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(json!({
            "ready": ready,
            "postgres": database_ok,
            "redis": redis_ok,
            "local_websockets": state.hub.local_connection_count().await,
        })),
    )
}
