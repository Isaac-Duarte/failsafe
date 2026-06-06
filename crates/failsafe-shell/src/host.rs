use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::relay::relay_channels_to_pty;

#[derive(Debug, Error)]
pub enum ShellError {
    #[error("failed to open pty: {0}")]
    Pty(String),

    #[error("failed to spawn shell: {0}")]
    Spawn(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned())
}

/// Spawn a login shell in a PTY and relay bytes through the provided channels.
pub async fn run_shell_host(
    rows: u16,
    cols: u16,
    from_stream: mpsc::Receiver<Vec<u8>>,
    to_stream: mpsc::Sender<Vec<u8>>,
) -> Result<(), ShellError> {
    let pty_system = NativePtySystem::default();
    let size = PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    };

    let pair = pty_system
        .openpty(size)
        .map_err(|error| ShellError::Pty(error.to_string()))?;

    let shell = default_shell();
    let mut command = CommandBuilder::new(&shell);
    command.env("TERM", std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_owned()));
    if shell.ends_with("bash") || shell.ends_with("zsh") {
        command.arg("-i");
    }

    let _child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| ShellError::Spawn(error.to_string()))?;

    drop(pair.slave);

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| ShellError::Pty(error.to_string()))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| ShellError::Pty(error.to_string()))?;

    debug!(%shell, %rows, %cols, "shell host started");

    let relay_result =
        relay_channels_to_pty(from_stream, to_stream, Box::new(reader), Box::new(writer)).await;

    if let Err(error) = pair.master.resize(size) {
        warn!("failed to restore pty size on shutdown: {error}");
    }

    relay_result.map_err(ShellError::Io)
}
