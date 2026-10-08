pub mod db;
pub mod endpoint;
pub mod slug;
pub mod types;

use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use crate::config::AuthModeConfig;
use crate::error::{McpTunnelError, Result};
use types::{MountCreateParams, MountRecord, MountStatus};

/// Mount registry backed by SQLite.
pub struct MountRegistry {
    db: Arc<Mutex<Connection>>,
}

impl MountRegistry {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self { db }
    }

    /// Create a new mount, generating a slug if not provided.
    pub fn create(&self, params: MountCreateParams) -> Result<MountRecord> {
        crate::tunnel::validate_selection(params.tunnel_mode, params.hostname.as_deref())
            .map_err(McpTunnelError::CliUsage)?;
        let conn = self
            .db
            .lock()
            .map_err(|e| McpTunnelError::Other(e.to_string()))?;

        if params.port == 0 {
            return Err(McpTunnelError::CliUsage(
                "--port must be between 1 and 65535".into(),
            ));
        }
        let name = params
            .slug
            .clone()
            .unwrap_or_else(|| endpoint::infer_name(&params.command));
        let mut path = params.path.clone().unwrap_or_else(|| format!("/{name}"));
        endpoint::validate_path(&path).map_err(McpTunnelError::CliUsage)?;
        let base_path = path.clone();
        let mut suffix = 2;
        while db::endpoint_exists(&conn, params.port, &path)? {
            if params.path.is_some() {
                return Err(McpTunnelError::CliUsage(format!(
                    "path '{path}' is already registered on port {}; choose --path or --port",
                    params.port
                )));
            }
            path = format!("{base_path}-{suffix}");
            suffix += 1;
        }
        let mount_slug = match params.slug {
            Some(s) => {
                slug::validate_slug(&s).map_err(McpTunnelError::CliUsage)?;
                if db::slug_exists(&conn, &s)? {
                    return Err(McpTunnelError::CliUsage(format!(
                        "slug '{s}' already in use"
                    )));
                }
                s
            }
            None => {
                let mut generated = name.clone();
                let mut suffix = 2;
                while db::slug_exists(&conn, &generated)? {
                    generated = format!("{name}-{suffix}");
                    suffix += 1;
                }
                generated
            }
        };

        let display_name = params.display_name.unwrap_or_else(|| mount_slug.clone());

        // Auto-generate Basic Auth credentials if auth mode is Basic and none provided
        let basic_credentials = if params.auth_mode == AuthModeConfig::Basic {
            params
                .basic_credentials
                .or_else(|| Some(crate::auth::basic::generate_credentials()))
        } else {
            params.basic_credentials
        };

        if params.auth_mode == AuthModeConfig::Inherit
            || (params.auth_mode == AuthModeConfig::OAuth
                && params.oauth_config.as_ref().is_none_or(|c| {
                    c.builtin || crate::auth::oauth::validate_https_url(&c.issuer).is_err()
                }))
        {
            return Err(McpTunnelError::CliUsage("choose basic, public or OAuth with an external HTTPS issuer; builtin and inherit authentication are disabled".into()));
        }
        let oauth_config = params.oauth_config;

        let now = chrono::Utc::now();
        let record = MountRecord {
            id: uuid::Uuid::new_v4(),
            slug: mount_slug,
            port: params.port,
            path,
            display_name,
            command: params.command,
            cwd: params.cwd,
            auth_mode: params.auth_mode,
            basic_credentials,
            oauth_config,
            policy: params.policy,
            tunnel_mode: params.tunnel_mode,
            hostname: params.hostname,
            status: MountStatus::Starting,
            created_at: now,
            updated_at: now,
            last_probe_at: None,
            last_probe_result: None,
            env_vars: params.env_vars,
        };

        db::insert_mount(&conn, &record)?;
        Ok(record)
    }

    /// List all mounts.
    pub fn list(&self) -> Result<Vec<MountRecord>> {
        let conn = self
            .db
            .lock()
            .map_err(|e| McpTunnelError::Other(e.to_string()))?;
        Ok(db::list_mounts(&conn)?)
    }

    /// Get a mount by slug.
    pub fn get(&self, slug: &str) -> Result<Option<MountRecord>> {
        let conn = self
            .db
            .lock()
            .map_err(|e| McpTunnelError::Other(e.to_string()))?;
        Ok(db::get_mount_by_slug(&conn, slug)?)
    }

    /// Remove a mount by slug.
    pub fn remove(&self, slug: &str) -> Result<bool> {
        let conn = self
            .db
            .lock()
            .map_err(|e| McpTunnelError::Other(e.to_string()))?;
        Ok(db::delete_mount(&conn, slug)?)
    }

    /// Update mount status.
    pub fn set_status(&self, slug: &str, status: MountStatus) -> Result<()> {
        let conn = self
            .db
            .lock()
            .map_err(|e| McpTunnelError::Other(e.to_string()))?;
        db::update_mount_status(&conn, slug, status)?;
        Ok(())
    }

    /// Update probe result.
    pub fn set_probe_result(&self, slug: &str, probe: &types::ProbeResult) -> Result<()> {
        let conn = self
            .db
            .lock()
            .map_err(|e| McpTunnelError::Other(e.to_string()))?;
        db::update_probe_result(&conn, slug, probe)?;
        Ok(())
    }
}
