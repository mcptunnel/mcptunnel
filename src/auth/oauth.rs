use super::types::{OAuthConfig, ProtectedResourceMetadata, TokenClaims};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, RwLock};

struct IssuerKeys {
    keys: HashMap<String, DecodingKey>,
    fetched: Instant,
}
/// Keys are issuer-scoped; no key or credential reuse across authorization servers.
#[derive(Default)]
pub struct JwksCache {
    issuers: RwLock<HashMap<String, IssuerKeys>>,
    refresh: Mutex<()>,
    attempts: RwLock<HashMap<String, Instant>>,
}

pub fn validate_https_url(value: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value).map_err(|_| "invalid URL")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
    {
        return Err("HTTPS URL without userinfo, query or fragment required".into());
    }
    Ok(url)
}

impl JwksCache {
    pub fn new() -> Self {
        Self::default()
    }
    async fn refresh(&self, issuer: &str) -> Result<(), String> {
        let _guard = self.refresh.lock().await;
        if self
            .issuers
            .read()
            .await
            .get(issuer)
            .is_some_and(|e| e.fetched.elapsed() < Duration::from_secs(3600))
        {
            return Ok(());
        }
        if self
            .attempts
            .read()
            .await
            .get(issuer)
            .is_some_and(|t| t.elapsed() < Duration::from_secs(60))
        {
            return Err("issuer discovery temporarily unavailable; retry after 60s".into());
        }
        self.attempts
            .write()
            .await
            .insert(issuer.into(), Instant::now());
        validate_https_url(issuer)?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "HTTP client unavailable")?;
        let discovery = fetch_json(
            &client,
            &format!(
                "{}/.well-known/openid-configuration",
                issuer.trim_end_matches('/')
            ),
        )
        .await?;
        if discovery["issuer"].as_str() != Some(issuer) {
            return Err("OIDC discovery issuer mismatch".into());
        }
        let jwks_uri = discovery["jwks_uri"]
            .as_str()
            .ok_or("OIDC discovery missing jwks_uri")?;
        validate_https_url(jwks_uri)?;
        let jwks = fetch_json(&client, jwks_uri).await?;
        let keys = jwks["keys"].as_array().ok_or("JWKS lacks keys")?;
        if keys.len() > 64 {
            return Err("too many JWKS keys".into());
        }
        let mut key_map = HashMap::new();
        for key in keys {
            if key["kty"] != "RSA"
                || key.get("use").is_some_and(|v| v != "sig")
                || key.get("alg").is_some_and(|v| v != "RS256")
            {
                continue;
            }
            let (Some(kid), Some(n), Some(e)) =
                (key["kid"].as_str(), key["n"].as_str(), key["e"].as_str())
            else {
                continue;
            };
            if n.len() > 4096 || e.len() > 32 || kid.len() > 256 {
                continue;
            }
            let key = DecodingKey::from_rsa_components(n, e).map_err(|_| "invalid RSA key")?;
            if key_map.insert(kid.to_owned(), key).is_some() {
                return Err("duplicate JWKS key ID".into());
            }
        }
        if key_map.is_empty() {
            return Err("no RS256 signing keys in JWKS".into());
        }
        self.issuers.write().await.insert(
            issuer.into(),
            IssuerKeys {
                keys: key_map,
                fetched: Instant::now(),
            },
        );
        Ok(())
    }
}

async fn fetch_json(client: &reqwest::Client, url: &str) -> Result<serde_json::Value, String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "discovery network error")?
        .error_for_status()
        .map_err(|_| "discovery HTTP error")?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "discovery read error")? {
        if bytes.len() + chunk.len() > 256 * 1024 {
            return Err("discovery document too large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "discovery is not JSON".into())
}

pub async fn validate_bearer_token_with_cache(
    header: Option<&str>,
    config: &OAuthConfig,
    cache: &JwksCache,
) -> Result<TokenClaims, TokenValidationError> {
    let token = extract_bearer_token(header)?;
    let jwt_header = decode_header(&token)
        .map_err(|_| TokenValidationError::InvalidToken("invalid JWT header".into()))?;
    if jwt_header.alg != Algorithm::RS256 {
        return Err(TokenValidationError::InvalidToken(
            "only RS256 access tokens are supported".into(),
        ));
    }
    cache
        .refresh(&config.issuer)
        .await
        .map_err(TokenValidationError::InvalidToken)?;
    let keys = cache.issuers.read().await;
    let entry = keys
        .get(&config.issuer)
        .ok_or_else(|| TokenValidationError::InvalidToken("no issuer keys".into()))?;
    let key = match jwt_header.kid {
        Some(kid) => entry.keys.get(&kid),
        None if entry.keys.len() == 1 => entry.keys.values().next(),
        _ => None,
    }
    .ok_or_else(|| {
        TokenValidationError::InvalidToken(
            "unknown signing key; retry after issuer cache refresh".into(),
        )
    })?;
    validate_bearer_token(header, config, key)
}

pub fn validate_bearer_token(
    header: Option<&str>,
    config: &OAuthConfig,
    key: &DecodingKey,
) -> Result<TokenClaims, TokenValidationError> {
    let token = extract_bearer_token(header)?;
    let audience = config.audience.as_ref().ok_or_else(|| {
        TokenValidationError::InvalidToken("resource audience is required".into())
    })?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[&config.issuer]);
    validation.set_audience(&[audience]);
    validation.set_required_spec_claims(&["iss", "sub", "aud", "exp"]);
    validation.validate_nbf = true;
    validation.leeway = 0;
    let claims = decode::<Claims>(&token, key, &validation)
        .map_err(|_| {
            TokenValidationError::InvalidToken("token signature or claims rejected".into())
        })?
        .claims;
    Ok(TokenClaims {
        subject: Some(claims.sub),
        issuer: claims.iss,
        scopes: claims
            .scope
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_owned)
            .collect(),
        expires_at: chrono::DateTime::from_timestamp(claims.exp as i64, 0),
    })
}

pub fn check_scopes(claims: &TokenClaims, required: &[String]) -> bool {
    required.iter().all(|s| claims.scopes.contains(s))
}
pub fn extract_bearer_token(header: Option<&str>) -> Result<String, TokenValidationError> {
    let header = header.ok_or(TokenValidationError::MissingToken)?;
    let (scheme, token) = header
        .split_once(' ')
        .ok_or(TokenValidationError::MissingToken)?;
    if !scheme.eq_ignore_ascii_case("bearer")
        || token.is_empty()
        || token.len() > 16384
        || token.bytes().any(|b| b.is_ascii_whitespace())
    {
        return Err(TokenValidationError::MissingToken);
    }
    Ok(token.into())
}
pub fn build_protected_resource_metadata(
    resource_url: &str,
    config: &OAuthConfig,
) -> ProtectedResourceMetadata {
    ProtectedResourceMetadata {
        resource: resource_url.into(),
        authorization_servers: config.authorization_servers.clone(),
        scopes_supported: config.default_scopes.clone(),
        bearer_methods_supported: Some(vec!["header".into()]),
        resource_documentation: None,
    }
}
pub fn build_www_authenticate(url: &str, scopes: &[String]) -> String {
    format!(
        "Bearer resource_metadata=\"{url}\", scope=\"{}\"",
        scopes.join(" ")
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
            Self::InvalidToken(s) => write!(f, "invalid token: {s}"),
        }
    }
}
#[derive(Deserialize)]
struct Claims {
    sub: String,
    iss: String,
    exp: u64,
    scope: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_and_url_parsing() {
        assert_eq!(extract_bearer_token(Some("BEARER test")).unwrap(), "test");
        for h in [None, Some("Basic abc"), Some("Bearer "), Some("Bearer a b")] {
            assert!(extract_bearer_token(h).is_err());
        }
        for url in [
            "http://example.com",
            "https://user:pass@example.com",
            "https://example.com/#bad",
        ] {
            assert!(validate_https_url(url).is_err());
        }
        assert!(validate_https_url("https://auth.example.com/tenant").is_ok());
    }
    #[test]
    fn claims_are_strictly_resource_bound() {
        use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey};
        let key = rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 2048).unwrap();
        let encoding = jsonwebtoken::EncodingKey::from_rsa_pem(
            key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
        let decoding = DecodingKey::from_rsa_pem(
            key.to_public_key()
                .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
        let config = OAuthConfig {
            issuer: "https://issuer.example".into(),
            audience: Some("https://resource.example/test/mcp".into()),
            authorization_servers: vec![],
            default_scopes: vec![],
        };
        let now = chrono::Utc::now().timestamp();
        let valid = serde_json::json!({"iss":config.issuer,"aud":config.audience,"sub":"alice","exp":now+60,"scope":"read"});
        let sign = |value: &serde_json::Value| {
            format!(
                "Bearer {}",
                jsonwebtoken::encode(
                    &jsonwebtoken::Header::new(Algorithm::RS256),
                    value,
                    &encoding
                )
                .unwrap()
            )
        };
        assert!(validate_bearer_token(Some(&sign(&valid)), &config, &decoding).is_ok());
        for (field, value) in [
            ("iss", serde_json::json!("https://other.example")),
            ("aud", serde_json::json!("https://other-resource.example")),
            ("exp", serde_json::json!(now - 1)),
            ("nbf", serde_json::json!(now + 60)),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = value;
            assert!(validate_bearer_token(Some(&sign(&invalid)), &config, &decoding).is_err());
        }
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove("exp");
        assert!(validate_bearer_token(Some(&sign(&missing)), &config, &decoding).is_err());
    }
}
