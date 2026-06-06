use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::device::DeviceId;

#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlRequest {
    OpenShell {
        target: DeviceId,
        rows: u16,
        cols: u16,
    },
    OpenScreenShare {
        target: DeviceId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ControlResponse {
    Ready,
    Error { message: String },
}

pub fn control_socket_path() -> Result<PathBuf, ControlError> {
    let base = dirs::runtime_dir()
        .or_else(dirs::config_dir)
        .ok_or_else(|| {
            ControlError::Config("could not determine control socket directory".to_owned())
        })?;
    Ok(base.join("failsafe").join("control.sock"))
}

pub async fn write_message(stream: &mut UnixStream, message: &[u8]) -> Result<(), ControlError> {
    let len = u32::try_from(message.len())
        .map_err(|_| ControlError::Config("control message too large".to_owned()))?;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(message).await?;
    stream.flush().await?;
    Ok(())
}

pub async fn read_message(stream: &mut UnixStream) -> Result<Vec<u8>, ControlError> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 1024 * 1024 {
        return Err(ControlError::Config("control message too large".to_owned()));
    }
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).await?;
    Ok(payload)
}

pub async fn send_request(
    stream: &mut UnixStream,
    request: &ControlRequest,
) -> Result<(), ControlError> {
    let payload = serde_json::to_vec(request).map_err(|error| {
        ControlError::Config(format!("failed to encode control request: {error}"))
    })?;
    write_message(stream, &payload).await
}

pub async fn recv_response(stream: &mut UnixStream) -> Result<ControlResponse, ControlError> {
    let payload = read_message(stream).await?;
    serde_json::from_slice(&payload).map_err(|error| {
        ControlError::Config(format!("failed to decode control response: {error}"))
    })
}

pub async fn send_response(
    stream: &mut UnixStream,
    response: &ControlResponse,
) -> Result<(), ControlError> {
    let payload = serde_json::to_vec(response).map_err(|error| {
        ControlError::Config(format!("failed to encode control response: {error}"))
    })?;
    write_message(stream, &payload).await
}

pub async fn recv_request(stream: &mut UnixStream) -> Result<ControlRequest, ControlError> {
    let payload = read_message(stream).await?;
    serde_json::from_slice(&payload).map_err(|error| {
        ControlError::Config(format!("failed to decode control request: {error}"))
    })
}

pub async fn remove_stale_socket(path: &Path) -> Result<(), ControlError> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

pub async fn read_framed_payload(stream: &mut UnixStream) -> Result<Vec<u8>, ControlError> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 16 * 1024 * 1024 {
        return Err(ControlError::Config("screen frame too large".to_owned()));
    }
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).await?;
    Ok(payload)
}

pub async fn write_framed_payload(
    stream: &mut UnixStream,
    payload: &[u8],
) -> Result<(), ControlError> {
    let len = u32::try_from(payload.len())
        .map_err(|_| ControlError::Config("screen frame too large".to_owned()))?;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(payload).await?;
    stream.flush().await?;
    Ok(())
}
