use serde::{Deserialize, Serialize};

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
    #[allow(dead_code)]
    pub issuer: String,
    pub scopes: Vec<String>,
    #[allow(dead_code)]
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
