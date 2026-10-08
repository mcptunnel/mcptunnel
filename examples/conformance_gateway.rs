//! Test-only in-memory mount: never starts a tunnel or modifies the user's daemon/database.
use mcptunnel::{
    config::{AuthModeConfig, Config, TunnelMode},
    daemon::state::AppState,
    mount::{db, types::*, MountRegistry},
    policy::types::PolicyConfig,
    session::types::SessionConfig,
};
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command: Vec<String> = std::env::args().skip(1).collect();
    anyhow::ensure!(!command.is_empty(), "supply a stdio fixture command");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:43123").await?;
    let mut config = Config::default();
    config.daemon.bind = listener.local_addr()?.to_string();
    config.tunnel.default_mode = TunnelMode::Local;
    let conn = rusqlite::Connection::open_in_memory()?;
    db::init_db(&conn)?;
    let registry = MountRegistry::new(Arc::new(Mutex::new(conn)));
    let mount = registry.create(MountCreateParams {
        slug: Some("test".into()),
        display_name: None,
        command: CommandConfig {
            program: command[0].clone(),
            args: command[1..].to_vec(),
        },
        cwd: None,
        tunnel_mode: TunnelMode::Local,
        hostname: None,
        auth_mode: AuthModeConfig::Public,
        basic_credentials: None,
        oauth_config: None,
        policy: PolicyConfig::default(),
        env_vars: vec![],
    })?;
    let probe = mcptunnel::upstream::probe::probe_upstream(&mount.command, None, &[]).await;
    anyhow::ensure!(probe.ok, "fixture initialization failed: {:?}", probe.error);
    registry.set_probe_result("test", &probe)?;
    let state = Arc::new(AppState::new(config, registry, SessionConfig::default()));
    println!("READY http://127.0.0.1:43123/test/mcp");
    axum::serve(listener, mcptunnel::gateway::build_router(state.clone()))
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            state.cancellation.cancel();
        })
        .await?;
    Ok(())
}
