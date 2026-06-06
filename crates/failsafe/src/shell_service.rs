use std::sync::Arc;

use failsafe_shell::run_shell_host;
use failsafe_transport::iroh::{
    IrohTransport, ShellSession, relay_shell_streams, relay_shell_to_channels,
};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::error::DaemonError;

pub async fn start_shell_acceptor(iroh: Arc<IrohTransport>) -> mpsc::Receiver<ShellSession> {
    let (tx, rx) = mpsc::channel(8);
    iroh.set_shell_acceptor(tx).await;
    rx
}

pub async fn stop_shell_acceptor(iroh: &IrohTransport) {
    iroh.clear_shell_acceptor().await;
}

pub async fn handle_incoming_shell(session: ShellSession) {
    let device = session.from;
    debug!(%device, "accepted shell session");

    let (to_pty_tx, to_pty_rx) = mpsc::channel::<Vec<u8>>(64);
    let (from_pty_tx, from_pty_rx) = mpsc::channel::<Vec<u8>>(64);

    let host = tokio::spawn(async move {
        if let Err(error) = run_shell_host(session.rows, session.cols, to_pty_rx, from_pty_tx).await
        {
            warn!(%device, "shell host exited with error: {error}");
        }
    });

    let relay = tokio::spawn(async move {
        if let Err(error) =
            relay_shell_to_channels(session.send, session.recv, from_pty_rx, to_pty_tx).await
        {
            warn!(%device, "shell relay exited with error: {error}");
        }
    });

    let _ = tokio::join!(host, relay);
}

pub async fn run_outgoing_shell(
    _iroh: &IrohTransport,
    session: ShellSession,
    input: impl tokio::io::AsyncRead + Unpin,
    output: impl tokio::io::AsyncWrite + Unpin,
) -> Result<(), DaemonError> {
    relay_shell_streams(session.send, session.recv, input, output)
        .await
        .map_err(DaemonError::Transport)
}
