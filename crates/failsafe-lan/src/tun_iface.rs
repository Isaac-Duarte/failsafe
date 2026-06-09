use std::io;
use std::net::Ipv4Addr;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;

use failsafe_core::virtual_lan::network_address;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};
use tun::AbstractDevice;

use crate::tun_fd::recv_tun_fd;
use crate::tun_setup::{
    build_tun_config, configure_routes, map_tun_error, open_sync_device, remove_routes,
};

#[derive(Debug, Error)]
pub enum TunError {
    #[error("virtual LAN requires administrator privileges: {0}")]
    PermissionDenied(String),

    #[error("failed to configure virtual network interface: {0}")]
    Configuration(String),

    #[error("tun device error: {0}")]
    Io(#[from] io::Error),
}

pub struct TunHandle {
    device: Arc<Mutex<tun::AsyncDevice>>,
    name: String,
    network: Ipv4Addr,
    elevated: bool,
}

impl TunHandle {
    pub fn open(local_ip: Ipv4Addr) -> Result<Self, TunError> {
        match open_direct(local_ip) {
            Ok(handle) => Ok(handle),
            #[cfg(unix)]
            Err(TunError::PermissionDenied(_)) => open_via_sudo_helper(local_ip),
            Err(error) => Err(error),
        }
    }

    pub fn device(&self) -> Arc<Mutex<tun::AsyncDevice>> {
        self.device.clone()
    }

    pub fn subnet_cidr(&self) -> String {
        format!("{}/24", self.network)
    }
}

impl Drop for TunHandle {
    fn drop(&mut self) {
        if let Err(error) = remove_routes(&self.name, self.network, self.elevated) {
            warn!(interface = %self.name, "failed to remove virtual LAN routes: {error}");
        } else {
            debug!(interface = %self.name, "virtual LAN routes removed");
        }
    }
}

fn open_direct(local_ip: Ipv4Addr) -> Result<TunHandle, TunError> {
    let network = network_address(local_ip);
    let config = build_tun_config(local_ip);
    let device = tun::create_as_async(&config).map_err(map_tun_error)?;
    let name = device
        .tun_name()
        .map_err(|error| TunError::Configuration(error.to_string()))?;

    configure_routes(&name, network)?;

    info!(%local_ip, interface = %name, "virtual LAN interface ready");

    Ok(TunHandle {
        device: Arc::new(Mutex::new(device)),
        name,
        network,
        elevated: false,
    })
}

#[cfg(unix)]
fn open_via_sudo_helper(local_ip: Ipv4Addr) -> Result<TunHandle, TunError> {
    use std::os::fd::IntoRawFd;
    let network = network_address(local_ip);
    let socket_path = tun_helper_socket_path()?;

    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(&socket_path).map_err(|error| {
        TunError::Configuration(format!(
            "failed to bind tun helper socket {}: {error}",
            socket_path.display()
        ))
    })?;

    let exe = std::env::current_exe().map_err(|error| {
        TunError::Configuration(format!("failed to resolve failsafe binary path: {error}"))
    })?;

    eprintln!(
        "Virtual LAN needs administrator permission to create a network interface."
    );
    eprintln!("Enter your password when sudo prompts.");

    let mut child = Command::new("sudo");
    child
        .arg(&exe)
        .arg("tun-helper")
        .arg("--socket")
        .arg(&socket_path)
        .arg("--ip")
        .arg(local_ip.to_string())
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let mut child = child
        .spawn()
        .map_err(|error| TunError::Configuration(format!("failed to run sudo: {error}")))?;

    let (stream, _) = listener.accept().map_err(|error| {
        TunError::Configuration(format!("failed to accept tun helper connection: {error}"))
    })?;

    let (name, owned_fd) = recv_tun_fd(&stream).map_err(|error| {
        TunError::Configuration(format!("failed to receive tun fd from helper: {error}"))
    })?;

    let status = child
        .wait()
        .map_err(|error| TunError::Configuration(format!("failed to wait for sudo helper: {error}")))?;

    if !status.success() {
        return Err(TunError::PermissionDenied(
            "virtual LAN setup was cancelled or denied; run `failsafe run` in a terminal and approve sudo when prompted".to_owned(),
        ));
    }

    let raw_fd = owned_fd.into_raw_fd();
    let mut config = tun::Configuration::default();
    config.raw_fd(raw_fd);
    let device = tun::create_as_async(&config).map_err(map_tun_error)?;

    info!(%local_ip, interface = %name, "virtual LAN interface ready via sudo helper");

    Ok(TunHandle {
        device: Arc::new(Mutex::new(device)),
        name,
        network,
        elevated: true,
    })
}

#[cfg(unix)]
fn tun_helper_socket_path() -> Result<PathBuf, TunError> {
    let base = dirs::runtime_dir().ok_or_else(|| {
        TunError::Configuration("could not determine runtime directory for tun helper".to_owned())
    })?;
    Ok(base.join("failsafe").join("tun-fd.sock"))
}

#[cfg(not(unix))]
fn open_via_sudo_helper(_local_ip: Ipv4Addr) -> Result<TunHandle, TunError> {
    Err(TunError::PermissionDenied(
        "virtual LAN elevation is not supported on this platform".to_owned(),
    ))
}
