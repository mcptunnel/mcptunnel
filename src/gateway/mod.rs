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
    use crate::auth::oauth_server;

    Router::new()
        // Health check
        .route("/healthz", get(health::healthz))
        // Per-mount routes — use path parameters
        .route("/{slug}", get(inspection::inspection_page))
        .route("/{slug}/mcp", post(mcp_handler::handle_post_mcp))
        .route("/{slug}/mcp", get(mcp_handler::handle_get_mcp))
        .route("/{slug}/mcp", delete(mcp_handler::handle_delete_mcp))
        // Protected Resource Metadata (RFC 9728)
        .route(
            "/{slug}/.well-known/oauth-protected-resource",
            get(mcp_handler::handle_oauth_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource/{slug}",
            get(oauth_server::handle_prm_path_based),
        )
        // OAuth Authorization Server endpoints (built-in AS)
        .route(
            "/.well-known/oauth-authorization-server/{slug}",
            get(oauth_server::handle_as_metadata),
        )
        .route(
            "/{slug}/.well-known/openid-configuration",
            get(oauth_server::handle_oidc_discovery),
        )
        .route("/{slug}/oauth/register", post(oauth_server::handle_dcr))
        .route(
            "/{slug}/oauth/authorize",
            get(oauth_server::handle_authorize_get).post(oauth_server::handle_authorize_post),
        )
        .route("/{slug}/oauth/token", post(oauth_server::handle_token))
        .route("/{slug}/oauth/jwks", get(oauth_server::handle_jwks))
        .with_state(state)
}
