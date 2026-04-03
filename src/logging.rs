use tracing_subscriber::{fmt, EnvFilter};

/// Initialize structured logging with tracing.
pub fn init_logging(level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .json()
        .init();
}

/// Initialize human-readable logging for CLI commands.
pub fn init_cli_logging() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn"));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .init();
}

/// Redact sensitive values in log output.
pub fn redact_secret(value: &str) -> String {
    if value.len() <= 8 {
        "***REDACTED***".to_string()
    } else {
        format!("{}...***REDACTED***", &value[..4])
    }
}

/// Compute a truncated SHA-256 hash of a session ID for logging.
pub fn hash_session_id(session_id: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(session_id.as_bytes());
    hex::encode(&hash[..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_secret() {
        assert_eq!(redact_secret("short"), "***REDACTED***");
        assert_eq!(redact_secret("Bearer eyJhbGciOi..."), "Bear...***REDACTED***");
    }

    #[test]
    fn test_hash_session_id() {
        let hash = hash_session_id("test-session-123");
        assert_eq!(hash.len(), 16); // 8 bytes = 16 hex chars
        // Deterministic
        assert_eq!(hash, hash_session_id("test-session-123"));
        // Different input = different hash
        assert_ne!(hash, hash_session_id("other-session"));
    }
}
