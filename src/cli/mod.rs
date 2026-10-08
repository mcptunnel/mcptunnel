pub mod commands;
pub mod output;

use clap::{Args, Parser, Subcommand};

/// mcptunnel — serve stdio MCP on localhost; add --public for temporary HTTPS.
#[derive(Parser, Debug)]
#[command(name = "mcptunnel", version, about, long_about = None)]
#[command(propagate_version = true)]
#[command(args_conflicts_with_subcommands = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub run: RunOptions,

    /// Command to run as an MCP server (when no subcommand is given).
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create/register a mount and expose an MCP server.
    Run(Box<RunArgs>),

    /// Show all mounts.
    List {
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Detailed mount info.
    Inspect {
        /// Mount slug.
        slug: String,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },

    /// View sanitized gateway logs.
    Logs {
        /// Mount slug.
        slug: String,
        /// Follow log output.
        #[arg(long, short)]
        follow: bool,
    },

    /// Remove a mount and terminate its sessions.
    Stop {
        /// Mount slug (omit when using --all).
        slug: Option<String>,
        /// Stop all mounts.
        #[arg(long)]
        all: bool,
    },

    /// Run environment + protocol diagnostics.
    Doctor {
        /// Optional mount slug for mount-specific checks.
        slug: Option<String>,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Daemon management.
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },

    /// Tunnel management.
    Tunnel {
        #[command(subcommand)]
        command: TunnelCommand,
    },

    /// Cleanup stale records and processes.
    Gc,
}

#[derive(Parser, Debug)]
pub struct RunArgs {
    #[command(flatten)]
    pub options: RunOptions,

    /// Command to run as an MCP server.
    #[arg(last = true, required = true)]
    pub command: Vec<String>,
}

#[derive(Args, Debug, Default)]
pub struct RunOptions {
    /// Open a temporary Cloudflare HTTPS URL for this server.
    #[arg(long, conflicts_with = "tunnel")]
    pub public: bool,

    /// Local listener port (default: 8000, or daemon.bind in custom config).
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
    pub port: Option<u16>,

    /// Exact MCP endpoint path (default: /<server-name>).
    #[arg(long)]
    pub path: Option<String>,
    /// Explicit mount slug.
    #[arg(long)]
    pub slug: Option<String>,

    /// Display name.
    #[arg(long)]
    pub name: Option<String>,

    /// Working directory for the child process.
    #[arg(long)]
    pub cwd: Option<String>,

    /// Environment variable for the child process (KEY=VALUE).
    #[arg(long, value_name = "KEY=VALUE")]
    pub env: Vec<String>,

    /// Tunnel mode.
    #[arg(long, value_parser = ["quick", "managed", "local"])]
    pub tunnel: Option<String>,

    /// Managed mode hostname.
    #[arg(long)]
    pub hostname: Option<String>,

    /// Auth mode.
    #[arg(long, value_parser = ["public", "basic", "inherit", "oauth"])]
    pub auth: Option<String>,

    /// Default scopes for OAuth mode.
    #[arg(long)]
    pub scope: Vec<String>,

    /// OIDC issuer URL (for external OAuth AS mode).
    #[arg(long)]
    pub oidc_issuer: Option<String>,

    /// Tool allowlist entry.
    #[arg(long)]
    pub allow_tool: Vec<String>,

    /// Tool denylist entry.
    #[arg(long)]
    pub deny_tool: Vec<String>,

    /// Hide all resources.
    #[arg(long)]
    pub hide_resources: bool,

    /// Hide all prompts.
    #[arg(long)]
    pub hide_prompts: bool,

    /// Probe behavior.
    #[arg(long, value_parser = ["eager", "lazy", "off"])]
    pub probe: Option<String>,

    /// Output as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Subcommand, Debug)]
pub enum DaemonCommand {
    /// Show local daemon state.
    Status {
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Stop the daemon.
    Stop {
        /// Force stop even if mounts are active.
        #[arg(long)]
        force: bool,
    },
    /// Start the daemon in the foreground.
    Start,
    /// Restart the daemon (stop + start in background).
    Restart,
}

#[derive(Subcommand, Debug)]
pub enum TunnelCommand {
    /// Show tunnel state.
    Status {
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optional_flags_and_argv_boundary() {
        let cli = Cli::try_parse_from([
            "mcptunnel",
            "--",
            "npx",
            "-y",
            "@playwright/mcp",
            "--port",
            "9000",
        ])
        .unwrap();
        assert!(!cli.run.public);
        assert_eq!(cli.run.port, None);
        assert_eq!(cli.args, ["npx", "-y", "@playwright/mcp", "--port", "9000"]);
        let cli = Cli::try_parse_from([
            "mcptunnel",
            "--public",
            "--port",
            "9001",
            "--path",
            "/team/files",
            "--json",
            "--",
            "uvx",
            "mcp-server-git",
        ])
        .unwrap();
        assert!(cli.run.public);
        assert_eq!(cli.run.port, Some(9001));
        assert_eq!(cli.run.path.as_deref(), Some("/team/files"));
        let cli = Cli::try_parse_from([
            "mcptunnel",
            "run",
            "--public",
            "--",
            "npx",
            "-y",
            "@playwright/mcp",
        ])
        .unwrap();
        assert!(matches!(cli.command, Some(Commands::Run(args)) if args.options.public));
        for port in ["0", "65536", "-1"] {
            assert!(
                Cli::try_parse_from(["mcptunnel", "--port", port, "--", "node", "server.js"])
                    .is_err()
            );
        }
        assert!(Cli::try_parse_from([
            "mcptunnel",
            "--public",
            "--tunnel",
            "local",
            "--",
            "node",
            "server.js"
        ])
        .is_err());
        assert!(matches!(
            Cli::try_parse_from(["mcptunnel", "list", "--json"])
                .unwrap()
                .command,
            Some(Commands::List { json: true })
        ));
    }
}
