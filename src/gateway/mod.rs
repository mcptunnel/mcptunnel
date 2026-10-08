pub mod bridge;
pub mod health;
pub mod inspection;
pub mod mcp_handler;
pub mod security;

use crate::daemon::state::AppState;
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Path, State},
    http::{Method, Request, StatusCode},
    response::{IntoResponse, Response},
    Router,
};
use std::sync::Arc;

#[derive(Clone)]
struct ListenerContext {
    state: Arc<AppState>,
    port: u16,
    public_only: bool,
}

async fn dispatch(State(context): State<ListenerContext>, request: Request<Body>) -> Response {
    let state = context.state;
    if let Err(response) =
        security::validate_boundary(&state, context.port, request.headers()).await
    {
        return response;
    }
    let path = request.uri().path();
    let mounts = match state.mount_registry.list() {
        Ok(mounts) => mounts,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let mounts: Vec<_> = mounts
        .into_iter()
        .filter(|m| {
            m.port == context.port
                && (!context.public_only || m.tunnel_mode == crate::config::TunnelMode::Quick)
        })
        .collect();
    if path == "/healthz" && request.method() == Method::GET {
        return axum::Json(serde_json::json!({"status":"ok", "mounts":mounts.len()}))
            .into_response();
    }
    let metadata_path = path.strip_prefix("/.well-known/oauth-protected-resource");
    if let Some(mount) = mounts
        .iter()
        .find(|m| metadata_path == Some(m.path.as_str()))
    {
        if request.method() != Method::GET {
            return StatusCode::METHOD_NOT_ALLOWED.into_response();
        }
        return mcp_handler::handle_oauth_metadata(State(state), Path(mount.slug.clone())).await;
    }
    if !context.public_only && request.method() == Method::GET {
        if let Some(mount) = mounts
            .iter()
            .find(|m| path == format!("/_mcptunnel/{}", m.slug))
        {
            return inspection::inspection_page(
                State(state),
                Path(mount.slug.clone()),
                request.headers().clone(),
            )
            .await;
        }
    }
    if let Some(mount) = mounts.iter().find(|m| path == m.path) {
        return mcp_handler::handle_mcp(State(state), Path(mount.slug.clone()), request).await;
    }
    StatusCode::NOT_FOUND.into_response()
}

pub fn build_router(state: Arc<AppState>) -> Router {
    let port = state.default_port();
    build_listener_router(state, port, false)
}

pub fn build_listener_router(state: Arc<AppState>, port: u16, public_only: bool) -> Router {
    Router::new()
        .fallback(dispatch)
        .layer(DefaultBodyLimit::max(mcp_handler::BODY_LIMIT))
        .with_state(ListenerContext {
            state,
            port,
            public_only,
        })
}
