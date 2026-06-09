use std::net::Ipv4Addr;
use std::os::fd::IntoRawFd;
use std::process::Command;

use failsafe_core::virtual_lan::{network_address, subnet_mask};
use tracing::warn;
use tun::AbstractDevice;

use crate::tun_iface::TunError;

pub const PREFERRED_INTERFACE_NAME: &str = "failsafe0";

pub fn build_tun_config(local_ip: Ipv4Addr) -> tun::Configuration {
    let network = network_address(local_ip);
    let mask = subnet_mask();

    let mut config = tun::Configuration::default();
    config
        .tun_name(PREFERRED_INTERFACE_NAME)
        .address(local_ip)
        .netmask(mask)
        .destination(network)
        .up();
    config
}

pub fn open_sync_device(local_ip: Ipv4Addr) -> Result<(std::os::fd::RawFd, String), TunError> {
    let config = build_tun_config(local_ip);
    let device = tun::create(&config).map_err(map_tun_error)?;
    let name = device
        .tun_name()
        .map_err(|error| TunError::Configuration(error.to_string()))?;
    let network = network_address(local_ip);
    configure_routes(&name, network)?;
    Ok((device.into_raw_fd(), name))
}

pub fn configure_routes(name: &str, network: Ipv4Addr) -> Result<(), TunError> {
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
        run_cmd(
            "route",
            &["ADD", &net, "MASK", "255.255.255.0", &network.to_string()],
        )?;
        return Ok(());
    }

    warn!("unknown platform; skipping route configuration for {name}");
    Ok(())
}

pub fn remove_routes(name: &str, network: Ipv4Addr, elevated: bool) -> Result<(), TunError> {
    let net = format!("{}/24", network);

    if cfg!(target_os = "linux") {
        let args = ["route", "del", &net, "dev", name];
        if elevated {
            run_privileged("ip", &args)?;
        } else {
            let _ = run_cmd("ip", &args);
        }
    } else if cfg!(target_os = "macos") {
        let args = ["-n", "delete", "-net", &net];
        if elevated {
            run_privileged("route", &args)?;
        } else {
            let _ = run_cmd("route", &args);
        }
    } else if cfg!(target_os = "windows") {
        let _ = run_cmd("route", &["DELETE", &net]);
    }

    Ok(())
}

fn run_privileged(program: &str, args: &[&str]) -> Result<(), TunError> {
    let mut cmd = Command::new("sudo");
    cmd.arg(program).args(args);
    cmd.stdin(std::process::Stdio::inherit());
    let output = cmd
        .output()
        .map_err(|error| TunError::Configuration(format!("failed to run sudo: {error}")))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(TunError::Configuration(format!(
        "sudo {program} {} failed: {stderr}",
        args.join(" ")
    )))
}

pub fn map_tun_error(error: tun::Error) -> TunError {
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
