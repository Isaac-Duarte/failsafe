//! Privileged helper entry point: open TUN and pass fd to the daemon.

use std::net::Ipv4Addr;
use std::os::unix::net::UnixStream;
use std::path::Path;

use failsafe_core::virtual_lan::parse_virtual_ip;
use tracing::info;

use crate::tun_fd::send_tun_fd;
use crate::tun_setup::open_sync_device;

#[derive(Debug, thiserror::Error)]
pub enum TunHelperError {
    #[error("{0}")]
    Message(String),
}

pub fn run_tun_helper(socket_path: &Path, local_ip: &str) -> Result<(), TunHelperError> {
    let ip = parse_virtual_ip(local_ip).ok_or_else(|| {
        TunHelperError::Message(format!("invalid virtual IP `{local_ip}`"))
    })?;

    let (fd, name) = open_sync_device(ip).map_err(|error| TunHelperError::Message(error.to_string()))?;

    let stream = UnixStream::connect(socket_path).map_err(|error| {
        TunHelperError::Message(format!(
            "failed to connect to daemon socket {}: {error}",
            socket_path.display()
        ))
    })?;

    send_tun_fd(&stream, &name, fd).map_err(|error| TunHelperError::Message(error.to_string()))?;

    info!(%name, %ip, "passed virtual LAN interface to daemon");
    Ok(())
}

pub fn parse_helper_ip(value: &str) -> Result<Ipv4Addr, TunHelperError> {
    parse_virtual_ip(value).ok_or_else(|| {
        TunHelperError::Message(format!("invalid virtual IP `{value}`"))
    })
}
