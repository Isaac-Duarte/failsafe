use std::sync::Arc;

use failsafe_screen::{run_screen_host, write_screen_list};
use failsafe_transport::iroh::{IrohTransport, ScreenSession};
use tracing::{debug, warn};

pub async fn start_screen_acceptor(
    iroh: Arc<IrohTransport>,
) -> tokio::sync::mpsc::Receiver<ScreenSession> {
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    iroh.set_screen_acceptor(tx).await;
    rx
}

pub async fn stop_screen_acceptor(iroh: &IrohTransport) {
    iroh.clear_screen_acceptor().await;
}

pub async fn handle_incoming_screen(session: ScreenSession) {
    match session {
        ScreenSession::List(list) => {
            let device = list.from;
            debug!(%device, "accepted screen list session");
            if let Err(error) = write_screen_list(list.send).await {
                warn!(%device, "screen list session failed: {error}");
            }
        }
        ScreenSession::Stream(stream) => {
            let device = stream.from;
            debug!(%device, screen_id = stream.screen_id, "accepted screen stream session");
            if let Err(error) = run_screen_host(stream.screen_id, stream.send).await {
                warn!(%device, "screen stream session failed: {error}");
            }
        }
    }
}
