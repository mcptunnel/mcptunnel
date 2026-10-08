use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::config::AuthModeConfig;
use crate::daemon::state::AppState;
use crate::mount::types::*;

/// Admin RPC request.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum AdminRequest {
    MountCreate(Box<MountCreateRequest>),
    MountDelete { slug: String },
    MountList,
    MountInspect { slug: String },
    MountLogsTail { slug: String, lines: Option<usize> },
    DaemonStatus,
    DaemonShutdown,
    TunnelStatus,
    DoctorRun { slug: Option<String> },
}

/// Admin RPC response.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum AdminResponse {
    #[serde(rename = "ok")]
    Ok { data: serde_json::Value },
    #[serde(rename = "error")]
    Error { message: String, code: i32 },
}

/// Request to create a mount (simplified for IPC).
#[derive(Debug, Serialize, Deserialize)]
pub struct MountCreateRequest {
    pub slug: Option<String>,
    pub port: Option<u16>,
    pub path: Option<String>,
    pub display_name: Option<String>,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub tunnel_mode: Option<String>,
    pub hostname: Option<String>,
    pub auth_mode: Option<String>,
    pub oidc_issuer: Option<String>,
    pub scopes: Vec<String>,
    pub allow_tools: Vec<String>,
    pub deny_tools: Vec<String>,
    pub hide_resources: bool,
    pub hide_prompts: bool,
    pub env_vars: Vec<(String, String)>,
    pub probe_mode: Option<String>,
}

/// Handle an admin RPC request.
pub async fn handle_admin_request(state: &Arc<AppState>, request: AdminRequest) -> AdminResponse {
    match request {
        AdminRequest::MountCreate(req) => handle_mount_create(state, *req).await,
        AdminRequest::MountDelete { slug } => handle_mount_delete(state, &slug).await,
        AdminRequest::MountList => handle_mount_list(state).await,
        AdminRequest::MountInspect { slug } => handle_mount_inspect(state, &slug).await,
        AdminRequest::MountLogsTail { slug, lines } => {
            handle_mount_logs(state, &slug, lines.unwrap_or(50)).await
        }
        AdminRequest::DaemonStatus => handle_daemon_status(state).await,
        AdminRequest::DaemonShutdown => handle_daemon_shutdown(state).await,
        AdminRequest::TunnelStatus => handle_tunnel_status(state).await,
        AdminRequest::DoctorRun { slug } => handle_doctor_run(state, slug.as_deref()).await,
    }
}

async fn handle_mount_create(state: &Arc<AppState>, req: MountCreateRequest) -> AdminResponse {
    let _registration = state.registration.lock().await;
    let tunnel_mode = match req.tunnel_mode.as_deref() {
        Some("managed") => crate::config::TunnelMode::Managed,
        Some("quick") => crate::config::TunnelMode::Quick,
        Some("local") => crate::config::TunnelMode::Local,
        _ => crate::config::TunnelMode::Local,
    };
    if let Err(message) = crate::tunnel::validate_selection(tunnel_mode, req.hostname.as_deref()) {
        return AdminResponse::Error { message, code: 2 };
    }
    let auth_mode = match req.auth_mode.as_deref() {
        Some("public") => AuthModeConfig::Public,
        Some("basic") => AuthModeConfig::Basic,
        Some("inherit") => AuthModeConfig::Inherit,
        Some("oauth") => AuthModeConfig::OAuth,
        _ => state.config.defaults.auth_mode,
    };
    let probe_mode = match req.probe_mode.as_deref() {
        Some("lazy") => crate::config::ProbeMode::Lazy,
        Some("off") => crate::config::ProbeMode::Off,
        _ => state.config.defaults.probe_mode,
    };
    if auth_mode == AuthModeConfig::Inherit {
        return AdminResponse::Error {
            message: "stdio auth cannot be inherited; choose basic, oauth or explicit public"
                .into(),
            code: 2,
        };
    }
    if auth_mode == AuthModeConfig::OAuth && req.oidc_issuer.is_none() {
        return AdminResponse::Error { message: "built-in OAuth is disabled; supply --oidc-issuer for an external OAuth server with resource-bound tokens, PKCE and CIMD or safe DCR".into(), code: 2 };
    }
    if let Some(issuer) = &req.oidc_issuer {
        if crate::auth::oauth::validate_https_url(issuer).is_err() {
            return AdminResponse::Error {
                message:
                    "--oidc-issuer must be an HTTPS issuer URL without credentials or fragments"
                        .into(),
                code: 2,
            };
        }
    }

    // External resource-server configuration only.
    let oauth_config = if auth_mode == AuthModeConfig::OAuth {
        if let Some(ref issuer) = req.oidc_issuer {
            // External AS mode
            Some(OAuthMountConfig {
                issuer: issuer.clone(),
                authorization_servers: vec![issuer.clone()],
                default_scopes: req.scopes.clone(),
                per_tool_scopes: std::collections::HashMap::new(),
                builtin: false,
                builtin_user: None,
                builtin_password: None,
            })
        } else {
            None
        }
    } else {
        None
    };

    let params = MountCreateParams {
        slug: req.slug,
        port: req.port.unwrap_or_else(|| state.default_port()),
        path: req.path,
        display_name: req.display_name,
        command: CommandConfig {
            program: req.program,
            args: req.args,
        },
        cwd: req.cwd,
        tunnel_mode,
        hostname: req.hostname,
        auth_mode,
        basic_credentials: None,
        oauth_config,
        policy: crate::policy::types::PolicyConfig {
            allow_tools: req.allow_tools,
            deny_tools: req.deny_tools,
            expose_resources: !req.hide_resources,
            expose_prompts: !req.hide_prompts,
            ..Default::default()
        },
        env_vars: req.env_vars,
    };

    // Create mount
    let record = match state.mount_registry.create(params) {
        Ok(r) => r,
        Err(e) => {
            return AdminResponse::Error {
                message: e.to_string(),
                code: e.exit_code(),
            }
        }
    };

    if let Err(error) = state.ensure_listener(record.port).await {
        let _ = state.mount_registry.remove(&record.slug);
        let _ = state.prune_listeners().await;
        return AdminResponse::Error {
            message: error.to_string(),
            code: error.exit_code(),
        };
    }
    // Run probe if eager
    if probe_mode == crate::config::ProbeMode::Eager {
        let probe = crate::upstream::probe::probe_upstream(
            &record.command,
            record.cwd.as_deref(),
            &record.env_vars,
        )
        .await;

        if !probe.ok {
            // Clean up
            let _ = state.mount_registry.remove(&record.slug);
            let _ = state.prune_listeners().await;
            return AdminResponse::Error {
                message: format!("Upstream probe failed: {}", probe.error.unwrap_or_default()),
                code: crate::error::exit_codes::UPSTREAM_PROBE_FAILED,
            };
        }

        let _ = state.mount_registry.set_probe_result(&record.slug, &probe);
        let _ = state
            .mount_registry
            .set_status(&record.slug, MountStatus::Ready);
    } else {
        let _ = state
            .mount_registry
            .set_status(&record.slug, MountStatus::Ready);
    }

    if tunnel_mode == crate::config::TunnelMode::Quick {
        if let Err(error) = state.ensure_public(record.port).await {
            let _ = state.mount_registry.remove(&record.slug);
            let _ = state.prune_listeners().await;
            return AdminResponse::Error {
                message: error.to_string(),
                code: error.exit_code(),
            };
        }
    }
    let url = state.connection_url(&record.slug).await;
    let public_url = if tunnel_mode == crate::config::TunnelMode::Quick {
        url.clone()
    } else {
        None
    };

    // Reveal Basic credentials only to the authenticated local creation command.
    let basic_credentials = record.basic_credentials.as_ref();

    AdminResponse::Ok {
        data: serde_json::json!({
            "mount": {
                "slug": record.slug,
                "port": record.port,
                "path": record.path,
                "display_name": record.display_name,
                "status": "ready",
                "auth_mode": auth_mode.to_string(),
                "tunnel_mode": tunnel_mode.to_string(),
                "public_url": public_url,
                "url": url,
                "local_url": state.local_url(&record.slug),
                "basic_credentials": basic_credentials,
            }
        }),
    }
}

async fn handle_mount_delete(state: &Arc<AppState>, slug: &str) -> AdminResponse {
    let _registration = state.registration.lock().await;
    if let Some(ct) = state.mount_cancellation.write().await.remove(slug) {
        ct.cancel();
    }
    state.mount_permits.write().await.remove(slug);
    // Remove sessions
    let removed = state.session_manager.remove_mount_sessions(slug).await;

    // Shut down upstream children for those sessions
    let mut children = state.upstream_children.write().await;
    for session in &removed {
        if let Some(child) = children.remove(&session.session_id) {
            child.cancel();
            if let Ok(child) = Arc::try_unwrap(child) {
                child.shutdown().await;
            }
        }
    }
    drop(children);
    state.mount_logs.write().await.remove(slug);

    match state.mount_registry.remove(slug) {
        Ok(true) => {
            if let Err(error) = state.prune_listeners().await {
                return AdminResponse::Error {
                    message: error.to_string(),
                    code: error.exit_code(),
                };
            }
            AdminResponse::Ok {
                data: serde_json::json!({
                    "deleted": true,
                    "slug": slug,
                    "sessions_terminated": removed.len(),
                }),
            }
        }
        Ok(false) => AdminResponse::Error {
            message: format!("mount '{slug}' not found"),
            code: crate::error::exit_codes::MOUNT_NOT_FOUND,
        },
        Err(e) => AdminResponse::Error {
            message: e.to_string(),
            code: crate::error::exit_codes::INTERNAL_GATEWAY_ERROR,
        },
    }
}

async fn handle_mount_list(state: &Arc<AppState>) -> AdminResponse {
    let mounts = state.mount_registry.list().unwrap_or_default();
    let summaries: Vec<serde_json::Value> =
        futures::future::join_all(mounts.iter().map(|m| async {
            let sessions = state.session_manager.active_count(&m.slug).await;
            let tools = m
                .last_probe_result
                .as_ref()
                .map(|p| p.tools.len())
                .unwrap_or(0);
            let url = state.connection_url(&m.slug).await;

            serde_json::json!({
                "slug": m.slug,
                "port": m.port,
                "path": m.path,
                "status": m.status.to_string(),
                "tunnel": m.tunnel_mode.to_string(),
                "auth": m.auth_mode.to_string(),
                "sessions": sessions,
                "tools": tools,
                "url": url,
                "local_url": state.local_url(&m.slug),
            })
        }))
        .await;

    AdminResponse::Ok {
        data: serde_json::json!({ "mounts": summaries }),
    }
}

async fn handle_mount_inspect(state: &Arc<AppState>, slug: &str) -> AdminResponse {
    let mount = match state.mount_registry.get(slug) {
        Ok(Some(m)) => m,
        Ok(None) => {
            return AdminResponse::Error {
                message: format!("mount '{slug}' not found"),
                code: crate::error::exit_codes::MOUNT_NOT_FOUND,
            }
        }
        Err(e) => {
            return AdminResponse::Error {
                message: e.to_string(),
                code: crate::error::exit_codes::INTERNAL_GATEWAY_ERROR,
            }
        }
    };

    let sessions = state.session_manager.active_count(slug).await;
    let public_url = state.connection_url(slug).await;

    AdminResponse::Ok {
        data: serde_json::json!({
            "slug": mount.slug,
            "port": mount.port,
            "path": mount.path,
            "display_name": mount.display_name,
            "status": mount.status.to_string(),
            "auth_mode": mount.auth_mode.to_string(),
            "tunnel_mode": mount.tunnel_mode.to_string(),
            "command": format!("{} {}", mount.command.program, mount.command.args.join(" ")),
            "url": public_url,
            "public_url": if mount.tunnel_mode == crate::config::TunnelMode::Quick { public_url.clone() } else { None },
            "local_url": state.local_url(slug),
            "active_sessions": sessions,
            "created_at": mount.created_at.to_rfc3339(),
            "last_probe": mount.last_probe_result,
            "policy": mount.policy,
        }),
    }
}

async fn handle_mount_logs(state: &Arc<AppState>, slug: &str, lines: usize) -> AdminResponse {
    // Check mount exists
    match state.mount_registry.get(slug) {
        Ok(Some(_)) => {}
        _ => {
            return AdminResponse::Error {
                message: format!("mount '{slug}' not found"),
                code: crate::error::exit_codes::MOUNT_NOT_FOUND,
            }
        }
    }

    let log_lines = state.get_logs(slug, lines).await;
    AdminResponse::Ok {
        data: serde_json::json!({
            "slug": slug,
            "lines": log_lines,
            "count": log_lines.len(),
        }),
    }
}

async fn handle_daemon_shutdown(state: &Arc<AppState>) -> AdminResponse {
    let mounts = state.mount_registry.list().unwrap_or_default();
    let mount_count = mounts.len();

    // Notify the daemon to shut down (the response is sent before exit)
    state.shutdown.notify_one();

    AdminResponse::Ok {
        data: serde_json::json!({
            "shutdown": true,
            "mounts_active": mount_count,
        }),
    }
}

async fn handle_daemon_status(state: &Arc<AppState>) -> AdminResponse {
    let mounts = state.mount_registry.list().unwrap_or_default();
    let listeners = state.listener_status().await;

    AdminResponse::Ok {
        data: serde_json::json!({
            "instance_id": state.daemon_instance_id,
            "bind": state.config.daemon.bind,
            "mounts": mounts.len(),
            "listeners": listeners,
        }),
    }
}

async fn handle_tunnel_status(state: &Arc<AppState>) -> AdminResponse {
    let listeners = state.listener_status().await;

    AdminResponse::Ok {
        data: serde_json::json!({
            "listeners": listeners,
        }),
    }
}

async fn handle_doctor_run(state: &Arc<AppState>, slug: Option<&str>) -> AdminResponse {
    let mut checks =
        crate::doctor::format_checks_json(&crate::doctor::run_local_doctor(&state.config).await)
            ["checks"]
            .as_array()
            .cloned()
            .unwrap_or_default();

    // Check tunnel
    let tunnel_origins = state.tunnel_origins.read().await.clone();
    checks.push(serde_json::json!({
        "check": "tunnel",
        "status": if tunnel_origins.is_empty() { "PASS" } else { "WARN" },
        "detail": if tunnel_origins.is_empty() { "Localhost is the default; --public opens a temporary Cloudflare URL" } else { "Provider announced an origin; public/client connectivity still requires a real connection test" },
    }));

    // Mount-specific checks
    if let Some(slug) = slug {
        if let Ok(Some(mount)) = state.mount_registry.get(slug) {
            checks.extend(
                crate::doctor::format_checks_json(&crate::doctor::mount_checks(&mount))["checks"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default(),
            );
            let live = tokio::time::timeout(
                std::time::Duration::from_secs(45),
                crate::upstream::probe::probe_upstream(
                    &mount.command,
                    mount.cwd.as_deref(),
                    &mount.env_vars,
                ),
            )
            .await;
            match live {
                Ok(probe) => {
                    checks.push(serde_json::json!({"check":"upstream_live","status":if probe.ok {"PASS"} else {"FAIL"},"detail":if probe.ok {"fresh stdio lifecycle/list probe succeeded"} else {probe.error.as_deref().unwrap_or("stdio probe failed; check executable path, cwd/env and clean JSON stdout")} }));
                    let _ = state.mount_registry.set_probe_result(slug,&probe);
                }
                Err(_) => checks.push(serde_json::json!({"check":"upstream_live","status":"FAIL","detail":"stdio probe exceeded 45s; check startup, stdout protocol and advertised list methods"})),
            }
            let logs = state.get_logs(slug, 8).await;
            if logs
                .iter()
                .any(|line| line.contains("status=4") || line.contains("status=5"))
            {
                checks.push(serde_json::json!({"check":"recent_http_errors","status":"WARN","detail":"Recent HTTP errors: 400=metadata/header/body mismatch; 401=missing/invalid credential or exact OAuth audience; 403=Origin/scopes/policy; 404=mount/session or unsupported method; 405=unsupported GET/DELETE; 406=Accept; 415=Content-Type; 429=concurrency; 503=disabled auth/managed backend. See docs/CLIENT_COMPATIBILITY.md."}));
            }
            // Check upstream probe
            match &mount.last_probe_result {
                Some(probe) if probe.ok => {
                    checks.push(serde_json::json!({
                        "check": "upstream_probe",
                        "status": "PASS",
                        "detail": format!("server={}, tools={}",
                            probe.server_name.as_deref().unwrap_or("?"),
                            probe.tools.len()),
                    }));

                    // Suspicious tool heuristic
                    let suspicious = crate::policy::filter::flag_suspicious_tools(&probe.tools);
                    if !suspicious.is_empty() {
                        checks.push(serde_json::json!({
                            "check": "tool_safety",
                            "status": "WARN",
                            "detail": format!("potentially dangerous tools: {}", suspicious.join(", ")),
                        }));
                    }
                }
                Some(probe) => {
                    checks.push(serde_json::json!({
                        "check": "upstream_probe",
                        "status": "FAIL",
                        "detail": probe.error.as_deref().unwrap_or("probe failed"),
                    }));
                }
                None => {
                    checks.push(serde_json::json!({
                        "check": "upstream_probe",
                        "status": "WARN",
                        "detail": "no probe data",
                    }));
                }
            }

            // Quick mode warnings
            if mount.tunnel_mode == crate::config::TunnelMode::Quick {
                checks.push(serde_json::json!({
                    "check": "quick_mode",
                    "status": "WARN",
                    "detail": "Quick Tunnel is for testing/development only. SSE not supported.",
                }));
            }

            // Public auth warning
            if mount.auth_mode == AuthModeConfig::Public {
                checks.push(serde_json::json!({
                    "check": "auth_public",
                    "status": "WARN",
                    "detail": "Anyone who can reach this endpoint can invoke its tools without authentication.",
                }));
            }
        } else {
            checks.push(serde_json::json!({
                "check": "mount",
                "status": "FAIL",
                "detail": format!("mount '{slug}' not found"),
            }));
        }
    }

    AdminResponse::Ok {
        data: serde_json::json!({ "checks": checks }),
    }
}
