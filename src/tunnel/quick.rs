use super::supervisor::CloudflaredSupervisor;
use super::{TunnelState, TunnelStatus};
use tracing::info;

/// Quick tunnel manager — spawns `cloudflared tunnel --url <local>`.
pub struct QuickTunnel {
    supervisor: Option<CloudflaredSupervisor>,
    cloudflared_path: String,
    local_bind: String,
}

impl QuickTunnel {
    pub fn new(cloudflared_path: &str, local_bind: &str) -> Self {
        Self {
            supervisor: None,
            cloudflared_path: cloudflared_path.to_string(),
            local_bind: local_bind.to_string(),
        }
    }

    /// Start the quick tunnel.
    pub async fn start(&mut self) -> Result<String, crate::error::McpTunnelError> {
        // Stop previous tunnel if any
        self.stop().await;

        let url = format!("http://{}", self.local_bind);
        // Use --config /dev/null to ignore ~/.cloudflared/config.yml which
        // can interfere with quick tunnels (e.g. catch-all 404 ingress rules).
        let args = vec![
            "tunnel".to_string(),
            "--config".to_string(),
            if cfg!(windows) { "NUL" } else { "/dev/null" }.to_string(),
            "--url".to_string(),
            url,
        ];

        let mut supervisor = CloudflaredSupervisor::start(&self.cloudflared_path, args)
            .await
            .map_err(|e| crate::error::McpTunnelError::CloudflaredMissing(e.to_string()))?;

        // Wait up to 30 seconds for the URL
        let public_url = supervisor
            .wait_for_url(std::time::Duration::from_secs(30))
            .await
            .ok_or_else(|| {
                crate::error::McpTunnelError::TunnelCreateFailed(
                    "Timed out waiting for quick tunnel URL".to_string(),
                )
            })?;

        info!(url = %public_url, "Quick tunnel ready");
        self.supervisor = Some(supervisor);
        Ok(public_url)
    }

    /// Get the current status.
    pub fn status(&mut self) -> TunnelStatus {
        if let Some(ref mut sup) = self.supervisor {
            if sup.is_running() {
                TunnelStatus {
                    mode: "quick".to_string(),
                    status: TunnelState::Ready,
                    public_origin: sup.public_origin(),
                    cloudflared_pid: sup.pid(),
                    error: None,
                }
            } else {
                TunnelStatus {
                    mode: "quick".to_string(),
                    status: TunnelState::Failed,
                    public_origin: None,
                    cloudflared_pid: None,
                    error: Some("cloudflared process exited".to_string()),
                }
            }
        } else {
            TunnelStatus {
                mode: "quick".to_string(),
                status: TunnelState::Stopped,
                public_origin: None,
                cloudflared_pid: None,
                error: None,
            }
        }
    }

    /// Stop the tunnel.
    pub async fn stop(&mut self) {
        if let Some(ref mut sup) = self.supervisor {
            sup.stop().await;
        }
        self.supervisor = None;
    }
}
