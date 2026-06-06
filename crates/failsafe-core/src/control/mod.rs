use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

mod transport;

pub use transport::{
    bind_control, connect_control, remove_stale_control_endpoint, ControlListener, ControlStream,
};

#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortProtocol {
    Tcp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlRequest {
    OpenShell {
        target: crate::device::DeviceId,
        rows: u16,
        cols: u16,
    },
    OpenPortForward {
        target: crate::device::DeviceId,
        local_port: u16,
        remote_port: u16,
        protocol: PortProtocol,
    },
    SendFiles {
        target: crate::device::DeviceId,
        paths: Vec<PathBuf>,
        transfer_id: Uuid,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SendPhase {
    Preparing,
    Storing,
    Sending,
    WaitingForAck,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlEvent {
    SendProgress {
        phase: SendPhase,
        bytes_done: u64,
        bytes_total: u64,
        current_file: Option<String>,
    },
    SendComplete {
        transfer_id: Uuid,
    },
    SendFailed {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ControlResponse {
    Ready,
    Error { message: String },
}

pub fn control_socket_path() -> Result<PathBuf, ControlError> {
    transport::endpoint_path()
}

pub async fn write_message<S>(stream: &mut S, message: &[u8]) -> Result<(), ControlError>
where
    S: AsyncWrite + Unpin,
{
    let len = u32::try_from(message.len())
        .map_err(|_| ControlError::Config("control message too large".to_owned()))?;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(message).await?;
    stream.flush().await?;
    Ok(())
}

pub async fn read_message<S>(stream: &mut S) -> Result<Vec<u8>, ControlError>
where
    S: AsyncRead + Unpin,
{
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

pub async fn send_request<S>(
    stream: &mut S,
    request: &ControlRequest,
) -> Result<(), ControlError>
where
    S: AsyncWrite + Unpin,
{
    let payload = serde_json::to_vec(request).map_err(|error| {
        ControlError::Config(format!("failed to encode control request: {error}"))
    })?;
    write_message(stream, &payload).await
}

pub async fn recv_response<S>(stream: &mut S) -> Result<ControlResponse, ControlError>
where
    S: AsyncRead + Unpin,
{
    let payload = read_message(stream).await?;
    serde_json::from_slice(&payload).map_err(|error| {
        ControlError::Config(format!("failed to decode control response: {error}"))
    })
}

pub async fn send_response<S>(
    stream: &mut S,
    response: &ControlResponse,
) -> Result<(), ControlError>
where
    S: AsyncWrite + Unpin,
{
    let payload = serde_json::to_vec(response).map_err(|error| {
        ControlError::Config(format!("failed to encode control response: {error}"))
    })?;
    write_message(stream, &payload).await
}

pub async fn recv_request<S>(stream: &mut S) -> Result<ControlRequest, ControlError>
where
    S: AsyncRead + Unpin,
{
    let payload = read_message(stream).await?;
    serde_json::from_slice(&payload)
        .map_err(|error| ControlError::Config(format!("failed to decode control request: {error}")))
}

pub async fn write_event<S>(stream: &mut S, event: &ControlEvent) -> Result<(), ControlError>
where
    S: AsyncWrite + Unpin,
{
    let payload = serde_json::to_vec(event).map_err(|error| {
        ControlError::Config(format!("failed to encode control event: {error}"))
    })?;
    write_message(stream, &payload).await
}

pub async fn read_event<S>(stream: &mut S) -> Result<ControlEvent, ControlError>
where
    S: AsyncRead + Unpin,
{
    let payload = read_message(stream).await?;
    serde_json::from_slice(&payload).map_err(|error| {
        ControlError::Config(format!("failed to decode control event: {error}"))
    })
}

pub async fn remove_stale_socket(path: &Path) -> Result<(), ControlError> {
    remove_stale_control_endpoint(path).await
}
