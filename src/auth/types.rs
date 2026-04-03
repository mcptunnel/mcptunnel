use serde::{Deserialize, Serialize};

/// Auth mode for a mount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    /// No auth enforcement; inject `noauth` where needed.
    Public,
    /// Preserve upstream auth metadata as-is.
    Inherit,
    /// Gateway-managed OAuth-compatible endpoint protection.
    OAuth,
}

impl std::fmt::Display for AuthMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Public => write!(f, "public"),
            Self::Inherit => write!(f, "inherit"),
            Self::OAuth => write!(f, "oauth"),
        }
    }
}

/// OAuth validation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub issuer: String,
    pub authorization_servers: Vec<String>,
    pub default_scopes: Vec<String>,
    pub audience: Option<String>,
}

/// Result of token validation.
#[derive(Debug, Clone)]
pub struct TokenClaims {
    pub subject: Option<String>,
    pub issuer: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Protected resource metadata response (RFC 9728).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectedResourceMetadata {
    pub resource: String,
    pub authorization_servers: Vec<String>,
    pub scopes_supported: Vec<String>,
    pub resource_documentation: Option<String>,
}
