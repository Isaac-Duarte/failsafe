use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use failsafe_core::peer::PeerDirectory;
use failsafe_daemon::{
    Config, Credentials, Daemon, DaemonError, ServerClient, create_transport_bundle,
};
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
    /// Authenticate with the registration server.
    Login {
        /// Path to the config file.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Account email address.
        #[arg(long)]
        email: String,
        /// Account password.
        #[arg(long)]
        password: String,
        /// Create a new account instead of logging in.
        #[arg(long)]
        register: bool,
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
        Command::Login {
            config,
            email,
            password,
            register,
        } => login(config, email, password, register).await,
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
        failsafe_daemon::config::TransportKind::Iroh => {}
    }

    let credentials = Credentials::load_or_error()?;
    let server_client = ServerClient::new(config.server_url.clone(), credentials.auth_token);

    let peers = Arc::new(PeerDirectory::new());
    let bundle = create_transport_bundle(&config, None).await?;

    if let Some(key) = &bundle.iroh_public_key {
        info!(iroh_public_key = %key, "iroh endpoint ready");
    }

    let mut daemon = Daemon::from_config(&config, bundle, peers, Some(server_client))?;
    daemon.run().await
}

async fn login(
    config_path: Option<PathBuf>,
    email: String,
    password: String,
    register: bool,
) -> Result<(), DaemonError> {
    let path = config_path_or_default(config_path)?;
    let config = Config::load_or_create(&path)?;

    let response = if register {
        ServerClient::register(&config.server_url, &email, &password).await?
    } else {
        ServerClient::login(&config.server_url, &email, &password).await?
    };

    let credentials_path = Credentials::default_path().ok_or_else(|| {
        DaemonError::Config("could not determine credentials path for this platform".to_owned())
    })?;
    Credentials {
        auth_token: response.token,
    }
    .save(&credentials_path)?;

    info!(
        credentials = %credentials_path.display(),
        "saved authentication credentials"
    );
    Ok(())
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
    println!("device_name: {}", config.device_name);
    println!("server_url: {}", config.server_url);
    if let Some(credentials_path) = Credentials::default_path() {
        println!(
            "credentials: {}",
            if credentials_path.exists() {
                format!("present at {}", credentials_path.display())
            } else {
                "not found".to_owned()
            }
        );
    }
    println!("transport: {:?}", config.transport);
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
