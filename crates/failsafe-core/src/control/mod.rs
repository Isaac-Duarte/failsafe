use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use uuid::Uuid;

use crate::screen::ScreenInfo;

mod transport;

pub use transport::{
    ControlListener, ControlStream, bind_control, connect_control, remove_stale_control_endpoint,
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
pub struct ControlEnvelope {
    pub token: String,
    pub request: ControlRequest,
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
        paths: Vec<SendPathSpec>,
        transfer_id: Uuid,
        #[serde(default)]
        resume: bool,
    },
    CancelTransfers,
    ListScreens {
        target: crate::device::DeviceId,
    },
    OpenScreenShare {
        target: crate::device::DeviceId,
        screen_id: u32,
    },
}

/// A local file or directory to send, with the archive path the receiver should see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendPathSpec {
    pub local: PathBuf,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SendPhase {
    Preparing,
    Storing,
    Sending,
    WaitingForAck,
}

/// User-facing label for a send progress phase in the CLI.
pub fn send_phase_label(phase: SendPhase, current_file: Option<&str>) -> String {
    match phase {
        SendPhase::Preparing => current_file
            .map(|name| format!("Reading {name}"))
            .unwrap_or_else(|| "Staging files locally".to_owned()),
        SendPhase::Storing => "Finalizing".to_owned(),
        SendPhase::Sending => "Starting transfer".to_owned(),
        SendPhase::WaitingForAck => "Transferring to receiver".to_owned(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlEvent {
    SendProgress {
        sequence: u64,
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
    CancelTransfers { sends: usize, receives: usize },
    ScreenList { screens: Vec<ScreenInfo> },
}

pub fn control_socket_path() -> Result<PathBuf, ControlError> {
    transport::endpoint_path()
}

pub fn control_token_path() -> Result<PathBuf, ControlError> {
    let base = dirs::config_dir().ok_or_else(|| {
        ControlError::Config("could not determine control token directory".to_owned())
    })?;
    Ok(base.join("failsafe").join("control.token"))
}

pub fn generate_control_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

pub fn write_control_token(path: &Path, token: &str) -> Result<(), ControlError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, token)?;
    restrict_file_permissions(path)?;
    Ok(())
}

pub fn read_control_token(path: &Path) -> Result<String, ControlError> {
    let token = std::fs::read_to_string(path)?;
    let token = token.trim().to_owned();
    if token.is_empty() {
        return Err(ControlError::Config(
            "control token file is empty".to_owned(),
        ));
    }
    Ok(token)
}

fn restrict_file_permissions(path: &Path) -> Result<(), ControlError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
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
    token: &str,
    request: &ControlRequest,
) -> Result<(), ControlError>
where
    S: AsyncWrite + Unpin,
{
    let envelope = ControlEnvelope {
        token: token.to_owned(),
        request: request.clone(),
    };
    let payload = serde_json::to_vec(&envelope).map_err(|error| {
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

pub async fn recv_envelope<S>(stream: &mut S) -> Result<ControlEnvelope, ControlError>
where
    S: AsyncRead + Unpin,
{
    let payload = read_message(stream).await?;
    serde_json::from_slice(&payload)
        .map_err(|error| ControlError::Config(format!("failed to decode control request: {error}")))
}

pub async fn recv_request<S>(stream: &mut S) -> Result<ControlRequest, ControlError>
where
    S: AsyncRead + Unpin,
{
    Ok(recv_envelope(stream).await?.request)
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
    serde_json::from_slice(&payload)
        .map_err(|error| ControlError::Config(format!("failed to decode control event: {error}")))
}

pub async fn remove_stale_socket(path: &Path) -> Result<(), ControlError> {
    remove_stale_control_endpoint(path).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_phase_labels_match_workflow() {
        assert_eq!(
            send_phase_label(SendPhase::Preparing, None),
            "Staging files locally"
        );
        assert_eq!(
            send_phase_label(SendPhase::Preparing, Some("doc.txt")),
            "Reading doc.txt"
        );
        assert_eq!(
            send_phase_label(SendPhase::WaitingForAck, None),
            "Transferring to receiver"
        );
    }
}
