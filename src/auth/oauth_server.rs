//! Protected-resource discovery. Signing, registrations, CIMD/DCR and PKCE belong to the
//! configured external authorization server. The unsafe memory-only built-in AS is disabled.
use crate::{config::AuthModeConfig, daemon::state::AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use std::sync::Arc;

pub async fn handle_prm_path_based(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> (StatusCode, Json<Value>) {
    let mount = match state.mount_registry.get(&slug) {
        Ok(Some(m)) => m,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error":"mount not found"})),
            )
        }
    };
    let config = match mount.oauth_config {
        Some(c) if mount.auth_mode == AuthModeConfig::OAuth && !c.builtin => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error":"external OAuth not configured"})),
            )
        }
    };
    let oauth = crate::auth::types::OAuthConfig {
        issuer: config.issuer,
        authorization_servers: config.authorization_servers,
        default_scopes: config.default_scopes,
        audience: Some(state.public_url(&slug).await),
    };
    let metadata = crate::auth::oauth::build_protected_resource_metadata(
        &state.public_url(&slug).await,
        &oauth,
    );
    (
        StatusCode::OK,
        Json(serde_json::to_value(metadata).unwrap_or_default()),
    )
}
