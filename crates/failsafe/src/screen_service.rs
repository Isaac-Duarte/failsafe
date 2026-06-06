use std::sync::Arc;

use failsafe_screen::{ProtocolError, relay_tagged_bidirectional, run_screen_host};
use failsafe_transport::iroh::{IrohTransport, ScreenSession};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use failsafe_transport::transport::TransportError;

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
        if let Err(error) = run_screen_host(session.send, session.recv).await {
            warn!(%device, "screen host exited with error: {error}");
        }
    });
}

pub async fn run_outgoing_screen_relay(
    session: ScreenSession,
    unix_stream: UnixStream,
) -> Result<(), DaemonError> {
    let (unix_read, unix_write) = unix_stream.into_split();
    relay_tagged_bidirectional(session.recv, unix_write, unix_read, session.send)
        .await
        .map_err(map_protocol_error)
}

fn map_protocol_error(error: ProtocolError) -> DaemonError {
    DaemonError::Transport(match error {
        ProtocolError::Io(error) => TransportError::Codec(error.to_string()),
        other => TransportError::Codec(other.to_string()),
    })
}
