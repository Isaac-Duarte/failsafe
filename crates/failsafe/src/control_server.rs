use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use failsafe_core::peer::PeerDirectory;
use failsafe_transport::iroh::IrohTransport;
use failsafe_transport::transport::Transport;
use tokio::io::AsyncWriteExt;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::control::{
    ControlRequest, ControlResponse, control_socket_path, recv_request, remove_stale_socket,
    send_response,
};
use crate::error::DaemonError;
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
        Ok(Self {
            path: control_socket_path()?,
            iroh,
            local_features,
            peers,
        })
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
            ControlRequest::OpenShell {
                target,
                rows,
                cols,
            } => {
                self.handle_open_shell(&mut stream, target, rows, cols)
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
        if !self
            .local_features
            .read()
            .await
            .contains(&FeatureId::Shell)
        {
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
}
