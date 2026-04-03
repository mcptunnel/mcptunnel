use super::supervisor::CloudflaredSupervisor;
use super::{TunnelState, TunnelStatus};
use tracing::info;

/// Managed tunnel — uses a named Cloudflare tunnel with a stable hostname.
pub struct ManagedTunnel {
    supervisor: Option<CloudflaredSupervisor>,
    cloudflared_path: String,
    local_bind: String,
    hostname: String,
}

impl ManagedTunnel {
    pub fn new(cloudflared_path: &str, local_bind: &str, hostname: &str) -> Self {
        Self {
            supervisor: None,
            cloudflared_path: cloudflared_path.to_string(),
            local_bind: local_bind.to_string(),
            hostname: hostname.to_string(),
        }
    }

    /// Start the managed tunnel.
    /// Requires `cloudflared` to be authenticated and the tunnel+DNS to be pre-configured.
    pub async fn start(&mut self) -> Result<String, crate::error::McpTunnelError> {
        // For managed mode, cloudflared uses a config file or CLI args
        // pointing to an existing named tunnel.
        let url = format!("http://{}", self.local_bind);
        let args = vec![
            "tunnel".to_string(),
            "run".to_string(),
            "--url".to_string(),
            url,
        ];

        let supervisor = CloudflaredSupervisor::start(&self.cloudflared_path, args)
            .await
            .map_err(|e| crate::error::McpTunnelError::CloudflaredMissing(e.to_string()))?;

        let public_url = format!("https://{}", self.hostname);
        info!(url = %public_url, "Managed tunnel started");
        self.supervisor = Some(supervisor);
        Ok(public_url)
    }

    /// Get the current status.
    pub fn status(&mut self) -> TunnelStatus {
        if let Some(ref mut sup) = self.supervisor {
            if sup.is_running() {
                TunnelStatus {
                    mode: "managed".to_string(),
                    status: TunnelState::Ready,
                    public_origin: Some(format!("https://{}", self.hostname)),
                    cloudflared_pid: sup.pid(),
                    error: None,
                }
            } else {
                TunnelStatus {
                    mode: "managed".to_string(),
                    status: TunnelState::Failed,
                    public_origin: None,
                    cloudflared_pid: None,
                    error: Some("cloudflared process exited".to_string()),
                }
            }
        } else {
            TunnelStatus {
                mode: "managed".to_string(),
                status: TunnelState::Stopped,
                public_origin: None,
                cloudflared_pid: None,
                error: None,
            }
        }
    }

    /// Get the public origin URL.
    pub fn public_origin(&self) -> Option<String> {
        if self.supervisor.is_some() {
            Some(format!("https://{}", self.hostname))
        } else {
            None
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
