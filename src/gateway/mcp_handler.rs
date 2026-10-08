use crate::{daemon::state::AppState, gateway::bridge::Bridge, upstream::child::UpstreamChild};
use axum::{
    body::{to_bytes, Body},
    extract::{Path, State},
    http::{Method, Request, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use futures::StreamExt;
use rmcp::transport::streamable_http_server::{
    session::never::NeverSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

pub const BODY_LIMIT: usize = 1024 * 1024;
pub const SUPPORTED: [&str; 4] = ["2026-07-28", "2025-11-25", "2025-06-18", "2025-03-26"];

pub fn rpc_error(status: StatusCode, id: Value, code: i64, message: &str) -> Response {
    (
        status,
        Json(json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}})),
    )
        .into_response()
}

/// Authentication and application-level legacy sessions wrap the SDK's stateless transport.
/// Modern requests never allocate a protocol session, even if a client sends an old session ID.
pub async fn handle_mcp(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request<Body>,
) -> Response {
    let mount = match state.mount_registry.get(&slug) {
        Ok(Some(m)) => m,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    if mount.tunnel_mode == crate::config::TunnelMode::Managed {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "managed provisioning is not implemented",
        )
            .into_response();
    }
    let Ok(_global_permit) = state.request_permits.clone().try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let principal =
        match crate::gateway::security::authorize(&state, &mount, request.headers()).await {
            Ok(p) => p,
            Err(r) => return r,
        };
    if request.method() == Method::GET {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            [("allow", "POST, DELETE")],
            "SSE subscriptions are not implemented; Quick Tunnels do not support SSE",
        )
            .into_response();
    }
    let header_version = request
        .headers()
        .get("mcp-protocol-version")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let sid = request
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    if request.method() == Method::DELETE {
        if header_version.as_deref() == Some(SUPPORTED[0]) {
            return StatusCode::METHOD_NOT_ALLOWED.into_response();
        }
        let Some(sid) = sid else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        if !owns_session(&state, &sid, &slug, &principal).await {
            return StatusCode::NOT_FOUND.into_response();
        }
        close_session(&state, &sid).await;
        return StatusCode::NO_CONTENT.into_response();
    }
    if request.method() != Method::POST {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let gate = state
        .mount_permits
        .write()
        .await
        .entry(slug.clone())
        .or_insert_with(|| {
            Arc::new(tokio::sync::Semaphore::new(
                state.config.defaults.max_inflight_per_session,
            ))
        })
        .clone();
    let Ok(_mount_permit) = gate.try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let (parts, body) = request.into_parts();
    let bytes = match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        to_bytes(body, BODY_LIMIT),
    )
    .await
    {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(_)) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
        Err(_) => return StatusCode::REQUEST_TIMEOUT.into_response(),
    };
    let body: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return rpc_error(StatusCode::BAD_REQUEST, Value::Null, -32700, "parse error"),
    };
    let id = body.get("id").cloned().unwrap_or(Value::Null);
    if !body.is_object()
        || body["jsonrpc"] != "2.0"
        || !body["method"].is_string()
        || body
            .get("id")
            .is_some_and(|v| !v.is_string() && !v.is_i64() && !v.is_u64())
    {
        return rpc_error(
            StatusCode::BAD_REQUEST,
            id,
            -32600,
            "invalid JSON-RPC message; batching is unsupported",
        );
    }
    let method = body["method"].as_str().unwrap_or_default();
    if method == "tools/call" {
        if let Some(required) = mount.oauth_config.as_ref().and_then(|c| {
            body["params"]["name"]
                .as_str()
                .and_then(|name| c.per_tool_scopes.get(name))
        }) {
            let mut scoped_mount = mount.clone();
            scoped_mount
                .oauth_config
                .as_mut()
                .unwrap()
                .default_scopes
                .extend(required.iter().cloned());
            if let Err(response) =
                crate::gateway::security::authorize(&state, &scoped_mount, &parts.headers).await
            {
                return response;
            }
        }
    }
    let meta_version = body
        .pointer("/params/_meta/io.modelcontextprotocol~1protocolVersion")
        .and_then(Value::as_str);
    let modern =
        meta_version.is_some() || header_version.as_deref().is_some_and(|v| v >= SUPPORTED[0]);
    // rmcp's HTTP validator rejects unknown version headers as HeaderMismatch before dispatch.
    // Preserve its consistency checks, but report a consistent unknown declaration using the
    // specification's dedicated error so clients can select a supported revision.
    if let Some(version) = header_version.as_deref() {
        if !SUPPORTED.contains(&version) && meta_version == Some(version) {
            return (StatusCode::BAD_REQUEST, Json(json!({"jsonrpc":"2.0","id":id,"error":{"code":-32022,"message":"Unsupported protocol version","data":{"supported":SUPPORTED,"requested":version}}}))).into_response();
        }
    }
    let initialize = method == "initialize";
    let mut _session_permit = None;
    if !modern && !initialize {
        let Some(ref sid) = sid else {
            return rpc_error(
                StatusCode::BAD_REQUEST,
                id,
                -32001,
                "legacy request requires initialization and Mcp-Session-Id",
            );
        };
        let Some(session) = state
            .session_manager
            .get_session(sid)
            .await
            .filter(|s| s.mount_slug == slug && s.principal == principal)
        else {
            return StatusCode::NOT_FOUND.into_response();
        };
        if header_version
            .as_deref()
            .is_some_and(|v| v != session.protocol_version)
        {
            return rpc_error(
                StatusCode::BAD_REQUEST,
                id,
                -32022,
                "protocol version differs from legacy session",
            );
        }
        if let Some(gate) = session.permits {
            _session_permit = match gate.try_acquire_owned() {
                Ok(p) => Some(p),
                Err(_) => return StatusCode::TOO_MANY_REQUESTS.into_response(),
            };
        }
        state.session_manager.touch_session(sid).await;
    }
    // SDK validates per-request metadata, standard/custom headers, media types, notification
    // semantics, version errors, resultType/cache hints, and request disconnect cancellation.
    let mount_ct = state
        .mount_cancellation
        .write()
        .await
        .entry(slug.clone())
        .or_insert_with(|| state.cancellation.child_token())
        .clone();
    let sdk_config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_sse_keep_alive(None)
        .with_allowed_hosts(vec![parts
            .headers
            .get("host")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("127.0.0.1")
            .to_owned()])
        .with_max_request_body_bytes(BODY_LIMIT)
        .with_cancellation_token(mount_ct);
    let bridge_state = state.clone();
    let bridge_mount = mount.clone();
    let service = StreamableHttpService::new(
        move || {
            Ok(Bridge {
                state: bridge_state.clone(),
                mount: bridge_mount.clone(),
            })
        },
        Arc::new(NeverSessionManager::default()),
        sdk_config,
    );
    let sdk_request = Request::from_parts(parts, Body::from(bytes));
    let response = match service.oneshot(sdk_request).await {
        Ok(r) => r,
        Err(never) => match never {},
    };
    let mut response = response.map(Body::new);
    if initialize && !modern && response.status().is_success() {
        let response_bytes = match to_bytes(response.into_body(), BODY_LIMIT).await {
            Ok(b) => b,
            Err(_) => return StatusCode::BAD_GATEWAY.into_response(),
        };
        let reply: Value = serde_json::from_slice(&response_bytes).unwrap_or(Value::Null);
        if reply.get("result").is_none() {
            return (StatusCode::OK, Json(reply)).into_response();
        }
        let session = match state.session_manager.create_session(&slug).await {
            Ok(s) => s,
            Err(_) => return StatusCode::TOO_MANY_REQUESTS.into_response(),
        };
        let mut reservation = SessionReservation {
            state: state.clone(),
            id: Some(session.session_id.clone()),
        };
        let version = reply["result"]["protocolVersion"]
            .as_str()
            .unwrap_or("2025-03-26")
            .to_owned();
        state
            .session_manager
            .bind(&session.session_id, principal, version)
            .await;
        let child =
            match UpstreamChild::spawn(&mount.command, mount.cwd.as_deref(), &mount.env_vars).await
            {
                Ok(c) => c,
                Err(_) => {
                    close_session(&state, &session.session_id).await;
                    return rpc_error(
                        StatusCode::BAD_GATEWAY,
                        id,
                        -32603,
                        "upstream initialization failed; run doctor",
                    );
                }
            };
        state
            .upstream_children
            .write()
            .await
            .insert(session.session_id.clone(), Arc::new(child));
        reservation.id = None;
        response = Json(reply).into_response();
        response
            .headers_mut()
            .insert("mcp-session-id", session.session_id.parse().unwrap());
    }
    response
        .headers_mut()
        .insert("cache-control", "no-store".parse().unwrap());
    state
        .append_log(
            &slug,
            format!("method={method} status={}", response.status().as_u16()),
        )
        .await;
    // Hold admission permits until the body ends or the client disconnects, including SSE.
    let (response_parts, body) = response.into_parts();
    let stream = futures::stream::unfold(
        (
            body.into_data_stream(),
            (_global_permit, _mount_permit, _session_permit),
        ),
        |(mut stream, guards)| async move { stream.next().await.map(|chunk| (chunk, (stream, guards))) },
    );
    Response::from_parts(response_parts, Body::from_stream(stream))
}

struct SessionReservation {
    state: Arc<AppState>,
    id: Option<String>,
}
impl Drop for SessionReservation {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            let state = self.state.clone();
            tokio::spawn(async move {
                close_session(&state, &id).await;
            });
        }
    }
}

async fn owns_session(state: &AppState, sid: &str, slug: &str, principal: &str) -> bool {
    state
        .session_manager
        .get_session(sid)
        .await
        .is_some_and(|s| s.mount_slug == slug && s.principal == principal)
}

pub async fn close_session(state: &AppState, sid: &str) {
    state.session_manager.delete_session(sid).await;
    let child = state.upstream_children.write().await.remove(sid);
    if let Some(child) = child {
        child.cancel();
        if let Ok(child) = Arc::try_unwrap(child) {
            child.shutdown().await;
        }
    }
}

pub async fn handle_oauth_metadata(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> Response {
    crate::auth::oauth_server::handle_prm_path_based(State(state), Path(slug))
        .await
        .into_response()
}
