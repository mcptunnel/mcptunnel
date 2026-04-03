use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::types::{OAuthConfig, ProtectedResourceMetadata, TokenClaims};

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
fn extract_bearer_token(header: Option<&str>) -> Result<String, TokenValidationError> {
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

/// Build a tool-level auth challenge error result for MCP.
pub fn build_tool_auth_challenge(
    request_id: serde_json::Value,
    resource_metadata_url: &str,
    error_description: &str,
) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": {
            "content": [{
                "type": "text",
                "text": "Authentication required."
            }],
            "_meta": {
                "mcp/www_authenticate": [
                    format!("Bearer resource_metadata=\"{resource_metadata_url}\", error=\"insufficient_scope\", error_description=\"{error_description}\"")
                ]
            },
            "isError": true
        }
    })
}

#[derive(Debug)]
pub enum TokenValidationError {
    MissingToken,
    InvalidToken(String),
    InsufficientScope,
}

impl std::fmt::Display for TokenValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingToken => write!(f, "missing bearer token"),
            Self::InvalidToken(e) => write!(f, "invalid token: {e}"),
            Self::InsufficientScope => write!(f, "insufficient scope"),
        }
    }
}

/// JWT claims structure.
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: Option<String>,
    iss: Option<String>,
    aud: Option<serde_json::Value>,
    exp: Option<u64>,
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

    #[test]
    fn test_build_tool_auth_challenge() {
        let result = build_tool_auth_challenge(
            json!(4),
            "https://mcp.example.com/painter/.well-known/oauth-protected-resource",
            "Login required",
        );
        assert_eq!(result["result"]["isError"], true);
        assert!(result["result"]["_meta"]["mcp/www_authenticate"].is_array());
    }
}
