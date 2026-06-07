use std::collections::HashSet;
use std::sync::Arc;

use failsafe_core::control::PortProtocol;
use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use failsafe_core::peer::PeerDirectory;
use failsafe_transport::iroh::{IrohTransport, PortSession, relay_shell_streams};
use failsafe_transport::transport::Transport;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

#[derive(Debug, thiserror::Error)]
pub enum PortError {
    #[error("{0}")]
    Client(String),
}

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
            warn!(
                %device,
                %remote_port,
                "port forward: nothing listening on 127.0.0.1:{remote_port} on this device: {error}"
            );
            return;
        }
    };

    info!(%device, %remote_port, "port forward connected to local service");

    let (read_half, write_half) = tcp.into_split();

    if let Err(error) = relay_shell_streams(session.send, session.recv, read_half, write_half).await
    {
        warn!(%device, %remote_port, "port forward relay exited with error: {error}");
    }
}

pub async fn prepare_outgoing_port_forward(
    iroh: &IrohTransport,
    peers: &PeerDirectory,
    local_features: &HashSet<FeatureId>,
    target: DeviceId,
    local_port: u16,
    _remote_port: u16,
    protocol: PortProtocol,
) -> Result<TcpListener, PortError> {
    if !local_features.contains(&FeatureId::PortForward) {
        return Err(PortError::Client(
            "port_forward is not enabled on this device; enable it in the web UI or with `failsafe devices features`, then wait for the daemon to sync".to_owned(),
        ));
    }

    if !peers
        .is_feature_enabled(target, FeatureId::PortForward)
        .await
    {
        return Err(PortError::Client(format!(
            "port_forward is not enabled on device {target}; enable it on both devices"
        )));
    }

    if !iroh.connected_peers().await.contains(&target) {
        return Err(PortError::Client(format!(
            "device {target} is offline or unreachable"
        )));
    }

    if protocol != PortProtocol::Tcp {
        return Err(PortError::Client(
            "only tcp port forwarding is supported".to_owned(),
        ));
    }

    TcpListener::bind(("127.0.0.1", local_port))
        .await
        .map_err(|error| {
            PortError::Client(format!("failed to bind local port {local_port}: {error}"))
        })
}

pub async fn run_outgoing_port_forward(
    iroh: Arc<IrohTransport>,
    target: DeviceId,
    local_port: u16,
    remote_port: u16,
    listener: TcpListener,
    mut control_shutdown: impl AsyncRead + Unpin,
) {
    debug!(%target, %local_port, %remote_port, "port forward ready, accepting local connections");
    info!(%local_port, "port forward listening on 127.0.0.1:{local_port}");
    let mut shutdown_buf = [0u8; 1];

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                match accepted {
                    Ok((tcp, _)) => {
                        let iroh = iroh.clone();
                        tokio::spawn(async move {
                            match iroh
                                .open_port_stream(target, remote_port, PortProtocol::Tcp)
                                .await
                            {
                                Ok(session) => {
                                    let (read_half, write_half) = tcp.into_split();
                                    if let Err(error) = relay_shell_streams(
                                        session.send,
                                        session.recv,
                                        read_half,
                                        write_half,
                                    )
                                    .await
                                    {
                                        warn!(%target, %remote_port, "port forward relay ended with error: {error}");
                                    }
                                }
                                Err(error) => {
                                    warn!(%target, %remote_port, "failed to open port stream: {error}");
                                }
                            }
                        });
                    }
                    Err(error) => {
                        warn!(%local_port, "port forward accept failed: {error}");
                        break;
                    }
                }
            }
            read = control_shutdown.read(&mut shutdown_buf) => {
                match read {
                    Ok(0) | Err(_) => break,
                    Ok(_) => continue,
                }
            }
        }
    }

    debug!(%target, %local_port, "port forward stopped");
}

#[cfg(test)]
mod tests;
