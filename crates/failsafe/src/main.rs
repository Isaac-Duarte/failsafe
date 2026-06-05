use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Parser, Subcommand};
use failsafe::{
    Config, Credentials, Daemon, DaemonError, ServerClient, create_transport_bundle,
    register_local_device,
};
use failsafe_core::api::DevicePatchRequest;
use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use failsafe_core::peer::PeerDirectory;
use std::io::{self, IsTerminal, Write};
use std::str::FromStr;
use tracing::info;

#[derive(Parser)]
#[command(name = "failsafe", about = "Failsafe device sync daemon")]
struct Cli {
    /// Registration server base URL (overrides config; saved to config when set).
    #[arg(long, global = true)]
    server_url: Option<String>,
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
    /// Manage registered devices on the server.
    Devices {
        #[command(subcommand)]
        command: DevicesCommand,
    },
}

#[derive(Subcommand)]
enum DevicesCommand {
    /// List devices linked to your account.
    List {
        /// Path to the config file.
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Rename a device.
    Rename {
        /// Path to the config file.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Device ID to rename.
        #[arg(long)]
        id: String,
        /// New device name.
        #[arg(long)]
        name: String,
    },
    /// Remove a device from your account.
    Remove {
        /// Path to the config file.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Device ID to remove.
        #[arg(long)]
        id: String,
        /// Skip confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Set which features a device can sync with others.
    Features {
        /// Path to the config file.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Device ID to update.
        #[arg(long)]
        id: String,
        /// Comma-separated feature list (e.g. clipboard).
        #[arg(long)]
        features: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), DaemonError> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let server_url = cli
        .server_url
        .or_else(|| std::env::var("FAILSAFE_SERVER_URL").ok());

    match cli.command {
        Command::Run { config } => run(config, server_url).await,
        Command::Register {
            config,
            email,
            password,
        } => authenticate(config, server_url, email, password, true).await,
        Command::Login {
            config,
            email,
            password,
        } => authenticate(config, server_url, email, password, false).await,
        Command::Pair { config, code, name } => pair(config, server_url, code, name).await,
        Command::Status { config } => status(config, server_url),
        Command::Devices { command } => devices(command, server_url).await,
    }
}

fn load_config(
    path: &Path,
    server_url: Option<String>,
    create: bool,
) -> Result<Config, DaemonError> {
    let mut config = if create {
        Config::load_or_create(path)?
    } else {
        Config::load(path)?
    };
    config.apply_server_url_override(path, server_url)?;
    Ok(config)
}

async fn run(config_path: Option<PathBuf>, server_url: Option<String>) -> Result<(), DaemonError> {
    let path = config_path_or_default(config_path)?;
    let config = load_config(&path, server_url, true)?;

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
    server_url: Option<String>,
    email: String,
    password: String,
    register: bool,
) -> Result<(), DaemonError> {
    let path = config_path_or_default(config_path)?;
    let config = load_config(&path, server_url, true)?;

    let response = if register {
        ServerClient::register(&config.server_url, &email, &password).await?
    } else {
        ServerClient::login(&config.server_url, &email, &password).await?
    };

    let credentials_path = Credentials::default_path().ok_or_else(|| {
        DaemonError::Config("could not determine credentials path for this platform".to_owned())
    })?;
    Credentials {
        auth_token: response.token.clone(),
    }
    .save(&credentials_path)?;

    register_local_device(&config, response.token).await?;

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
    server_url: Option<String>,
    code: Option<String>,
    device_name: Option<String>,
) -> Result<(), DaemonError> {
    let path = config_path_or_default(config_path)?;

    match code {
        Some(code) => pair_join(&path, server_url, &code, device_name).await,
        None => pair_host(&path, server_url).await,
    }
}

async fn pair_host(config_path: &Path, server_url: Option<String>) -> Result<(), DaemonError> {
    let config = load_config(config_path, server_url, true)?;
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
    config_path: &Path,
    server_url: Option<String>,
    code: &str,
    device_name: Option<String>,
) -> Result<(), DaemonError> {
    let mut config = load_config(config_path, server_url, true)?;
    if let Some(name) = device_name {
        config.device_name = name;
    } else if config.device_name == "my-device" {
        config.device_name = default_hostname();
    }

    let normalized = normalize_pairing_code(code).ok_or_else(|| {
        DaemonError::Config("pairing code must be 6 uppercase alphanumeric characters".to_owned())
    })?;

    let response = ServerClient::redeem_pairing_code(&config.server_url, &normalized).await?;

    let credentials_path = Credentials::default_path().ok_or_else(|| {
        DaemonError::Config("could not determine credentials path for this platform".to_owned())
    })?;
    Credentials {
        auth_token: response.token.clone(),
    }
    .save(&credentials_path)?;
    config.save(config_path)?;

    register_local_device(&config, response.token).await?;

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
    gethostname::gethostname().to_string_lossy().into_owned()
}

fn status(config_path: Option<PathBuf>, server_url: Option<String>) -> Result<(), DaemonError> {
    let path = config_path_or_default(config_path)?;

    if !path.exists() {
        return Err(DaemonError::Config(format!(
            "config file not found at {}",
            path.display()
        )));
    }

    let config = load_config(&path, server_url, false)?;

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

async fn devices(
    command: DevicesCommand,
    server_url: Option<String>,
) -> Result<(), DaemonError> {
    match command {
        DevicesCommand::List { config } => devices_list(config, server_url).await,
        DevicesCommand::Rename { config, id, name } => {
            devices_rename(config, server_url, &id, &name).await
        }
        DevicesCommand::Remove { config, id, yes } => {
            devices_remove(config, server_url, &id, yes).await
        }
        DevicesCommand::Features {
            config,
            id,
            features,
        } => devices_features(config, server_url, &id, &features).await,
    }
}

async fn server_client_from_config(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
) -> Result<ServerClient, DaemonError> {
    let path = config_path_or_default(config_path)?;
    let config = load_config(&path, server_url, true)?;
    let credentials = Credentials::load_or_error()?;
    Ok(ServerClient::new(
        config.server_url.clone(),
        credentials.auth_token,
    ))
}

fn parse_device_id(id: &str) -> Result<DeviceId, DaemonError> {
    DeviceId::from_str(id.trim()).map_err(|error| {
        DaemonError::Config(format!("invalid device id `{id}`: {error}"))
    })
}

fn parse_feature_list(features: &str) -> Result<Vec<FeatureId>, DaemonError> {
    if features.trim().is_empty() {
        return Ok(vec![]);
    }

    features
        .split(',')
        .map(|part| {
            FeatureId::from_str(part.trim()).map_err(|error| {
                DaemonError::Config(format!("unknown feature `{}`: {error}", part.trim()))
            })
        })
        .collect()
}

async fn devices_list(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
) -> Result<(), DaemonError> {
    let client = server_client_from_config(config_path, server_url).await?;
    let response = client.list_devices().await?;

    if response.devices.is_empty() {
        println!("No devices registered.");
        return Ok(());
    }

    println!(
        "{:<20} {:<38} {:<12} {:<20} {}",
        "NAME", "DEVICE ID", "STATUS", "FEATURES", "LAST SEEN"
    );
    for device in response.devices {
        let features = device
            .enabled_features
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let status = if device.online { "online" } else { "offline" };
        let last_seen = device.last_seen.unwrap_or_else(|| "—".to_owned());
        println!(
            "{:<20} {:<38} {:<12} {:<20} {}",
            device.name, device.device_id, status, features, last_seen
        );
    }

    Ok(())
}

async fn devices_rename(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
    id: &str,
    name: &str,
) -> Result<(), DaemonError> {
    let client = server_client_from_config(config_path, server_url).await?;
    let device_id = parse_device_id(id)?;

    if name.trim().is_empty() {
        return Err(DaemonError::Config("name cannot be empty".to_owned()));
    }

    client
        .patch_device(
            device_id,
            DevicePatchRequest {
                name: Some(name.trim().to_owned()),
                enabled_features: None,
            },
        )
        .await?;

    println!("Renamed device {device_id} to {name}");
    Ok(())
}

async fn devices_remove(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
    id: &str,
    skip_confirm: bool,
) -> Result<(), DaemonError> {
    let client = server_client_from_config(config_path, server_url).await?;
    let device_id = parse_device_id(id)?;

    if !skip_confirm && io::stdin().is_terminal() {
        print!("Remove device {device_id}? This cannot be undone without re-pairing. [y/N] ");
        io::stdout().flush().map_err(DaemonError::Io)?;

        let mut answer = String::new();
        io::stdin().read_line(&mut answer).map_err(DaemonError::Io)?;
        let answer = answer.trim().to_ascii_lowercase();
        if answer != "y" && answer != "yes" {
            println!("Cancelled.");
            return Ok(());
        }
    }

    client.delete_device(device_id).await?;
    println!("Removed device {device_id}");
    Ok(())
}

async fn devices_features(
    config_path: Option<PathBuf>,
    server_url: Option<String>,
    id: &str,
    features: &str,
) -> Result<(), DaemonError> {
    let client = server_client_from_config(config_path, server_url).await?;
    let device_id = parse_device_id(id)?;
    let enabled_features = parse_feature_list(features)?;

    client
        .patch_device(
            device_id,
            DevicePatchRequest {
                name: None,
                enabled_features: Some(enabled_features.clone()),
            },
        )
        .await?;

    let feature_list = enabled_features
        .iter()
        .map(|f| f.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    if feature_list.is_empty() {
        println!("Cleared features for device {device_id}");
    } else {
        println!("Updated features for device {device_id}: {feature_list}");
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
