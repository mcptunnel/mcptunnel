use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect};
use axum::Json;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{encode, Algorithm, DecodingKey, EncodingKey, Header};
use rand::Rng;
use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey};
use rsa::traits::PublicKeyParts;
use rsa::RsaPrivateKey;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::config::AuthModeConfig;
use crate::daemon::state::AppState;

// ---------------------------------------------------------------------------
// RSA Key Pair
// ---------------------------------------------------------------------------

/// Holds the RSA key pair used for JWT signing and JWKS.
pub struct SigningKeyPair {
    pub encoding_key: EncodingKey,
    pub decoding_key: DecodingKey,
    /// Base64url-encoded RSA modulus (for JWKS).
    pub n: String,
    /// Base64url-encoded RSA exponent (for JWKS).
    pub e: String,
    /// Key ID.
    pub kid: String,
}

/// Generate an RSA-2048 key pair for JWT signing.
pub fn generate_rsa_keypair() -> Result<SigningKeyPair, String> {
    let mut rng = rand::thread_rng();
    let bits = 2048;
    let private_key = RsaPrivateKey::new(&mut rng, bits)
        .map_err(|e| format!("RSA key generation failed: {e}"))?;

    let private_pem = private_key
        .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
        .map_err(|e| format!("Private key PEM export failed: {e}"))?;
    let public_pem = private_key
        .to_public_key()
        .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
        .map_err(|e| format!("Public key PEM export failed: {e}"))?;

    let encoding_key = EncodingKey::from_rsa_pem(private_pem.as_bytes())
        .map_err(|e| format!("EncodingKey creation failed: {e}"))?;
    let decoding_key = DecodingKey::from_rsa_pem(public_pem.as_bytes())
        .map_err(|e| format!("DecodingKey creation failed: {e}"))?;

    // Extract n and e for JWKS
    let public_key = private_key.to_public_key();
    let n_bytes = public_key.n().to_bytes_be();
    let e_bytes = public_key.e().to_bytes_be();
    let n = URL_SAFE_NO_PAD.encode(&n_bytes);
    let e = URL_SAFE_NO_PAD.encode(&e_bytes);

    let kid = format!("mcptunnel-{}", &uuid::Uuid::new_v4().to_string()[..8]);

    Ok(SigningKeyPair {
        encoding_key,
        decoding_key,
        n,
        e,
        kid,
    })
}

// ---------------------------------------------------------------------------
// OAuth Server State
// ---------------------------------------------------------------------------

/// In-memory state for the built-in OAuth Authorization Server.
pub struct OAuthServerState {
    pub signing_key: SigningKeyPair,
    clients: RwLock<HashMap<String, ClientRegistration>>,
    auth_codes: RwLock<HashMap<String, AuthCodeData>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRegistration {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uris: Vec<String>,
    pub client_name: Option<String>,
    pub token_endpoint_auth_method: String,
    pub grant_types: Vec<String>,
    pub response_types: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct AuthCodeData {
    code: String,
    client_id: String,
    redirect_uri: String,
    code_challenge: String,
    #[allow(dead_code)]
    resource: Option<String>,
    scope: String,
    #[allow(dead_code)]
    slug: String,
    user: String,
    expires_at: DateTime<Utc>,
    consumed: bool,
}

impl OAuthServerState {
    pub fn new() -> Result<Self, String> {
        let signing_key = generate_rsa_keypair()?;
        Ok(Self {
            signing_key,
            clients: RwLock::new(HashMap::new()),
            auth_codes: RwLock::new(HashMap::new()),
        })
    }

    async fn register_client(&self, reg: ClientRegistration) {
        self.clients
            .write()
            .await
            .insert(reg.client_id.clone(), reg);
    }

    async fn get_client(&self, client_id: &str) -> Option<ClientRegistration> {
        self.clients.read().await.get(client_id).cloned()
    }

    async fn store_auth_code(&self, data: AuthCodeData) {
        self.auth_codes
            .write()
            .await
            .insert(data.code.clone(), data);
    }

    async fn consume_auth_code(&self, code: &str) -> Option<AuthCodeData> {
        let mut codes = self.auth_codes.write().await;
        if let Some(data) = codes.get_mut(code) {
            if data.consumed || data.expires_at < Utc::now() {
                codes.remove(code);
                return None;
            }
            data.consumed = true;
            let result = data.clone();
            codes.remove(code);
            Some(result)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn generate_random_string(len: usize) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

fn verify_pkce_s256(code_verifier: &str, code_challenge: &str) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let computed = URL_SAFE_NO_PAD.encode(hasher.finalize());
    computed.as_bytes().ct_eq(code_challenge.as_bytes()).into()
}

/// Build the issuer URL for a mount.
fn build_issuer(tunnel_origin: &str, slug: &str) -> String {
    format!("{}/{}", tunnel_origin.trim_end_matches('/'), slug)
}

// ---------------------------------------------------------------------------
// JWT Claims
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
struct AccessTokenClaims {
    iss: String,
    sub: String,
    aud: String,
    exp: u64,
    iat: u64,
    scope: String,
    jti: String,
}

fn issue_access_token(
    encoding_key: &EncodingKey,
    kid: &str,
    issuer: &str,
    subject: &str,
    audience: &str,
    scope: &str,
    expires_in_secs: i64,
) -> Result<String, String> {
    let now = Utc::now();
    let exp = now + Duration::seconds(expires_in_secs);

    let claims = AccessTokenClaims {
        iss: issuer.to_string(),
        sub: subject.to_string(),
        aud: audience.to_string(),
        exp: exp.timestamp() as u64,
        iat: now.timestamp() as u64,
        scope: scope.to_string(),
        jti: uuid::Uuid::new_v4().to_string(),
    };

    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_string());

    encode(&header, &claims, encoding_key).map_err(|e| format!("JWT encoding failed: {e}"))
}

// ---------------------------------------------------------------------------
// Endpoint Handlers
// ---------------------------------------------------------------------------

/// GET /.well-known/oauth-authorization-server/{slug}
pub async fn handle_as_metadata(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let mount = match state.mount_registry.get(&slug) {
        Ok(Some(m)) => m,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "mount not found"})),
            )
        }
    };

    if mount.auth_mode != AuthModeConfig::OAuth {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "OAuth not configured"})),
        );
    }

    // Only serve AS metadata for built-in mode
    if mount.oauth_config.as_ref().map_or(true, |c| !c.builtin) {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "external OAuth AS configured, not built-in"})),
        );
    }

    let tunnel_origin = state
        .tunnel_origin
        .read()
        .await
        .clone()
        .unwrap_or_else(|| "https://localhost".to_string());
    let issuer = build_issuer(&tunnel_origin, &slug);

    let metadata = build_as_metadata(&issuer);
    (StatusCode::OK, Json(metadata))
}

/// GET /{slug}/.well-known/openid-configuration
pub async fn handle_oidc_discovery(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    // Delegate to same logic
    handle_as_metadata(State(state), Path(slug)).await
}

fn build_as_metadata(issuer: &str) -> serde_json::Value {
    serde_json::json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{}/oauth/authorize", issuer),
        "token_endpoint": format!("{}/oauth/token", issuer),
        "registration_endpoint": format!("{}/oauth/register", issuer),
        "jwks_uri": format!("{}/oauth/jwks", issuer),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none", "client_secret_post"],
        "scopes_supported": ["mcp:read"],
    })
}

/// GET /{slug}/oauth/jwks
pub async fn handle_jwks(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let mount = match state.mount_registry.get(&slug) {
        Ok(Some(m)) => m,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "mount not found"})),
            )
        }
    };

    if mount.auth_mode != AuthModeConfig::OAuth
        || mount.oauth_config.as_ref().map_or(true, |c| !c.builtin)
    {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "built-in OAuth not configured"})),
        );
    }

    let oauth_state = match &state.oauth_server {
        Some(s) => s,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "OAuth server not initialized"})),
            )
        }
    };

    let jwks = serde_json::json!({
        "keys": [{
            "kty": "RSA",
            "use": "sig",
            "alg": "RS256",
            "kid": oauth_state.signing_key.kid,
            "n": oauth_state.signing_key.n,
            "e": oauth_state.signing_key.e,
        }]
    });

    (StatusCode::OK, Json(jwks))
}

// ---------------------------------------------------------------------------
// Dynamic Client Registration (RFC 7591)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DcrRequest {
    redirect_uris: Option<Vec<String>>,
    token_endpoint_auth_method: Option<String>,
    grant_types: Option<Vec<String>>,
    response_types: Option<Vec<String>>,
    client_name: Option<String>,
    #[serde(flatten)]
    _extra: HashMap<String, serde_json::Value>,
}

/// POST /{slug}/oauth/register
pub async fn handle_dcr(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    Json(body): Json<DcrRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let mount = match state.mount_registry.get(&slug) {
        Ok(Some(m)) => m,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "mount not found"})),
            )
        }
    };

    if mount.auth_mode != AuthModeConfig::OAuth
        || mount.oauth_config.as_ref().map_or(true, |c| !c.builtin)
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "built-in OAuth not configured"})),
        );
    }

    let oauth_state = match &state.oauth_server {
        Some(s) => s,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "OAuth server not initialized"})),
            )
        }
    };

    let redirect_uris = match body.redirect_uris {
        Some(uris) if !uris.is_empty() => uris,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_client_metadata",
                    "error_description": "redirect_uris is required"
                })),
            )
        }
    };

    let client_id = uuid::Uuid::new_v4().to_string();
    // Always generate client_secret — ChatGPT expects it even with auth_method "none"
    let client_secret = generate_random_string(48);
    let auth_method = body
        .token_endpoint_auth_method
        .unwrap_or_else(|| "none".to_string());
    let grant_types = body
        .grant_types
        .unwrap_or_else(|| vec!["authorization_code".to_string()]);
    let response_types = body
        .response_types
        .unwrap_or_else(|| vec!["code".to_string()]);

    let registration = ClientRegistration {
        client_id: client_id.clone(),
        client_secret: client_secret.clone(),
        redirect_uris: redirect_uris.clone(),
        client_name: body.client_name.clone(),
        token_endpoint_auth_method: auth_method.clone(),
        grant_types: grant_types.clone(),
        response_types: response_types.clone(),
        created_at: Utc::now(),
    };

    oauth_state.register_client(registration).await;

    info!(slug = %slug, client_id = %client_id, "OAuth: client registered via DCR");

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "client_id": client_id,
            "client_secret": client_secret,
            "client_secret_expires_at": 0,
            "redirect_uris": redirect_uris,
            "token_endpoint_auth_method": auth_method,
            "grant_types": grant_types,
            "response_types": response_types,
            "client_name": body.client_name,
        })),
    )
}

// ---------------------------------------------------------------------------
// Authorization Endpoint
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AuthorizeQuery {
    response_type: Option<String>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    state: Option<String>,
    scope: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    resource: Option<String>,
}

/// GET /{slug}/oauth/authorize — render login form
pub async fn handle_authorize_get(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    Query(params): Query<AuthorizeQuery>,
) -> axum::response::Response {
    let mount = match state.mount_registry.get(&slug) {
        Ok(Some(m)) => m,
        _ => return (StatusCode::NOT_FOUND, "mount not found").into_response(),
    };

    if mount.auth_mode != AuthModeConfig::OAuth
        || mount.oauth_config.as_ref().map_or(true, |c| !c.builtin)
    {
        return (StatusCode::BAD_REQUEST, "built-in OAuth not configured").into_response();
    }

    // Validate client_id
    let client_id = match &params.client_id {
        Some(id) => id.clone(),
        None => return (StatusCode::BAD_REQUEST, "client_id required").into_response(),
    };

    let oauth_state = match &state.oauth_server {
        Some(s) => s,
        None => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "OAuth server not initialized")
                .into_response()
        }
    };

    if oauth_state.get_client(&client_id).await.is_none() {
        return (StatusCode::BAD_REQUEST, "unknown client_id").into_response();
    }

    // Validate required params
    if params.response_type.as_deref() != Some("code") {
        return (StatusCode::BAD_REQUEST, "response_type must be 'code'").into_response();
    }
    if params.code_challenge.is_none() {
        return (StatusCode::BAD_REQUEST, "code_challenge required (PKCE)").into_response();
    }
    if params.code_challenge_method.as_deref() != Some("S256") {
        return (StatusCode::BAD_REQUEST, "code_challenge_method must be S256").into_response();
    }

    let display_name = &mount.display_name;
    let form_html = render_login_form(
        &slug,
        display_name,
        &params.client_id.unwrap_or_default(),
        &params.redirect_uri.unwrap_or_default(),
        &params.state.unwrap_or_default(),
        &params.scope.unwrap_or_default(),
        &params.code_challenge.unwrap_or_default(),
        &params.code_challenge_method.unwrap_or_default(),
        &params.resource.unwrap_or_default(),
        None,
    );

    Html(form_html).into_response()
}

#[derive(Debug, Deserialize)]
pub struct AuthorizeForm {
    username: String,
    password: String,
    client_id: String,
    redirect_uri: String,
    state: String,
    scope: String,
    code_challenge: String,
    code_challenge_method: String,
    resource: String,
    #[allow(dead_code)]
    slug: String,
}

/// POST /{slug}/oauth/authorize — validate credentials and issue auth code
pub async fn handle_authorize_post(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    axum::extract::Form(form): axum::extract::Form<AuthorizeForm>,
) -> axum::response::Response {
    let mount = match state.mount_registry.get(&slug) {
        Ok(Some(m)) => m,
        _ => return (StatusCode::NOT_FOUND, "mount not found").into_response(),
    };

    let oauth_config = match &mount.oauth_config {
        Some(c) if c.builtin => c,
        _ => return (StatusCode::BAD_REQUEST, "built-in OAuth not configured").into_response(),
    };

    let oauth_state = match &state.oauth_server {
        Some(s) => s,
        None => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "OAuth server not initialized")
                .into_response()
        }
    };

    // Validate client
    let client = match oauth_state.get_client(&form.client_id).await {
        Some(c) => c,
        None => return (StatusCode::BAD_REQUEST, "unknown client_id").into_response(),
    };

    // Validate redirect_uri against registration
    if !client.redirect_uris.contains(&form.redirect_uri) {
        return (StatusCode::BAD_REQUEST, "redirect_uri mismatch").into_response();
    }

    // Validate credentials
    let expected_user = oauth_config.builtin_user.as_deref().unwrap_or("");
    let expected_pass = oauth_config.builtin_password.as_deref().unwrap_or("");

    let user_ok: bool = form
        .username
        .as_bytes()
        .ct_eq(expected_user.as_bytes())
        .into();
    let pass_ok: bool = form
        .password
        .as_bytes()
        .ct_eq(expected_pass.as_bytes())
        .into();

    if !user_ok || !pass_ok {
        warn!(slug = %slug, "OAuth: login failed — invalid credentials");
        let form_html = render_login_form(
            &slug,
            &mount.display_name,
            &form.client_id,
            &form.redirect_uri,
            &form.state,
            &form.scope,
            &form.code_challenge,
            &form.code_challenge_method,
            &form.resource,
            Some("Invalid username or password"),
        );
        return Html(form_html).into_response();
    }

    // Validate PKCE method
    if form.code_challenge_method != "S256" {
        return (StatusCode::BAD_REQUEST, "code_challenge_method must be S256").into_response();
    }

    // Generate authorization code
    let code = generate_random_string(64);
    let auth_code = AuthCodeData {
        code: code.clone(),
        client_id: form.client_id.clone(),
        redirect_uri: form.redirect_uri.clone(),
        code_challenge: form.code_challenge.clone(),
        resource: if form.resource.is_empty() {
            None
        } else {
            Some(form.resource.clone())
        },
        scope: form.scope.clone(),
        slug: slug.clone(),
        user: form.username.clone(),
        expires_at: Utc::now() + Duration::minutes(5),
        consumed: false,
    };
    oauth_state.store_auth_code(auth_code).await;

    info!(slug = %slug, user = %form.username, "OAuth: authorization code issued");

    // Redirect back to client
    let mut redirect_url = form.redirect_uri.clone();
    redirect_url.push_str(if redirect_url.contains('?') {
        "&"
    } else {
        "?"
    });
    redirect_url.push_str(&format!("code={code}"));
    if !form.state.is_empty() {
        redirect_url.push_str(&format!("&state={}", form.state));
    }

    Redirect::to(&redirect_url).into_response()
}

// ---------------------------------------------------------------------------
// Token Endpoint
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    grant_type: Option<String>,
    code: Option<String>,
    redirect_uri: Option<String>,
    client_id: Option<String>,
    code_verifier: Option<String>,
    #[allow(dead_code)]
    resource: Option<String>,
    #[allow(dead_code)]
    client_secret: Option<String>,
}

/// POST /{slug}/oauth/token
pub async fn handle_token(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    axum::extract::Form(form): axum::extract::Form<TokenRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let mount = match state.mount_registry.get(&slug) {
        Ok(Some(m)) => m,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "mount not found"})),
            )
        }
    };

    if mount.auth_mode != AuthModeConfig::OAuth
        || mount.oauth_config.as_ref().map_or(true, |c| !c.builtin)
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "built-in OAuth not configured"})),
        );
    }

    let oauth_state = match &state.oauth_server {
        Some(s) => s,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "OAuth server not initialized"})),
            )
        }
    };

    // Validate grant_type
    if form.grant_type.as_deref() != Some("authorization_code") {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "unsupported_grant_type",
                "error_description": "only authorization_code is supported"
            })),
        );
    }

    let code = match &form.code {
        Some(c) => c.clone(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_request",
                    "error_description": "code is required"
                })),
            )
        }
    };

    let code_verifier = match &form.code_verifier {
        Some(v) => v.clone(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_request",
                    "error_description": "code_verifier is required"
                })),
            )
        }
    };

    // Consume the auth code
    let auth_code = match oauth_state.consume_auth_code(&code).await {
        Some(ac) => ac,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_grant",
                    "error_description": "invalid or expired authorization code"
                })),
            )
        }
    };

    // Validate client_id
    if let Some(ref client_id) = form.client_id {
        if client_id != &auth_code.client_id {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_grant",
                    "error_description": "client_id mismatch"
                })),
            );
        }
    }

    // Validate redirect_uri
    if let Some(ref redirect_uri) = form.redirect_uri {
        if redirect_uri != &auth_code.redirect_uri {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_grant",
                    "error_description": "redirect_uri mismatch"
                })),
            );
        }
    }

    // Verify PKCE
    if !verify_pkce_s256(&code_verifier, &auth_code.code_challenge) {
        warn!(slug = %slug, "OAuth: PKCE verification failed");
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_grant",
                "error_description": "PKCE code_verifier verification failed"
            })),
        );
    }

    // Issue access token
    let tunnel_origin = state
        .tunnel_origin
        .read()
        .await
        .clone()
        .unwrap_or_else(|| "https://localhost".to_string());
    let issuer = build_issuer(&tunnel_origin, &slug);
    let scope = if auth_code.scope.is_empty() {
        "mcp:read".to_string()
    } else {
        auth_code.scope.clone()
    };

    let access_token = match issue_access_token(
        &oauth_state.signing_key.encoding_key,
        &oauth_state.signing_key.kid,
        &issuer,
        &auth_code.user,
        &issuer, // audience = issuer for self-contained AS
        &scope,
        3600,
    ) {
        Ok(token) => token,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "server_error",
                    "error_description": format!("token creation failed: {e}")
                })),
            )
        }
    };

    info!(
        slug = %slug,
        user = %auth_code.user,
        "OAuth: access token issued"
    );

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "access_token": access_token,
            "token_type": "Bearer",
            "expires_in": 3600,
            "scope": scope,
        })),
    )
}

// ---------------------------------------------------------------------------
// Protected Resource Metadata (updated for built-in mode)
// ---------------------------------------------------------------------------

/// GET /.well-known/oauth-protected-resource/{slug} (RFC 9728 path-based)
pub async fn handle_prm_path_based(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    crate::gateway::mcp_handler::handle_oauth_metadata(State(state), Path(slug)).await
}

// ---------------------------------------------------------------------------
// HTML Login Form
// ---------------------------------------------------------------------------

/// Escape HTML special characters to prevent XSS.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

fn render_login_form(
    slug: &str,
    display_name: &str,
    client_id: &str,
    redirect_uri: &str,
    state_param: &str,
    scope: &str,
    code_challenge: &str,
    code_challenge_method: &str,
    resource: &str,
    error_msg: Option<&str>,
) -> String {
    // Escape all user-controlled values to prevent XSS
    let slug = html_escape(slug);
    let display_name = html_escape(display_name);
    let client_id = html_escape(client_id);
    let redirect_uri = html_escape(redirect_uri);
    let state_param = html_escape(state_param);
    let scope = html_escape(scope);
    let code_challenge = html_escape(code_challenge);
    let code_challenge_method = html_escape(code_challenge_method);
    let resource = html_escape(resource);

    let error_html = error_msg
        .map(|e| format!(r#"<p style="color:#e53e3e;margin-bottom:16px">{}</p>"#, html_escape(e)))
        .unwrap_or_default();

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Sign in — {display_name}</title>
<style>
*{{margin:0;padding:0;box-sizing:border-box}}
body{{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,sans-serif;background:#f7f8fa;display:flex;align-items:center;justify-content:center;min-height:100vh}}
.card{{background:#fff;border-radius:12px;box-shadow:0 2px 12px rgba(0,0,0,.08);padding:40px;width:100%;max-width:400px}}
h1{{font-size:20px;margin-bottom:8px;color:#1a1a2e}}
.sub{{font-size:14px;color:#666;margin-bottom:24px}}
label{{display:block;font-size:14px;font-weight:500;margin-bottom:4px;color:#333}}
input[type=text],input[type=password]{{width:100%;padding:10px 12px;border:1px solid #ddd;border-radius:8px;font-size:15px;margin-bottom:16px}}
input:focus{{outline:none;border-color:#4a6cf7;box-shadow:0 0 0 3px rgba(74,108,247,.12)}}
button{{width:100%;padding:12px;background:#4a6cf7;color:#fff;border:none;border-radius:8px;font-size:15px;font-weight:600;cursor:pointer}}
button:hover{{background:#3b5de7}}
.footer{{text-align:center;margin-top:16px;font-size:12px;color:#999}}
</style>
</head>
<body>
<div class="card">
<h1>Sign in</h1>
<p class="sub">Authorize access to <strong>{display_name}</strong></p>
{error_html}
<form method="POST" action="/{slug}/oauth/authorize">
<input type="hidden" name="client_id" value="{client_id}">
<input type="hidden" name="redirect_uri" value="{redirect_uri}">
<input type="hidden" name="state" value="{state_param}">
<input type="hidden" name="scope" value="{scope}">
<input type="hidden" name="code_challenge" value="{code_challenge}">
<input type="hidden" name="code_challenge_method" value="{code_challenge_method}">
<input type="hidden" name="resource" value="{resource}">
<input type="hidden" name="slug" value="{slug}">
<label for="username">Username</label>
<input type="text" id="username" name="username" required autocomplete="username">
<label for="password">Password</label>
<input type="password" id="password" name="password" required autocomplete="current-password">
<button type="submit">Sign in</button>
</form>
<p class="footer">Powered by mcptunnel</p>
</div>
</body>
</html>"#
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_rsa_keypair() {
        let kp = generate_rsa_keypair().unwrap();
        assert!(!kp.n.is_empty());
        assert!(!kp.e.is_empty());
        assert!(kp.kid.starts_with("mcptunnel-"));
    }

    #[test]
    fn test_issue_and_decode_token() {
        let kp = generate_rsa_keypair().unwrap();
        let token = issue_access_token(
            &kp.encoding_key,
            &kp.kid,
            "https://example.com/test",
            "admin",
            "https://example.com/test",
            "mcp:read",
            3600,
        )
        .unwrap();

        // Decode and verify
        let mut validation = jsonwebtoken::Validation::new(Algorithm::RS256);
        validation.set_issuer(&["https://example.com/test"]);
        validation.set_audience(&["https://example.com/test"]);

        let decoded = jsonwebtoken::decode::<AccessTokenClaims>(
            &token,
            &kp.decoding_key,
            &validation,
        )
        .unwrap();

        assert_eq!(decoded.claims.iss, "https://example.com/test");
        assert_eq!(decoded.claims.sub, "admin");
        assert_eq!(decoded.claims.aud, "https://example.com/test");
        assert_eq!(decoded.claims.scope, "mcp:read");
        assert!(!decoded.claims.jti.is_empty());
    }

    #[test]
    fn test_verify_pkce_s256_valid() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());
        assert!(verify_pkce_s256(verifier, &challenge));
    }

    #[test]
    fn test_verify_pkce_s256_invalid() {
        assert!(!verify_pkce_s256("wrong_verifier", "wrong_challenge"));
    }

    #[test]
    fn test_generate_random_string() {
        let s = generate_random_string(32);
        assert_eq!(s.len(), 32);
        assert!(s.chars().all(|c| c.is_ascii_alphanumeric()));

        let s2 = generate_random_string(32);
        assert_ne!(s, s2); // probabilistically true
    }

    #[test]
    fn test_build_as_metadata_format() {
        let meta = build_as_metadata("https://example.com/myslug");
        assert_eq!(meta["issuer"], "https://example.com/myslug");
        assert_eq!(
            meta["authorization_endpoint"],
            "https://example.com/myslug/oauth/authorize"
        );
        assert_eq!(
            meta["token_endpoint"],
            "https://example.com/myslug/oauth/token"
        );
        assert_eq!(
            meta["registration_endpoint"],
            "https://example.com/myslug/oauth/register"
        );
        assert_eq!(
            meta["jwks_uri"],
            "https://example.com/myslug/oauth/jwks"
        );

        let methods = meta["code_challenge_methods_supported"]
            .as_array()
            .unwrap();
        assert!(methods.contains(&serde_json::json!("S256")));
    }

    #[tokio::test]
    async fn test_oauth_server_state_client_lifecycle() {
        let state = OAuthServerState::new().unwrap();

        // Register a client
        let reg = ClientRegistration {
            client_id: "test-client".to_string(),
            client_secret: "secret123".to_string(),
            redirect_uris: vec!["https://example.com/callback".to_string()],
            client_name: Some("Test".to_string()),
            token_endpoint_auth_method: "none".to_string(),
            grant_types: vec!["authorization_code".to_string()],
            response_types: vec!["code".to_string()],
            created_at: Utc::now(),
        };
        state.register_client(reg).await;

        let client = state.get_client("test-client").await;
        assert!(client.is_some());
        assert_eq!(client.unwrap().client_secret, "secret123");

        assert!(state.get_client("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_auth_code_lifecycle() {
        let state = OAuthServerState::new().unwrap();

        let code_data = AuthCodeData {
            code: "testcode123".to_string(),
            client_id: "client1".to_string(),
            redirect_uri: "https://example.com/cb".to_string(),
            code_challenge: "challenge".to_string(),
            resource: None,
            scope: "mcp:read".to_string(),
            slug: "test".to_string(),
            user: "admin".to_string(),
            expires_at: Utc::now() + Duration::minutes(5),
            consumed: false,
        };
        state.store_auth_code(code_data).await;

        // Consume should work once
        let consumed = state.consume_auth_code("testcode123").await;
        assert!(consumed.is_some());
        assert_eq!(consumed.unwrap().client_id, "client1");

        // Second consume should fail (already consumed/removed)
        assert!(state.consume_auth_code("testcode123").await.is_none());
    }

    #[tokio::test]
    async fn test_expired_auth_code() {
        let state = OAuthServerState::new().unwrap();

        let code_data = AuthCodeData {
            code: "expiredcode".to_string(),
            client_id: "client1".to_string(),
            redirect_uri: "https://example.com/cb".to_string(),
            code_challenge: "challenge".to_string(),
            resource: None,
            scope: "mcp:read".to_string(),
            slug: "test".to_string(),
            user: "admin".to_string(),
            expires_at: Utc::now() - Duration::minutes(1), // already expired
            consumed: false,
        };
        state.store_auth_code(code_data).await;

        assert!(state.consume_auth_code("expiredcode").await.is_none());
    }

    #[test]
    fn test_build_issuer() {
        assert_eq!(
            build_issuer("https://example.com", "myslug"),
            "https://example.com/myslug"
        );
        assert_eq!(
            build_issuer("https://example.com/", "myslug"),
            "https://example.com/myslug"
        );
    }

    #[test]
    fn test_render_login_form_contains_fields() {
        let html = render_login_form(
            "test",
            "Test Server",
            "client123",
            "https://example.com/cb",
            "state456",
            "mcp:read",
            "challenge789",
            "S256",
            "https://example.com/test",
            None,
        );
        assert!(html.contains("client123"));
        assert!(html.contains("https://example.com/cb"));
        assert!(html.contains("state456"));
        assert!(html.contains("challenge789"));
        assert!(html.contains("Test Server"));
        assert!(!html.contains("color:#e53e3e")); // no error
    }

    #[test]
    fn test_render_login_form_with_error() {
        let html = render_login_form(
            "test",
            "Test",
            "c",
            "u",
            "s",
            "sc",
            "ch",
            "S256",
            "r",
            Some("Bad credentials"),
        );
        assert!(html.contains("Bad credentials"));
        assert!(html.contains("color:#e53e3e"));
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<script>alert(1)</script>"), "&lt;script&gt;alert(1)&lt;/script&gt;");
        assert_eq!(html_escape("a&b"), "a&amp;b");
        assert_eq!(html_escape(r#"x"y"#), "x&quot;y");
        assert_eq!(html_escape("a'b"), "a&#x27;b");
    }

    #[test]
    fn test_render_login_form_xss_escaped() {
        let html = render_login_form(
            "test",
            "<script>alert('xss')</script>",
            "client<>",
            "https://evil.com?a=1&b=2",
            "state\"injection",
            "scope",
            "challenge",
            "S256",
            "resource",
            None,
        );
        // Script tags must be escaped
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        // Quotes must be escaped in attribute values
        assert!(!html.contains("state\"injection"));
        assert!(html.contains("state&quot;injection"));
    }
}
