use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use failsafe_clipboard::limits::ClipboardLimits;
use failsafe_core::control::{
    ControlListener, ControlStream, bind_control, control_token_path, generate_control_token,
    recv_envelope, send_response, write_control_token,
};
use failsafe_core::control::{ControlRequest, ControlResponse};
use failsafe_core::feature::{ControlContext, FeatureControl, FeatureId};
use failsafe_core::peer::PeerDirectory;
use failsafe_send::SendCoordinator;
use failsafe_transport::blobs::BlobTransfer;
use failsafe_transport::iroh::IrohTransport;
use failsafe_transport::transport::Transport;
use failsafe_feature_registry::{ControlBuildContext, build_control_handlers};
use failsafe_lan::SharedLanState;
use tokio::sync::RwLock;
use tracing::warn;

use crate::control::control_socket_path;
use crate::error::DaemonError;

pub struct ControlServer {
    path: PathBuf,
    token: String,
    handlers: Vec<Box<dyn FeatureControl>>,
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
        lan_runtime: SharedLanState,
    ) -> Result<Self, DaemonError> {
        let token = generate_control_token();
        Ok(Self::with_path(
            control_socket_path()?,
            token,
            iroh,
            transport,
            blob_transfer,
            device_name,
            send_limits,
            coordinator,
            local_features,
            peers,
            lan_runtime,
        ))
    }

    pub(crate) fn with_path(
        path: PathBuf,
        token: String,
        iroh: Arc<IrohTransport>,
        transport: Arc<dyn Transport>,
        blob_transfer: Arc<dyn BlobTransfer>,
        device_name: String,
        send_limits: ClipboardLimits,
        coordinator: Arc<SendCoordinator>,
        local_features: Arc<RwLock<HashSet<FeatureId>>>,
        peers: Arc<PeerDirectory>,
        lan_runtime: SharedLanState,
    ) -> Self {
        let handlers = build_control_handlers(&ControlBuildContext {
            iroh,
            transport,
            blob_transfer,
            device_name,
            send_limits,
            coordinator,
            local_features: local_features.clone(),
            peers: peers.clone(),
            lan_runtime,
        });

        Self {
            path,
            token,
            handlers,
            local_features,
            peers,
        }
    }

    pub async fn bind(&self) -> Result<ControlListener, DaemonError> {
        let token_path = control_token_path().map_err(DaemonError::Control)?;
        write_control_token(&token_path, &self.token).map_err(DaemonError::Control)?;
        bind_control(&self.path).await.map_err(DaemonError::Control)
    }

    pub async fn handle_connection(&self, mut stream: ControlStream) {
        let request = match recv_envelope(&mut stream).await {
            Ok(envelope) if envelope.token == self.token => envelope.request,
            Ok(_) => {
                warn!("rejected control connection with invalid token");
                let _ = send_response(
                    &mut stream,
                    &ControlResponse::Error {
                        message: "unauthorized".to_owned(),
                    },
                )
                .await;
                return;
            }
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

        self.dispatch_request(stream, request).await;
    }

    async fn dispatch_request(&self, mut stream: ControlStream, request: ControlRequest) {
        let local_features = self.local_features.read().await.clone();
        let ctx = ControlContext {
            peers: &self.peers,
            local_features: &local_features,
        };

        for handler in &self.handlers {
            if handler.control_id() != request.feature {
                continue;
            }

            if let Err(error) = handler.handle_control(&ctx, stream, request.body).await {
                warn!("control handler failed: {error}");
            }
            return;
        }

        let _ = send_response(
            &mut stream,
            &ControlResponse::Error {
                message: format!("unknown feature `{}`", request.feature),
            },
        )
        .await;
    }
}
