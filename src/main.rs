use clap::Parser;
use mcptunnel::cli;

#[tokio::main]
async fn main() {
    let cli = cli::Cli::parse();
    let exit_code = cli::commands::execute(cli).await;
    std::process::exit(exit_code);
}
