use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{info, warn};

use crate::config::{AuthModeConfig, TunnelMode};
use crate::daemon::state::AppState;
use crate::logging::hash_session_id;
use crate::policy::filter;

/// POST /<slug>/mcp — main JSON-RPC entrypoint.
pub async fn handle_post_mcp(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, HeaderMap, Json<Value>) {
    let mut resp_headers = HeaderMap::new();
    let start = std::time::Instant::now();

    // Look up mount
    let mount = match state.mount_registry.get(&slug) {
        Ok(Some(m)) => m,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                resp_headers,
                Json(json_rpc_error(body.get("id").cloned(), -32001, "mount not found")),
            );
        }
    };

    // Handle batch requests
    if let Some(batch) = body.as_array() {
        let mut results = Vec::new();
        for item in batch {
            let result = handle_single_request(&state, &slug, &mount, &headers, item, &mut resp_headers).await;
            results.push(result);
        }
        let duration = start.elapsed();
        info!(slug = %slug, duration_ms = duration.as_millis(), batch_size = results.len(), "Batch request completed");
        return (StatusCode::OK, resp_headers, Json(Value::Array(results)));
    }

    // Single request
    let result = handle_single_request(&state, &slug, &mount, &headers, &body, &mut resp_headers).await;
    let duration = start.elapsed();
    let method = body.get("method").and_then(|m| m.as_str()).unwrap_or("unknown");
    info!(
        slug = %slug,
        method = method,
        duration_ms = duration.as_millis(),
        "Request completed"
    );

    (StatusCode::OK, resp_headers, Json(result))
}

/// Handle a single JSON-RPC request.
async fn handle_single_request(
    state: &Arc<AppState>,
    slug: &str,
    mount: &crate::mount::types::MountRecord,
    headers: &HeaderMap,
    request: &Value,
    resp_headers: &mut HeaderMap,
) -> Value {
    let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = request.get("id").cloned();
    let params = request.get("params").cloned();

    match method {
        "initialize" => handle_initialize(state, slug, mount, id, params, resp_headers).await,
        "notifications/initialized" => {
            // No-op notification, just acknowledge
            Value::Null
        }
        "tools/list" => handle_tools_list(state, slug, mount, headers, id).await,
        "tools/call" => handle_tools_call(state, slug, mount, headers, id, params).await,
        "resources/list" => handle_resources_list(state, slug, mount, id).await,
        "resources/read" => handle_resources_read(state, slug, mount, headers, id, params).await,
        "prompts/list" => handle_prompts_list(state, slug, mount, id).await,
        "prompts/get" => handle_prompts_get(state, slug, mount, headers, id, params).await,
        "ping" => json_rpc_result(id, json!({})),
        _ => {
            // Check for notifications (no id = notification)
            if id.is_none() || id.as_ref() == Some(&Value::Null) {
                Value::Null // Notifications don't get responses
            } else {
                json_rpc_error(id, -32601, &format!("method not found: {method}"))
            }
        }
    }
}

/// Handle initialize request — create session and upstream child.
async fn handle_initialize(
    state: &Arc<AppState>,
    slug: &str,
    mount: &crate::mount::types::MountRecord,
    id: Option<Value>,
    _params: Option<Value>,
    resp_headers: &mut HeaderMap,
) -> Value {
    // Create session
    let session = match state.session_manager.create_session(slug).await {
        Ok(s) => s,
        Err(e) => return json_rpc_error(id, -32000, &format!("session creation failed: {e}")),
    };

    // Spawn upstream child
    let child = match crate::upstream::child::UpstreamChild::spawn(
        &mount.command,
        mount.cwd.as_deref(),
        &mount.env_vars,
    ).await {
        Ok(c) => c,
        Err(e) => {
            state.session_manager.delete_session(&session.session_id).await;
            return json_rpc_error(id, -32000, &format!("upstream spawn failed: {e}"));
        }
    };

    // Get server info from child
    let peer_info = child.peer().peer_info();
    let server_info = peer_info.map(|i| {
        json!({
            "name": &i.server_info.name,
            "version": &i.server_info.version,
        })
    });

    let capabilities = build_downstream_capabilities(mount);
    let protocol_version = peer_info
        .map(|i| i.protocol_version.as_str().to_string())
        .unwrap_or_else(|| "2025-03-26".to_string());

    // Store child in state
    state.upstream_children.write().await.insert(
        session.session_id.clone(),
        child,
    );

    // Set session ID header
    resp_headers.insert(
        "mcp-session-id",
        session.session_id.parse().unwrap(),
    );

    info!(
        slug = %slug,
        session_id_hash = %hash_session_id(&session.session_id),
        "Session created"
    );

    json_rpc_result(id, json!({
        "protocolVersion": protocol_version,
        "capabilities": capabilities,
        "serverInfo": server_info.unwrap_or(json!({
            "name": &mount.display_name,
            "version": "0.1.0"
        })),
    }))
}

/// Build downstream server capabilities based on mount config and mode.
fn build_downstream_capabilities(mount: &crate::mount::types::MountRecord) -> Value {
    let mut caps = json!({});
    if let Some(obj) = caps.as_object_mut() {
        // Always advertise tools if policy allows any
        obj.insert("tools".to_string(), json!({}));

        if mount.policy.expose_resources {
            obj.insert("resources".to_string(), json!({}));
        }
        if mount.policy.expose_prompts {
            obj.insert("prompts".to_string(), json!({}));
        }
    }
    caps
}

/// Handle tools/list — filter by policy + overlay auth metadata.
async fn handle_tools_list(
    state: &Arc<AppState>,
    slug: &str,
    mount: &crate::mount::types::MountRecord,
    headers: &HeaderMap,
    id: Option<Value>,
) -> Value {
    let session_id = get_session_id(headers);
    let children = state.upstream_children.read().await;

    let child = match session_id.as_deref().and_then(|sid| children.get(sid)) {
        Some(c) => c,
        None => return json_rpc_error(id, -32001, "session not found or expired"),
    };

    // Touch session activity
    if let Some(sid) = &session_id {
        state.session_manager.touch_session(sid).await;
    }

    match child.send_raw_request("tools/list", None).await {
        Ok(result) => {
            let mut tools = result.get("tools")
                .and_then(|t| t.as_array())
                .cloned()
                .unwrap_or_default();

            // Apply policy filter
            tools = filter::filter_tools(&mount.policy, tools);

            // Apply auth metadata overlay
            if mount.auth_mode == AuthModeConfig::Public {
                crate::auth::public::inject_noauth_scheme(&mut tools);
            }

            json_rpc_result(id, json!({ "tools": tools }))
        }
        Err(e) => json_rpc_error(id, -32000, &e.to_string()),
    }
}

/// Handle tools/call — check policy + forward.
async fn handle_tools_call(
    state: &Arc<AppState>,
    slug: &str,
    mount: &crate::mount::types::MountRecord,
    headers: &HeaderMap,
    id: Option<Value>,
    params: Option<Value>,
) -> Value {
    let session_id = get_session_id(headers);
    let children = state.upstream_children.read().await;

    let child = match session_id.as_deref().and_then(|sid| children.get(sid)) {
        Some(c) => c,
        None => return json_rpc_error(id, -32001, "session not found or expired"),
    };

    if let Some(sid) = &session_id {
        state.session_manager.touch_session(sid).await;
    }

    // Check policy
    if let Some(ref p) = params {
        if let Some(tool_name) = p.get("name").and_then(|n| n.as_str()) {
            if !filter::is_tool_allowed(&mount.policy, tool_name) {
                warn!(slug = %slug, tool = tool_name, "Policy blocked tool call");
                return json_rpc_error(id, filter::POLICY_VIOLATION_CODE, &format!(
                    "tool '{tool_name}' is not exposed by policy"
                ));
            }
        }
    }

    match child.send_raw_request("tools/call", params).await {
        Ok(result) => json_rpc_result(id, result),
        Err(e) => json_rpc_error(id, -32000, &e.to_string()),
    }
}

/// Handle resources/list — filter by policy.
async fn handle_resources_list(
    state: &Arc<AppState>,
    slug: &str,
    mount: &crate::mount::types::MountRecord,
    id: Option<Value>,
) -> Value {
    if !mount.policy.expose_resources {
        return json_rpc_result(id, json!({ "resources": [] }));
    }
    // For resources/list without a session, return from probe data if available
    if let Some(ref probe) = mount.last_probe_result {
        let resources: Vec<Value> = probe.resources.iter()
            .map(|r| json!({"uri": r, "name": r}))
            .collect();
        let filtered = filter::filter_resources(&mount.policy, resources);
        return json_rpc_result(id, json!({ "resources": filtered }));
    }
    json_rpc_result(id, json!({ "resources": [] }))
}

/// Handle resources/read — check policy + forward.
async fn handle_resources_read(
    state: &Arc<AppState>,
    slug: &str,
    mount: &crate::mount::types::MountRecord,
    headers: &HeaderMap,
    id: Option<Value>,
    params: Option<Value>,
) -> Value {
    let session_id = get_session_id(headers);
    let children = state.upstream_children.read().await;

    let child = match session_id.as_deref().and_then(|sid| children.get(sid)) {
        Some(c) => c,
        None => return json_rpc_error(id, -32001, "session not found or expired"),
    };

    if let Some(sid) = &session_id {
        state.session_manager.touch_session(sid).await;
    }

    match child.send_raw_request("resources/read", params).await {
        Ok(result) => json_rpc_result(id, result),
        Err(e) => json_rpc_error(id, -32000, &e.to_string()),
    }
}

/// Handle prompts/list — filter by policy.
async fn handle_prompts_list(
    state: &Arc<AppState>,
    slug: &str,
    mount: &crate::mount::types::MountRecord,
    id: Option<Value>,
) -> Value {
    if !mount.policy.expose_prompts {
        return json_rpc_result(id, json!({ "prompts": [] }));
    }
    if let Some(ref probe) = mount.last_probe_result {
        let prompts: Vec<Value> = probe.prompts.iter()
            .map(|p| json!({"name": p}))
            .collect();
        let filtered = filter::filter_prompts(&mount.policy, prompts);
        return json_rpc_result(id, json!({ "prompts": filtered }));
    }
    json_rpc_result(id, json!({ "prompts": [] }))
}

/// Handle prompts/get — forward to upstream.
async fn handle_prompts_get(
    state: &Arc<AppState>,
    slug: &str,
    mount: &crate::mount::types::MountRecord,
    headers: &HeaderMap,
    id: Option<Value>,
    params: Option<Value>,
) -> Value {
    let session_id = get_session_id(headers);
    let children = state.upstream_children.read().await;

    let child = match session_id.as_deref().and_then(|sid| children.get(sid)) {
        Some(c) => c,
        None => return json_rpc_error(id, -32001, "session not found or expired"),
    };

    if let Some(sid) = &session_id {
        state.session_manager.touch_session(sid).await;
    }

    match child.send_raw_request("prompts/get", params).await {
        Ok(result) => json_rpc_result(id, result),
        Err(e) => json_rpc_error(id, -32000, &e.to_string()),
    }
}

/// GET /<slug>/mcp — SSE stream (managed mode) or 405 (quick mode).
pub async fn handle_get_mcp(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> (StatusCode, Json<Value>) {
    let mount = match state.mount_registry.get(&slug) {
        Ok(Some(m)) => m,
        _ => return (StatusCode::NOT_FOUND, Json(json!({"error": "mount not found"}))),
    };

    if mount.tunnel_mode == TunnelMode::Quick {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            Json(json!({"error": "GET SSE not supported in quick mode"})),
        );
    }

    // Managed mode: SSE would be implemented here
    // For MVP, return 501 to indicate it's planned but not yet implemented
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({"error": "SSE streaming not yet implemented for managed mode"})),
    )
}

/// DELETE /<slug>/mcp — terminate session.
pub async fn handle_delete_mcp(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    let session_id = match get_session_id(&headers) {
        Some(sid) => sid,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "MCP-Session-Id header required"})),
            );
        }
    };

    // Remove upstream child
    let child = state.upstream_children.write().await.remove(&session_id);
    if let Some(child) = child {
        child.shutdown().await;
    }

    // Delete session
    match state.session_manager.delete_session(&session_id).await {
        Some(_) => {
            info!(
                slug = %slug,
                session_id_hash = %hash_session_id(&session_id),
                "Session deleted"
            );
            (StatusCode::OK, Json(json!({"deleted": true})))
        }
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "session not found"}))),
    }
}

/// GET /<slug>/.well-known/oauth-protected-resource
pub async fn handle_oauth_metadata(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> (StatusCode, Json<Value>) {
    let mount = match state.mount_registry.get(&slug) {
        Ok(Some(m)) => m,
        _ => return (StatusCode::NOT_FOUND, Json(json!({"error": "mount not found"}))),
    };

    if mount.auth_mode != AuthModeConfig::OAuth {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "OAuth not configured for this mount"})));
    }

    let oauth = match mount.oauth_config {
        Some(ref c) => c,
        None => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "OAuth config missing"}))),
    };

    let tunnel_origin = state.tunnel_origin.read().await.clone()
        .unwrap_or_else(|| "https://localhost".to_string());

    let resource_url = format!("{}/{}", tunnel_origin, slug);

    let metadata = crate::auth::oauth::build_protected_resource_metadata(
        &resource_url,
        &crate::auth::types::OAuthConfig {
            issuer: oauth.issuer.clone(),
            authorization_servers: oauth.authorization_servers.clone(),
            default_scopes: oauth.default_scopes.clone(),
            audience: None,
        },
    );

    (StatusCode::OK, Json(serde_json::to_value(metadata).unwrap_or_default()))
}

/// Extract MCP-Session-Id from headers.
fn get_session_id(headers: &HeaderMap) -> Option<String> {
    headers.get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Build a JSON-RPC success response.
fn json_rpc_result(id: Option<Value>, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result,
    })
}

/// Build a JSON-RPC error response.
fn json_rpc_error(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": {
            "code": code,
            "message": message,
        },
    })
}
