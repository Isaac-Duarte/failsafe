use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use tokio::net::{TcpListener, TcpStream};

use super::super::ControlError;

pub type ControlStream = TcpStream;

pub struct ControlListener {
    listener: TcpListener,
}

impl ControlListener {
    pub async fn accept(&self) -> Result<(ControlStream, ()), ControlError> {
        self.listener
            .accept()
            .await
            .map(|(stream, _)| (stream, ()))
            .map_err(ControlError::Io)
    }
}

pub fn endpoint_path() -> Result<PathBuf, ControlError> {
    let base = dirs::config_dir().ok_or_else(|| {
        ControlError::Config("could not determine control endpoint directory".to_owned())
    })?;
    Ok(base.join("failsafe").join("control.port"))
}

pub async fn bind_control(path: &Path) -> Result<ControlListener, ControlError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    remove_stale_control_endpoint(path).await?;

    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .map_err(ControlError::Io)?;
    let port = listener
        .local_addr()
        .map_err(ControlError::Io)?
        .port();
    tokio::fs::write(path, port.to_string())
        .await
        .map_err(ControlError::Io)?;

    Ok(ControlListener { listener })
}

pub async fn connect_control(path: &Path) -> Result<ControlStream, ControlError> {
    let port = tokio::fs::read_to_string(path)
        .await
        .map_err(ControlError::Io)?
        .trim()
        .parse::<u16>()
        .map_err(|error| {
            ControlError::Config(format!("invalid control endpoint port file: {error}"))
        })?;
    TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], port)))
        .await
        .map_err(ControlError::Io)
}

pub async fn remove_stale_control_endpoint(path: &Path) -> Result<(), ControlError> {
    if path.exists() {
        tokio::fs::remove_file(path)
            .await
            .map_err(ControlError::Io)?;
    }
    Ok(())
}
