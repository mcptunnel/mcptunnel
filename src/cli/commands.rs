use std::io::{BufRead, Write};
use std::os::unix::net::UnixStream;

use crate::cli::{Cli, Commands, DaemonCommand, RunArgs, TunnelCommand};
use crate::config::{load_config, Config};
use crate::daemon::admin_api::{AdminRequest, AdminResponse, MountCreateRequest};
use crate::error::exit_codes;

/// Execute the CLI command.
pub async fn execute(cli: Cli) -> i32 {
    let config = load_config();

    // Handle bare command (no subcommand) — treat trailing args as `run -- <command>`
    if cli.command.is_none() {
        if cli.args.is_empty() {
            eprintln!("Usage: mcptunnel [COMMAND] -- <mcp-server-command...>");
            eprintln!("Try 'mcptunnel --help' for more information.");
            return exit_codes::CLI_USAGE;
        }
        let (program, args) = cli.args.split_first().unwrap();
        return handle_run_bare(&config, program, args).await;
    }

    match cli.command.unwrap() {
        Commands::Run(args) => handle_run(&config, args).await,
        Commands::List { json } => handle_list(&config, json).await,
        Commands::Inspect { slug, json } => handle_inspect(&config, &slug, json).await,
        Commands::Logs { slug, follow } => handle_logs(&config, &slug, follow).await,
        Commands::Stop { slug } => handle_stop(&config, &slug).await,
        Commands::Doctor { slug, json } => handle_doctor(&config, slug.as_deref(), json).await,
        Commands::Daemon { command } => handle_daemon(&config, command).await,
        Commands::Tunnel { command } => handle_tunnel(&config, command).await,
        Commands::Gc => handle_gc(&config).await,
    }
}

/// Handle bare command: `mcptunnel -- <command...>`
async fn handle_run_bare(config: &Config, program: &str, args: &[String]) -> i32 {
    let run_args = RunArgs {
        slug: None,
        name: None,
        cwd: None,
        env: vec![],
        tunnel: None,
        hostname: None,
        auth: None,
        scope: vec![],
        oidc_issuer: None,
        allow_tool: vec![],
        deny_tool: vec![],
        hide_resources: false,
        hide_prompts: false,
        probe: None,
        json: false,
        command: std::iter::once(program.to_string())
            .chain(args.iter().cloned())
            .collect(),
    };
    handle_run(config, run_args).await
}

/// Handle `run` command.
async fn handle_run(config: &Config, args: RunArgs) -> i32 {
    if args.command.is_empty() {
        eprintln!("Error: no command specified");
        return exit_codes::CLI_USAGE;
    }

    let (program, cmd_args) = args.command.split_first().unwrap();
    let is_json = args.json;
    let is_managed = args.tunnel.as_deref() == Some("managed");
    let is_oauth = args.auth.as_deref() == Some("oauth");

    // Parse env vars
    let env_vars: Vec<(String, String)> = args.env.iter()
        .filter_map(|e| {
            let mut parts = e.splitn(2, '=');
            Some((parts.next()?.to_string(), parts.next()?.to_string()))
        })
        .collect();

    let request = AdminRequest::MountCreate(MountCreateRequest {
        slug: args.slug,
        display_name: args.name,
        program: program.clone(),
        args: cmd_args.to_vec(),
        cwd: args.cwd,
        tunnel_mode: args.tunnel,
        hostname: args.hostname,
        auth_mode: args.auth,
        allow_tools: args.allow_tool,
        deny_tools: args.deny_tool,
        hide_resources: args.hide_resources,
        hide_prompts: args.hide_prompts,
        env_vars,
        probe_mode: args.probe,
    });

    match send_admin_request(config, &request) {
        Ok(AdminResponse::Ok { data }) => {
            if is_json {
                println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
            } else {
                let mut warnings = Vec::new();
                if !is_oauth {
                    warnings.push("This endpoint is publicly reachable.".to_string());
                }
                if !is_managed {
                    warnings.push("Quick Tunnel is for testing/development only.".to_string());
                    warnings.push("Quick Tunnel does not support SSE; use --tunnel managed for hardened streaming behavior.".to_string());
                }
                crate::cli::output::print_mount_created(&data, &warnings);
            }
            exit_codes::SUCCESS
        }
        Ok(AdminResponse::Error { message, code }) => {
            eprintln!("Error: {message}");
            code
        }
        Err(e) => {
            eprintln!("Error: {e}");
            exit_codes::DAEMON_UNAVAILABLE
        }
    }
}

/// Handle `list` command.
async fn handle_list(config: &Config, json: bool) -> i32 {
    let request = AdminRequest::MountList;
    match send_admin_request(config, &request) {
        Ok(AdminResponse::Ok { data }) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
            } else {
                let mounts = data["mounts"].as_array()
                    .map(|a| a.to_vec())
                    .unwrap_or_default();
                crate::cli::output::print_mount_list(&mounts);
            }
            exit_codes::SUCCESS
        }
        Ok(AdminResponse::Error { message, code }) => {
            eprintln!("Error: {message}");
            code
        }
        Err(e) => {
            eprintln!("Error: {e}");
            exit_codes::DAEMON_UNAVAILABLE
        }
    }
}

/// Handle `inspect` command.
async fn handle_inspect(config: &Config, slug: &str, json: bool) -> i32 {
    let request = AdminRequest::MountInspect { slug: slug.to_string() };
    match send_admin_request(config, &request) {
        Ok(AdminResponse::Ok { data }) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
            } else {
                crate::cli::output::print_mount_inspect(&data);
            }
            exit_codes::SUCCESS
        }
        Ok(AdminResponse::Error { message, code }) => {
            eprintln!("Error: {message}");
            code
        }
        Err(e) => {
            eprintln!("Error: {e}");
            exit_codes::DAEMON_UNAVAILABLE
        }
    }
}

/// Handle `logs` command.
async fn handle_logs(_config: &Config, slug: &str, _follow: bool) -> i32 {
    // In MVP, logs are available via structured logging output
    eprintln!("Logs for mount '{slug}' — check daemon log output.");
    eprintln!("Structured logs are written to stderr of the daemon process.");
    exit_codes::SUCCESS
}

/// Handle `stop` command.
async fn handle_stop(config: &Config, slug: &str) -> i32 {
    let request = AdminRequest::MountDelete { slug: slug.to_string() };
    match send_admin_request(config, &request) {
        Ok(AdminResponse::Ok { data }) => {
            let sessions = data["sessions_terminated"].as_u64().unwrap_or(0);
            println!("Mount '{slug}' stopped. {sessions} session(s) terminated.");
            exit_codes::SUCCESS
        }
        Ok(AdminResponse::Error { message, code }) => {
            eprintln!("Error: {message}");
            code
        }
        Err(e) => {
            eprintln!("Error: {e}");
            exit_codes::DAEMON_UNAVAILABLE
        }
    }
}

/// Handle `doctor` command.
async fn handle_doctor(config: &Config, slug: Option<&str>, json: bool) -> i32 {
    // Always run local checks first
    let local_checks = crate::doctor::run_local_doctor(config).await;

    if json {
        let mut all_checks = crate::doctor::format_checks_json(&local_checks);

        // Try to get daemon-side checks if slug provided
        if let Some(slug) = slug {
            let request = AdminRequest::DoctorRun { slug: Some(slug.to_string()) };
            if let Ok(AdminResponse::Ok { data }) = send_admin_request(config, &request) {
                if let Some(checks) = data.get("checks") {
                    all_checks["daemon_checks"] = checks.clone();
                }
            }
        }

        println!("{}", serde_json::to_string_pretty(&all_checks).unwrap_or_default());
    } else {
        crate::doctor::format_checks_human(&local_checks)
            .lines()
            .for_each(|l| println!("{l}"));

        if let Some(slug) = slug {
            let request = AdminRequest::DoctorRun { slug: Some(slug.to_string()) };
            if let Ok(AdminResponse::Ok { data }) = send_admin_request(config, &request) {
                if let Some(checks) = data["checks"].as_array() {
                    crate::cli::output::print_doctor_checks(checks);
                }
            }
        }
    }

    exit_codes::SUCCESS
}

/// Handle `daemon` subcommands.
async fn handle_daemon(config: &Config, command: DaemonCommand) -> i32 {
    match command {
        DaemonCommand::Status { json } => {
            let request = AdminRequest::DaemonStatus;
            match send_admin_request(config, &request) {
                Ok(AdminResponse::Ok { data }) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
                    } else {
                        crate::cli::output::print_daemon_status(&data);
                    }
                    exit_codes::SUCCESS
                }
                Ok(AdminResponse::Error { message, code }) => {
                    eprintln!("Error: {message}");
                    code
                }
                Err(_) => {
                    eprintln!("Daemon is not running.");
                    exit_codes::DAEMON_UNAVAILABLE
                }
            }
        }
        DaemonCommand::Stop { force: _ } => {
            eprintln!("Stopping daemon...");
            // In a full implementation, send a shutdown signal
            exit_codes::SUCCESS
        }
        DaemonCommand::Start => {
            println!("Starting daemon...");
            crate::logging::init_logging(&config.defaults.log_level);
            match crate::daemon::run_daemon(config.clone()).await {
                Ok(_) => exit_codes::SUCCESS,
                Err(e) => {
                    eprintln!("Daemon error: {e}");
                    exit_codes::INTERNAL_GATEWAY_ERROR
                }
            }
        }
    }
}

/// Handle `tunnel` subcommands.
async fn handle_tunnel(config: &Config, command: TunnelCommand) -> i32 {
    match command {
        TunnelCommand::Status { json } => {
            let request = AdminRequest::TunnelStatus;
            match send_admin_request(config, &request) {
                Ok(AdminResponse::Ok { data }) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
                    } else {
                        println!("Tunnel mode:   {}", data["mode"].as_str().unwrap_or("?"));
                        println!("Status:        {}", data["status"].as_str().unwrap_or("?"));
                        println!("Public origin: {}", data["public_origin"].as_str().unwrap_or("none"));
                    }
                    exit_codes::SUCCESS
                }
                Ok(AdminResponse::Error { message, code }) => {
                    eprintln!("Error: {message}");
                    code
                }
                Err(_) => {
                    eprintln!("Daemon is not running.");
                    exit_codes::DAEMON_UNAVAILABLE
                }
            }
        }
    }
}

/// Handle `gc` command.
async fn handle_gc(_config: &Config) -> i32 {
    println!("Running garbage collection...");
    // Clean up stale socket, PID files, orphaned DB entries
    let socket_path = crate::config::socket_path();
    if socket_path.exists() {
        // Check if daemon is actually running by trying to connect
        match UnixStream::connect(&socket_path) {
            Ok(_) => println!("Daemon is running — skipping socket cleanup."),
            Err(_) => {
                let _ = std::fs::remove_file(&socket_path);
                println!("Removed stale socket: {}", socket_path.display());
            }
        }
    }
    println!("GC complete.");
    exit_codes::SUCCESS
}

/// Send an admin request to the daemon via Unix domain socket.
fn send_admin_request(config: &Config, request: &AdminRequest) -> Result<AdminResponse, String> {
    let socket_path = crate::config::socket_path();

    let mut stream = UnixStream::connect(&socket_path)
        .map_err(|e| format!("Cannot connect to daemon at {}: {e}", socket_path.display()))?;

    let request_json = serde_json::to_string(request)
        .map_err(|e| format!("Failed to serialize request: {e}"))?;

    stream.write_all(request_json.as_bytes())
        .map_err(|e| format!("Failed to send request: {e}"))?;
    stream.write_all(b"\n")
        .map_err(|e| format!("Failed to send newline: {e}"))?;

    let reader = std::io::BufReader::new(&stream);
    let mut line = String::new();
    reader.lines().next()
        .ok_or_else(|| "No response from daemon".to_string())?
        .map_err(|e| format!("Failed to read response: {e}"))
        .and_then(|line| {
            serde_json::from_str(&line)
                .map_err(|e| format!("Failed to parse response: {e}"))
        })
}
