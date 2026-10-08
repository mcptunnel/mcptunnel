use crate::daemon::ipc::Client as AdminStream;
use std::io::{BufRead, Write};
#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::cli::{Cli, Commands, DaemonCommand, RunArgs, TunnelCommand};
use crate::config::{load_config, Config};
use crate::daemon::admin_api::{AdminRequest, AdminResponse, MountCreateRequest};
use crate::error::exit_codes;

/// Execute the CLI command.
pub async fn execute(cli: Cli) -> i32 {
    let config = match load_config() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Error: {error}");
            return exit_codes::CLI_USAGE;
        }
    };

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
        Commands::Run(args) => handle_run(&config, *args).await,
        Commands::List { json } => handle_list(&config, json).await,
        Commands::Inspect { slug, json } => handle_inspect(&config, &slug, json).await,
        Commands::Logs { slug, follow } => handle_logs(&config, &slug, follow).await,
        Commands::Stop { slug, all } => handle_stop(&config, slug.as_deref(), all).await,
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

    // Parse env vars
    let env_vars: Vec<(String, String)> = args
        .env
        .iter()
        .filter_map(|e| {
            let mut parts = e.splitn(2, '=');
            Some((parts.next()?.to_string(), parts.next()?.to_string()))
        })
        .collect();

    let request = AdminRequest::MountCreate(Box::new(MountCreateRequest {
        slug: args.slug,
        display_name: args.name,
        program: program.clone(),
        args: cmd_args.to_vec(),
        cwd: args.cwd.or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|path| path.to_string_lossy().into_owned())
        }),
        tunnel_mode: args.tunnel,
        hostname: args.hostname,
        auth_mode: args.auth,
        oidc_issuer: args.oidc_issuer,
        scopes: args.scope,
        allow_tools: args.allow_tool,
        deny_tools: args.deny_tool,
        hide_resources: args.hide_resources,
        hide_prompts: args.hide_prompts,
        env_vars,
        probe_mode: args.probe,
    }));

    match send_admin_request(config, &request) {
        Ok(AdminResponse::Ok { data }) => {
            if is_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&data).unwrap_or_default()
                );
            } else {
                let mut warnings = Vec::new();
                if data["mount"]["auth_mode"] == "public" {
                    warnings.push(
                        "No authentication. This endpoint is publicly reachable.".to_string(),
                    );
                }
                if data["mount"]["tunnel_mode"] == "quick" {
                    warnings.push("Quick Tunnel is for testing/development only.".to_string());
                    warnings.push("Quick Tunnel URLs are temporary and SSE is unavailable. Managed provisioning is not implemented.".to_string());
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
                println!(
                    "{}",
                    serde_json::to_string_pretty(&data).unwrap_or_default()
                );
            } else {
                let mounts = data["mounts"]
                    .as_array()
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
    let request = AdminRequest::MountInspect {
        slug: slug.to_string(),
    };
    match send_admin_request(config, &request) {
        Ok(AdminResponse::Ok { data }) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&data).unwrap_or_default()
                );
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
async fn handle_logs(config: &Config, slug: &str, follow: bool) -> i32 {
    let request = AdminRequest::MountLogsTail {
        slug: slug.to_string(),
        lines: Some(100),
    };
    match send_admin_request(config, &request) {
        Ok(AdminResponse::Ok { data }) => {
            if let Some(lines) = data["lines"].as_array() {
                for line in lines {
                    if let Some(s) = line.as_str() {
                        println!("{s}");
                    }
                }
                if lines.is_empty() {
                    println!("No log entries for mount '{slug}'.");
                }
            }

            if follow {
                eprintln!("(--follow: polling every 2 seconds, Ctrl+C to stop)");
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    let request = AdminRequest::MountLogsTail {
                        slug: slug.to_string(),
                        lines: Some(5),
                    };
                    if let Ok(AdminResponse::Ok { data }) = send_admin_request(config, &request) {
                        if let Some(lines) = data["lines"].as_array() {
                            for line in lines {
                                if let Some(s) = line.as_str() {
                                    println!("{s}");
                                }
                            }
                        }
                    }
                }
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

/// Handle `stop` command.
async fn handle_stop(config: &Config, slug: Option<&str>, all: bool) -> i32 {
    if all {
        // Stop all mounts
        let list_request = AdminRequest::MountList;
        let slugs = match send_admin_request(config, &list_request) {
            Ok(AdminResponse::Ok { data }) => data["mounts"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| m["slug"].as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            Ok(AdminResponse::Error { message, code }) => {
                eprintln!("Error: {message}");
                return code;
            }
            Err(e) => {
                eprintln!("Error: {e}");
                return exit_codes::DAEMON_UNAVAILABLE;
            }
        };

        if slugs.is_empty() {
            println!("No mounts to stop.");
            return exit_codes::SUCCESS;
        }

        let mut total_sessions = 0u64;
        for s in &slugs {
            let request = AdminRequest::MountDelete { slug: s.clone() };
            match send_admin_request(config, &request) {
                Ok(AdminResponse::Ok { data }) => {
                    let sessions = data["sessions_terminated"].as_u64().unwrap_or(0);
                    total_sessions += sessions;
                    println!("Stopped '{s}' ({sessions} session(s))");
                }
                Ok(AdminResponse::Error { message, .. }) => {
                    eprintln!("Failed to stop '{s}': {message}");
                }
                Err(e) => {
                    eprintln!("Failed to stop '{s}': {e}");
                }
            }
        }
        println!(
            "All {} mount(s) stopped. {total_sessions} session(s) terminated.",
            slugs.len()
        );
        return exit_codes::SUCCESS;
    }

    let slug = match slug {
        Some(s) => s,
        None => {
            eprintln!("Error: provide a mount slug or use --all");
            return exit_codes::CLI_USAGE;
        }
    };

    let request = AdminRequest::MountDelete {
        slug: slug.to_string(),
    };
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
            let request = AdminRequest::DoctorRun {
                slug: Some(slug.to_string()),
            };
            if let Ok(AdminResponse::Ok { data }) =
                send_admin_request_no_autostart(config, &request)
            {
                if let Some(checks) = data.get("checks") {
                    all_checks["daemon_checks"] = checks.clone();
                }
            }
        }

        println!(
            "{}",
            serde_json::to_string_pretty(&all_checks).unwrap_or_default()
        );
    } else {
        crate::doctor::format_checks_human(&local_checks)
            .lines()
            .for_each(|l| println!("{l}"));

        if let Some(slug) = slug {
            let request = AdminRequest::DoctorRun {
                slug: Some(slug.to_string()),
            };
            if let Ok(AdminResponse::Ok { data }) =
                send_admin_request_no_autostart(config, &request)
            {
                if let Some(checks) = data["checks"].as_array() {
                    crate::cli::output::print_doctor_checks(checks);
                }
            }
        }
    }

    exit_codes::SUCCESS
}

/// Handle `daemon` subcommands.
/// Daemon commands never auto-start the daemon — explicit management only.
async fn handle_daemon(config: &Config, command: DaemonCommand) -> i32 {
    match command {
        DaemonCommand::Status { json } => {
            let request = AdminRequest::DaemonStatus;
            match send_admin_request_no_autostart(config, &request) {
                Ok(AdminResponse::Ok { data }) => {
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&data).unwrap_or_default()
                        );
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
            let request = AdminRequest::DaemonShutdown;
            match send_admin_request_no_autostart(config, &request) {
                Ok(AdminResponse::Ok { data }) => {
                    let mounts = data["mounts_active"].as_u64().unwrap_or(0);
                    println!("Daemon stopped ({mounts} mount(s) were active).");
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
        DaemonCommand::Restart => {
            // Stop the daemon if running
            let request = AdminRequest::DaemonShutdown;
            match send_admin_request_no_autostart(config, &request) {
                Ok(AdminResponse::Ok { .. }) => {
                    println!("Daemon stopped.");
                    // Wait for socket to be cleaned up
                    let socket_path = crate::config::socket_path();
                    for _ in 0..30 {
                        if !socket_path.exists() {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
                Err(_) => {
                    println!("Daemon was not running.");
                }
                _ => {}
            }

            // Start new daemon in background
            match auto_start_daemon() {
                Ok(()) => {
                    println!("Daemon restarted.");
                    exit_codes::SUCCESS
                }
                Err(e) => {
                    eprintln!("Failed to restart daemon: {e}");
                    exit_codes::INTERNAL_GATEWAY_ERROR
                }
            }
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
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&data).unwrap_or_default()
                        );
                    } else {
                        println!("Tunnel mode:   {}", data["mode"].as_str().unwrap_or("?"));
                        println!("Status:        {}", data["status"].as_str().unwrap_or("?"));
                        println!(
                            "Public origin: {}",
                            data["public_origin"].as_str().unwrap_or("none")
                        );
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
        match crate::daemon::ipc::connect() {
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
/// If the daemon is not running and auto_start is enabled, start it first.
fn send_admin_request(config: &Config, request: &AdminRequest) -> Result<AdminResponse, String> {
    send_admin_request_inner(request, config.daemon.auto_start)
}

/// Send an admin request without auto-starting the daemon.
fn send_admin_request_no_autostart(
    _config: &Config,
    request: &AdminRequest,
) -> Result<AdminResponse, String> {
    send_admin_request_inner(request, false)
}

fn send_admin_request_inner(
    request: &AdminRequest,
    auto_start: bool,
) -> Result<AdminResponse, String> {
    let socket_path = crate::config::socket_path();

    match crate::daemon::ipc::connect() {
        Ok(stream) => send_on_stream(stream, request),
        Err(_) if auto_start => {
            eprintln!("Daemon not running. Auto-starting...");
            auto_start_daemon()?;
            let stream = crate::daemon::ipc::connect()
                .map_err(|e| format!("Cannot connect to daemon after auto-start: {e}"))?;
            send_on_stream(stream, request)
        }
        Err(e) => Err(format!(
            "Cannot connect to daemon at {}: {e}\nStart the daemon with: mcptunnel daemon start",
            socket_path.display()
        )),
    }
}

/// Send a request on an already-connected Unix stream.
fn send_on_stream(
    mut stream: AdminStream,
    request: &AdminRequest,
) -> Result<AdminResponse, String> {
    let request_json =
        serde_json::to_string(request).map_err(|e| format!("Failed to serialize request: {e}"))?;

    stream
        .write_all(request_json.as_bytes())
        .map_err(|e| format!("Failed to send request: {e}"))?;
    stream
        .write_all(b"\n")
        .map_err(|e| format!("Failed to send newline: {e}"))?;

    let reader = std::io::BufReader::new(&stream);
    reader
        .lines()
        .next()
        .ok_or_else(|| "No response from daemon".to_string())?
        .map_err(|e| format!("Failed to read response: {e}"))
        .and_then(|line| {
            serde_json::from_str(&line).map_err(|e| format!("Failed to parse response: {e}"))
        })
}

/// Auto-start the daemon as a background process.
fn auto_start_daemon() -> Result<(), String> {
    use std::process::Command;

    // Ensure state directory exists
    let state_dir = crate::config::state_dir();
    std::fs::create_dir_all(&state_dir).map_err(|e| format!("Failed to create state dir: {e}"))?;

    // Get the current executable path
    let exe =
        std::env::current_exe().map_err(|e| format!("Failed to get current executable: {e}"))?;

    // Spawn daemon as a background process
    let mut command = Command::new(&exe);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let child = command
        .args(["daemon", "start"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn daemon: {e}"))?;

    eprintln!("Daemon started (PID: {}).", child.id());

    // Wait for the socket to appear (up to 5 seconds)
    let socket_path = crate::config::socket_path();
    for _ in 0..50 {
        if socket_path.exists() {
            // Give it a moment to start accepting connections
            std::thread::sleep(std::time::Duration::from_millis(100));
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    Err("Daemon started but socket not available after 5 seconds".to_string())
}
