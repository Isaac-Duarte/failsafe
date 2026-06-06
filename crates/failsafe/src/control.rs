use std::path::{Path, PathBuf};

use failsafe_core::device::DeviceId;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::error::DaemonError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlRequest {
    OpenShell {
        target: DeviceId,
        rows: u16,
        cols: u16,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ControlResponse {
    Ready,
    Error { message: String },
}

pub fn control_socket_path() -> Result<PathBuf, DaemonError> {
    let base = dirs::runtime_dir()
        .or_else(dirs::config_dir)
        .ok_or_else(|| DaemonError::Config("could not determine control socket directory".to_owned()))?;
    Ok(base.join("failsafe").join("control.sock"))
}

pub async fn write_message(stream: &mut UnixStream, message: &[u8]) -> Result<(), DaemonError> {
    let len = u32::try_from(message.len())
        .map_err(|_| DaemonError::Config("control message too large".to_owned()))?;
    stream
        .write_all(&len.to_be_bytes())
        .await
        .map_err(DaemonError::Io)?;
    stream
        .write_all(message)
        .await
        .map_err(DaemonError::Io)?;
    stream.flush().await.map_err(DaemonError::Io)
}

pub async fn read_message(stream: &mut UnixStream) -> Result<Vec<u8>, DaemonError> {
    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .await
        .map_err(DaemonError::Io)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 1024 * 1024 {
        return Err(DaemonError::Config("control message too large".to_owned()));
    }
    let mut payload = vec![0u8; len];
    stream
        .read_exact(&mut payload)
        .await
        .map_err(DaemonError::Io)?;
    Ok(payload)
}

pub async fn send_request(stream: &mut UnixStream, request: &ControlRequest) -> Result<(), DaemonError> {
    let payload = serde_json::to_vec(request)
        .map_err(|error| DaemonError::Config(format!("failed to encode control request: {error}")))?;
    write_message(stream, &payload).await
}

pub async fn recv_response(stream: &mut UnixStream) -> Result<ControlResponse, DaemonError> {
    let payload = read_message(stream).await?;
    serde_json::from_slice(&payload)
        .map_err(|error| DaemonError::Config(format!("failed to decode control response: {error}")))
}

pub async fn send_response(
    stream: &mut UnixStream,
    response: &ControlResponse,
) -> Result<(), DaemonError> {
    let payload = serde_json::to_vec(response)
        .map_err(|error| DaemonError::Config(format!("failed to encode control response: {error}")))?;
    write_message(stream, &payload).await
}

pub async fn recv_request(stream: &mut UnixStream) -> Result<ControlRequest, DaemonError> {
    let payload = read_message(stream).await?;
    serde_json::from_slice(&payload)
        .map_err(|error| DaemonError::Config(format!("failed to decode control request: {error}")))
}

pub async fn remove_stale_socket(path: &Path) -> Result<(), DaemonError> {
    if path.exists() {
        std::fs::remove_file(path).map_err(DaemonError::Io)?;
    }
    Ok(())
}

pub async fn relay_terminal_io(stream: &mut UnixStream) -> Result<(), DaemonError> {
    let (mut stream_read, mut stream_write) = tokio::io::split(stream);
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    let stream_to_local = async {
        let mut buf = [0u8; 4096];
        loop {
            let read = stream_read.read(&mut buf).await.map_err(DaemonError::Io)?;
            if read == 0 {
                break;
            }
            stdout
                .write_all(&buf[..read])
                .await
                .map_err(DaemonError::Io)?;
        }
        Ok::<(), DaemonError>(())
    };

    let local_to_stream = async {
        let mut buf = [0u8; 4096];
        loop {
            let read = stdin.read(&mut buf).await.map_err(DaemonError::Io)?;
            if read == 0 {
                break;
            }
            stream_write
                .write_all(&buf[..read])
                .await
                .map_err(DaemonError::Io)?;
        }
        stream_write.flush().await.map_err(DaemonError::Io)?;
        Ok::<(), DaemonError>(())
    };

    tokio::try_join!(stream_to_local, local_to_stream)?;
    Ok(())
}
