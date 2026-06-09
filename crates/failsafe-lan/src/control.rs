use std::sync::Arc;

use async_trait::async_trait;
use failsafe_core::control::{ControlError, ControlResponse, ControlStream, send_response};
use failsafe_core::feature::{ControlContext, FeatureControl, FeatureId, FeatureSpec};
use failsafe_transport::iroh::IrohTransport;
use serde::{Deserialize, Serialize};

use crate::feature::LanFeatureSpec;
use crate::state::SharedLanState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum LanControlBody {
    Status,
}

pub struct LanFeatureControl {
    runtime: SharedLanState,
    _iroh: Arc<IrohTransport>,
}

impl LanFeatureControl {
    pub fn new(runtime: SharedLanState, iroh: Arc<IrohTransport>) -> Self {
        Self {
            runtime,
            _iroh: iroh,
        }
    }
}

#[async_trait]
impl FeatureControl for LanFeatureControl {
    fn control_id(&self) -> FeatureId {
        LanFeatureSpec::feature_id()
    }

    async fn handle_control(
        &self,
        _ctx: &ControlContext<'_>,
        mut stream: ControlStream,
        body: serde_json::Value,
    ) -> Result<(), ControlError> {
        let request: LanControlBody = serde_json::from_value(body).map_err(|error| {
            ControlError::Config(format!("invalid virtual lan control request: {error}"))
        })?;

        match request {
            LanControlBody::Status => {
                let state = self.runtime.read().await;
                send_response(
                    &mut stream,
                    &ControlResponse::LanStatus {
                        virtual_ip: state.virtual_ip.clone(),
                        subnet_cidr: state.subnet_cidr.clone(),
                        interface_up: state.interface_up,
                        message: state.message.clone(),
                    },
                )
                .await?;
            }
        }

        Ok(())
    }
}
