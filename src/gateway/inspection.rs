use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use crate::daemon::state::AppState;

/// GET /<slug> — human-readable inspection page.
pub async fn inspection_page(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    let mount = match state.mount_registry.get(&slug) {
        Ok(Some(m)) => m,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "text/html")],
                "<h1>404 — Mount not found</h1>".to_string(),
            )
                .into_response();
        }
    };
    if let Err(response) = super::security::authorize(&state, &mount, &headers).await {
        return response;
    }

    let public_url = escape(
        &state
            .connection_url(&slug)
            .await
            .unwrap_or_else(|| "unavailable; check tunnel status".into()),
    );
    let active_sessions = state.session_manager.active_count(&slug).await;

    let probe_info = mount.last_probe_result.as_ref().map(|p| {
        format!(
            "<li><strong>Server:</strong> {} (v{})</li>\n<li><strong>Protocol:</strong> {}</li>\n<li><strong>Tools:</strong> {}</li>",
            escape(p.server_name.as_deref().unwrap_or("unknown")),
            escape(p.server_version.as_deref().unwrap_or("?")),
            escape(p.protocol_version.as_deref().unwrap_or("?")),
            escape(&p.tools.join(", "))
        )
    }).unwrap_or_else(|| "<li>No probe data available</li>".to_string());

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head><title>mcptunnel — {slug}</title>
<style>
  body {{ font-family: system-ui, sans-serif; max-width: 600px; margin: 40px auto; padding: 0 20px; }}
  h1 {{ color: #333; }}
  .url {{ background: #f0f0f0; padding: 12px; border-radius: 4px; font-family: monospace; word-break: break-all; }}
  .warn {{ background: #fff3cd; border: 1px solid #ffc107; padding: 12px; border-radius: 4px; margin: 16px 0; }}
  ul {{ list-style: none; padding: 0; }}
  li {{ padding: 4px 0; }}
</style>
</head>
<body>
<h1>mcptunnel — {slug}</h1>
<p class="url">{public_url}</p>
<ul>
  <li><strong>Display name:</strong> {display_name}</li>
  <li><strong>Auth mode:</strong> {auth_mode}</li>
  <li><strong>Tunnel mode:</strong> {tunnel_mode}</li>
  <li><strong>Active sessions:</strong> {active_sessions}</li>
  <li><strong>Status:</strong> {status}</li>
  {probe_info}
</ul>
{warn_html}
</body>
</html>"#,
        slug = mount.slug,
        display_name = escape(&mount.display_name),
        auth_mode = mount.auth_mode,
        tunnel_mode = mount.tunnel_mode,
        status = mount.status,
        warn_html = if mount.auth_mode == crate::config::AuthModeConfig::Public {
            "<div class=\"warn\"><strong>Warning:</strong> This endpoint is publicly reachable with no authentication.</div>"
        } else {
            ""
        },
    );

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/html"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; style-src 'unsafe-inline'; frame-ancestors 'none'",
            ),
            (header::CACHE_CONTROL, "no-store"),
        ],
        html,
    )
        .into_response()
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
