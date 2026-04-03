mod auth;
mod cli;
mod config;
mod daemon;
mod doctor;
mod error;
mod gateway;
mod logging;
mod mount;
mod policy;
mod session;
mod tunnel;
mod upstream;

use clap::Parser;

#[tokio::main]
async fn main() {
    let cli = cli::Cli::parse();
    let exit_code = cli::commands::execute(cli).await;
    std::process::exit(exit_code);
}
