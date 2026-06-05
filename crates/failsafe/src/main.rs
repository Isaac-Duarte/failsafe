use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use failsafe_core::peer::PeerDirectory;
use failsafe::{
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
    /// Create a new account on the registration server.
    Register {
        /// Path to the config file.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Account email address.
        #[arg(long)]
        email: String,
        /// Account password.
        #[arg(long)]
        password: String,
    },
    /// Log in to an existing account on the registration server.
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
    },
    /// Pair this device with an account.
    Pair {
        /// Path to the config file.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Pairing code from another paired device.
        #[arg(long)]
        code: Option<String>,
        /// Device name to use when joining with a code.
        #[arg(long)]
        name: Option<String>,
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
        Command::Register {
            config,
            email,
            password,
        } => authenticate(config, email, password, true).await,
        Command::Login {
            config,
            email,
            password,
        } => authenticate(config, email, password, false).await,
        Command::Pair { config, code, name } => pair(config, code, name).await,
        Command::Status { config } => status(config),
    }
}

async fn run(config_path: Option<PathBuf>) -> Result<(), DaemonError> {
    let path = config_path_or_default(config_path)?;
    let config = Config::load_or_create(&path)?;

    match config.transport {
        failsafe::config::TransportKind::Mock => {
            info!("mock transport is in-memory only; use transport = \"iroh\" for real devices");
        }
        failsafe::config::TransportKind::Iroh => {}
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

async fn authenticate(
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

    if register {
        println!("Account created and logged in.");
    } else {
        println!("Logged in.");
    }
    println!("Credentials saved to {}", credentials_path.display());
    println!();
    println!("Start syncing with:");
    println!("  failsafe run");
    Ok(())
}

async fn pair(
    config_path: Option<PathBuf>,
    code: Option<String>,
    device_name: Option<String>,
) -> Result<(), DaemonError> {
    let path = config_path_or_default(config_path)?;

    match code {
        Some(code) => pair_join(&path, &code, device_name).await,
        None => pair_host(&path).await,
    }
}

async fn pair_host(config_path: &PathBuf) -> Result<(), DaemonError> {
    let config = Config::load_or_create(config_path)?;
    let credentials = Credentials::load_or_error()?;
    let client = ServerClient::new(config.server_url.clone(), credentials.auth_token);
    let response = client.create_pairing_code().await?;

    println!("Pairing code: {}", response.code);
    println!("Expires at:   {}", response.expires_at);
    println!();
    println!("On the new device, run:");
    println!("  failsafe pair --code {}", response.code);
    Ok(())
}

async fn pair_join(
    config_path: &PathBuf,
    code: &str,
    device_name: Option<String>,
) -> Result<(), DaemonError> {
    let mut config = Config::load_or_create(config_path)?;
    if let Some(name) = device_name {
        config.device_name = name;
    } else if config.device_name == "my-device" {
        config.device_name = default_hostname();
    }

    let normalized = normalize_pairing_code(code).ok_or_else(|| {
        DaemonError::Config(
            "pairing code must be 6 uppercase alphanumeric characters".to_owned(),
        )
    })?;

    let response = ServerClient::redeem_pairing_code(&config.server_url, &normalized).await?;

    let credentials_path = Credentials::default_path().ok_or_else(|| {
        DaemonError::Config("could not determine credentials path for this platform".to_owned())
    })?;
    Credentials {
        auth_token: response.token,
    }
    .save(&credentials_path)?;
    config.save(config_path)?;

    println!("Paired successfully.");
    println!("Device ID:   {}", config.device_id);
    println!("Device name: {}", config.device_name);
    println!();
    println!("Start syncing with:");
    println!("  failsafe run");
    Ok(())
}

fn normalize_pairing_code(code: &str) -> Option<String> {
    let normalized = code.trim().to_uppercase();
    if normalized.len() != 6 {
        return None;
    }

    if !normalized
        .chars()
        .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
    {
        return None;
    }

    Some(normalized)
}

fn default_hostname() -> String {
    gethostname::gethostname()
        .to_string_lossy()
        .into_owned()
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
                "not found — run `failsafe register`, `failsafe login`, or `failsafe pair --code`"
                    .to_owned()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_pairing_code_accepts_case_insensitive_input() {
        assert_eq!(normalize_pairing_code("a3k9z1").as_deref(), Some("A3K9Z1"));
        assert!(normalize_pairing_code("too-short").is_none());
    }
}
