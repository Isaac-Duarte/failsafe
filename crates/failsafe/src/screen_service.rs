use std::sync::Arc;

use failsafe_screen::run_screen_host;
use failsafe_transport::iroh::{
    IrohTransport, ScreenSession, relay_screen_inbound,
};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::error::DaemonError;

pub async fn start_screen_acceptor(iroh: Arc<IrohTransport>) -> mpsc::Receiver<ScreenSession> {
    let (tx, rx) = mpsc::channel(8);
    iroh.set_screen_acceptor(tx).await;
    rx
}

pub async fn stop_screen_acceptor(iroh: &IrohTransport) {
    iroh.clear_screen_acceptor().await;
}

pub async fn handle_incoming_screen(session: ScreenSession) {
    let device = session.from;
    debug!(%device, "accepted screen share session");

    tokio::spawn(async move {
        if let Err(error) = run_screen_host(session.send).await {
            warn!(%device, "screen host exited with error: {error}");
        }
    });
}

pub async fn run_outgoing_screen_relay(
    session: ScreenSession,
    output: impl tokio::io::AsyncWrite + Unpin,
) -> Result<(), DaemonError> {
    relay_screen_inbound(session.recv, output)
        .await
        .map_err(DaemonError::Transport)
}
