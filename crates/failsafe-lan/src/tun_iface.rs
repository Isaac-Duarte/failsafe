use std::io;
use std::net::Ipv4Addr;
use std::process::Command;
use std::sync::Arc;

use failsafe_core::virtual_lan::{network_address, subnet_mask};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

const INTERFACE_NAME: &str = "failsafe0";

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
    local_ip: Ipv4Addr,
    network: Ipv4Addr,
}

impl TunHandle {
    pub fn open(local_ip: Ipv4Addr) -> Result<Self, TunError> {
        let network = network_address(local_ip);
        let mask = subnet_mask();

        let mut config = tun::Configuration::default();
        config
            .name(INTERFACE_NAME)
            .address(local_ip)
            .netmask(mask)
            .destination(network)
            .up();

        #[cfg(target_os = "linux")]
        config.platform_config(|platform| {
            platform.packet_information(false);
        });

        let device = tun::create_as_async(&config).map_err(map_tun_error)?;

        let name = device
            .get_ref()
            .name()
            .map_err(|error| TunError::Configuration(error.to_string()))?
            .to_owned();

        configure_routes(&name, network)?;

        info!(%local_ip, interface = %name, "virtual LAN interface ready");

        Ok(Self {
            device: Arc::new(Mutex::new(device)),
            name,
            local_ip,
            network,
        })
    }

    pub fn device(&self) -> Arc<Mutex<tun::AsyncDevice>> {
        self.device.clone()
    }

    pub fn local_ip(&self) -> Ipv4Addr {
        self.local_ip
    }

    pub fn subnet_cidr(&self) -> String {
        format!("{}/24", self.network)
    }

    pub fn interface_up(&self) -> bool {
        true
    }

    pub fn last_error(&self) -> Option<String> {
        None
    }
}

impl Drop for TunHandle {
    fn drop(&mut self) {
        if let Err(error) = remove_routes(&self.name, self.network) {
            warn!(interface = %self.name, "failed to remove virtual LAN routes: {error}");
        } else {
            debug!(interface = %self.name, "virtual LAN routes removed");
        }
    }
}

fn map_tun_error(error: tun::Error) -> TunError {
    let message = error.to_string();
    if message.contains("Permission denied")
        || message.contains("denied")
        || message.contains("Access is denied")
        || message.contains("Operation not permitted")
    {
        TunError::PermissionDenied(message)
    } else {
        TunError::Configuration(message)
    }
}

fn configure_routes(name: &str, network: Ipv4Addr) -> Result<(), TunError> {
    let net = format!("{}/24", network);

    if cfg!(target_os = "linux") {
        run_cmd(
            "ip",
            &["route", "replace", &net, "dev", name, "scope", "link"],
        )?;
        return Ok(());
    }

    if cfg!(target_os = "macos") {
        run_cmd("route", &["-n", "add", "-net", &net, "-interface", name])?;
        return Ok(());
    }

    if cfg!(target_os = "windows") {
        run_cmd("route", &["ADD", &net, "MASK", "255.255.255.0", &network.to_string()])?;
        return Ok(());
    }

    warn!("unknown platform; skipping route configuration for {name}");
    Ok(())
}

fn remove_routes(name: &str, network: Ipv4Addr) -> Result<(), TunError> {
    let net = format!("{}/24", network);

    if cfg!(target_os = "linux") {
        let _ = run_cmd("ip", &["route", "del", &net, "dev", name]);
    } else if cfg!(target_os = "macos") {
        let _ = run_cmd("route", &["-n", "delete", "-net", &net]);
    } else if cfg!(target_os = "windows") {
        let _ = run_cmd("route", &["DELETE", &net]);
    }

    Ok(())
}

fn run_cmd(program: &str, args: &[&str]) -> Result<(), TunError> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| TunError::Configuration(format!("failed to run {program}: {error}")))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(TunError::Configuration(format!(
        "{program} {} failed: {stderr}",
        args.join(" ")
    )))
}
