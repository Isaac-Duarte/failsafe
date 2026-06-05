use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use failsafe_core::peer::PeerDirectory;
use failsafe_daemon::{Config, Daemon, DaemonError, create_transport};
use tracing::info;

#[derive(Parser)]
#[command(name = "failsafe", about = "Failsafe device sync daemon")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the sync daemon.
    Run {
        /// Path to the config file.
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Print the current configuration.
    Status {
        /// Path to the config file.
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<(), DaemonError> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Run { config } => run(config).await,
        Command::Status { config } => status(config),
    }
}

async fn run(config_path: Option<PathBuf>) -> Result<(), DaemonError> {
    let path = config_path_or_default(config_path)?;
    let config = Config::load_or_create(&path)?;

    match config.transport {
        failsafe_daemon::config::TransportKind::Mock => {
            info!("mock transport is in-memory only; use transport = \"iroh\" for real devices");
        }
        failsafe_daemon::config::TransportKind::Iroh => {
            info!("using iroh transport; share your iroh public key with peers for peer_addresses");
        }
    }

    let peers = Arc::new(PeerDirectory::new());
    peers.replace_peers(config.peers.clone()).await;

    let transport = create_transport(&config, None).await?;
    peers.replace_peers(config.peers.clone()).await;

    let mut daemon = Daemon::from_config(&config, transport, peers)?;
    daemon.apply_config(&config).await;
    daemon.run().await
}

fn status(config_path: Option<PathBuf>) -> Result<(), DaemonError> {
    let path = config_path_or_default(config_path)?;

    if !path.exists() {
        return Err(DaemonError::Config(format!(
            "config file not found at {}",
            path.display()
        )));
    }

    let config = Config::load(&path)?;

    println!("config: {}", path.display());
    println!("device_id: {}", config.device_id);
    println!("transport: {:?}", config.transport);
    println!("peers: {}", config.peers.len());
    for peer in &config.peers {
        println!("  - {peer}");
    }
    println!("peer_addresses: {}", config.peer_addresses.len());
    for (peer, address) in &config.peer_addresses {
        println!("  - {peer} = {address}");
    }
    println!("enabled_features:");
    for feature in &config.enabled_features {
        println!("  - {feature}");
    }

    Ok(())
}

fn config_path_or_default(path: Option<PathBuf>) -> Result<PathBuf, DaemonError> {
    path.or_else(Config::default_path).ok_or_else(|| {
        DaemonError::Config("could not determine config path for this platform".to_owned())
    })
}
