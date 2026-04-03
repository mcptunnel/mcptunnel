use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::basic::BasicAuthCredentials;
use crate::config::{AuthModeConfig, TunnelMode};
use crate::policy::types::PolicyConfig;

/// A mount record representing a registered MCP server endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountRecord {
    pub id: Uuid,
    pub slug: String,
    pub display_name: String,
    pub command: CommandConfig,
    pub cwd: Option<String>,
    pub auth_mode: AuthModeConfig,
    pub basic_credentials: Option<BasicAuthCredentials>,
    pub oauth_config: Option<OAuthMountConfig>,
    pub policy: PolicyConfig,
    pub tunnel_mode: TunnelMode,
    pub hostname: Option<String>,
    pub status: MountStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_probe_at: Option<DateTime<Utc>>,
    pub last_probe_result: Option<ProbeResult>,
    pub env_vars: Vec<(String, String)>,
}

/// Command configuration for the upstream MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandConfig {
    pub program: String,
    pub args: Vec<String>,
}

/// OAuth configuration for a mount.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthMountConfig {
    pub issuer: String,
    pub authorization_servers: Vec<String>,
    pub default_scopes: Vec<String>,
    pub per_tool_scopes: std::collections::HashMap<String, Vec<String>>,
}

/// Mount status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MountStatus {
    Starting,
    Ready,
    Degraded,
    Stopped,
}

impl std::fmt::Display for MountStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Starting => write!(f, "starting"),
            Self::Ready => write!(f, "ready"),
            Self::Degraded => write!(f, "degraded"),
            Self::Stopped => write!(f, "stopped"),
        }
    }
}

/// Result of probing an upstream MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub ok: bool,
    pub protocol_version: Option<String>,
    pub server_name: Option<String>,
    pub server_version: Option<String>,
    pub tools: Vec<String>,
    pub resources: Vec<String>,
    pub prompts: Vec<String>,
    pub capabilities: Option<serde_json::Value>,
    pub error: Option<String>,
    pub stderr_snippet: Option<String>,
}

/// Parameters for creating a new mount.
#[derive(Debug, Clone)]
pub struct MountCreateParams {
    pub slug: Option<String>,
    pub display_name: Option<String>,
    pub command: CommandConfig,
    pub cwd: Option<String>,
    pub tunnel_mode: TunnelMode,
    pub hostname: Option<String>,
    pub auth_mode: AuthModeConfig,
    pub basic_credentials: Option<BasicAuthCredentials>,
    pub oauth_config: Option<OAuthMountConfig>,
    pub policy: PolicyConfig,
    pub env_vars: Vec<(String, String)>,
}
