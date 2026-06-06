use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use failsafe_clipboard::limits::ClipboardLimits;
use failsafe_core::control::PortProtocol;
use failsafe_core::control::{ControlEvent, SendPhase};
use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use failsafe_core::message::FeatureMessage;
use failsafe_core::peer::PeerDirectory;
use failsafe_port::{prepare_outgoing_port_forward, run_outgoing_port_forward};
use failsafe_send::{
    encode_envelope, eprint_send, mark_send_complete, mark_send_failed, prepare_send_payload,
    send_ack_timeout, SendCoordinator, SendEnvelope, SendProgressReporter,
};
use failsafe_transport::blobs::BlobTransfer;
use failsafe_transport::iroh::IrohTransport;
use failsafe_transport::transport::Transport;
use failsafe_core::control::{bind_control, ControlListener, ControlStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::control::{
    control_socket_path, recv_request, send_response, write_event, ControlRequest,
    ControlResponse,
};
use crate::error::DaemonError;
use crate::shell_service::run_outgoing_shell;

pub struct ControlServer {
    path: PathBuf,
    iroh: Arc<IrohTransport>,
    transport: Arc<dyn Transport>,
    blob_transfer: Arc<dyn BlobTransfer>,
    device_name: String,
    send_limits: ClipboardLimits,
    coordinator: Arc<SendCoordinator>,
    local_features: Arc<RwLock<HashSet<FeatureId>>>,
    peers: Arc<PeerDirectory>,
}

impl ControlServer {
    pub fn new(
        iroh: Arc<IrohTransport>,
        transport: Arc<dyn Transport>,
        blob_transfer: Arc<dyn BlobTransfer>,
        device_name: String,
        send_limits: ClipboardLimits,
        coordinator: Arc<SendCoordinator>,
        local_features: Arc<RwLock<HashSet<FeatureId>>>,
        peers: Arc<PeerDirectory>,
    ) -> Result<Self, DaemonError> {
        Ok(Self::with_path(
            control_socket_path()?,
            iroh,
            transport,
            blob_transfer,
            device_name,
            send_limits,
            coordinator,
            local_features,
            peers,
        ))
    }

    pub(crate) fn with_path(
        path: PathBuf,
        iroh: Arc<IrohTransport>,
        transport: Arc<dyn Transport>,
        blob_transfer: Arc<dyn BlobTransfer>,
        device_name: String,
        send_limits: ClipboardLimits,
        coordinator: Arc<SendCoordinator>,
        local_features: Arc<RwLock<HashSet<FeatureId>>>,
        peers: Arc<PeerDirectory>,
    ) -> Self {
        Self {
            path,
            iroh,
            transport,
            blob_transfer,
            device_name,
            send_limits,
            coordinator,
            local_features,
            peers,
        }
    }

    pub async fn bind(&self) -> Result<ControlListener, DaemonError> {
        bind_control(&self.path).await.map_err(DaemonError::Control)
    }

    pub async fn handle_connection(&self, mut stream: ControlStream) {
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
            ControlRequest::OpenPortForward {
                target,
                local_port,
                remote_port,
                protocol,
            } => {
                self.handle_open_port_forward(&mut stream, target, local_port, remote_port, protocol)
                    .await;
            }
            ControlRequest::SendFiles {
                target,
                paths,
                transfer_id,
                resume,
            } => {
                self.handle_send_files(stream, target, paths, transfer_id, resume)
                    .await;
            }
        }
    }

    async fn handle_open_shell(
        &self,
        stream: &mut ControlStream,
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

    async fn handle_open_port_forward(
        &self,
        stream: &mut ControlStream,
        target: DeviceId,
        local_port: u16,
        remote_port: u16,
        protocol: PortProtocol,
    ) {
        let local_features = self.local_features.read().await.clone();
        let listener = match prepare_outgoing_port_forward(
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
                let _ = send_response(
                    stream,
                    &ControlResponse::Error {
                        message: error.to_string(),
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

        let (control_read, _control_write) = tokio::io::split(stream);
        run_outgoing_port_forward(
            self.iroh.clone(),
            target,
            local_port,
            remote_port,
            listener,
            control_read,
        )
        .await;
    }

    async fn handle_send_files(
        &self,
        mut stream: ControlStream,
        target: DeviceId,
        paths: Vec<PathBuf>,
        transfer_id: Uuid,
        resume: bool,
    ) {
        if !self
            .local_features
            .read()
            .await
            .contains(&FeatureId::FileSend)
        {
            let _ = send_response(
                &mut stream,
                &ControlResponse::Error {
                    message: "file_send is not enabled on this device; enable it in the web UI or with `failsafe devices features`, then wait for the daemon to sync".to_owned(),
                },
            )
            .await;
            return;
        }

        if !self
            .peers
            .is_feature_enabled(target, FeatureId::FileSend)
            .await
        {
            let _ = send_response(
                &mut stream,
                &ControlResponse::Error {
                    message: format!(
                        "file_send is not enabled on device {target}; enable it on both devices"
                    ),
                },
            )
            .await;
            return;
        }

        if !self.transport.connected_peers().await.contains(&target) {
            let _ = send_response(
                &mut stream,
                &ControlResponse::Error {
                    message: format!("device {target} is offline or unreachable"),
                },
            )
            .await;
            return;
        }

        if send_response(&mut stream, &ControlResponse::Ready)
            .await
            .is_err()
        {
            return;
        }

        let (mut read_half, mut write_half) = tokio::io::split(stream);

        let cancel = CancellationToken::new();
        let cancel_child = cancel.child_token();
        tokio::spawn(async move {
            let mut buf = [0u8; 64];
            loop {
                match read_half.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
            cancel_child.cancel();
        });

        let (progress_tx, mut progress_rx) = mpsc::channel::<ControlEvent>(256);
        let progress_writer = tokio::spawn(async move {
            while let Some(event) = progress_rx.recv().await {
                if write_event(&mut write_half, &event).await.is_err() {
                    break;
                }
            }
            write_half
        });

        let ack_rx = self.coordinator.register(transfer_id).await;
        info!(%transfer_id, %target, resume, "starting file send");
        eprint_send(format_args!(
            " starting send {transfer_id} -> {target} (resume={resume})"
        ));

        let progress = SendProgressReporter::new(progress_tx.clone());

        let mut result = async {
            let mut emit_progress: Box<dyn FnMut(SendPhase, u64, u64, Option<String>) + Send> =
                Box::new({
                    let progress = progress.clone();
                    move |phase, bytes_done, bytes_total, current_file| {
                        progress.try_emit(phase, bytes_done, bytes_total, current_file);
                    }
                });
            let payload = prepare_send_payload(
                &paths,
                target,
                self.blob_transfer.clone(),
                self.send_limits,
                self.device_name.clone(),
                transfer_id,
                resume,
                &cancel,
                &mut emit_progress,
            )
            .await?;

            if cancel.is_cancelled() {
                return Err("transfer cancelled".to_owned());
            }

            let total_bytes = payload.entries.iter().map(|entry| entry.size).sum::<u64>();

            progress
                .emit(
                    SendPhase::Sending,
                    total_bytes,
                    total_bytes,
                    None,
                )
                .await;

            if cancel.is_cancelled() {
                return Err("transfer cancelled".to_owned());
            }

            let local_id = self.transport.local_device_id();
            let envelope = SendEnvelope::Transfer(payload);
            let transfer_message = FeatureMessage::new(
                local_id,
                target,
                FeatureId::FileSend,
                encode_envelope(&envelope),
            );
            debug!(%transfer_id, "sending transfer metadata message");
            self.transport
                .send(transfer_message)
                .await
                .map_err(|error| error.to_string())?;
            debug!(%transfer_id, "transfer metadata message sent");

            progress
                .emit(
                    SendPhase::WaitingForAck,
                    0,
                    total_bytes,
                    None,
                )
                .await;
            drop(progress_tx);

            let ack_timeout = send_ack_timeout(total_bytes);
            info!(%transfer_id, %target, ?ack_timeout, "waiting for receiver acknowledgement");
            eprint_send(format_args!(
                " waiting for ack transfer_id={transfer_id} target={target}"
            ));
            tokio::select! {
                _ = cancel.cancelled() => Err("transfer cancelled".to_owned()),
                ack_result = tokio::time::timeout(ack_timeout, ack_rx) => match ack_result {
                    Ok(Ok(Ok(()))) => {
                        info!(%transfer_id, %target, "receiver acknowledged file transfer");
                        eprint_send(format_args!(" ack received for {transfer_id}"));
                        Ok(())
                    }
                    Ok(Ok(Err(message))) => {
                        warn!(%transfer_id, %target, %message, "receiver reported transfer failure");
                        Err(message)
                    }
                    Ok(Err(_)) => {
                        warn!(%transfer_id, %target, "acknowledgement channel closed before response");
                        Err("transfer acknowledgement channel closed".to_owned())
                    }
                    Err(_) => {
                        warn!(%transfer_id, %target, ?ack_timeout, "timed out waiting for receiver acknowledgement");
                        eprint_send(format_args!(" ack timeout for {transfer_id}"));
                        Err("timed out waiting for receiver acknowledgement".to_owned())
                    }
                },
            }
        }
        .await;

        if cancel.is_cancelled() {
            self.coordinator.cancel(transfer_id).await;
            if !matches!(&result, Err(message) if message == "transfer cancelled") {
                result = Err("transfer cancelled".to_owned());
            }
        }

        let mut write_half = progress_writer.await.unwrap_or_else(|_| {
            panic!("progress writer task panicked")
        });

        match result {
            Ok(()) => {
                let _ = mark_send_complete(transfer_id).await;
                let _ = write_event(
                    &mut write_half,
                    &ControlEvent::SendComplete { transfer_id },
                )
                .await;
            }
            Err(message) => {
                let _ = mark_send_failed(transfer_id, message.clone()).await;
                let _ = write_event(
                    &mut write_half,
                    &ControlEvent::SendFailed { message },
                )
                .await;
            }
        }

        let _ = write_half.shutdown().await;
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use failsafe_core::device::DeviceId;
    use failsafe_core::feature::FeatureId;
    use failsafe_core::peer::PeerDirectory;
    use failsafe_core::peer_address::PeerAddressBook;
    use failsafe_send::SendCoordinator;
    use failsafe_transport::iroh::{IrohConfig, IrohTransport};
    use failsafe_transport::transport::Transport;
    use tempfile::TempDir;
    use tokio::net::UnixStream;
    use tokio::sync::{RwLock, mpsc};

    use crate::control::{ControlRequest, ControlResponse, recv_response, send_request};
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
        let coordinator = SendCoordinator::new();
        let server = Arc::new(ControlServer::with_path(
            temp.path().join("control.sock"),
            transport_a.clone(),
            transport_a.clone(),
            transport_a.blob_transfer().clone(),
            "device-a".to_owned(),
            ClipboardLimits::default(),
            coordinator,
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

        drop(client_one);
        drop(client_two);

        accept_task.abort();
        shell_host_task.abort();
    }
}
