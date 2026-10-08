pub mod admin_api;
pub mod ipc;
pub mod listeners;
pub mod state;

use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tracing::{error, info};

use crate::config::Config;
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
    if config.defaults.session_idle_ttl.is_zero()
        || config.defaults.session_hard_ttl.is_zero()
        || config.defaults.cidr_filter_mode != "disabled"
    {
        return Err(crate::error::McpTunnelError::CliUsage(
            "TTL values must be positive; CIDR filtering is unimplemented and must remain disabled"
                .into(),
        ));
    }
    if let Some(origin) = &config.tunnel.external_origin {
        let url = crate::auth::oauth::validate_https_url(origin)
            .map_err(crate::error::McpTunnelError::CliUsage)?;
        if url.path() != "/" {
            return Err(crate::error::McpTunnelError::CliUsage(
                "external_origin must be an HTTPS origin without a path".into(),
            ));
        }
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
    db::init_db_at_port(&conn, bind.port())?;
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
    // Bind both endpoints before creating background tasks; startup failures leave no workers.
    let mut listener = ipc::Listener::bind()?;

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
    info!(path = %socket_path.display(), "Admin API listening");

    let admin_task = tokio::spawn(async move {
        let mut clients = tokio::task::JoinSet::new();
        loop {
            let accepted = tokio::select! {
                _ = admin_state.cancellation.cancelled() => break,
                Some(_) = clients.join_next(), if !clients.is_empty() => continue,
                stream = listener.accept() => stream,
            };
            match accepted {
                Ok(stream) => {
                    if clients.len() >= 32 {
                        drop(stream);
                        continue;
                    }
                    let state = admin_state.clone();
                    clients.spawn(async move {
                        let (reader, mut writer) = tokio::io::split(stream);
                        let mut buf_reader = BufReader::new(reader.take(1024 * 1024));
                        let mut line = String::new();

                        while matches!(tokio::time::timeout(std::time::Duration::from_secs(10), buf_reader.read_line(&mut line)).await, Ok(Ok(n)) if n > 0) {
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
                            if tokio::time::timeout(std::time::Duration::from_secs(5), async { writer.write_all(response_json.as_bytes()).await?; writer.write_all(b"\n").await }).await.is_err() { break; }
                            line.clear();
                        }
                    });
                }
                Err(e) => {
                    error!(error = %e, "Admin API accept error");
                }
            }
        }
        clients.abort_all();
        while clients.join_next().await.is_some() {}
    });

    // Restore saved ports/paths without starting a provider for local-only ports.
    let restore_state = state.clone();
    let restore_task = tokio::spawn(async move {
        let _registration = restore_state.registration.lock().await;
        let mounts = restore_state.mount_registry.list().unwrap_or_default();
        for mount in mounts {
            if restore_state.cancellation.is_cancelled() {
                break;
            }
            let restored = async {
                restore_state.ensure_listener(mount.port).await?;
                if mount.tunnel_mode == crate::config::TunnelMode::Quick {
                    restore_state.ensure_public(mount.port).await?;
                }
                Ok::<_, crate::error::McpTunnelError>(())
            }
            .await;
            if let Err(error) = restored {
                error!(mount = %mount.slug, %error, "Cannot restore mount listener/provider");
                let _ = restore_state
                    .mount_registry
                    .set_status(&mount.slug, crate::mount::types::MountStatus::Degraded);
            } else {
                let _ = restore_state
                    .mount_registry
                    .set_status(&mount.slug, crate::mount::types::MountStatus::Ready);
            }
        }
    });

    tokio::select! { _ = state.shutdown.notified() => {}, _ = tokio::signal::ctrl_c() => {} }
    state.cancellation.cancel();
    info!("Shutdown signal received, stopping daemon...");

    for mut task in [admin_task, eviction_task, restore_task] {
        if tokio::time::timeout(std::time::Duration::from_secs(3), &mut task)
            .await
            .is_err()
        {
            task.abort();
            let _ = task.await;
        }
    }
    state.stop_listeners().await;
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
