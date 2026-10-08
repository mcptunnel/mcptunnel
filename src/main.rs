use clap::Parser;
use mcptunnel::cli;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = cli::Cli::parse();
    let exit_code = cli::commands::execute(cli).await;
    std::process::ExitCode::from(exit_code as u8)
}
