pub mod admin_api;
pub mod state;

use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tracing::{error, info, warn};

use crate::config::Config;
use crate::gateway;
use crate::mount::{db, MountRegistry};
use crate::session::types::SessionConfig;
use state::AppState;

/// Start the daemon: HTTP server + UDS admin API + tunnel + session eviction.
pub async fn run_daemon(config: Config) -> crate::error::Result<()> {
    // Ensure state directory exists
    let state_dir = crate::config::state_dir();
    std::fs::create_dir_all(&state_dir)?;

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

    let mut app_state = AppState::new(config.clone(), mount_registry, session_config);

    // Initialize built-in OAuth Authorization Server
    match app_state.init_oauth_server() {
        Ok(()) => info!("Built-in OAuth Authorization Server initialized"),
        Err(e) => warn!(error = %e, "Failed to initialize OAuth server (OAuth auth mode will not work)"),
    }

    let state = Arc::new(app_state);

    // Start session eviction task
    let eviction_state = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let evicted = eviction_state.session_manager.evict_expired().await;
            for session in &evicted {
                // Shut down upstream children for evicted sessions
                if let Some(child) = eviction_state.upstream_children.write().await.remove(&session.session_id) {
                    child.shutdown().await;
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
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)?;
    info!(path = %socket_path.display(), "Admin API listening");

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let state = admin_state.clone();
                    tokio::spawn(async move {
                        let (reader, mut writer) = stream.into_split();
                        let mut buf_reader = BufReader::new(reader);
                        let mut line = String::new();

                        while buf_reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                            let trimmed = line.trim();
                            if trimmed.is_empty() {
                                line.clear();
                                continue;
                            }

                            let response = match serde_json::from_str::<admin_api::AdminRequest>(trimmed) {
                                Ok(req) => admin_api::handle_admin_request(&state, req).await,
                                Err(e) => admin_api::AdminResponse::Error {
                                    message: format!("invalid request: {e}"),
                                    code: 2,
                                },
                            };

                            let response_json = serde_json::to_string(&response).unwrap_or_default();
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
    tokio::spawn(async move {
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
                // Managed mode requires explicit hostname configuration
                // It will be started when a mount with managed tunnel mode is created
                info!("Managed tunnel mode — tunnel starts with mount creation");
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
            shutdown_state.shutdown.notified().await;
            info!("Shutdown signal received, stopping daemon...");
        })
        .await?;

    // Clean up socket file
    let _ = std::fs::remove_file(&socket_path);
    info!("Daemon stopped.");

    Ok(())
}
