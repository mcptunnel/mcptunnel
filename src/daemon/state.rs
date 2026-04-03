use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::auth::oauth::JwksCache;
use crate::config::Config;
use crate::mount::MountRegistry;
use crate::session::SessionManager;
use crate::session::types::SessionConfig;
use crate::upstream::child::UpstreamChild;

/// SSE sender for forwarding notifications to connected clients.
pub type SseSender = tokio::sync::mpsc::Sender<Result<String, std::convert::Infallible>>;

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
    /// SSE senders per session ID — for forwarding server notifications to SSE clients.
    pub sse_senders: RwLock<HashMap<String, SseSender>>,
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
            sse_senders: RwLock::new(HashMap::new()),
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

    /// Register an SSE sender for a session.
    pub async fn register_sse_sender(&self, session_id: &str, sender: SseSender) {
        self.sse_senders.write().await.insert(session_id.to_string(), sender);
    }

    /// Remove an SSE sender for a session.
    pub async fn remove_sse_sender(&self, session_id: &str) {
        self.sse_senders.write().await.remove(session_id);
    }

    /// Send an SSE event to a session's connected client.
    pub async fn send_sse_event(&self, session_id: &str, event: &str, data: &str) -> bool {
        let senders = self.sse_senders.read().await;
        if let Some(sender) = senders.get(session_id) {
            let msg = format!("event: {event}\ndata: {data}\n\n");
            sender.send(Ok(msg)).await.is_ok()
        } else {
            false
        }
    }
}
