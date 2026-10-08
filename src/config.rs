use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Global configuration loaded from `~/.config/mcptunnel/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct Config {
    pub daemon: DaemonConfig,
    pub tunnel: TunnelConfig,
    pub defaults: DefaultsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    pub bind: String,
    pub auto_start: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TunnelConfig {
    pub default_mode: TunnelMode,
    pub cloudflared_path: String,
    pub quick_fail_if_config_yaml_present: bool,
    pub external_origin: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DefaultsConfig {
    pub auth_mode: AuthModeConfig,
    pub probe_mode: ProbeMode,
    #[serde(with = "humantime_serde")]
    pub session_idle_ttl: Duration,
    #[serde(with = "humantime_serde")]
    pub session_hard_ttl: Duration,
    pub max_sessions_per_mount: usize,
    pub max_inflight_per_session: usize,
    pub log_level: String,
    pub cidr_filter_mode: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TunnelMode {
    Quick,
    Managed,
    Local,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthModeConfig {
    Public,
    Basic,
    Inherit,
    OAuth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProbeMode {
    Eager,
    Lazy,
    Off,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:43123".to_string(),
            auto_start: true,
        }
    }
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            default_mode: TunnelMode::Quick,
            cloudflared_path: "cloudflared".to_string(),
            quick_fail_if_config_yaml_present: true,
            external_origin: None,
        }
    }
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            auth_mode: AuthModeConfig::Basic,
            probe_mode: ProbeMode::Eager,
            session_idle_ttl: Duration::from_secs(15 * 60),
            session_hard_ttl: Duration::from_secs(12 * 60 * 60),
            max_sessions_per_mount: 8,
            max_inflight_per_session: 8,
            log_level: "info".to_string(),
            cidr_filter_mode: "disabled".to_string(),
        }
    }
}

impl Default for TunnelMode {
    fn default() -> Self {
        Self::Quick
    }
}

impl Default for AuthModeConfig {
    fn default() -> Self {
        Self::Basic
    }
}

impl Default for ProbeMode {
    fn default() -> Self {
        Self::Eager
    }
}

impl std::fmt::Display for TunnelMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Quick => write!(f, "quick"),
            Self::Managed => write!(f, "managed"),
            Self::Local => write!(f, "local"),
        }
    }
}

impl std::fmt::Display for AuthModeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Public => write!(f, "public"),
            Self::Basic => write!(f, "basic"),
            Self::Inherit => write!(f, "inherit"),
            Self::OAuth => write!(f, "oauth"),
        }
    }
}

/// Duration serialization as human-readable strings ("15m", "12h").
mod humantime_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let secs = duration.as_secs();
        let s = if secs % 3600 == 0 {
            format!("{}h", secs / 3600)
        } else if secs % 60 == 0 {
            format!("{}m", secs / 60)
        } else {
            format!("{secs}s")
        };
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_duration(&s).map_err(serde::de::Error::custom)
    }

    fn parse_duration(s: &str) -> Result<Duration, String> {
        let s = s.trim();
        if let Some(h) = s.strip_suffix('h') {
            let n: u64 = h.parse().map_err(|e| format!("invalid hours: {e}"))?;
            Ok(Duration::from_secs(n * 3600))
        } else if let Some(m) = s.strip_suffix('m') {
            let n: u64 = m.parse().map_err(|e| format!("invalid minutes: {e}"))?;
            Ok(Duration::from_secs(n * 60))
        } else if let Some(sec) = s.strip_suffix('s') {
            let n: u64 = sec.parse().map_err(|e| format!("invalid seconds: {e}"))?;
            Ok(Duration::from_secs(n))
        } else {
            let n: u64 = s.parse().map_err(|e| format!("invalid duration: {e}"))?;
            Ok(Duration::from_secs(n))
        }
    }
}

/// Get platform-appropriate directories.
pub fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("", "", "mcptunnel")
}

/// Config file path: `~/.config/mcptunnel/config.toml`
pub fn config_path() -> PathBuf {
    project_dirs()
        .map(|d| d.config_dir().join("config.toml"))
        .unwrap_or_else(|| PathBuf::from("config.toml"))
}

/// State directory: `~/.local/state/mcptunnel/`
pub fn state_dir() -> PathBuf {
    project_dirs()
        .map(|d| d.data_local_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".mcptunnel"))
}

/// Database path: `~/.local/state/mcptunnel/state.db`
pub fn db_path() -> PathBuf {
    state_dir().join("state.db")
}

/// Daemon socket path: `~/.local/state/mcptunnel/daemon.sock`
pub fn socket_path() -> PathBuf {
    state_dir().join("daemon.sock")
}

/// Load config from file, falling back to defaults.
pub fn load_config() -> Config {
    let path = config_path();
    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(config) => return config,
                Err(e) => {
                    eprintln!("Warning: failed to parse config at {}: {e}", path.display());
                }
            },
            Err(e) => {
                eprintln!("Warning: failed to read config at {}: {e}", path.display());
            }
        }
    }
    Config::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.daemon.bind, "127.0.0.1:43123");
        assert!(config.daemon.auto_start);
        assert_eq!(config.tunnel.default_mode, TunnelMode::Quick);
        assert_eq!(config.defaults.auth_mode, AuthModeConfig::Basic);
        assert_eq!(config.defaults.session_idle_ttl, Duration::from_secs(900));
        assert_eq!(config.defaults.session_hard_ttl, Duration::from_secs(43200));
        assert_eq!(config.defaults.max_sessions_per_mount, 8);
    }

    #[test]
    fn test_config_toml_roundtrip() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.daemon.bind, config.daemon.bind);
        assert_eq!(parsed.tunnel.default_mode, config.tunnel.default_mode);
    }

    #[test]
    fn test_config_parse_custom() {
        let toml_str = r#"
[daemon]
bind = "127.0.0.1:9999"
auto_start = false

[tunnel]
default_mode = "managed"
cloudflared_path = "/usr/bin/cloudflared"

[defaults]
auth_mode = "oauth"
session_idle_ttl = "30m"
session_hard_ttl = "6h"
max_sessions_per_mount = 4
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.daemon.bind, "127.0.0.1:9999");
        assert!(!config.daemon.auto_start);
        assert_eq!(config.tunnel.default_mode, TunnelMode::Managed);
        assert_eq!(config.defaults.auth_mode, AuthModeConfig::OAuth);
        assert_eq!(config.defaults.session_idle_ttl, Duration::from_secs(1800));
        assert_eq!(config.defaults.session_hard_ttl, Duration::from_secs(21600));
        assert_eq!(config.defaults.max_sessions_per_mount, 4);
    }
}
