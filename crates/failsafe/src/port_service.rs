use std::sync::Arc;

use failsafe_core::control::PortProtocol;
use failsafe_transport::iroh::{IrohTransport, PortSession, relay_port_streams};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::{debug, warn};

pub async fn start_port_acceptor(iroh: Arc<IrohTransport>) -> mpsc::Receiver<PortSession> {
    let (tx, rx) = mpsc::channel(8);
    iroh.set_port_acceptor(tx).await;
    rx
}

pub async fn stop_port_acceptor(iroh: &IrohTransport) {
    iroh.clear_port_acceptor().await;
}

pub async fn handle_incoming_port(session: PortSession) {
    let device = session.from;
    let remote_port = session.remote_port;
    debug!(%device, %remote_port, "accepted port forward session");

    if session.protocol != PortProtocol::Tcp {
        warn!(%device, "unsupported port forward protocol");
        return;
    }

    let tcp = match TcpStream::connect(("127.0.0.1", remote_port)).await {
        Ok(stream) => stream,
        Err(error) => {
            warn!(%device, %remote_port, "failed to connect to local port: {error}");
            return;
        }
    };

    let (read_half, write_half) = tcp.into_split();

    if let Err(error) = relay_port_streams(session.send, session.recv, read_half, write_half).await
    {
        warn!(%device, %remote_port, "port forward relay exited with error: {error}");
    }
}
