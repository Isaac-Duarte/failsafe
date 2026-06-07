mod cli;

use clap::Parser;
use failsafe::DaemonError;

use cli::{Cli, Command};

fn main() -> Result<(), DaemonError> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let mut runtime = if matches!(cli.command, Command::Screen { .. }) {
        // minifb uses AppKit APIs that must run on the process main thread on macOS.
        tokio::runtime::Builder::new_current_thread()
    } else {
        tokio::runtime::Builder::new_multi_thread()
    };

    runtime
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(cli::execute(cli))
}
