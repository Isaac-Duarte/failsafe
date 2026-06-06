use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub use failsafe_core::control::{ControlRequest, ControlResponse};
use failsafe_core::control::ControlError;
use failsafe_core::control::ControlStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::error::DaemonError;

pub fn control_socket_path() -> Result<PathBuf, DaemonError> {
    failsafe_core::control::control_socket_path().map_err(DaemonError::Control)
}

pub fn map_control_connect_error(error: ControlError) -> DaemonError {
    match &error {
        ControlError::Io(io_error)
            if io_error.kind() == io::ErrorKind::NotFound
                || io_error.kind() == io::ErrorKind::ConnectionRefused =>
        {
            DaemonError::Config(
                "daemon is not running; start it with `failsafe run`".to_owned(),
            )
        }
        _ => DaemonError::Control(error),
    }
}

pub async fn send_request(
    stream: &mut ControlStream,
    request: &ControlRequest,
) -> Result<(), DaemonError> {
    failsafe_core::control::send_request(stream, request)
        .await
        .map_err(DaemonError::Control)
}

pub async fn recv_response(stream: &mut ControlStream) -> Result<ControlResponse, DaemonError> {
    failsafe_core::control::recv_response(stream)
        .await
        .map_err(DaemonError::Control)
}

pub async fn send_response(
    stream: &mut ControlStream,
    response: &ControlResponse,
) -> Result<(), DaemonError> {
    failsafe_core::control::send_response(stream, response)
        .await
        .map_err(DaemonError::Control)
}

pub async fn recv_request(stream: &mut ControlStream) -> Result<ControlRequest, DaemonError> {
    failsafe_core::control::recv_request(stream)
        .await
        .map_err(DaemonError::Control)
}

pub async fn remove_stale_socket(path: &Path) -> Result<(), DaemonError> {
    failsafe_core::control::remove_stale_socket(path)
        .await
        .map_err(DaemonError::Control)
}

pub async fn relay_terminal_io(stream: &mut ControlStream) -> Result<(), DaemonError> {
    let (mut stream_read, mut stream_write) = tokio::io::split(stream);
    let (stdin_tx, mut stdin_rx) = mpsc::channel::<Vec<u8>>(32);
    let (stdout_tx, mut stdout_rx) = mpsc::channel::<Vec<u8>>(32);

    let stdin_tx_for_reader = stdin_tx.clone();

    let stdin_task = tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
        let mut stdin = std::io::stdin();
        let mut buf = [0u8; 256];
        loop {
            let read = stdin.read(&mut buf)?;
            if read == 0 {
                break;
            }
            if stdin_tx_for_reader
                .blocking_send(buf[..read].to_vec())
                .is_err()
            {
                break;
            }
        }
        Ok(())
    });

    let stdout_task = tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
        let mut stdout = std::io::stdout();
        while let Some(data) = stdout_rx.blocking_recv() {
            stdout.write_all(&data)?;
            stdout.flush()?;
        }
        Ok(())
    });

    let socket_to_stdout = async {
        let mut buf = [0u8; 4096];
        loop {
            let read = stream_read.read(&mut buf).await.map_err(DaemonError::Io)?;
            if read == 0 {
                break;
            }
            if stdout_tx.send(buf[..read].to_vec()).await.is_err() {
                break;
            }
        }
        drop(stdout_tx);
        Ok::<(), DaemonError>(())
    };

    let stdin_to_socket = async {
        while let Some(data) = stdin_rx.recv().await {
            stream_write
                .write_all(&data)
                .await
                .map_err(DaemonError::Io)?;
            stream_write.flush().await.map_err(DaemonError::Io)?;
        }
        stream_write.shutdown().await.map_err(DaemonError::Io)?;
        Ok::<(), DaemonError>(())
    };

    let relay_result = tokio::select! {
        result = socket_to_stdout => {
            drop(stdin_tx);
            result
        }
        result = stdin_to_socket => result,
    };

    relay_result?;

    stdin_task
        .await
        .map_err(|error| DaemonError::Io(std::io::Error::other(error)))??;
    stdout_task
        .await
        .map_err(|error| DaemonError::Io(std::io::Error::other(error)))??;

    Ok(())
}
