use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "failsafe", about = "Failsafe device sync daemon")]
pub struct Cli {
    /// Registration server base URL (overrides config; saved to config when set).
    #[arg(long, global = true)]
    pub server_url: Option<String>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
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
    /// Open an interactive shell on a paired device.
    Shell {
        /// Path to the config file.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Device name or ID. Omit for interactive selection.
        device: Option<String>,
    },
    /// Send files to a paired device.
    Send {
        /// Path to the config file.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Files or directories to send.
        paths: Vec<PathBuf>,
        /// Resume an interrupted send by transfer ID.
        #[arg(long)]
        resume: Option<uuid::Uuid>,
        /// Device name or ID. Omit for interactive selection.
        #[arg(long)]
        device: Option<String>,
        /// Skip confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Forward a local TCP port to a paired device.
    Port {
        /// Path to the config file.
        #[arg(long)]
        config: Option<PathBuf>,
        /// Local port, or LOCAL:REMOTE (e.g. 8080:3000).
        port: Option<String>,
        /// Protocol (tcp only for now).
        protocol: Option<String>,
        /// Override remote port when PORT is a single number.
        #[arg(long)]
        remote_port: Option<u16>,
        /// Device name or ID. Omit for interactive selection.
        device: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum DevicesCommand {
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
