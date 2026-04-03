pub mod quick;
pub mod managed;
pub mod supervisor;

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
