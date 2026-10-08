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
    pub tunnel_origin: RwLock<Option<String>>,
    pub daemon_instance_id: String,
    pub upstream_children: RwLock<HashMap<String, Arc<UpstreamChild>>>,
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
            tunnel_origin: RwLock::new(None),
            daemon_instance_id: uuid::Uuid::new_v4().to_string(),
            upstream_children: RwLock::new(HashMap::new()),
            request_permits: Arc::new(tokio::sync::Semaphore::new(64)),
            mount_permits: RwLock::new(HashMap::new()),
            mount_cancellation: RwLock::new(HashMap::new()),
            cancellation: tokio_util::sync::CancellationToken::new(),
            jwks_cache: Arc::new(JwksCache::new()),
            mount_logs: RwLock::new(HashMap::new()),
            shutdown: Notify::new(),
        }
    }

    /// Set the tunnel origin URL.
    pub async fn set_tunnel_origin(&self, origin: Option<String>) {
        let mut o = self.tunnel_origin.write().await;
        *o = origin;
    }

    /// Wait until the quick-tunnel origin is populated, up to `timeout`.
    /// Returns immediately if tunnel mode is not Quick or if origin is already set.
    pub async fn wait_for_tunnel_ready(&self, timeout: std::time::Duration) {
        if self.config.tunnel.default_mode != crate::config::TunnelMode::Quick {
            return;
        }
        let deadline = tokio::time::Instant::now() + timeout;
        while tokio::time::Instant::now() < deadline {
            if self.tunnel_origin.read().await.is_some() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    /// Get the public MCP URL for a mount.
    pub async fn public_url(&self, slug: &str) -> String {
        let origin = self.public_origin().await;
        format!("{origin}/{slug}/mcp")
    }

    /// No public connection URL is claimed while an ephemeral provider is unavailable.
    pub async fn connection_url(&self, slug: &str) -> Option<String> {
        if self.config.tunnel.default_mode == crate::config::TunnelMode::Quick
            && self.tunnel_origin.read().await.is_none()
        {
            return None;
        }
        Some(self.public_url(slug).await)
    }

    pub async fn public_origin(&self) -> String {
        self.tunnel_origin
            .read()
            .await
            .clone()
            .or_else(|| self.config.tunnel.external_origin.clone())
            .unwrap_or_else(|| format!("http://{}", self.config.daemon.bind))
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
                .set_tunnel_origin(Some("https://example.trycloudflare.com".to_string()))
                .await;
        });

        let start = tokio::time::Instant::now();
        state.wait_for_tunnel_ready(Duration::from_secs(2)).await;
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_millis(600),
            "expected <600ms, got {elapsed:?}"
        );
        assert!(state.tunnel_origin.read().await.is_some());
    }

    #[tokio::test]
    async fn wait_times_out_when_origin_never_set() {
        let state = test_state(TunnelMode::Quick);

        let start = tokio::time::Instant::now();
        state
            .wait_for_tunnel_ready(Duration::from_millis(450))
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
        assert!(state.tunnel_origin.read().await.is_none());
    }

    #[tokio::test]
    async fn wait_hot_path_returns_immediately() {
        let state = test_state(TunnelMode::Quick);
        state
            .set_tunnel_origin(Some("https://ready.example.com".to_string()))
            .await;

        let start = tokio::time::Instant::now();
        state.wait_for_tunnel_ready(Duration::from_secs(5)).await;
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_millis(50),
            "expected <50ms, got {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn wait_managed_mode_bypasses() {
        let state = test_state(TunnelMode::Managed);
        // tunnel_origin is None, but Managed mode should skip the wait entirely

        let start = tokio::time::Instant::now();
        state.wait_for_tunnel_ready(Duration::from_secs(10)).await;
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_millis(50),
            "expected <50ms, got {elapsed:?}"
        );
    }
}
