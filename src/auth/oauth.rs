use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::{info, warn};

use super::types::{OAuthConfig, ProtectedResourceMetadata, TokenClaims};

/// Cached JWKS keys for token validation.
pub struct JwksCache {
    /// Map from key ID (kid) to DecodingKey.
    keys: RwLock<HashMap<String, DecodingKey>>,
    /// The JWKS endpoint URL.
    jwks_uri: RwLock<Option<String>>,
    /// Last fetch time.
    last_fetched: RwLock<Option<chrono::DateTime<chrono::Utc>>>,
}

impl JwksCache {
    pub fn new() -> Self {
        Self {
            keys: RwLock::new(HashMap::new()),
            jwks_uri: RwLock::new(None),
            last_fetched: RwLock::new(None),
        }
    }

    /// Fetch JWKS from the issuer's discovery endpoint and cache the keys.
    pub async fn refresh(&self, issuer: &str) -> Result<(), String> {
        // Discover JWKS URI from OpenID Connect discovery
        let discovery_url = format!("{}/.well-known/openid-configuration", issuer.trim_end_matches('/'));

        let client = reqwest::Client::new();
        let discovery: serde_json::Value = client
            .get(&discovery_url)
            .send()
            .await
            .map_err(|e| format!("OIDC discovery fetch failed: {e}"))?
            .json()
            .await
            .map_err(|e| format!("OIDC discovery parse failed: {e}"))?;

        let jwks_uri = discovery
            .get("jwks_uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "No jwks_uri in OIDC discovery".to_string())?;

        // Fetch JWKS
        let jwks: JwksResponse = client
            .get(jwks_uri)
            .send()
            .await
            .map_err(|e| format!("JWKS fetch failed: {e}"))?
            .json()
            .await
            .map_err(|e| format!("JWKS parse failed: {e}"))?;

        // Parse keys
        let mut key_map = HashMap::new();
        for key in &jwks.keys {
            if key.kty != "RSA" {
                continue;
            }
            let Some(ref kid) = key.kid else { continue };
            let Some(ref n) = key.n else { continue };
            let Some(ref e) = key.e else { continue };

            match DecodingKey::from_rsa_components(n, e) {
                Ok(dk) => {
                    key_map.insert(kid.clone(), dk);
                }
                Err(err) => {
                    warn!(kid = kid, error = %err, "Failed to parse RSA key from JWKS");
                }
            }
        }

        if key_map.is_empty() {
            return Err("No usable RSA keys found in JWKS".to_string());
        }

        info!(keys = key_map.len(), jwks_uri = jwks_uri, "JWKS cache refreshed");
        *self.keys.write().await = key_map;
        *self.jwks_uri.write().await = Some(jwks_uri.to_string());
        *self.last_fetched.write().await = Some(chrono::Utc::now());
        Ok(())
    }

    /// Get a decoding key by key ID.
    pub async fn get_key(&self, kid: &str) -> Option<DecodingKey> {
        self.keys.read().await.get(kid).cloned()
    }

    /// Get any available key (when token has no kid).
    pub async fn get_any_key(&self) -> Option<DecodingKey> {
        self.keys.read().await.values().next().cloned()
    }

    /// Check if cache needs refresh (older than 1 hour).
    pub async fn needs_refresh(&self) -> bool {
        match *self.last_fetched.read().await {
            Some(t) => {
                let age = chrono::Utc::now() - t;
                age.num_seconds() > 3600
            }
            None => true,
        }
    }

    /// Check if cache has any keys.
    pub async fn has_keys(&self) -> bool {
        !self.keys.read().await.is_empty()
    }
}

/// JWKS response structure.
#[derive(Debug, Deserialize)]
struct JwksResponse {
    keys: Vec<JwkKey>,
}

/// A single JWK key.
#[derive(Debug, Deserialize)]
struct JwkKey {
    kty: String,
    kid: Option<String>,
    n: Option<String>,
    e: Option<String>,
    #[serde(rename = "use")]
    #[allow(dead_code)]
    use_: Option<String>,
    #[allow(dead_code)]
    alg: Option<String>,
}

/// Validate a bearer token using the JWKS cache.
pub async fn validate_bearer_token_with_cache(
    auth_header: Option<&str>,
    config: &OAuthConfig,
    cache: &JwksCache,
) -> Result<TokenClaims, TokenValidationError> {
    let token = extract_bearer_token(auth_header)?;

    // Refresh JWKS if needed
    if cache.needs_refresh().await {
        if let Err(e) = cache.refresh(&config.issuer).await {
            // If we have cached keys, warn and continue; otherwise fail
            if !cache.has_keys().await {
                return Err(TokenValidationError::InvalidToken(
                    format!("JWKS fetch failed and no cached keys: {e}"),
                ));
            }
            warn!(error = %e, "JWKS refresh failed, using cached keys");
        }
    }

    // Decode header to get kid
    let header = decode_header(&token)
        .map_err(|e| TokenValidationError::InvalidToken(format!("invalid token header: {e}")))?;

    // Find the right key
    let decoding_key = if let Some(ref kid) = header.kid {
        cache.get_key(kid).await
            .ok_or_else(|| TokenValidationError::InvalidToken(
                format!("key ID '{kid}' not found in JWKS")
            ))?
    } else {
        cache.get_any_key().await
            .ok_or_else(|| TokenValidationError::InvalidToken(
                "no keys in JWKS cache".to_string()
            ))?
    };

    validate_bearer_token(Some(&format!("Bearer {token}")), config, &decoding_key)
}

/// Validate a bearer token from the Authorization header.
pub fn validate_bearer_token(
    auth_header: Option<&str>,
    config: &OAuthConfig,
    decoding_key: &DecodingKey,
) -> Result<TokenClaims, TokenValidationError> {
    let token = extract_bearer_token(auth_header)?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[&config.issuer]);
    if let Some(ref aud) = config.audience {
        validation.set_audience(&[aud]);
    } else {
        validation.validate_aud = false;
    }

    let token_data = decode::<Claims>(&token, decoding_key, &validation)
        .map_err(|e| TokenValidationError::InvalidToken(e.to_string()))?;

    let claims = token_data.claims;
    let scopes: Vec<String> = claims
        .scope
        .unwrap_or_default()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    Ok(TokenClaims {
        subject: claims.sub,
        issuer: claims.iss.unwrap_or_default(),
        scopes,
        expires_at: claims.exp.map(|e| {
            chrono::DateTime::from_timestamp(e as i64, 0)
                .unwrap_or_else(|| chrono::Utc::now())
        }),
    })
}

/// Check if required scopes are present.
pub fn check_scopes(claims: &TokenClaims, required: &[String]) -> bool {
    if required.is_empty() {
        return true;
    }
    required.iter().all(|r| claims.scopes.contains(r))
}

/// Extract bearer token from Authorization header.
pub fn extract_bearer_token(header: Option<&str>) -> Result<String, TokenValidationError> {
    let header = header.ok_or(TokenValidationError::MissingToken)?;
    let token = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
        .ok_or(TokenValidationError::MissingToken)?;
    Ok(token.to_string())
}

/// Build protected resource metadata response.
pub fn build_protected_resource_metadata(
    resource_url: &str,
    config: &OAuthConfig,
) -> ProtectedResourceMetadata {
    ProtectedResourceMetadata {
        resource: resource_url.to_string(),
        authorization_servers: config.authorization_servers.clone(),
        scopes_supported: config.default_scopes.clone(),
        resource_documentation: Some(resource_url.to_string()),
    }
}

/// Build a WWW-Authenticate challenge header value.
pub fn build_www_authenticate(resource_metadata_url: &str, scopes: &[String]) -> String {
    let scope_str = scopes.join(" ");
    format!(
        "Bearer resource_metadata=\"{resource_metadata_url}\", scope=\"{scope_str}\""
    )
}

#[derive(Debug)]
pub enum TokenValidationError {
    MissingToken,
    InvalidToken(String),
}

impl std::fmt::Display for TokenValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingToken => write!(f, "missing bearer token"),
            Self::InvalidToken(e) => write!(f, "invalid token: {e}"),
        }
    }
}

/// JWT claims structure.
#[derive(Debug, Deserialize)]
struct Claims {
    sub: Option<String>,
    iss: Option<String>,
    #[allow(dead_code)]
    aud: Option<serde_json::Value>,
    exp: Option<u64>,
    #[allow(dead_code)]
    nbf: Option<u64>,
    scope: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_bearer_token() {
        assert_eq!(
            extract_bearer_token(Some("Bearer abc123")).unwrap(),
            "abc123"
        );
        assert!(extract_bearer_token(None).is_err());
        assert!(extract_bearer_token(Some("Basic abc")).is_err());
    }

    #[test]
    fn test_check_scopes() {
        let claims = TokenClaims {
            subject: None,
            issuer: "test".to_string(),
            scopes: vec!["read".to_string(), "write".to_string()],
            expires_at: None,
        };
        assert!(check_scopes(&claims, &["read".to_string()]));
        assert!(check_scopes(&claims, &["read".to_string(), "write".to_string()]));
        assert!(!check_scopes(&claims, &["admin".to_string()]));
        assert!(check_scopes(&claims, &[]));
    }

    #[test]
    fn test_build_protected_resource_metadata() {
        let config = OAuthConfig {
            issuer: "https://auth.example.com".to_string(),
            authorization_servers: vec!["https://auth.example.com".to_string()],
            default_scopes: vec!["painter.use".to_string()],
            audience: None,
        };
        let meta = build_protected_resource_metadata("https://mcp.example.com/painter", &config);
        assert_eq!(meta.resource, "https://mcp.example.com/painter");
        assert_eq!(meta.authorization_servers, vec!["https://auth.example.com"]);
    }

    #[test]
    fn test_build_www_authenticate() {
        let header = build_www_authenticate(
            "https://mcp.example.com/painter/.well-known/oauth-protected-resource",
            &["painter.use".to_string()],
        );
        assert!(header.contains("Bearer"));
        assert!(header.contains("resource_metadata="));
        assert!(header.contains("painter.use"));
    }

    #[tokio::test]
    async fn test_jwks_cache_needs_refresh() {
        let cache = JwksCache::new();
        assert!(cache.needs_refresh().await);
        assert!(!cache.has_keys().await);
    }
}
