use std::sync::Arc;

use async_trait::async_trait;
use failsafe_core::feature::{Feature, FeatureError, FeatureId, FeatureSpec};
use failsafe_core::message::FeatureMessage;
use failsafe_transport::iroh::IrohTransport;
use tokio::task::JoinHandle;

use crate::relay::{
    spawn_acceptor_loops, start_desktop_acceptor, start_input_acceptor, stop_desktop_acceptor,
    stop_input_acceptor,
};

pub const ID: &str = "desktop";

pub struct DesktopFeatureSpec;

impl FeatureSpec for DesktopFeatureSpec {
    fn id() -> &'static str {
        ID
    }

    fn label() -> &'static str {
        "Desktop"
    }

    fn description() -> &'static str {
        "Accept remote desktop view and control sessions. Works alongside clipboard sync when both features are enabled."
    }
}

pub struct DesktopFeature {
    iroh: Arc<IrohTransport>,
    host_task: Option<JoinHandle<()>>,
}

impl DesktopFeature {
    pub fn new(iroh: Arc<IrohTransport>) -> Self {
        Self {
            iroh,
            host_task: None,
        }
    }
}

#[async_trait]
impl Feature for DesktopFeature {
    fn id(&self) -> FeatureId {
        DesktopFeatureSpec::feature_id()
    }

    async fn start(&mut self) -> Result<(), FeatureError> {
        if self.host_task.is_some() {
            return Ok(());
        }

        let desktop_sessions = start_desktop_acceptor(self.iroh.clone()).await;
        let input_sessions = start_input_acceptor(self.iroh.clone()).await;
        self.host_task = Some(spawn_acceptor_loops(
            self.iroh.clone(),
            desktop_sessions,
            input_sessions,
        ));

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), FeatureError> {
        if let Some(task) = self.host_task.take() {
            task.abort();
        }
        stop_desktop_acceptor(&self.iroh).await;
        stop_input_acceptor(&self.iroh).await;
        Ok(())
    }

    async fn handle_message(&mut self, _message: FeatureMessage) -> Result<(), FeatureError> {
        Ok(())
    }
}
