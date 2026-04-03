use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::watch;
use tracing::info;

/// Supervise a cloudflared process, parsing its output for the public URL.
pub struct CloudflaredSupervisor {
    child: Option<Child>,
    origin_rx: watch::Receiver<Option<String>>,
    origin_tx: watch::Sender<Option<String>>,
}

impl CloudflaredSupervisor {
    /// Start cloudflared with the given arguments.
    pub async fn start(
        cloudflared_path: &str,
        args: Vec<String>,
    ) -> Result<Self, std::io::Error> {
        let (origin_tx, origin_rx) = watch::channel(None);

        let mut cmd = Command::new(cloudflared_path);
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn()?;

        let stderr = child.stderr.take();
        let tx = origin_tx.clone();

        // Spawn a task to parse cloudflared output for the public URL.
        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    // Quick tunnel URL appears in stderr like:
                    // "... https://something.trycloudflare.com ..."
                    if let Some(url) = parse_tunnel_url(&line) {
                        info!(url = %url, "Tunnel URL detected");
                        let _ = tx.send(Some(url));
                    }
                    // Log cloudflared output at debug level
                    tracing::debug!(cloudflared = %line);
                }
            });
        }

        let pid = child.id();
        info!(pid = pid, "cloudflared started");

        Ok(Self {
            child: Some(child),
            origin_rx,
            origin_tx,
        })
    }

    /// Wait for the tunnel URL to become available.
    pub async fn wait_for_url(&mut self, timeout: std::time::Duration) -> Option<String> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if let Some(url) = self.origin_rx.borrow().clone() {
                return Some(url);
            }
            if tokio::time::Instant::now() >= deadline {
                return None;
            }
            tokio::select! {
                _ = self.origin_rx.changed() => {},
                _ = tokio::time::sleep_until(deadline) => return None,
            }
        }
    }

    /// Get the current public origin URL.
    pub fn public_origin(&self) -> Option<String> {
        self.origin_rx.borrow().clone()
    }

    /// Get the cloudflared process ID.
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().and_then(|c| c.id())
    }

    /// Stop the cloudflared process.
    pub async fn stop(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill().await;
            let _ = child.wait().await;
            info!("cloudflared stopped");
        }
        self.child = None;
        let _ = self.origin_tx.send(None);
    }

    /// Check if the process is still running.
    pub fn is_running(&mut self) -> bool {
        if let Some(ref mut child) = self.child {
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }
}

impl Drop for CloudflaredSupervisor {
    fn drop(&mut self) {
        // kill_on_drop is set, so child will be killed when dropped
    }
}

/// Parse a tunnel URL from cloudflared output.
fn parse_tunnel_url(line: &str) -> Option<String> {
    // Look for trycloudflare.com URLs specifically — these are quick tunnel URLs.
    // Also accept non-cloudflare custom domain URLs for managed tunnels.
    for word in line.split_whitespace() {
        if !word.starts_with("https://") {
            continue;
        }
        // Clean trailing punctuation and box-drawing characters
        let clean = word.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '/');
        let clean = clean.trim_end_matches('/');

        // Accept trycloudflare.com URLs (quick tunnels)
        if clean.contains(".trycloudflare.com") {
            return Some(clean.to_string());
        }

        // Accept non-cloudflare domains (managed tunnels with custom hostnames)
        if !clean.contains("cloudflare.com") {
            return Some(clean.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tunnel_url() {
        // Standard quick tunnel URL in box
        assert_eq!(
            parse_tunnel_url("2026-04-03T20:31:28Z INF |  https://grass-engagement-ride-medical.trycloudflare.com                                   |"),
            Some("https://grass-engagement-ride-medical.trycloudflare.com".to_string())
        );
        // No URL line
        assert_eq!(
            parse_tunnel_url("2026-04-02T12:00:00Z INF +-----------------------------------------------------------+"),
            None
        );
        // URL in prose
        assert_eq!(
            parse_tunnel_url("Your quick Tunnel has been created! Visit it at (it may take some time to be reachable): https://test-abc-123.trycloudflare.com"),
            Some("https://test-abc-123.trycloudflare.com".to_string())
        );
        // Must NOT match cloudflare docs/marketing URLs
        assert_eq!(
            parse_tunnel_url("following: https://developers.cloudflare.com/cloudflare-one/connections/connect-apps"),
            None
        );
        assert_eq!(
            parse_tunnel_url("Terms of Use (https://www.cloudflare.com/website-terms/)"),
            None
        );
    }
}
