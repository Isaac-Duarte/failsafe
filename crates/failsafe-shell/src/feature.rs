use std::sync::Arc;

use async_trait::async_trait;
use failsafe_core::feature::{Feature, FeatureError, FeatureId, FeatureSpec};
use failsafe_core::message::FeatureMessage;
use failsafe_transport::iroh::{IrohTransport, ShellSession};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::relay::{handle_incoming_shell, start_shell_acceptor, stop_shell_acceptor};

pub const ID: &str = "shell";

pub struct ShellFeatureSpec;

impl FeatureSpec for ShellFeatureSpec {
    fn id() -> &'static str {
        ID
    }

    fn label() -> &'static str {
        "Shell"
    }

    fn description() -> &'static str {
        "Accept remote terminal sessions from other devices"
    }
}

pub struct ShellFeature {
    iroh: Arc<IrohTransport>,
    sessions: Option<mpsc::Receiver<ShellSession>>,
    host_task: Option<JoinHandle<()>>,
}

impl ShellFeature {
    pub fn new(iroh: Arc<IrohTransport>) -> Self {
        Self {
            iroh,
            sessions: None,
            host_task: None,
        }
    }
}

#[async_trait]
impl Feature for ShellFeature {
    fn id(&self) -> FeatureId {
        ShellFeatureSpec::feature_id()
    }

    async fn start(&mut self) -> Result<(), FeatureError> {
        if self.sessions.is_some() {
            return Ok(());
        }

        let mut sessions = start_shell_acceptor(self.iroh.clone()).await;
        self.host_task = Some(tokio::spawn(async move {
            while let Some(session) = sessions.recv().await {
                tokio::spawn(handle_incoming_shell(session));
            }
        }));

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), FeatureError> {
        if let Some(task) = self.host_task.take() {
            task.abort();
        }
        stop_shell_acceptor(&self.iroh).await;
        self.sessions = None;
        Ok(())
    }

    async fn handle_message(&mut self, _message: FeatureMessage) -> Result<(), FeatureError> {
        Ok(())
    }
}
