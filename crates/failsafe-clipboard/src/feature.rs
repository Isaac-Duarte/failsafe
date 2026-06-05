use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use failsafe_core::feature::{Feature, FeatureError, FeatureId};
use failsafe_core::message::FeatureMessage;
use failsafe_core::outbound::{OutboundMessage, OutboundPublisher};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::io::{ClipboardIo, ClipboardIoError, SystemClipboardIo};
use crate::payload;

const POLL_INTERVAL: Duration = Duration::from_millis(300);

struct ClipboardState {
    publisher: Arc<dyn OutboundPublisher>,
    clipboard: Arc<dyn ClipboardIo>,
    last_emitted: Mutex<Option<String>>,
    applying_remote: AtomicBool,
}

/// Syncs UTF-8 clipboard text with peer devices via the runtime publisher.
pub struct ClipboardFeature {
    state: Arc<ClipboardState>,
    watch_task: Option<JoinHandle<()>>,
}

impl ClipboardFeature {
    pub fn new(publisher: Arc<dyn OutboundPublisher>) -> Self {
        Self::with_clipboard(publisher, Arc::new(SystemClipboardIo))
    }

    fn with_clipboard(
        publisher: Arc<dyn OutboundPublisher>,
        clipboard: Arc<dyn ClipboardIo>,
    ) -> Self {
        Self {
            state: Arc::new(ClipboardState {
                publisher,
                clipboard,
                last_emitted: Mutex::new(None),
                applying_remote: AtomicBool::new(false),
            }),
            watch_task: None,
        }
    }
}

#[async_trait]
impl Feature for ClipboardFeature {
    fn id(&self) -> FeatureId {
        FeatureId::Clipboard
    }

    async fn start(&mut self) -> Result<(), FeatureError> {
        if self.watch_task.is_some() {
            return Ok(());
        }

        let state = self.state.clone();
        self.watch_task = Some(tokio::spawn(async move {
            watch_clipboard(state).await;
        }));

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), FeatureError> {
        if let Some(task) = self.watch_task.take() {
            task.abort();
        }
        Ok(())
    }

    async fn handle_message(&mut self, message: FeatureMessage) -> Result<(), FeatureError> {
        let payload = payload::decode(&message.payload)?;
        let text = payload.text;

        self.state.applying_remote.store(true, Ordering::SeqCst);

        let result = self
            .state
            .clipboard
            .set_text(&text)
            .await
            .map_err(io_error_to_feature_error);

        *self.state.last_emitted.lock().await = Some(text);
        self.state
            .applying_remote
            .store(false, Ordering::SeqCst);

        result
    }
}

async fn watch_clipboard(state: Arc<ClipboardState>) {
    let mut interval = tokio::time::interval(POLL_INTERVAL);

    loop {
        interval.tick().await;

        if state.applying_remote.load(Ordering::SeqCst) {
            continue;
        }

        let text = match state.clipboard.get_text().await {
            Ok(Some(text)) => text,
            Ok(None) => continue,
            Err(error) => {
                eprintln!("clipboard read failed: {error}");
                continue;
            }
        };

        {
            let last_emitted = state.last_emitted.lock().await;
            if last_emitted.as_deref() == Some(text.as_str()) {
                continue;
            }
        }

        *state.last_emitted.lock().await = Some(text.clone());

        let outbound =
            OutboundMessage::new(FeatureId::Clipboard, payload::encode(&text));

        if let Err(error) = state.publisher.publish(outbound).await {
            eprintln!("clipboard publish failed: {error}");
        }
    }
}

fn io_error_to_feature_error(error: ClipboardIoError) -> FeatureError {
    FeatureError::Failed(FeatureId::Clipboard, error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use failsafe_core::device::DeviceId;
    use failsafe_core::feature::Feature;
    use failsafe_core::peer::PeerDirectory;
    use failsafe_transport::mock::MockTransport;
    use failsafe_transport::router::MessageRouter;
    use failsafe_transport::transport::Transport;

    use super::*;
    use crate::io::mock::MockClipboardIo;

    #[tokio::test]
    async fn handle_message_updates_clipboard() {
        let (transport, _peer) = MockTransport::pair();
        let publisher = MessageRouter::into_publisher(Arc::new(transport), Arc::new(PeerDirectory::new()));
        let clipboard = MockClipboardIo::new();

        let mut feature = ClipboardFeature::with_clipboard(publisher, clipboard.clone());

        feature
            .handle_message(FeatureMessage::new(
                DeviceId::new(),
                DeviceId::new(),
                FeatureId::Clipboard,
                payload::encode("synced"),
            ))
            .await
            .unwrap();

        assert_eq!(clipboard.current_text().await, Some("synced".to_owned()));
    }

    #[tokio::test]
    async fn watch_broadcasts_local_changes() {
        let (local_transport, peer_transport) = MockTransport::pair();
        let peer_id = peer_transport.local_device_id();

        let peers = Arc::new(PeerDirectory::new());
        peers.replace_peers([peer_id]).await;

        let publisher = MessageRouter::into_publisher(Arc::new(local_transport), peers);

        let clipboard = MockClipboardIo::new();
        clipboard.set_text("local copy").await.unwrap();

        let mut feature = ClipboardFeature::with_clipboard(publisher, clipboard);
        feature.start().await.unwrap();

        tokio::time::sleep(Duration::from_millis(400)).await;

        let received = peer_transport.recv().await.unwrap();
        let payload = payload::decode(&received.payload).unwrap();
        assert_eq!(payload.text, "local copy");

        feature.stop().await.unwrap();
    }
}
