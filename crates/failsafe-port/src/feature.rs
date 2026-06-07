use std::sync::Arc;

use async_trait::async_trait;
use failsafe_core::feature::{Feature, FeatureError, FeatureId, FeatureSpec};
use failsafe_core::message::FeatureMessage;
use failsafe_transport::iroh::IrohTransport;
use tokio::task::JoinHandle;

use crate::{handle_incoming_port, start_port_acceptor, stop_port_acceptor};

pub const ID: &str = "port_forward";

pub struct PortFeatureSpec;

impl FeatureSpec for PortFeatureSpec {
    fn id() -> &'static str {
        ID
    }

    fn label() -> &'static str {
        "Port Forward"
    }

    fn description() -> &'static str {
        "Accept forwarded TCP connections from other devices"
    }
}

pub struct PortFeature {
    iroh: Arc<IrohTransport>,
    host_task: Option<JoinHandle<()>>,
}

impl PortFeature {
    pub fn new(iroh: Arc<IrohTransport>) -> Self {
        Self {
            iroh,
            host_task: None,
        }
    }
}

#[async_trait]
impl Feature for PortFeature {
    fn id(&self) -> FeatureId {
        PortFeatureSpec::feature_id()
    }

    async fn start(&mut self) -> Result<(), FeatureError> {
        if self.host_task.is_some() {
            return Ok(());
        }

        let mut sessions = start_port_acceptor(self.iroh.clone()).await;
        self.host_task = Some(tokio::spawn(async move {
            while let Some(session) = sessions.recv().await {
                tokio::spawn(handle_incoming_port(session));
            }
        }));

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), FeatureError> {
        if let Some(task) = self.host_task.take() {
            task.abort();
        }
        stop_port_acceptor(&self.iroh).await;
        Ok(())
    }

    async fn handle_message(&mut self, _message: FeatureMessage) -> Result<(), FeatureError> {
        Ok(())
    }
}
