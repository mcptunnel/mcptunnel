pub mod commands;
pub mod output;

use clap::{Parser, Subcommand};

/// mcptunnel — expose local stdio MCP servers as public HTTPS endpoints.
#[derive(Parser, Debug)]
#[command(name = "mcptunnel", version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Command to run as an MCP server (when no subcommand is given).
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create/register a mount and expose an MCP server.
    Run(RunArgs),

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

    /// View child stderr + gateway logs.
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
    #[arg(long, value_parser = ["quick", "managed"])]
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

    /// OIDC issuer URL.
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

    /// Command to run as an MCP server.
    #[arg(last = true, required = true)]
    pub command: Vec<String>,
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
