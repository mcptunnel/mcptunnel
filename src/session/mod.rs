pub mod types;

use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use types::{generate_session_id, Session, SessionConfig, SessionStatus};

/// Session manager: tracks active sessions across all mounts.
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    config: SessionConfig,
}

impl SessionManager {
    pub fn new(config: SessionConfig) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Create a new session for a mount.
    pub async fn create_session(&self, mount_slug: &str) -> Result<Session, String> {
        let mut sessions = self.sessions.write().await;

        // Check session limit per mount
        let mount_count = sessions
            .values()
            .filter(|s| s.mount_slug == mount_slug && s.status == SessionStatus::Active)
            .count();

        if mount_count >= self.config.max_sessions_per_mount {
            return Err(format!(
                "max sessions ({}) reached for mount '{mount_slug}'",
                self.config.max_sessions_per_mount
            ));
        }

        let session_id = generate_session_id();
        let now = Utc::now();
        let session = Session {
            id: uuid::Uuid::new_v4().to_string(),
            mount_slug: mount_slug.to_string(),
            session_id: session_id.clone(),
            principal: String::new(),
            protocol_version: String::new(),
            permits: Some(Arc::new(tokio::sync::Semaphore::new(
                self.config.max_inflight_per_session,
            ))),
            upstream_pid: None,
            status: SessionStatus::Active,
            created_at: now,
            last_activity_at: now,
            ended_at: None,
            end_reason: None,
        };

        sessions.insert(session_id, session.clone());
        Ok(session)
    }

    pub async fn bind(&self, id: &str, principal: String, protocol_version: String) {
        if let Some(session) = self.sessions.write().await.get_mut(id) {
            session.principal = principal;
            session.protocol_version = protocol_version;
        }
    }

    /// Look up a session by its session ID.
    pub async fn get_session(&self, session_id: &str) -> Option<Session> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).cloned()
    }

    /// Update last activity time for a session.
    pub async fn touch_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.last_activity_at = Utc::now();
        }
    }

    /// Delete a session.
    pub async fn delete_session(&self, session_id: &str) -> Option<Session> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.status = SessionStatus::Ended;
            session.ended_at = Some(Utc::now());
            session.end_reason = Some("explicit_delete".to_string());
        }
        sessions.remove(session_id)
    }

    /// Count active sessions for a mount.
    pub async fn active_count(&self, mount_slug: &str) -> usize {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .filter(|s| s.mount_slug == mount_slug && s.status == SessionStatus::Active)
            .count()
    }

    /// Evict idle sessions (past idle TTL or hard TTL).
    pub async fn evict_expired(&self) -> Vec<Session> {
        let mut sessions = self.sessions.write().await;
        let now = Utc::now();
        let mut evicted = Vec::new();

        let expired_keys: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| {
                if s.status != SessionStatus::Active {
                    return false;
                }
                let idle_expired = (now - s.last_activity_at)
                    .to_std()
                    .map(|d| d > self.config.idle_ttl)
                    .unwrap_or(false);
                let hard_expired = (now - s.created_at)
                    .to_std()
                    .map(|d| d > self.config.hard_ttl)
                    .unwrap_or(false);
                idle_expired || hard_expired
            })
            .map(|(k, _)| k.clone())
            .collect();

        for key in expired_keys {
            if let Some(mut session) = sessions.remove(&key) {
                session.status = SessionStatus::Ended;
                session.ended_at = Some(now);
                session.end_reason = Some("evicted_idle".to_string());
                evicted.push(session);
            }
        }

        evicted
    }

    /// Remove all sessions for a mount (used when stopping a mount).
    pub async fn remove_mount_sessions(&self, mount_slug: &str) -> Vec<Session> {
        let mut sessions = self.sessions.write().await;
        let keys: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| s.mount_slug == mount_slug)
            .map(|(k, _)| k.clone())
            .collect();

        let mut removed = Vec::new();
        for key in keys {
            if let Some(mut session) = sessions.remove(&key) {
                session.status = SessionStatus::Ended;
                session.ended_at = Some(Utc::now());
                session.end_reason = Some("mount_stopped".to_string());
                removed.push(session);
            }
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_create_and_get_session() {
        let mgr = SessionManager::new(SessionConfig::default());
        let session = mgr.create_session("painter").await.unwrap();
        assert_eq!(session.mount_slug, "painter");
        assert_eq!(session.status, SessionStatus::Active);

        let fetched = mgr.get_session(&session.session_id).await.unwrap();
        assert_eq!(fetched.id, session.id);
    }

    #[tokio::test]
    async fn test_delete_session() {
        let mgr = SessionManager::new(SessionConfig::default());
        let session = mgr.create_session("painter").await.unwrap();
        let sid = session.session_id.clone();

        let deleted = mgr.delete_session(&sid).await;
        assert!(deleted.is_some());
        assert!(mgr.get_session(&sid).await.is_none());
    }

    #[tokio::test]
    async fn test_session_limit() {
        let config = SessionConfig {
            max_sessions_per_mount: 2,
            ..Default::default()
        };
        let mgr = SessionManager::new(config);

        mgr.create_session("test").await.unwrap();
        mgr.create_session("test").await.unwrap();
        let result = mgr.create_session("test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_active_count() {
        let mgr = SessionManager::new(SessionConfig::default());
        mgr.create_session("a").await.unwrap();
        mgr.create_session("a").await.unwrap();
        mgr.create_session("b").await.unwrap();

        assert_eq!(mgr.active_count("a").await, 2);
        assert_eq!(mgr.active_count("b").await, 1);
        assert_eq!(mgr.active_count("c").await, 0);
    }

    #[tokio::test]
    async fn test_evict_expired() {
        let config = SessionConfig {
            idle_ttl: Duration::from_millis(1),
            hard_ttl: Duration::from_secs(3600),
            ..Default::default()
        };
        let mgr = SessionManager::new(config);

        let session = mgr.create_session("test").await.unwrap();
        let sid = session.session_id.clone();

        // Wait for idle TTL
        tokio::time::sleep(Duration::from_millis(10)).await;

        let evicted = mgr.evict_expired().await;
        assert_eq!(evicted.len(), 1);
        assert!(mgr.get_session(&sid).await.is_none());
    }

    #[tokio::test]
    async fn test_remove_mount_sessions() {
        let mgr = SessionManager::new(SessionConfig::default());
        mgr.create_session("mount-a").await.unwrap();
        mgr.create_session("mount-a").await.unwrap();
        mgr.create_session("mount-b").await.unwrap();

        let removed = mgr.remove_mount_sessions("mount-a").await;
        assert_eq!(removed.len(), 2);
        assert_eq!(mgr.active_count("mount-a").await, 0);
        assert_eq!(mgr.active_count("mount-b").await, 1);
    }
}
