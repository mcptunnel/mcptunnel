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
    let mut checks = configuration_checks(config);

    // Check cloudflared
    let mut command = tokio::process::Command::new(&config.tunnel.cloudflared_path);
    command.arg("--version").kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let cf_result = tokio::time::timeout(std::time::Duration::from_secs(5), command.output()).await;

    match cf_result {
        Ok(Ok(output)) if output.status.success() => {
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
                status: if config.tunnel.default_mode == crate::config::TunnelMode::Quick {
                    DoctorStatus::Fail
                } else {
                    DoctorStatus::Warn
                },
                detail: format!(
                    "cloudflared returned error at '{}'",
                    config.tunnel.cloudflared_path
                ),
            });
        }
        Err(_) => {
            checks.push(DoctorCheck {
                name: "cloudflared".to_string(),
                status: if config.tunnel.default_mode == crate::config::TunnelMode::Quick {
                    DoctorStatus::Fail
                } else {
                    DoctorStatus::Warn
                },
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

pub fn configuration_checks(config: &Config) -> Vec<DoctorCheck> {
    let loopback = config
        .daemon
        .bind
        .parse::<std::net::SocketAddr>()
        .is_ok_and(|addr| addr.ip().is_loopback());
    let mut checks = vec![DoctorCheck { name:"bind".into(),status:if loopback {DoctorStatus::Pass} else {DoctorStatus::Fail},detail:"daemon.bind must use a loopback IP:port; route an external proxy to this address".into() },
        DoctorCheck {name:"protocol_headers".into(),status:DoctorStatus::Warn,detail:"Current MCP: set Content-Type: application/json, Accept: application/json, text/event-stream, MCP-Protocol-Version: 2026-07-28, Mcp-Method and (for named operations) Mcp-Name. Match these to params/_meta and the body; supply required Mcp-Param-* schema headers. Legacy: initialize and use returned session ID/version. See docs/CLIENT_COMPATIBILITY.md.".into()}];
    match config.tunnel.default_mode {
        crate::config::TunnelMode::Quick => checks.push(DoctorCheck {name:"quick_sse".into(),status:DoctorStatus::Warn,detail:"Quick URLs change on restart; synchronous JSON only. Streaming/progress requires independently hosted SSE-capable transport. Update client URL and OAuth resource audience after reconnect.".into()}),
        crate::config::TunnelMode::Managed => checks.push(DoctorCheck {name:"managed_entitlement".into(),status:DoctorStatus::Fail,detail:"Managed hosting is unavailable: a trusted paid entitlement/provisioning backend is required. Choose quick or independently hosted local mode.".into()}),
        crate::config::TunnelMode::Local => checks.push(DoctorCheck {name:"local_hosting".into(),status:DoctorStatus::Warn,detail:"No managed tunnel is provisioned. Independently configure HTTPS, preserve Host/auth/MCP headers, and disable proxy buffering for request SSE.".into()}),
    }
    checks
}

pub fn mount_checks(mount: &crate::mount::types::MountRecord) -> Vec<DoctorCheck> {
    use crate::config::AuthModeConfig;
    let mut checks = Vec::new();
    let (status, detail) = match mount.auth_mode {
        AuthModeConfig::Public => (DoctorStatus::Warn,"Public endpoint: restrict tool policy before sharing. Anyone with the URL can invoke allowed operations."),
        AuthModeConfig::Basic if mount.basic_credentials.is_some() => (DoctorStatus::Pass,"Basic requires the Authorization header printed at creation; URL userinfo is unsupported. Claude documents fixed Authorization headers for limited beta organizations; ChatGPT Basic support and actual client E2E remain unverified. See docs/CLIENT_COMPATIBILITY.md."),
        AuthModeConfig::Basic => (DoctorStatus::Fail,"Basic credentials missing; recreate the mount."),
        AuthModeConfig::Inherit => (DoctorStatus::Fail,"stdio authentication cannot protect HTTP; recreate using Basic, external OAuth or explicit public mode."),
        AuthModeConfig::OAuth if mount.oauth_config.as_ref().is_some_and(|c| !c.builtin && crate::auth::oauth::validate_https_url(&c.issuer).is_ok()) => (DoctorStatus::Warn,"External OAuth: AS must support resource indicators, PKCE and CIMD or safe registration. RS256 JWT needs iss/sub/aud/exp and required scopes; aud is the exact current /slug/mcp URL. Browser authorization E2E is not established by configuration."),
        AuthModeConfig::OAuth => (DoctorStatus::Fail,"Built-in/missing/invalid OAuth configuration; recreate with --oidc-issuer https://issuer and required --scope values."),
    };
    checks.push(DoctorCheck {
        name: "authentication".into(),
        status,
        detail: detail.into(),
    });
    if let Some(probe) = &mount.last_probe_result {
        let compatible = probe
            .protocol_version
            .as_deref()
            .is_some_and(|v| crate::gateway::mcp_handler::SUPPORTED.contains(&v));
        checks.push(DoctorCheck {name:"upstream_protocol".into(),status:if probe.ok && compatible {DoctorStatus::Pass} else {DoctorStatus::Fail},detail:format!("Cached probe revision: {}. Modern discovery or legacy initialize must return a supported version and valid advertised lists; stdout must contain only newline-delimited JSON-RPC. Use an absolute executable path on Windows and verify cwd/env.",probe.protocol_version.as_deref().unwrap_or("unknown"))});
    } else {
        checks.push(DoctorCheck {name:"upstream_protocol".into(),status:DoctorStatus::Warn,detail:"No probe: use eager probing to establish capabilities and x-mcp-header schemas before connecting.".into()});
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
