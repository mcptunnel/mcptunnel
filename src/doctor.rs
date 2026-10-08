use crate::config::Config;
use serde_json::json;

/// Doctor check result.
pub struct DoctorCheck {
    pub name: String,
    pub status: DoctorStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Copy)]
pub enum DoctorStatus {
    Pass,
    Warn,
    Fail,
}

impl std::fmt::Display for DoctorStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pass => write!(f, "PASS"),
            Self::Warn => write!(f, "WARN"),
            Self::Fail => write!(f, "FAIL"),
        }
    }
}

/// Run local environment doctor checks (no daemon required).
pub async fn run_local_doctor(config: &Config) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();

    // Check cloudflared
    let cf_result = tokio::process::Command::new(&config.tunnel.cloudflared_path)
        .arg("--version")
        .output()
        .await;

    match cf_result {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            checks.push(DoctorCheck {
                name: "cloudflared".to_string(),
                status: DoctorStatus::Pass,
                detail: format!("found: {}", version.trim()),
            });
        }
        Ok(_) => {
            checks.push(DoctorCheck {
                name: "cloudflared".to_string(),
                status: DoctorStatus::Fail,
                detail: format!(
                    "cloudflared returned error at '{}'",
                    config.tunnel.cloudflared_path
                ),
            });
        }
        Err(_) => {
            checks.push(DoctorCheck {
                name: "cloudflared".to_string(),
                status: DoctorStatus::Fail,
                detail: format!(
                    "cloudflared not found at '{}'",
                    config.tunnel.cloudflared_path
                ),
            });
        }
    }

    // Check state directory
    let state_dir = crate::config::state_dir();
    if state_dir.exists() {
        checks.push(DoctorCheck {
            name: "state_dir".to_string(),
            status: DoctorStatus::Pass,
            detail: format!("exists: {}", state_dir.display()),
        });
    } else {
        checks.push(DoctorCheck {
            name: "state_dir".to_string(),
            status: DoctorStatus::Warn,
            detail: format!("will be created: {}", state_dir.display()),
        });
    }

    // Check config file
    let config_path = crate::config::config_path();
    if config_path.exists() {
        checks.push(DoctorCheck {
            name: "config".to_string(),
            status: DoctorStatus::Pass,
            detail: format!("loaded: {}", config_path.display()),
        });
    } else {
        checks.push(DoctorCheck {
            name: "config".to_string(),
            status: DoctorStatus::Warn,
            detail: "no config file; using defaults".to_string(),
        });
    }

    // Check socket
    let socket_path = crate::config::socket_path();
    if socket_path.exists() {
        checks.push(DoctorCheck {
            name: "daemon_socket".to_string(),
            status: DoctorStatus::Pass,
            detail: format!("socket exists: {}", socket_path.display()),
        });
    } else {
        checks.push(DoctorCheck {
            name: "daemon_socket".to_string(),
            status: DoctorStatus::Warn,
            detail: "daemon not running (no socket)".to_string(),
        });
    }

    // Check for cloudflared config.yaml that might interfere with quick tunnels
    if config.tunnel.quick_fail_if_config_yaml_present {
        let cf_config = std::env::var("HOME")
            .ok()
            .map(|h| std::path::PathBuf::from(h).join(".cloudflared/config.yaml"));
        if let Some(cf_path) = cf_config {
            if cf_path.exists() {
                checks.push(DoctorCheck {
                    name: "cloudflared_config".to_string(),
                    status: DoctorStatus::Warn,
                    detail: "~/.cloudflared/config.yaml exists; may interfere with quick tunnels"
                        .to_string(),
                });
            }
        }
    }

    checks
}

/// Format doctor checks for human output.
pub fn format_checks_human(checks: &[DoctorCheck]) -> String {
    checks
        .iter()
        .map(|c| format!("{:4}  {}", c.status, c.detail))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format doctor checks for JSON output.
pub fn format_checks_json(checks: &[DoctorCheck]) -> serde_json::Value {
    let items: Vec<serde_json::Value> = checks
        .iter()
        .map(|c| {
            json!({
                "check": c.name,
                "status": c.status.to_string(),
                "detail": c.detail,
            })
        })
        .collect();
    json!({ "checks": items })
}
