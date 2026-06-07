use std::sync::Arc;

use async_trait::async_trait;
use failsafe_core::control::{ControlError, ControlResponse, ControlStream, send_response};
use failsafe_core::device::DeviceId;
use failsafe_core::feature::{ControlContext, FeatureControl, FeatureId, FeatureSpec};
use failsafe_transport::transport::Transport;
use failsafe_transport::iroh::IrohTransport;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tracing::{debug, warn};

use crate::feature::ShellFeatureSpec;
use crate::relay::run_outgoing_shell;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenShellRequest {
    pub target: DeviceId,
    pub rows: u16,
    pub cols: u16,
}

pub struct ShellFeatureControl {
    iroh: Arc<IrohTransport>,
}

impl ShellFeatureControl {
    pub fn new(iroh: Arc<IrohTransport>) -> Self {
        Self { iroh }
    }
}

#[async_trait]
impl FeatureControl for ShellFeatureControl {
    fn control_id(&self) -> FeatureId {
        ShellFeatureSpec::feature_id()
    }

    async fn handle_control(
        &self,
        ctx: &ControlContext<'_>,
        mut stream: ControlStream,
        body: serde_json::Value,
    ) -> Result<(), ControlError> {
        let request: OpenShellRequest = serde_json::from_value(body).map_err(|error| {
            ControlError::Config(format!("invalid shell control request: {error}"))
        })?;

        let feature_id = ShellFeatureSpec::feature_id();
        if !ctx.local_features.contains(&feature_id) {
            send_response(
                &mut stream,
                &ControlResponse::Error {
                    message: "shell is not enabled on this device; enable it in the web UI or with `failsafe devices features`, then wait for the daemon to sync".to_owned(),
                },
            )
            .await?;
            return Ok(());
        }

        if !ctx
            .peers
            .is_feature_enabled(request.target, feature_id.clone())
            .await
        {
            send_response(
                &mut stream,
                &ControlResponse::Error {
                    message: format!(
                        "shell is not enabled on device {}; enable it on both devices",
                        request.target
                    ),
                },
            )
            .await?;
            return Ok(());
        }

        if !Transport::connected_peers(self.iroh.as_ref())
            .await
            .contains(&request.target)
        {
            send_response(
                &mut stream,
                &ControlResponse::Error {
                    message: format!("device {} is offline or unreachable", request.target),
                },
            )
            .await?;
            return Ok(());
        }

        let session = match self
            .iroh
            .open_shell_stream(request.target, request.rows, request.cols)
            .await
        {
            Ok(session) => session,
            Err(error) => {
                send_response(
                    &mut stream,
                    &ControlResponse::Error {
                        message: format!("failed to open shell: {error}"),
                    },
                )
                .await?;
                return Ok(());
            }
        };

        if send_response(&mut stream, &ControlResponse::Ready)
            .await
            .is_err()
        {
            return Ok(());
        }

        debug!(target = %request.target, "shell session ready, relaying io");

        let (mut read_half, mut write_half) = tokio::io::split(stream);
        if let Err(error) =
            run_outgoing_shell(&self.iroh, session, &mut read_half, &mut write_half).await
        {
            warn!(target = %request.target, "shell session ended with error: {error}");
        }

        let _ = write_half.shutdown().await;
        Ok(())
    }
}
