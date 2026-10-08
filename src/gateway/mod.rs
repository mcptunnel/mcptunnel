pub mod bridge;
pub mod health;
pub mod inspection;
pub mod mcp_handler;
pub mod security;

use crate::daemon::state::AppState;
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::Request,
    middleware::{self, Next},
    response::Response,
    routing::{any, get},
    Router,
};
use std::sync::Arc;

async fn boundary(
    State(state): State<Arc<AppState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if let Err(response) = security::validate_boundary(&state, request.headers()).await {
        return response;
    }
    next.run(request).await
}

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/{slug}", get(inspection::inspection_page))
        .route("/{slug}/mcp", any(mcp_handler::handle_mcp))
        .route(
            "/{slug}/.well-known/oauth-protected-resource",
            get(mcp_handler::handle_oauth_metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource/{slug}",
            get(crate::auth::oauth_server::handle_prm_path_based),
        )
        .route(
            "/.well-known/oauth-protected-resource/{slug}/mcp",
            get(crate::auth::oauth_server::handle_prm_path_based),
        )
        .layer(DefaultBodyLimit::max(mcp_handler::BODY_LIMIT))
        .layer(middleware::from_fn_with_state(state.clone(), boundary))
        .with_state(state)
}
