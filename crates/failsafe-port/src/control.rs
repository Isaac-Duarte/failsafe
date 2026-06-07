use std::sync::Arc;

use async_trait::async_trait;
use failsafe_core::control::{ControlError, ControlResponse, ControlStream, PortProtocol, send_response};
use failsafe_core::device::DeviceId;
use failsafe_core::feature::{ControlContext, FeatureControl, FeatureId, FeatureSpec};
use failsafe_transport::iroh::IrohTransport;
use serde::{Deserialize, Serialize};

use crate::feature::PortFeatureSpec;
use crate::{prepare_outgoing_port_forward, run_outgoing_port_forward};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenPortForwardRequest {
    pub target: DeviceId,
    pub local_port: u16,
    pub remote_port: u16,
    pub protocol: PortProtocol,
}

pub struct PortFeatureControl {
    iroh: Arc<IrohTransport>,
}

impl PortFeatureControl {
    pub fn new(iroh: Arc<IrohTransport>) -> Self {
        Self { iroh }
    }
}

#[async_trait]
impl FeatureControl for PortFeatureControl {
    fn control_id(&self) -> FeatureId {
        PortFeatureSpec::feature_id()
    }

    async fn handle_control(
        &self,
        ctx: &ControlContext<'_>,
        mut stream: ControlStream,
        body: serde_json::Value,
    ) -> Result<(), ControlError> {
        let request: OpenPortForwardRequest = serde_json::from_value(body).map_err(|error| {
            ControlError::Config(format!("invalid port forward control request: {error}"))
        })?;

        let listener = match prepare_outgoing_port_forward(
            &self.iroh,
            ctx.peers,
            ctx.local_features,
            request.target,
            request.local_port,
            request.remote_port,
            request.protocol,
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

        let (control_read, _control_write) = tokio::io::split(stream);
        run_outgoing_port_forward(
            self.iroh.clone(),
            request.target,
            request.local_port,
            request.remote_port,
            listener,
            control_read,
        )
        .await;

        Ok(())
    }
}
