mod image;
mod inbound;
mod outbound;
mod watcher;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use failsafe_core::feature::{Feature, FeatureError, FeatureId, FeatureSpec};
use failsafe_core::message::FeatureMessage;
use failsafe_core::outbound::OutboundPublisher;
use failsafe_transport::blobs::{BlobTransfer, MockBlobTransfer};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::io::{ClipboardIo, ClipboardIoError, SystemClipboardIo};
use crate::limits::ClipboardLimits;
use crate::payload;

pub(super) struct ClipboardState {
    pub(super) publisher: Arc<dyn OutboundPublisher>,
    pub(super) clipboard: Arc<dyn ClipboardIo>,
    pub(super) blob_transfer: Arc<dyn BlobTransfer>,
    pub(super) limits: ClipboardLimits,
    pub(super) last_emitted: Mutex<Option<String>>,
    pub(super) last_failed: Mutex<Option<String>>,
    pub(super) applying_remote: AtomicBool,
}

pub const ID: &str = "clipboard";

pub struct ClipboardFeatureSpec;

impl FeatureSpec for ClipboardFeatureSpec {
    fn id() -> &'static str {
        ID
    }

    fn label() -> &'static str {
        "Clipboard"
    }

    fn description() -> &'static str {
        "Sync clipboard content across devices"
    }
}

/// Syncs clipboard content with peer devices via the runtime publisher.
pub struct ClipboardFeature {
    state: Arc<ClipboardState>,
    watch_task: Option<JoinHandle<()>>,
}

impl ClipboardFeature {
    pub fn new(
        publisher: Arc<dyn OutboundPublisher>,
        blob_transfer: Option<Arc<dyn BlobTransfer>>,
    ) -> Self {
        Self::new_with_limits(publisher, blob_transfer, ClipboardLimits::default())
    }

    pub fn new_with_limits(
        publisher: Arc<dyn OutboundPublisher>,
        blob_transfer: Option<Arc<dyn BlobTransfer>>,
        limits: ClipboardLimits,
    ) -> Self {
        Self::with_dependencies(
            publisher,
            Arc::new(SystemClipboardIo),
            blob_transfer.unwrap_or_else(|| Arc::new(MockBlobTransfer::new())),
            limits,
        )
    }

    pub(crate) fn with_dependencies(
        publisher: Arc<dyn OutboundPublisher>,
        clipboard: Arc<dyn ClipboardIo>,
        blob_transfer: Arc<dyn BlobTransfer>,
        limits: ClipboardLimits,
    ) -> Self {
        Self {
            state: Arc::new(ClipboardState {
                publisher,
                clipboard,
                blob_transfer,
                limits,
                last_emitted: Mutex::new(None),
                last_failed: Mutex::new(None),
                applying_remote: AtomicBool::new(false),
            }),
            watch_task: None,
        }
    }
}

#[async_trait]
impl Feature for ClipboardFeature {
    fn id(&self) -> FeatureId {
        ClipboardFeatureSpec::feature_id()
    }

    async fn start(&mut self) -> Result<(), FeatureError> {
        if self.watch_task.is_some() {
            return Ok(());
        }

        let state = self.state.clone();
        self.watch_task = Some(tokio::spawn(async move {
            watcher::watch_clipboard(state).await;
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
        let content = inbound::resolve_payload_to_content(
            &payload,
            message.from,
            self.state.blob_transfer.clone(),
            self.state.limits,
        )
        .await
        .map_err(|error| FeatureError::Failed(ClipboardFeatureSpec::feature_id(), error))?;

        self.state.applying_remote.store(true, Ordering::SeqCst);

        let result = self
            .state
            .clipboard
            .write(&content)
            .await
            .map_err(io_error_to_feature_error);

        *self.state.last_emitted.lock().await = Some(payload::fingerprint(&payload));
        self.state.applying_remote.store(false, Ordering::SeqCst);

        result
    }
}

fn io_error_to_feature_error(error: ClipboardIoError) -> FeatureError {
    FeatureError::Failed(ClipboardFeatureSpec::feature_id(), error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use failsafe_core::device::DeviceId;
    use failsafe_core::feature::Feature;
    use failsafe_core::message::FeatureMessage;
    use failsafe_core::peer::PeerDirectory;
    use failsafe_transport::blobs::MockBlobTransfer;
    use failsafe_transport::mock::MockTransport;
    use failsafe_transport::router::MessageRouter;
    use failsafe_transport::transport::Transport;

    use super::*;
    use crate::io::ClipboardContent;
    use crate::io::mock::MockClipboardIo;
    use crate::payload::{ClipboardContent as PayloadContent, ClipboardPayload};

    #[tokio::test]
    async fn handle_message_updates_clipboard() {
        let (transport, _peer) = MockTransport::pair().await;
        let publisher =
            MessageRouter::into_publisher(Arc::new(transport), Arc::new(PeerDirectory::new()));
        let clipboard = MockClipboardIo::new();

        let mut feature = ClipboardFeature::with_dependencies(
            publisher,
            clipboard.clone(),
            Arc::new(MockBlobTransfer::new()),
            ClipboardLimits::default(),
        );

        let payload = ClipboardPayload {
            version: payload::CLIPBOARD_PAYLOAD_VERSION,
            content: PayloadContent::Text {
                text: "synced".to_owned(),
            },
        };

        feature
            .handle_message(FeatureMessage::new(
                DeviceId::new(),
                DeviceId::new(),
                ClipboardFeatureSpec::feature_id(),
                payload::encode(&payload),
            ))
            .await
            .unwrap();

        assert_eq!(
            clipboard.current().await,
            Some(ClipboardContent::Text("synced".to_owned()))
        );
    }

    #[tokio::test]
    async fn watch_broadcasts_local_changes() {
        let (local_transport, peer_transport) = MockTransport::pair().await;
        let peer_id = peer_transport.local_device_id();

        let peers = Arc::new(PeerDirectory::new());
        peers.replace_peers([peer_id]).await;

        let publisher = MessageRouter::into_publisher(Arc::new(local_transport), peers);

        let clipboard = MockClipboardIo::new();
        clipboard
            .write(&ClipboardContent::Text("local copy".to_owned()))
            .await
            .unwrap();

        let mut feature = ClipboardFeature::with_dependencies(
            publisher,
            clipboard,
            Arc::new(MockBlobTransfer::new()),
            ClipboardLimits::default(),
        );
        feature.start().await.unwrap();

        tokio::time::sleep(Duration::from_millis(400)).await;

        let received = peer_transport.recv().await.unwrap();
        let payload = payload::decode(&received.payload).unwrap();
        assert_eq!(
            payload.content,
            PayloadContent::Text {
                text: "local copy".to_owned()
            }
        );

        feature.stop().await.unwrap();
    }
}
