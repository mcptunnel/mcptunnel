use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::auth::oauth::JwksCache;
use crate::config::Config;
use crate::mount::MountRegistry;
use crate::session::SessionManager;
use crate::session::types::SessionConfig;
use crate::upstream::child::UpstreamChild;

/// Shared application state for the daemon.
pub struct AppState {
    pub config: Config,
    pub mount_registry: MountRegistry,
    pub session_manager: SessionManager,
    pub tunnel_origin: RwLock<Option<String>>,
    pub daemon_instance_id: String,
    pub upstream_children: RwLock<HashMap<String, UpstreamChild>>,
    pub jwks_cache: Arc<JwksCache>,
    /// Per-mount log ring buffers: mount_slug -> Vec<log_line>.
    pub mount_logs: RwLock<HashMap<String, Vec<String>>>,
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
            jwks_cache: Arc::new(JwksCache::new()),
            mount_logs: RwLock::new(HashMap::new()),
        }
    }

    /// Set the tunnel origin URL.
    pub async fn set_tunnel_origin(&self, origin: Option<String>) {
        let mut o = self.tunnel_origin.write().await;
        *o = origin;
    }

    /// Get the public MCP URL for a mount.
    pub async fn public_url(&self, slug: &str) -> String {
        let origin = self.tunnel_origin.read().await.clone()
            .unwrap_or_else(|| format!("http://{}", self.config.daemon.bind));
        format!("{}/{}/mcp", origin, slug)
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
