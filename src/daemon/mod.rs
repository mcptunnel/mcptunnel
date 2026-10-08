pub mod admin_api;
pub mod ipc;
pub mod state;

use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tracing::{error, info, warn};

use crate::config::Config;
use crate::gateway;
use crate::mount::{db, MountRegistry};
use crate::session::types::SessionConfig;
use state::AppState;

/// Start the daemon: HTTP server + UDS admin API + tunnel + session eviction.
pub async fn run_daemon(config: Config) -> crate::error::Result<()> {
    let bind: std::net::SocketAddr = config.daemon.bind.parse().map_err(|_| {
        crate::error::McpTunnelError::CliUsage("daemon.bind must be a loopback IP:port".into())
    })?;
    if !bind.ip().is_loopback() {
        return Err(crate::error::McpTunnelError::CliUsage(
            "gateway must bind a loopback address".into(),
        ));
    }
    crate::tunnel::validate_selection(config.tunnel.default_mode, None)
        .map_err(crate::error::McpTunnelError::CliUsage)?;
    if config.tunnel.default_mode == crate::config::TunnelMode::Quick
        && config.tunnel.external_origin.is_some()
    {
        return Err(crate::error::McpTunnelError::CliUsage("quick mode cannot assign external_origin; use local for independently operated infrastructure".into()));
    }
    if config.defaults.max_sessions_per_mount == 0 || config.defaults.max_inflight_per_session == 0
    {
        return Err(crate::error::McpTunnelError::CliUsage(
            "session and concurrency limits must be positive".into(),
        ));
    }
    // Ensure state directory exists
    let state_dir = crate::config::state_dir();
    std::fs::create_dir_all(&state_dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&state_dir, std::fs::Permissions::from_mode(0o700))?;
    }

    // Initialize database
    let db_path = crate::config::db_path();
    let conn = Connection::open(&db_path)?;
    db::init_db(&conn)?;
    let db = Arc::new(Mutex::new(conn));

    let mount_registry = MountRegistry::new(db.clone());
    let session_config = SessionConfig {
        idle_ttl: config.defaults.session_idle_ttl,
        hard_ttl: config.defaults.session_hard_ttl,
        max_sessions_per_mount: config.defaults.max_sessions_per_mount,
        max_inflight_per_session: config.defaults.max_inflight_per_session,
    };

    let app_state = AppState::new(config.clone(), mount_registry, session_config);
    let state = Arc::new(app_state);

    // Start session eviction task
    let eviction_state = state.clone();
    let eviction_task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let evicted = eviction_state.session_manager.evict_expired().await;
            for session in &evicted {
                // Shut down upstream children for evicted sessions
                if let Some(child) = eviction_state
                    .upstream_children
                    .write()
                    .await
                    .remove(&session.session_id)
                {
                    child.cancel();
                    if let Ok(child) = Arc::try_unwrap(child) {
                        child.shutdown().await;
                    }
                }
                info!(
                    session_id_hash = %crate::logging::hash_session_id(&session.session_id),
                    mount = %session.mount_slug,
                    reason = session.end_reason.as_deref().unwrap_or("unknown"),
                    "Session evicted"
                );
            }
        }
    });

    // Start UDS admin API
    let admin_state = state.clone();
    let socket_path = crate::config::socket_path();
    // Remove stale socket file
    let mut listener = ipc::Listener::bind()?;
    info!(path = %socket_path.display(), "Admin API listening");

    let admin_task = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok(stream) => {
                    let state = admin_state.clone();
                    tokio::spawn(async move {
                        let (reader, mut writer) = tokio::io::split(stream);
                        let mut buf_reader = BufReader::new(reader.take(1024 * 1024));
                        let mut line = String::new();

                        while buf_reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                            let trimmed = line.trim();
                            if trimmed.is_empty() {
                                line.clear();
                                continue;
                            }

                            let response =
                                match serde_json::from_str::<admin_api::AdminRequest>(trimmed) {
                                    Ok(req) => admin_api::handle_admin_request(&state, req).await,
                                    Err(e) => admin_api::AdminResponse::Error {
                                        message: format!("invalid request: {e}"),
                                        code: 2,
                                    },
                                };

                            let response_json =
                                serde_json::to_string(&response).unwrap_or_default();
                            let _ = writer.write_all(response_json.as_bytes()).await;
                            let _ = writer.write_all(b"\n").await;
                            line.clear();
                        }
                    });
                }
                Err(e) => {
                    error!(error = %e, "Admin API accept error");
                }
            }
        }
    });

    // Auto-start tunnel
    let tunnel_state = state.clone();
    let tunnel_config = config.clone();
    let tunnel_task = tokio::spawn(async move {
        info!(mode = %tunnel_config.tunnel.default_mode, "Starting tunnel");
        match tunnel_config.tunnel.default_mode {
            crate::config::TunnelMode::Quick => {
                let mut tunnel = crate::tunnel::quick::QuickTunnel::new(
                    &tunnel_config.tunnel.cloudflared_path,
                    &tunnel_config.daemon.bind,
                );
                match tunnel.start().await {
                    Ok(url) => {
                        info!(url = %url, "Quick tunnel ready");
                        tunnel_state.set_tunnel_origin(Some(url)).await;
                        // Keep tunnel alive — wait for process to exit
                        loop {
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            let status = tunnel.status();
                            if status.status == crate::tunnel::TunnelState::Failed {
                                tunnel_state.set_tunnel_origin(None).await;
                                warn!("Quick tunnel failed, restarting...");
                                match tunnel.start().await {
                                    Ok(new_url) => {
                                        info!(url = %new_url, "Quick tunnel restarted");
                                        tunnel_state.set_tunnel_origin(Some(new_url)).await;
                                    }
                                    Err(e) => {
                                        error!(error = %e, "Quick tunnel restart failed");
                                        tunnel_state.set_tunnel_origin(None).await;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to start quick tunnel");
                        tunnel_state.set_tunnel_origin(None).await;
                    }
                }
            }
            crate::config::TunnelMode::Managed => {
                unreachable!("managed selection was rejected before startup")
            }
            crate::config::TunnelMode::Local => {
                info!("Local mode: configure your own reverse proxy if needed")
            }
        }
    });

    // Build HTTP router
    let router = gateway::build_router(state.clone());

    // Start HTTP server
    let bind_addr = config.daemon.bind.clone();
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    info!(bind = %bind_addr, "HTTP gateway listening");

    // Run HTTP server until shutdown signal
    let shutdown_state = state.clone();
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            tokio::select! { _ = shutdown_state.shutdown.notified() => {}, _ = tokio::signal::ctrl_c() => {} }
            shutdown_state.cancellation.cancel();
            info!("Shutdown signal received, stopping daemon...");
        })
        .await?;

    for task in [admin_task, eviction_task, tunnel_task] {
        task.abort();
        let _ = task.await;
    }
    let children = std::mem::take(&mut *state.upstream_children.write().await);
    for (_, child) in children {
        child.cancel();
        if let Ok(child) = Arc::try_unwrap(child) {
            child.shutdown().await;
        }
    }

    // Clean up socket file
    let _ = std::fs::remove_file(&socket_path);
    info!("Daemon stopped.");

    Ok(())
}
