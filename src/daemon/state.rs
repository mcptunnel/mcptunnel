use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

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
}
