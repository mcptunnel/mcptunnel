pub mod managed;
pub mod quick;
pub mod supervisor;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TunnelMode;
    #[test]
    fn free_and_unavailable_managed_modes_fail_closed() {
        assert!(validate_selection(TunnelMode::Quick, None).is_ok());
        assert!(validate_selection(TunnelMode::Local, None).is_ok());
        assert!(validate_selection(TunnelMode::Quick, Some("stable.example")).is_err());
        assert!(validate_selection(TunnelMode::Managed, Some("stable.example")).is_err());
        assert!(validate_selection(TunnelMode::Managed, None).is_err());
    }
}

/// No local config can authorize official hosted provisioning. There is no paid backend yet.
pub fn validate_selection(
    mode: crate::config::TunnelMode,
    hostname: Option<&str>,
) -> Result<(), String> {
    match mode {
        crate::config::TunnelMode::Managed => Err("managed stable URLs require an authenticated server-side paid entitlement and hosted provisioning; that service is not implemented. Use quick for temporary URLs or local with infrastructure you operate".into()),
        _ if hostname.is_some() => Err("--hostname cannot assign a stable URL in free quick/local mode; configure independently operated infrastructure outside mcptunnel".into()),
        _ => Ok(()),
    }
}

use serde::{Deserialize, Serialize};

/// Tunnel status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelStatus {
    pub mode: String,
    pub status: TunnelState,
    pub public_origin: Option<String>,
    pub cloudflared_pid: Option<u32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TunnelState {
    Starting,
    Ready,
    Reconnecting,
    Failed,
    Stopped,
}

impl std::fmt::Display for TunnelState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Starting => write!(f, "starting"),
            Self::Ready => write!(f, "ready"),
            Self::Reconnecting => write!(f, "reconnecting"),
            Self::Failed => write!(f, "failed"),
            Self::Stopped => write!(f, "stopped"),
        }
    }
}
