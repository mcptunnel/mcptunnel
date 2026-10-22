use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Notify, RwLock};

use crate::auth::oauth::JwksCache;
use crate::config::Config;
use crate::mount::MountRegistry;
use crate::session::types::SessionConfig;
use crate::session::SessionManager;
use crate::upstream::child::UpstreamChild;

/// Shared application state for the daemon.
pub struct AppState {
    pub config: Config,
    pub mount_registry: MountRegistry,
    pub session_manager: SessionManager,
    pub tunnel_origins: RwLock<HashMap<u16, String>>,
    pub(crate) listeners:
        tokio::sync::Mutex<std::collections::BTreeMap<u16, super::listeners::PortRuntime>>,
    pub(crate) registration: tokio::sync::Mutex<()>,
    pub daemon_instance_id: String,
    pub upstream_children: RwLock<HashMap<String, Arc<UpstreamChild>>>,
    pub(crate) legacy_requests: std::sync::Mutex<
        HashMap<(String, String, rmcp::model::RequestId), tokio_util::sync::CancellationToken>,
    >,
    pub request_permits: Arc<tokio::sync::Semaphore>,
    pub mount_permits: RwLock<HashMap<String, Arc<tokio::sync::Semaphore>>>,
    pub mount_cancellation: RwLock<HashMap<String, tokio_util::sync::CancellationToken>>,
    pub cancellation: tokio_util::sync::CancellationToken,
    pub jwks_cache: Arc<JwksCache>,
    /// Per-mount log ring buffers: mount_slug -> Vec<log_line>.
    pub mount_logs: RwLock<HashMap<String, Vec<String>>>,
    /// Shutdown signal.
    pub shutdown: Notify,
}

impl AppState {
    pub fn new(
        config: Config,
        mount_registry: MountRegistry,
        session_config: SessionConfig,
    ) -> Self {
        Self {
            config,
            mount_registry,
            session_manager: SessionManager::new(session_config),
            tunnel_origins: RwLock::new(HashMap::new()),
            listeners: tokio::sync::Mutex::new(std::collections::BTreeMap::new()),
            registration: tokio::sync::Mutex::new(()),
            daemon_instance_id: uuid::Uuid::new_v4().to_string(),
            upstream_children: RwLock::new(HashMap::new()),
            legacy_requests: std::sync::Mutex::new(HashMap::new()),
            request_permits: Arc::new(tokio::sync::Semaphore::new(64)),
            mount_permits: RwLock::new(HashMap::new()),
            mount_cancellation: RwLock::new(HashMap::new()),
            cancellation: tokio_util::sync::CancellationToken::new(),
            jwks_cache: Arc::new(JwksCache::new()),
            mount_logs: RwLock::new(HashMap::new()),
            shutdown: Notify::new(),
        }
    }

    pub fn default_port(&self) -> u16 {
        self.config
            .daemon
            .bind
            .parse::<std::net::SocketAddr>()
            .map(|a| a.port())
            .unwrap_or(8000)
    }

    pub async fn set_tunnel_origin(&self, port: u16, origin: Option<String>) {
        let mut origins = self.tunnel_origins.write().await;
        if let Some(origin) = origin {
            origins.insert(port, origin);
        } else {
            origins.remove(&port);
        }
    }

    pub async fn wait_for_tunnel_ready(&self, port: u16, timeout: std::time::Duration) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if self.tunnel_origins.read().await.contains_key(&port) {
                return true;
            }
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    /// Get the public MCP URL for a mount.
    pub async fn public_url(&self, slug: &str) -> String {
        self.connection_url(slug)
            .await
            .or_else(|| self.local_url(slug))
            .unwrap_or_default()
    }

    /// No public connection URL is claimed while an ephemeral provider is unavailable.
    pub async fn connection_url(&self, slug: &str) -> Option<String> {
        let mount = self.mount_registry.get(slug).ok().flatten()?;
        let origin = self.mount_origin(&mount).await?;
        Some(format!("{origin}{}", mount.path))
    }

    pub fn local_url(&self, slug: &str) -> Option<String> {
        let mount = self.mount_registry.get(slug).ok().flatten()?;
        Some(format!("http://localhost:{}{}", mount.port, mount.path))
    }

    pub async fn mount_origin(&self, mount: &crate::mount::types::MountRecord) -> Option<String> {
        match mount.tunnel_mode {
            crate::config::TunnelMode::Quick => {
                self.tunnel_origins.read().await.get(&mount.port).cloned()
            }
            crate::config::TunnelMode::Local => Some(
                self.config
                    .tunnel
                    .external_origin
                    .clone()
                    .filter(|_| mount.port == self.default_port())
                    .unwrap_or_else(|| format!("http://localhost:{}", mount.port)),
            ),
            crate::config::TunnelMode::Managed => None,
        }
    }

    pub async fn metadata_url(&self, mount: &crate::mount::types::MountRecord) -> String {
        let origin = self
            .mount_origin(mount)
            .await
            .unwrap_or_else(|| format!("http://localhost:{}", mount.port));
        format!(
            "{origin}/.well-known/oauth-protected-resource{}",
            mount.path
        )
    }

    /// Append a log line for a mount (ring buffer, max 1000 lines).
    pub async fn append_log(&self, slug: &str, line: String) {
        let mut logs = self.mount_logs.write().await;
        let entries = logs.entry(slug.to_string()).or_insert_with(Vec::new);
        entries.push(line);
        // Keep only the last 1000 lines
        if entries.len() > 1000 {
            let drain_count = entries.len() - 1000;
            entries.drain(..drain_count);
        }
    }

    /// Get log lines for a mount.
    pub async fn get_logs(&self, slug: &str, tail: usize) -> Vec<String> {
        let logs = self.mount_logs.read().await;
        match logs.get(slug) {
            Some(entries) => {
                let start = entries.len().saturating_sub(tail);
                entries[start..].to_vec()
            }
            None => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, TunnelMode};
    use crate::mount::MountRegistry;
    use crate::session::types::SessionConfig;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn test_state(tunnel_mode: TunnelMode) -> AppState {
        let mut config = Config::default();
        config.tunnel.default_mode = tunnel_mode;
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::mount::db::init_db(&conn).unwrap();
        let db = Arc::new(Mutex::new(conn));
        let registry = MountRegistry::new(db);
        AppState::new(config, registry, SessionConfig::default())
    }

    #[tokio::test]
    async fn wait_returns_when_origin_set() {
        let state = Arc::new(test_state(TunnelMode::Quick));
        let state2 = state.clone();

        // Spawn task that sets origin after 150ms
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            state2
                .set_tunnel_origin(8000, Some("https://example.trycloudflare.com".to_string()))
                .await;
        });

        let start = tokio::time::Instant::now();
        state
            .wait_for_tunnel_ready(8000, Duration::from_secs(2))
            .await;
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_millis(600),
            "expected <600ms, got {elapsed:?}"
        );
        assert!(state.tunnel_origins.read().await.contains_key(&8000));
    }

    #[tokio::test]
    async fn wait_times_out_when_origin_never_set() {
        let state = test_state(TunnelMode::Quick);

        let start = tokio::time::Instant::now();
        state
            .wait_for_tunnel_ready(8000, Duration::from_millis(450))
            .await;
        let elapsed = start.elapsed();

        assert!(
            elapsed >= Duration::from_millis(400),
            "expected >=400ms, got {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_millis(1000),
            "expected <1000ms, got {elapsed:?}"
        );
        assert!(state.tunnel_origins.read().await.is_empty());
    }

    #[tokio::test]
    async fn wait_hot_path_returns_immediately() {
        let state = test_state(TunnelMode::Quick);
        state
            .set_tunnel_origin(8000, Some("https://ready.example.com".to_string()))
            .await;

        let start = tokio::time::Instant::now();
        state
            .wait_for_tunnel_ready(8000, Duration::from_secs(5))
            .await;
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_millis(50),
            "expected <50ms, got {elapsed:?}"
        );
    }
}
