use rusqlite::{params, Connection, Result as SqlResult};

use super::types::{MountRecord, MountStatus};
use crate::auth::basic::BasicAuthCredentials;
use crate::config::AuthModeConfig;

/// Initialize the database schema.
pub fn init_db(conn: &Connection) -> SqlResult<()> {
    init_db_at_port(conn, 8000)
}

pub fn init_db_at_port(conn: &Connection, legacy_port: u16) -> SqlResult<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS mounts (
            id TEXT PRIMARY KEY,
            slug TEXT UNIQUE NOT NULL,
            display_name TEXT NOT NULL,
            command_json TEXT NOT NULL,
            cwd TEXT,
            auth_mode TEXT NOT NULL DEFAULT 'public',
            oauth_config_json TEXT,
            policy_json TEXT NOT NULL DEFAULT '{}',
            tunnel_mode TEXT NOT NULL DEFAULT 'quick',
            hostname TEXT,
            status TEXT NOT NULL DEFAULT 'starting',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            last_probe_at TEXT,
            last_probe_json TEXT,
            env_vars_json TEXT NOT NULL DEFAULT '[]'
        );

        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            mount_id TEXT NOT NULL,
            session_id_hash TEXT NOT NULL,
            upstream_pid INTEGER,
            status TEXT NOT NULL DEFAULT 'active',
            created_at TEXT NOT NULL,
            last_activity_at TEXT NOT NULL,
            ended_at TEXT,
            end_reason TEXT,
            FOREIGN KEY (mount_id) REFERENCES mounts(id)
        );

        CREATE TABLE IF NOT EXISTS tunnel_state (
            daemon_instance_id TEXT PRIMARY KEY,
            mode TEXT NOT NULL,
            public_origin TEXT,
            cloudflared_pid INTEGER,
            status TEXT NOT NULL DEFAULT 'starting',
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS cidr_cache (
            source_url TEXT PRIMARY KEY,
            payload_json TEXT NOT NULL,
            fetched_at TEXT NOT NULL,
            etag TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_sessions_mount ON sessions(mount_id);
        CREATE INDEX IF NOT EXISTS idx_sessions_status ON sessions(status);
        ",
    )?;

    // Migration: add basic_credentials_json column if missing
    let has_basic_creds: bool = conn
        .prepare("SELECT basic_credentials_json FROM mounts LIMIT 0")
        .is_ok();
    if !has_basic_creds {
        conn.execute_batch("ALTER TABLE mounts ADD COLUMN basic_credentials_json TEXT;")?;
    }

    if conn.prepare("SELECT port FROM mounts LIMIT 0").is_err() {
        conn.execute_batch(&format!(
            "ALTER TABLE mounts ADD COLUMN port INTEGER NOT NULL DEFAULT {legacy_port};"
        ))?;
    }
    if conn.prepare("SELECT path FROM mounts LIMIT 0").is_err() {
        conn.execute_batch("ALTER TABLE mounts ADD COLUMN path TEXT NOT NULL DEFAULT ''; UPDATE mounts SET path = '/' || slug || '/mcp';")?;
    }
    conn.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_mount_endpoint ON mounts(port, path);",
    )?;
    Ok(())
}

/// Insert a mount record.
pub fn insert_mount(conn: &Connection, record: &MountRecord) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO mounts (id, slug, display_name, command_json, cwd, auth_mode, oauth_config_json, policy_json, tunnel_mode, hostname, status, created_at, updated_at, last_probe_at, last_probe_json, env_vars_json, basic_credentials_json, port, path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
        params![
            record.id.to_string(),
            record.slug,
            record.display_name,
            serde_json::to_string(&record.command).unwrap_or_default(),
            record.cwd,
            record.auth_mode.to_string(),
            record.oauth_config.as_ref().map(|c| serde_json::to_string(c).unwrap_or_default()),
            serde_json::to_string(&record.policy).unwrap_or_default(),
            record.tunnel_mode.to_string(),
            record.hostname,
            record.status.to_string(),
            record.created_at.to_rfc3339(),
            record.updated_at.to_rfc3339(),
            record.last_probe_at.map(|t| t.to_rfc3339()),
            record.last_probe_result.as_ref().map(|p| serde_json::to_string(p).unwrap_or_default()),
            serde_json::to_string(&record.env_vars).unwrap_or_default(),
            record.basic_credentials.as_ref().map(|c| serde_json::to_string(c).unwrap_or_default()),
            record.port,
            record.path,
        ],
    )?;
    Ok(())
}

/// List all mounts.
pub fn list_mounts(conn: &Connection) -> SqlResult<Vec<MountRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, slug, display_name, command_json, cwd, auth_mode, oauth_config_json, policy_json, tunnel_mode, hostname, status, created_at, updated_at, last_probe_at, last_probe_json, env_vars_json, basic_credentials_json, port, path FROM mounts"
    )?;

    let records = stmt
        .query_map([], |row| Ok(row_to_mount(row)))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(records)
}

/// Get a mount by slug.
pub fn get_mount_by_slug(conn: &Connection, slug: &str) -> SqlResult<Option<MountRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, slug, display_name, command_json, cwd, auth_mode, oauth_config_json, policy_json, tunnel_mode, hostname, status, created_at, updated_at, last_probe_at, last_probe_json, env_vars_json, basic_credentials_json, port, path FROM mounts WHERE slug = ?1"
    )?;

    let mut rows = stmt.query_map(params![slug], |row| Ok(row_to_mount(row)))?;

    Ok(rows.next().and_then(|r| r.ok()))
}

/// Delete a mount by slug.
pub fn delete_mount(conn: &Connection, slug: &str) -> SqlResult<bool> {
    let affected = conn.execute("DELETE FROM mounts WHERE slug = ?1", params![slug])?;
    Ok(affected > 0)
}

/// Update mount status.
pub fn update_mount_status(conn: &Connection, slug: &str, status: MountStatus) -> SqlResult<()> {
    conn.execute(
        "UPDATE mounts SET status = ?1, updated_at = ?2 WHERE slug = ?3",
        params![status.to_string(), chrono::Utc::now().to_rfc3339(), slug],
    )?;
    Ok(())
}

/// Update probe result.
pub fn update_probe_result(
    conn: &Connection,
    slug: &str,
    probe: &super::types::ProbeResult,
) -> SqlResult<()> {
    conn.execute(
        "UPDATE mounts SET last_probe_at = ?1, last_probe_json = ?2, updated_at = ?3 WHERE slug = ?4",
        params![
            chrono::Utc::now().to_rfc3339(),
            serde_json::to_string(probe).unwrap_or_default(),
            chrono::Utc::now().to_rfc3339(),
            slug,
        ],
    )?;
    Ok(())
}

/// Check if a slug is already in use.
pub fn slug_exists(conn: &Connection, slug: &str) -> SqlResult<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM mounts WHERE slug = ?1",
        params![slug],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub fn endpoint_exists(conn: &Connection, port: u16, path: &str) -> SqlResult<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM mounts WHERE port = ?1 AND path = ?2)",
        params![port, path],
        |row| row.get(0),
    )
}

fn row_to_mount(row: &rusqlite::Row) -> MountRecord {
    let id_str: String = row.get(0).unwrap_or_default();
    let command_json: String = row.get(3).unwrap_or_default();
    let oauth_json: Option<String> = row.get(6).ok().unwrap_or(None);
    let policy_json: String = row.get(7).unwrap_or_else(|_| "{}".to_string());
    let probe_json: Option<String> = row.get(14).ok().unwrap_or(None);
    let env_json: String = row.get(15).unwrap_or_else(|_| "[]".to_string());
    let basic_creds_json: Option<String> = row.get(16).ok().unwrap_or(None);

    let auth_mode_str: String = row.get(5).unwrap_or_else(|_| "public".to_string());
    let tunnel_mode_str: String = row.get(8).unwrap_or_else(|_| "quick".to_string());
    let status_str: String = row.get(10).unwrap_or_else(|_| "starting".to_string());
    let created_str: String = row.get(11).unwrap_or_default();
    let updated_str: String = row.get(12).unwrap_or_default();
    let probe_at_str: Option<String> = row.get(13).ok().unwrap_or(None);

    MountRecord {
        id: uuid::Uuid::parse_str(&id_str).unwrap_or_else(|_| uuid::Uuid::new_v4()),
        slug: row.get(1).unwrap_or_default(),
        port: row.get(17).unwrap_or(8000),
        path: row.get(18).unwrap_or_default(),
        display_name: row.get(2).unwrap_or_default(),
        command: serde_json::from_str(&command_json).unwrap_or(super::types::CommandConfig {
            program: String::new(),
            args: vec![],
        }),
        cwd: row.get(4).ok().unwrap_or(None),
        auth_mode: match auth_mode_str.as_str() {
            "basic" => AuthModeConfig::Basic,
            "inherit" => AuthModeConfig::Inherit,
            "oauth" => AuthModeConfig::OAuth,
            _ => AuthModeConfig::Public,
        },
        basic_credentials: basic_creds_json
            .and_then(|j| serde_json::from_str::<BasicAuthCredentials>(&j).ok()),
        oauth_config: oauth_json.and_then(|j| serde_json::from_str(&j).ok()),
        policy: serde_json::from_str(&policy_json).unwrap_or_default(),
        tunnel_mode: match tunnel_mode_str.as_str() {
            "managed" => crate::config::TunnelMode::Managed,
            "local" => crate::config::TunnelMode::Local,
            _ => crate::config::TunnelMode::Quick,
        },
        hostname: row.get(9).ok().unwrap_or(None),
        status: match status_str.as_str() {
            "ready" => MountStatus::Ready,
            "degraded" => MountStatus::Degraded,
            "stopped" => MountStatus::Stopped,
            _ => MountStatus::Starting,
        },
        created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        updated_at: chrono::DateTime::parse_from_rfc3339(&updated_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        last_probe_at: probe_at_str.and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .ok()
        }),
        last_probe_result: probe_json.and_then(|j| serde_json::from_str(&j).ok()),
        env_vars: serde_json::from_str(&env_json).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mount::types::*;
    use crate::policy::types::PolicyConfig;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn
    }

    fn make_record(slug: &str) -> MountRecord {
        MountRecord {
            id: uuid::Uuid::new_v4(),
            slug: slug.to_string(),
            port: 8000,
            path: format!("/{slug}"),
            display_name: format!("Test {slug}"),
            command: CommandConfig {
                program: "echo".to_string(),
                args: vec!["hello".to_string()],
            },
            cwd: None,
            auth_mode: AuthModeConfig::Public,
            basic_credentials: None,
            oauth_config: None,
            policy: PolicyConfig::default(),
            tunnel_mode: crate::config::TunnelMode::Quick,
            hostname: None,
            status: MountStatus::Ready,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            last_probe_at: None,
            last_probe_result: None,
            env_vars: vec![],
        }
    }

    #[test]
    fn test_crud() {
        let conn = test_db();
        let record = make_record("painter");

        insert_mount(&conn, &record).unwrap();
        assert!(slug_exists(&conn, "painter").unwrap());

        let fetched = get_mount_by_slug(&conn, "painter").unwrap().unwrap();
        assert_eq!(fetched.slug, "painter");
        assert_eq!(fetched.display_name, "Test painter");
        assert_eq!(fetched.port, 8000);
        assert_eq!(fetched.path, "/painter");

        let all = list_mounts(&conn).unwrap();
        assert_eq!(all.len(), 1);

        delete_mount(&conn, "painter").unwrap();
        assert!(!slug_exists(&conn, "painter").unwrap());
    }

    #[test]
    fn test_slug_uniqueness() {
        let conn = test_db();
        let record = make_record("unique-slug");
        insert_mount(&conn, &record).unwrap();

        let dup = make_record("unique-slug");
        assert!(insert_mount(&conn, &dup).is_err());
    }

    #[test]
    fn test_update_status() {
        let conn = test_db();
        let record = make_record("status-test");
        insert_mount(&conn, &record).unwrap();

        update_mount_status(&conn, "status-test", MountStatus::Stopped).unwrap();
        let fetched = get_mount_by_slug(&conn, "status-test").unwrap().unwrap();
        assert_eq!(fetched.status, MountStatus::Stopped);
    }

    #[test]
    fn test_basic_credentials_roundtrip() {
        let conn = test_db();
        let mut record = make_record("basic-auth-test");
        record.auth_mode = AuthModeConfig::Basic;
        record.basic_credentials = Some(BasicAuthCredentials {
            username: "mct".to_string(),
            password: "abcdef0123456789".to_string(),
        });
        insert_mount(&conn, &record).unwrap();

        let fetched = get_mount_by_slug(&conn, "basic-auth-test")
            .unwrap()
            .unwrap();
        assert_eq!(fetched.auth_mode, AuthModeConfig::Basic);
        let creds = fetched.basic_credentials.unwrap();
        assert_eq!(creds.username, "mct");
        assert_eq!(creds.password, "abcdef0123456789");
    }

    #[test]
    fn test_no_credentials_for_public() {
        let conn = test_db();
        let record = make_record("public-test");
        insert_mount(&conn, &record).unwrap();

        let fetched = get_mount_by_slug(&conn, "public-test").unwrap().unwrap();
        assert_eq!(fetched.auth_mode, AuthModeConfig::Public);
        assert!(fetched.basic_credentials.is_none());
    }

    #[test]
    fn legacy_endpoints_migrate_without_changing_visibility() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE mounts (slug TEXT, tunnel_mode TEXT, basic_credentials_json TEXT); INSERT INTO mounts VALUES ('old-private', 'local', NULL), ('old-public', 'quick', NULL);").unwrap();
        init_db_at_port(&conn, 43123).unwrap();
        let routes: Vec<(String, u16, String, String)> = conn
            .prepare("SELECT slug, port, path, tunnel_mode FROM mounts ORDER BY slug")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(
            routes,
            vec![
                (
                    "old-private".into(),
                    43123,
                    "/old-private/mcp".into(),
                    "local".into()
                ),
                (
                    "old-public".into(),
                    43123,
                    "/old-public/mcp".into(),
                    "quick".into()
                )
            ]
        );
        init_db_at_port(&conn, 8000).unwrap();
        assert_eq!(
            conn.query_row("SELECT port FROM mounts LIMIT 1", [], |row| row
                .get::<_, u16>(0))
                .unwrap(),
            43123
        );
    }
}
