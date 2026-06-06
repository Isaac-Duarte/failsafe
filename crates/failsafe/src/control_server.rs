use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use failsafe_core::control::PortProtocol;
use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use failsafe_core::peer::PeerDirectory;
use failsafe_transport::iroh::{IrohTransport, relay_port_streams};
use failsafe_transport::transport::Transport;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::control::{
    ControlRequest, ControlResponse, control_socket_path, recv_request, remove_stale_socket,
    send_response,
};
use crate::error::DaemonError;
use crate::screen_service::run_outgoing_screen_relay;
use crate::shell_service::run_outgoing_shell;

pub struct ControlServer {
    path: PathBuf,
    iroh: Arc<IrohTransport>,
    local_features: Arc<RwLock<HashSet<FeatureId>>>,
    peers: Arc<PeerDirectory>,
}

impl ControlServer {
    pub fn new(
        iroh: Arc<IrohTransport>,
        local_features: Arc<RwLock<HashSet<FeatureId>>>,
        peers: Arc<PeerDirectory>,
    ) -> Result<Self, DaemonError> {
        Ok(Self::with_path(
            control_socket_path()?,
            iroh,
            local_features,
            peers,
        ))
    }

    pub(crate) fn with_path(
        path: PathBuf,
        iroh: Arc<IrohTransport>,
        local_features: Arc<RwLock<HashSet<FeatureId>>>,
        peers: Arc<PeerDirectory>,
    ) -> Self {
        Self {
            path,
            iroh,
            local_features,
            peers,
        }
    }

    pub async fn bind(&self) -> Result<UnixListener, DaemonError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(DaemonError::Io)?;
        }
        remove_stale_socket(&self.path).await?;
        UnixListener::bind(&self.path).map_err(DaemonError::Io)
    }

    pub async fn handle_connection(&self, mut stream: UnixStream) {
        let request = match recv_request(&mut stream).await {
            Ok(request) => request,
            Err(error) => {
                warn!("failed to read control request: {error}");
                let _ = send_response(
                    &mut stream,
                    &ControlResponse::Error {
                        message: error.to_string(),
                    },
                )
                .await;
                return;
            }
        };

        match request {
            ControlRequest::OpenShell { target, rows, cols } => {
                self.handle_open_shell(&mut stream, target, rows, cols)
                    .await;
            }
            ControlRequest::OpenScreenShare { target } => {
                self.handle_open_screen_share(stream, target).await;
            }
            ControlRequest::OpenPortForward {
                target,
                local_port,
                remote_port,
                protocol,
            } => {
                self.handle_open_port_forward(&mut stream, target, local_port, remote_port, protocol)
                    .await;
            }
        }
    }

    async fn handle_open_shell(
        &self,
        stream: &mut UnixStream,
        target: DeviceId,
        rows: u16,
        cols: u16,
    ) {
        if !self.local_features.read().await.contains(&FeatureId::Shell) {
            let _ = send_response(
                stream,
                &ControlResponse::Error {
                    message: "shell is not enabled on this device; enable it in the web UI or with `failsafe devices features`, then wait for the daemon to sync".to_owned(),
                },
            )
            .await;
            return;
        }

        if !self
            .peers
            .is_feature_enabled(target, FeatureId::Shell)
            .await
        {
            let _ = send_response(
                stream,
                &ControlResponse::Error {
                    message: format!(
                        "shell is not enabled on device {target}; enable it on both devices"
                    ),
                },
            )
            .await;
            return;
        }

        if !self.iroh.connected_peers().await.contains(&target) {
            let _ = send_response(
                stream,
                &ControlResponse::Error {
                    message: format!("device {target} is offline or unreachable"),
                },
            )
            .await;
            return;
        }

        let session = match self.iroh.open_shell_stream(target, rows, cols).await {
            Ok(session) => session,
            Err(error) => {
                let _ = send_response(
                    stream,
                    &ControlResponse::Error {
                        message: format!("failed to open shell: {error}"),
                    },
                )
                .await;
                return;
            }
        };

        if send_response(stream, &ControlResponse::Ready)
            .await
            .is_err()
        {
            return;
        }

        debug!(%target, "shell session ready, relaying io");

        let (mut read_half, mut write_half) = tokio::io::split(stream);
        if let Err(error) =
            run_outgoing_shell(&self.iroh, session, &mut read_half, &mut write_half).await
        {
            warn!(%target, "shell session ended with error: {error}");
        }

        let _ = write_half.shutdown().await;
    }

    async fn handle_open_screen_share(&self, mut stream: UnixStream, target: DeviceId) {
        if !self
            .local_features
            .read()
            .await
            .contains(&FeatureId::ScreenShare)
        {
            let _ = send_response(
                &mut stream,
                &ControlResponse::Error {
                    message: "screen_share is not enabled on this device; enable it in the web UI or with `failsafe devices features`, then wait for the daemon to sync".to_owned(),
                },
            )
            .await;
            return;
        }

        if !self
            .peers
            .is_feature_enabled(target, FeatureId::ScreenShare)
            .await
        {
            let _ = send_response(
                &mut stream,
                &ControlResponse::Error {
                    message: format!(
                        "screen_share is not enabled on device {target}; enable it on both devices"
                    ),
                },
            )
            .await;
            return;
        }

        if !self.iroh.connected_peers().await.contains(&target) {
            let _ = send_response(
                &mut stream,
                &ControlResponse::Error {
                    message: format!("device {target} is offline or unreachable"),
                },
            )
            .await;
            return;
        }

        let session = match self.iroh.open_screen_stream(target).await {
            Ok(session) => session,
            Err(error) => {
                let _ = send_response(
                    &mut stream,
                    &ControlResponse::Error {
                        message: format!("failed to open screen share: {error}"),
                    },
                )
                .await;
                return;
            }
        };

        if send_response(&mut stream, &ControlResponse::Ready)
            .await
            .is_err()
        {
            return;
        }

        debug!(%target, "screen share session ready, relaying frames");

        if let Err(error) = run_outgoing_screen_relay(session, stream).await {
            warn!(%target, "screen share session ended with error: {error}");
        }
    }

    async fn handle_open_port_forward(
        &self,
        stream: &mut UnixStream,
        target: DeviceId,
        local_port: u16,
        remote_port: u16,
        protocol: PortProtocol,
    ) {
        if !self
            .local_features
            .read()
            .await
            .contains(&FeatureId::PortForward)
        {
            let _ = send_response(
                stream,
                &ControlResponse::Error {
                    message: "port_forward is not enabled on this device; enable it in the web UI or with `failsafe devices features`, then wait for the daemon to sync".to_owned(),
                },
            )
            .await;
            return;
        }

        if !self
            .peers
            .is_feature_enabled(target, FeatureId::PortForward)
            .await
        {
            let _ = send_response(
                stream,
                &ControlResponse::Error {
                    message: format!(
                        "port_forward is not enabled on device {target}; enable it on both devices"
                    ),
                },
            )
            .await;
            return;
        }

        if !self.iroh.connected_peers().await.contains(&target) {
            let _ = send_response(
                stream,
                &ControlResponse::Error {
                    message: format!("device {target} is offline or unreachable"),
                },
            )
            .await;
            return;
        }

        if protocol != PortProtocol::Tcp {
            let _ = send_response(
                stream,
                &ControlResponse::Error {
                    message: "only tcp port forwarding is supported".to_owned(),
                },
            )
            .await;
            return;
        }

        let listener = match TcpListener::bind(("127.0.0.1", local_port)).await {
            Ok(listener) => listener,
            Err(error) => {
                let _ = send_response(
                    stream,
                    &ControlResponse::Error {
                        message: format!("failed to bind local port {local_port}: {error}"),
                    },
                )
                .await;
                return;
            }
        };

        if send_response(stream, &ControlResponse::Ready)
            .await
            .is_err()
        {
            return;
        }

        debug!(%target, %local_port, %remote_port, "port forward ready, accepting local connections");
        info!(%local_port, "port forward listening on 127.0.0.1:{local_port}");

        let (mut control_read, _control_write) = tokio::io::split(stream);
        let iroh = self.iroh.clone();
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
                                        if let Err(error) = relay_port_streams(
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
                read = control_read.read(&mut shutdown_buf) => {
                    match read {
                        Ok(0) | Err(_) => break,
                        Ok(_) => continue,
                    }
                }
            }
        }

        debug!(%target, %local_port, "port forward stopped");
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use failsafe_core::control::PortProtocol;
    use failsafe_core::device::DeviceId;
    use failsafe_core::feature::FeatureId;
    use failsafe_core::peer::PeerDirectory;
    use failsafe_core::peer_address::PeerAddressBook;
    use failsafe_transport::iroh::{IrohConfig, IrohTransport};
    use failsafe_transport::transport::Transport;
    use tempfile::TempDir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream, UnixStream};
    use tokio::sync::{RwLock, mpsc};

    use crate::control::{ControlRequest, ControlResponse, recv_response, send_request};
    use crate::port_service::handle_incoming_port;
    use crate::shell_service::handle_incoming_shell;

    use super::*;

    async fn wait_for_connection(transport: &IrohTransport, peer: DeviceId) {
        for _ in 0..60 {
            if transport.connected_peers().await.contains(&peer) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        panic!("timed out waiting for connection to {peer}");
    }

    async fn open_shell_client(
        socket_path: &PathBuf,
        target: DeviceId,
    ) -> Result<UnixStream, DaemonError> {
        let mut stream = UnixStream::connect(socket_path)
            .await
            .map_err(DaemonError::Io)?;
        send_request(
            &mut stream,
            &ControlRequest::OpenShell {
                target,
                rows: 24,
                cols: 80,
            },
        )
        .await?;
        match recv_response(&mut stream).await? {
            ControlResponse::Ready => Ok(stream),
            ControlResponse::Error { message } => Err(DaemonError::Config(message)),
        }
    }

    #[tokio::test]
    async fn accepts_concurrent_shell_connections_to_same_peer() {
        let temp = TempDir::new().expect("tempdir");
        let device_a = DeviceId::new();
        let device_b = DeviceId::new();

        let transport_a = Arc::new(
            IrohTransport::start(IrohConfig {
                device_id: device_a,
                secret_key_path: temp.path().join("control-a.key"),
                blob_store_path: temp.path().join("control-blobs-a"),
                address_book: PeerAddressBook::default(),
            })
            .await
            .expect("start transport a"),
        );

        let mut addresses_b = HashMap::new();
        addresses_b.insert(device_a, transport_a.public_key().to_string());

        let transport_b = IrohTransport::start(IrohConfig {
            device_id: device_b,
            secret_key_path: temp.path().join("control-b.key"),
            blob_store_path: temp.path().join("control-blobs-b"),
            address_book: PeerAddressBook::from_map(addresses_b),
        })
        .await
        .expect("start transport b");

        let (acceptor_tx, mut acceptor_rx) = mpsc::channel(2);
        transport_b.set_shell_acceptor(acceptor_tx).await;

        let mut addresses_a = HashMap::new();
        addresses_a.insert(device_b, transport_b.public_key().to_string());
        transport_a
            .update_peers(PeerAddressBook::from_map(addresses_a))
            .expect("update peer addresses on a");

        wait_for_connection(&transport_a, device_b).await;
        wait_for_connection(&transport_b, device_a).await;

        let peers = Arc::new(PeerDirectory::new());
        peers.replace_peers([device_b]).await;
        peers
            .set_feature_enabled(device_b, FeatureId::Shell, true)
            .await;

        let local_features = Arc::new(RwLock::new(HashSet::from([FeatureId::Shell])));
        let server = Arc::new(ControlServer::with_path(
            temp.path().join("control.sock"),
            transport_a.clone(),
            local_features,
            peers,
        ));
        let listener = server.bind().await.expect("bind control socket");

        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept control connection");
                let server = Arc::clone(&server);
                tokio::spawn(async move {
                    server.handle_connection(stream).await;
                });
            }
        });

        let shell_host_task = tokio::spawn(async move {
            while let Some(session) = acceptor_rx.recv().await {
                tokio::spawn(handle_incoming_shell(session));
            }
        });

        let socket_path = temp.path().join("control.sock");
        let (client_one, client_two) = tokio::join!(
            open_shell_client(&socket_path, device_b),
            open_shell_client(&socket_path, device_b),
        );
        let client_one = client_one.expect("first shell client ready");
        let client_two = client_two.expect("second shell client ready");

        // Both sessions stay open concurrently; drop to end the relays.
        drop(client_one);
        drop(client_two);

        accept_task.abort();
        shell_host_task.abort();
    }

    async fn open_port_forward_client(
        socket_path: &PathBuf,
        target: DeviceId,
        local_port: u16,
        remote_port: u16,
    ) -> Result<UnixStream, DaemonError> {
        let mut stream = UnixStream::connect(socket_path)
            .await
            .map_err(DaemonError::Io)?;
        send_request(
            &mut stream,
            &ControlRequest::OpenPortForward {
                target,
                local_port,
                remote_port,
                protocol: PortProtocol::Tcp,
            },
        )
        .await?;
        match recv_response(&mut stream).await? {
            ControlResponse::Ready => Ok(stream),
            ControlResponse::Error { message } => Err(DaemonError::Config(message)),
        }
    }

    #[tokio::test]
    async fn forwards_local_tcp_connections_to_remote_port() {
        let temp = TempDir::new().expect("tempdir");
        let device_a = DeviceId::new();
        let device_b = DeviceId::new();

        let echo = TcpListener::bind("127.0.0.1:0").await.expect("bind echo");
        let echo_port = echo.local_addr().expect("echo addr").port();
        let echo_task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = echo.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 256];
                    loop {
                        let Ok(read) = stream.read(&mut buf).await else {
                            break;
                        };
                        if read == 0 {
                            break;
                        }
                        if stream.write_all(&buf[..read]).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });

        let transport_a = Arc::new(
            IrohTransport::start(IrohConfig {
                device_id: device_a,
                secret_key_path: temp.path().join("port-a.key"),
                blob_store_path: temp.path().join("port-blobs-a"),
                address_book: PeerAddressBook::default(),
            })
            .await
            .expect("start transport a"),
        );

        let mut addresses_b = HashMap::new();
        addresses_b.insert(device_a, transport_a.public_key().to_string());

        let transport_b = IrohTransport::start(IrohConfig {
            device_id: device_b,
            secret_key_path: temp.path().join("port-b.key"),
            blob_store_path: temp.path().join("port-blobs-b"),
            address_book: PeerAddressBook::from_map(addresses_b),
        })
        .await
        .expect("start transport b");

        let (port_acceptor_tx, mut port_acceptor_rx) = mpsc::channel(2);
        transport_b.set_port_acceptor(port_acceptor_tx).await;

        let mut addresses_a = HashMap::new();
        addresses_a.insert(device_b, transport_b.public_key().to_string());
        transport_a
            .update_peers(PeerAddressBook::from_map(addresses_a))
            .expect("update peer addresses on a");

        wait_for_connection(&transport_a, device_b).await;
        wait_for_connection(&transport_b, device_a).await;

        let peers = Arc::new(PeerDirectory::new());
        peers.replace_peers([device_b]).await;
        peers
            .set_feature_enabled(device_b, FeatureId::PortForward, true)
            .await;

        let local_features = Arc::new(RwLock::new(HashSet::from([FeatureId::PortForward])));
        let server = Arc::new(ControlServer::with_path(
            temp.path().join("port-control.sock"),
            transport_a.clone(),
            local_features,
            peers,
        ));
        let listener = server.bind().await.expect("bind control socket");

        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept control connection");
                let server = Arc::clone(&server);
                tokio::spawn(async move {
                    server.handle_connection(stream).await;
                });
            }
        });

        let port_host_task = tokio::spawn(async move {
            while let Some(session) = port_acceptor_rx.recv().await {
                tokio::spawn(handle_incoming_port(session));
            }
        });

        let local_forward_port = {
            let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind temp");
            listener.local_addr().expect("temp addr").port()
        };

        let socket_path = temp.path().join("port-control.sock");
        let control_stream = open_port_forward_client(
            &socket_path,
            device_b,
            local_forward_port,
            echo_port,
        )
        .await
        .expect("port forward client ready");

        let send_through_forward = async {
            let mut client = TcpStream::connect(("127.0.0.1", local_forward_port))
                .await
                .expect("connect to local forward");
            client
                .write_all(b"hello-forward")
                .await
                .expect("write to forward");
            let mut buf = [0u8; 32];
            let read = client.read(&mut buf).await.expect("read echo");
            assert_eq!(&buf[..read], b"hello-forward");
        };

        tokio::time::timeout(Duration::from_secs(10), send_through_forward)
            .await
            .expect("forward timed out");

        drop(control_stream);
        accept_task.abort();
        port_host_task.abort();
        echo_task.abort();
    }
}
