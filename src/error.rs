use thiserror::Error;

/// Exit codes per FR-CLI-4.
pub mod exit_codes {
    pub const SUCCESS: i32 = 0;
    pub const CLI_USAGE: i32 = 2;
    pub const DAEMON_UNAVAILABLE: i32 = 10;
    pub const CLOUDFLARED_MISSING: i32 = 11;
    pub const TUNNEL_CREATE_FAILED: i32 = 12;
    pub const UPSTREAM_SPAWN_FAILED: i32 = 20;
    pub const UPSTREAM_PROBE_FAILED: i32 = 21;
    pub const MOUNT_NOT_FOUND: i32 = 22;
    pub const MOUNT_POLICY_VIOLATION: i32 = 23;
    pub const OAUTH_CONFIG_INVALID: i32 = 30;
    pub const TOKEN_VERIFICATION_FAILED: i32 = 31;
    pub const INTERNAL_GATEWAY_ERROR: i32 = 40;
}

#[derive(Debug, Error)]
pub enum McpTunnelError {
    #[error("CLI usage error: {0}")]
    CliUsage(String),

    #[error("Daemon unavailable: {0}")]
    DaemonUnavailable(String),

    #[error("cloudflared not found or invalid: {0}")]
    CloudflaredMissing(String),

    #[error("Tunnel creation failed: {0}")]
    TunnelCreateFailed(String),

    #[error("Upstream command spawn failed: {0}")]
    UpstreamSpawnFailed(String),

    #[error("Upstream probe failed: {0}")]
    UpstreamProbeFailed(String),

    #[error("Mount not found: {0}")]
    MountNotFound(String),

    #[error("Mount policy violation: {0}")]
    MountPolicyViolation(String),

    #[error("OAuth configuration invalid: {0}")]
    OAuthConfigInvalid(String),

    #[error("Token verification failed: {0}")]
    TokenVerificationFailed(String),

    #[error("Internal gateway error: {0}")]
    InternalGatewayError(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

impl McpTunnelError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::CliUsage(_) => exit_codes::CLI_USAGE,
            Self::DaemonUnavailable(_) => exit_codes::DAEMON_UNAVAILABLE,
            Self::CloudflaredMissing(_) => exit_codes::CLOUDFLARED_MISSING,
            Self::TunnelCreateFailed(_) => exit_codes::TUNNEL_CREATE_FAILED,
            Self::UpstreamSpawnFailed(_) => exit_codes::UPSTREAM_SPAWN_FAILED,
            Self::UpstreamProbeFailed(_) => exit_codes::UPSTREAM_PROBE_FAILED,
            Self::MountNotFound(_) => exit_codes::MOUNT_NOT_FOUND,
            Self::MountPolicyViolation(_) => exit_codes::MOUNT_POLICY_VIOLATION,
            Self::OAuthConfigInvalid(_) => exit_codes::OAUTH_CONFIG_INVALID,
            Self::TokenVerificationFailed(_) => exit_codes::TOKEN_VERIFICATION_FAILED,
            Self::InternalGatewayError(_) => exit_codes::INTERNAL_GATEWAY_ERROR,
            _ => exit_codes::INTERNAL_GATEWAY_ERROR,
        }
    }
}

pub type Result<T> = std::result::Result<T, McpTunnelError>;
