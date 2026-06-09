use std::sync::Arc;

use async_trait::async_trait;
use failsafe_core::control::{ControlError, ControlResponse, ControlStream, send_response};
use failsafe_core::device::DeviceId;
use failsafe_core::feature::{ControlContext, FeatureControl, FeatureId, FeatureSpec};
use failsafe_transport::iroh::IrohTransport;
use failsafe_transport::transport::Transport;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::feature::DesktopFeatureSpec;
use crate::relay::run_outgoing_desktop;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenDesktopRequest {
    pub target: DeviceId,
    /// When true, only view the remote screen (no mouse/keyboard control).
    #[serde(default)]
    pub view_only: bool,
    /// Zero-based display index on the remote machine.
    #[serde(default)]
    pub display_index: u32,
}

pub struct DesktopFeatureControl {
    iroh: Arc<IrohTransport>,
}

impl DesktopFeatureControl {
    pub fn new(iroh: Arc<IrohTransport>) -> Self {
        Self { iroh }
    }
}

#[async_trait]
impl FeatureControl for DesktopFeatureControl {
    fn control_id(&self) -> FeatureId {
        DesktopFeatureSpec::feature_id()
    }

    async fn handle_control(
        &self,
        ctx: &ControlContext<'_>,
        mut stream: ControlStream,
        body: serde_json::Value,
    ) -> Result<(), ControlError> {
        let request: OpenDesktopRequest = serde_json::from_value(body).map_err(|error| {
            ControlError::Config(format!("invalid desktop control request: {error}"))
        })?;

        let feature_id = DesktopFeatureSpec::feature_id();
        if !ctx.local_features.contains(&feature_id) {
            send_response(
                &mut stream,
                &ControlResponse::Error {
                    message: "desktop is not enabled on this device; enable it in the web UI or with `failsafe devices features`, then wait for the daemon to sync".to_owned(),
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
                        "desktop is not enabled on device {}; enable it on both devices",
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
            .open_desktop_stream(request.target, request.view_only, request.display_index)
            .await
        {
            Ok(session) => session,
            Err(error) => {
                send_response(
                    &mut stream,
                    &ControlResponse::Error {
                        message: format!("failed to open desktop session: {error}"),
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

        debug!(
            target = %request.target,
            view_only = request.view_only,
            display = request.display_index,
            "desktop session ready, opening viewer"
        );

        if let Err(error) = run_outgoing_desktop(&self.iroh, session).await {
            warn!(target = %request.target, "desktop session ended with error: {error}");
        }

        Ok(())
    }
}
