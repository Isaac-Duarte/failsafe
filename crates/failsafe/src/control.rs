use std::io;
use std::path::{Path, PathBuf};

use failsafe_core::control::ControlError;
use failsafe_core::control::ControlStream;
pub use failsafe_core::control::{
    ControlEvent, ControlRequest, ControlResponse, SendPhase, control_token_path,
    read_control_token, read_event, send_phase_label, write_event,
};
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
            DaemonError::Config("daemon is not running; start it with `failsafe run`".to_owned())
        }
        _ => DaemonError::Control(error),
    }
}

pub fn load_control_token() -> Result<String, DaemonError> {
    let path = control_token_path().map_err(DaemonError::Control)?;
    if !path.exists() {
        return Err(DaemonError::Config(
            "daemon is not running; start it with `failsafe run`".to_owned(),
        ));
    }
    read_control_token(&path).map_err(DaemonError::Control)
}

pub async fn send_request(
    stream: &mut ControlStream,
    request: &ControlRequest,
) -> Result<(), DaemonError> {
    let token = load_control_token()?;
    failsafe_core::control::send_request(stream, &token, request)
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
    let (stream_read, stream_write) = tokio::io::split(stream);
    failsafe_core::io::relay_bidirectional(
        stream_read,
        stream_write,
        4096,
        tokio::io::stdin(),
        tokio::io::stdout(),
        256,
        DaemonError::Io,
        true,
    )
    .await
}
