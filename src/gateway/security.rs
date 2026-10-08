use crate::{config::AuthModeConfig, daemon::state::AppState, mount::types::MountRecord};
use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// Exact authority and browser-origin checks before auth or protocol processing.
pub async fn validate_boundary(
    state: &AppState,
    port: u16,
    headers: &HeaderMap,
) -> Result<(), Response> {
    if headers.get_all("host").iter().count() != 1 || headers.get_all("origin").iter().count() > 1 {
        return Err(StatusCode::BAD_REQUEST.into_response());
    }
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let tunnel_origin = state.tunnel_origins.read().await.get(&port).cloned();
    let public = tunnel_origin.as_deref().or_else(|| {
        state
            .config
            .tunnel
            .external_origin
            .as_deref()
            .filter(|_| port == state.default_port())
    });
    let public_url = public.and_then(|s| reqwest::Url::parse(s).ok());
    let public_host = public_url
        .as_ref()
        .map(|u| u[url::Position::BeforeHost..url::Position::AfterPort].to_owned());
    let mut bind: std::net::SocketAddr = state
        .config
        .daemon
        .bind
        .parse()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE.into_response())?;
    bind.set_port(port);
    let localhost = format!("localhost:{port}");
    if host != bind.to_string() && host != localhost && public_host.as_deref() != Some(host) {
        return Err((
            StatusCode::FORBIDDEN,
            "unrecognized Host; use the configured loopback address or current public hostname",
        )
            .into_response());
    }
    if let Some(origin) = headers.get("origin") {
        let origin = origin.to_str().unwrap_or("");
        let local = format!("http://{bind}");
        if origin != local
            && origin != format!("http://{localhost}")
            && public_url
                .as_ref()
                .is_none_or(|u| origin != u.origin().ascii_serialization())
        {
            return Err((StatusCode::FORBIDDEN, "untrusted browser Origin").into_response());
        }
    }
    Ok(())
}

pub async fn authorize(
    state: &AppState,
    mount: &MountRecord,
    headers: &HeaderMap,
) -> Result<String, Response> {
    if headers.get_all("authorization").iter().count() > 1 {
        return Err(StatusCode::BAD_REQUEST.into_response());
    }
    let header = headers.get("authorization").and_then(|v| v.to_str().ok());
    match mount.auth_mode {
        AuthModeConfig::Public => Ok("public".into()),
        AuthModeConfig::Inherit => Err((StatusCode::SERVICE_UNAVAILABLE, "inherit auth cannot authenticate stdio; recreate with basic, oauth or explicit public mode").into_response()),
        AuthModeConfig::Basic => {
            let Some(creds) = &mount.basic_credentials else { return Err(StatusCode::SERVICE_UNAVAILABLE.into_response()); };
            crate::auth::basic::validate_basic_auth(header, creds).map_err(|_| {
                (StatusCode::UNAUTHORIZED, [("www-authenticate", "Basic realm=\"mcptunnel\", charset=\"UTF-8\"")], Json(json!({"error":"authentication required"}))).into_response()
            })?;
            Ok(format!("basic:{}", mount.id))
        }
        AuthModeConfig::OAuth => {
            let Some(config) = &mount.oauth_config else { return Err(StatusCode::SERVICE_UNAVAILABLE.into_response()); };
            if config.builtin { return Err((StatusCode::SERVICE_UNAVAILABLE, "built-in OAuth is disabled; configure --oidc-issuer with an external authorization server").into_response()); }
            let resource = state.public_url(&mount.slug).await;
            let oauth = crate::auth::types::OAuthConfig { issuer: config.issuer.clone(), authorization_servers: config.authorization_servers.clone(), default_scopes: config.default_scopes.clone(), audience: Some(resource) };
            let metadata = state.metadata_url(mount).await;
            let challenge = crate::auth::oauth::build_www_authenticate(&metadata, &config.default_scopes);
            let claims = crate::auth::oauth::validate_bearer_token_with_cache(header, &oauth, &state.jwks_cache).await.map_err(|_| {
                (StatusCode::UNAUTHORIZED, [("www-authenticate", challenge)], Json(json!({"error":"invalid or missing access token"}))).into_response()
            })?;
            if !crate::auth::oauth::check_scopes(&claims, &config.default_scopes) {
                return Err((StatusCode::FORBIDDEN, [("www-authenticate", "Bearer error=\"insufficient_scope\"")], Json(json!({"error":"insufficient_scope"}))).into_response());
            }
            let sub = claims.subject.filter(|s| !s.is_empty()).ok_or_else(|| StatusCode::UNAUTHORIZED.into_response())?;
            Ok(format!("oauth:{}:{sub}", config.issuer))
        }
    }
}
