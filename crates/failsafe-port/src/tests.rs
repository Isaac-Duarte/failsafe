use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use failsafe_core::control::{
    ControlRequest, ControlResponse, PortProtocol, recv_request, recv_response, send_request,
    send_response,
};
use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use failsafe_core::peer::PeerDirectory;
use failsafe_core::peer_address::PeerAddressBook;
use failsafe_transport::iroh::{IrohConfig, IrohTransport};
use failsafe_transport::transport::Transport;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UnixStream};
use tokio::sync::RwLock;

use crate::{handle_incoming_port, start_port_acceptor};

struct ControlServer {
    path: PathBuf,
    iroh: Arc<IrohTransport>,
    local_features: Arc<RwLock<HashSet<FeatureId>>>,
    peers: Arc<PeerDirectory>,
}

impl ControlServer {
    fn new(
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

    async fn bind(&self) -> tokio::net::UnixListener {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        if self.path.exists() {
            let _ = std::fs::remove_file(&self.path);
        }
        tokio::net::UnixListener::bind(&self.path).expect("bind control socket")
    }

    async fn handle_connection(&self, mut stream: UnixStream) {
        let request = recv_request(&mut stream)
            .await
            .expect("read control request");

        match request {
            ControlRequest::OpenPortForward {
                target,
                local_port,
                remote_port,
                protocol,
            } => {
                let local_features = self.local_features.read().await.clone();
                let listener = match crate::prepare_outgoing_port_forward(
                    &self.iroh,
                    &self.peers,
                    &local_features,
                    target,
                    local_port,
                    remote_port,
                    protocol,
                )
                .await
                {
                    Ok(listener) => listener,
                    Err(error) => {
                        send_response(
                            &mut stream,
                            &ControlResponse::Error {
                                message: error.to_string(),
                            },
                        )
                        .await
                        .expect("send error");
                        return;
                    }
                };

                send_response(&mut stream, &ControlResponse::Ready)
                    .await
                    .expect("send ready");

                let (control_read, _) = tokio::io::split(stream);
                crate::run_outgoing_port_forward(
                    self.iroh.clone(),
                    target,
                    local_port,
                    remote_port,
                    listener,
                    control_read,
                )
                .await;
            }
            ControlRequest::OpenShell { .. } => {
                panic!("unexpected shell request in port test");
            }
            ControlRequest::SendFiles { .. } => {
                panic!("unexpected send request in port test");
            }
            ControlRequest::CancelTransfers => {
                panic!("unexpected cancel transfers request in port test");
            }
        }
    }
}

async fn wait_for_connection(transport: &IrohTransport, peer: DeviceId) {
    for _ in 0..60 {
        if transport.connected_peers().await.contains(&peer) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    panic!("timed out waiting for connection to {peer}");
}

async fn open_port_forward_client(
    socket_path: &PathBuf,
    target: DeviceId,
    local_port: u16,
    remote_port: u16,
) -> UnixStream {
    let mut stream = UnixStream::connect(socket_path)
        .await
        .expect("connect control socket");
    send_request(
        &mut stream,
        &ControlRequest::OpenPortForward {
            target,
            local_port,
            remote_port,
            protocol: PortProtocol::Tcp,
        },
    )
    .await
    .expect("send request");
    match recv_response(&mut stream).await.expect("recv response") {
        ControlResponse::Ready => stream,
        ControlResponse::Error { message } => panic!("port forward rejected: {message}"),
        ControlResponse::CancelTransfers { .. } => {
            panic!("unexpected cancel transfers response in port test")
        }
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

    let transport_b = Arc::new(
        IrohTransport::start(IrohConfig {
            device_id: device_b,
            secret_key_path: temp.path().join("port-b.key"),
            blob_store_path: temp.path().join("port-blobs-b"),
            address_book: PeerAddressBook::from_map(addresses_b),
        })
        .await
        .expect("start transport b"),
    );

    let mut port_acceptor_rx = start_port_acceptor(transport_b.clone()).await;

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
    let server = Arc::new(ControlServer::new(
        temp.path().join("port-control.sock"),
        transport_a.clone(),
        local_features,
        peers,
    ));
    let listener = server.bind().await;

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
    .await;

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
