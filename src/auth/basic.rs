use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use rand::Rng;
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

/// Credentials for HTTP Basic Auth (RFC 7617).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicAuthCredentials {
    pub username: String,
    pub password: String,
}

/// Generate new Basic Auth credentials.
/// Username is always `mct`; password is 16 random hex chars (64 bits of entropy).
pub fn generate_credentials() -> BasicAuthCredentials {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 8] = rng.gen();
    BasicAuthCredentials {
        username: "mct".to_string(),
        password: hex::encode(bytes),
    }
}

/// Validate an `Authorization` header against expected credentials.
/// Returns `Ok(())` if valid, `Err(reason)` if not.
pub fn validate_basic_auth(
    auth_header: Option<&str>,
    expected: &BasicAuthCredentials,
) -> Result<(), BasicAuthError> {
    let header = auth_header.ok_or(BasicAuthError::MissingHeader)?;

    let encoded = header
        .strip_prefix("Basic ")
        .or_else(|| header.strip_prefix("basic "))
        .ok_or(BasicAuthError::InvalidScheme)?;

    // Length guard: reject oversized headers before allocating a decode buffer
    let trimmed = encoded.trim();
    if trimmed.len() > 256 {
        return Err(BasicAuthError::InvalidEncoding);
    }

    let decoded_bytes = BASE64
        .decode(trimmed)
        .map_err(|_| BasicAuthError::InvalidEncoding)?;

    let decoded = String::from_utf8(decoded_bytes)
        .map_err(|_| BasicAuthError::InvalidEncoding)?;

    let (user, pass) = decoded
        .split_once(':')
        .ok_or(BasicAuthError::InvalidFormat)?;

    // Constant-time comparison to prevent timing side-channels
    let user_ok = user.as_bytes().ct_eq(expected.username.as_bytes());
    let pass_ok = pass.as_bytes().ct_eq(expected.password.as_bytes());
    if bool::from(user_ok & pass_ok) {
        Ok(())
    } else {
        Err(BasicAuthError::InvalidCredentials)
    }
}

/// Build a URL with embedded credentials: `https://user:pass@host/path`
pub fn build_credentials_url(base_url: &str, creds: &BasicAuthCredentials) -> String {
    if let Some(rest) = base_url.strip_prefix("https://") {
        format!("https://{}:{}@{}", creds.username, creds.password, rest)
    } else if let Some(rest) = base_url.strip_prefix("http://") {
        format!("http://{}:{}@{}", creds.username, creds.password, rest)
    } else {
        // Fallback: prepend credentials before the URL as-is
        format!("{}:{}@{}", creds.username, creds.password, base_url)
    }
}

#[derive(Debug)]
pub enum BasicAuthError {
    MissingHeader,
    InvalidScheme,
    InvalidEncoding,
    InvalidFormat,
    InvalidCredentials,
}

impl std::fmt::Display for BasicAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingHeader => write!(f, "missing Authorization header"),
            Self::InvalidScheme => write!(f, "expected Basic auth scheme"),
            Self::InvalidEncoding => write!(f, "invalid base64 encoding"),
            Self::InvalidFormat => write!(f, "invalid credentials format"),
            Self::InvalidCredentials => write!(f, "invalid credentials"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_credentials() {
        let creds = generate_credentials();
        assert_eq!(creds.username, "mct");
        assert_eq!(creds.password.len(), 16);
        // All hex chars
        assert!(creds.password.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_unique() {
        let a = generate_credentials();
        let b = generate_credentials();
        assert_ne!(a.password, b.password);
    }

    #[test]
    fn test_validate_valid() {
        let creds = BasicAuthCredentials {
            username: "mct".to_string(),
            password: "abcdef0123456789".to_string(),
        };
        let encoded = BASE64.encode("mct:abcdef0123456789");
        let header = format!("Basic {encoded}");
        assert!(validate_basic_auth(Some(&header), &creds).is_ok());
    }

    #[test]
    fn test_validate_missing_header() {
        let creds = generate_credentials();
        assert!(matches!(
            validate_basic_auth(None, &creds),
            Err(BasicAuthError::MissingHeader)
        ));
    }

    #[test]
    fn test_validate_wrong_password() {
        let creds = BasicAuthCredentials {
            username: "mct".to_string(),
            password: "correct_password".to_string(),
        };
        let encoded = BASE64.encode("mct:wrong_password");
        let header = format!("Basic {encoded}");
        assert!(matches!(
            validate_basic_auth(Some(&header), &creds),
            Err(BasicAuthError::InvalidCredentials)
        ));
    }

    #[test]
    fn test_validate_wrong_scheme() {
        let creds = generate_credentials();
        assert!(matches!(
            validate_basic_auth(Some("Bearer abc123"), &creds),
            Err(BasicAuthError::InvalidScheme)
        ));
    }

    #[test]
    fn test_validate_bad_base64() {
        let creds = generate_credentials();
        assert!(matches!(
            validate_basic_auth(Some("Basic !!!not-base64!!!"), &creds),
            Err(BasicAuthError::InvalidEncoding)
        ));
    }

    #[test]
    fn test_validate_oversized_header() {
        let creds = generate_credentials();
        let huge = "A".repeat(10_000);
        let header = format!("Basic {huge}");
        assert!(matches!(
            validate_basic_auth(Some(&header), &creds),
            Err(BasicAuthError::InvalidEncoding)
        ));
    }

    #[test]
    fn test_validate_no_colon() {
        let creds = generate_credentials();
        let encoded = BASE64.encode("no-colon-here");
        let header = format!("Basic {encoded}");
        assert!(matches!(
            validate_basic_auth(Some(&header), &creds),
            Err(BasicAuthError::InvalidFormat)
        ));
    }

    #[test]
    fn test_build_credentials_url_https() {
        let creds = BasicAuthCredentials {
            username: "mct".to_string(),
            password: "abc123".to_string(),
        };
        let url = build_credentials_url("https://example.com/slug/mcp", &creds);
        assert_eq!(url, "https://mct:abc123@example.com/slug/mcp");
    }

    #[test]
    fn test_build_credentials_url_http() {
        let creds = BasicAuthCredentials {
            username: "mct".to_string(),
            password: "abc123".to_string(),
        };
        let url = build_credentials_url("http://localhost:8080/slug/mcp", &creds);
        assert_eq!(url, "http://mct:abc123@localhost:8080/slug/mcp");
    }
}
