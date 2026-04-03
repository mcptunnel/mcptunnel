pub mod mcp_handler;
pub mod inspection;
pub mod health;

use axum::{
    Router,
    routing::{get, post, delete},
};
use std::sync::Arc;

use crate::daemon::state::AppState;

/// Build the HTTP gateway router.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Health check
        .route("/healthz", get(health::healthz))
        // Per-mount routes — use path parameters
        .route("/{slug}", get(inspection::inspection_page))
        .route("/{slug}/mcp", post(mcp_handler::handle_post_mcp))
        .route("/{slug}/mcp", get(mcp_handler::handle_get_mcp))
        .route("/{slug}/mcp", delete(mcp_handler::handle_delete_mcp))
        .route(
            "/{slug}/.well-known/oauth-protected-resource",
            get(mcp_handler::handle_oauth_metadata),
        )
        .with_state(state)
}
