use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A remote MCP session backed by a child process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub mount_slug: String,
    pub session_id: String,
    #[serde(default)]
    pub principal: String,
    #[serde(default)]
    pub protocol_version: String,
    #[serde(skip)]
    pub permits: Option<std::sync::Arc<tokio::sync::Semaphore>>,
    pub upstream_pid: Option<u32>,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub end_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Closing,
    Ended,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Closing => write!(f, "closing"),
            Self::Ended => write!(f, "ended"),
        }
    }
}

/// Configuration for session lifecycle.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub idle_ttl: Duration,
    pub hard_ttl: Duration,
    pub max_sessions_per_mount: usize,
    #[allow(dead_code)]
    pub max_inflight_per_session: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            idle_ttl: Duration::from_secs(15 * 60),
            hard_ttl: Duration::from_secs(12 * 60 * 60),
            max_sessions_per_mount: 8,
            max_inflight_per_session: 8,
        }
    }
}

/// Generate a cryptographically strong session ID.
pub fn generate_session_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_session_id() {
        let id1 = generate_session_id();
        let id2 = generate_session_id();
        assert_ne!(id1, id2);
        assert_eq!(id1.len(), 64); // 32 bytes = 64 hex chars
                                   // All hex characters
        assert!(id1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_default_session_config() {
        let config = SessionConfig::default();
        assert_eq!(config.idle_ttl, Duration::from_secs(900));
        assert_eq!(config.hard_ttl, Duration::from_secs(43200));
        assert_eq!(config.max_sessions_per_mount, 8);
    }
}
