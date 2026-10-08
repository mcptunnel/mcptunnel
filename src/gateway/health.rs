use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;
use std::sync::Arc;

use crate::daemon::state::AppState;

/// GET /healthz — basic daemon and tunnel health.
pub async fn healthz(State(state): State<Arc<AppState>>) -> (StatusCode, Json<serde_json::Value>) {
    let mounts = state.mount_registry.list().unwrap_or_default();
    let tunnel_origin = state.tunnel_origins.read().await.clone();

    let status = "ok";

    (
        StatusCode::OK,
        Json(json!({
            "status": status,
            "mounts": mounts.len(),
            "tunnel_origin": tunnel_origin,
        })),
    )
}
