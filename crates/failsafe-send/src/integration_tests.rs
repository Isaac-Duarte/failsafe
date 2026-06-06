use std::sync::Arc;

use failsafe_clipboard::limits::ClipboardLimits;
use failsafe_core::feature::FeatureId;
use failsafe_transport::blobs::BlobTransfer;
use failsafe_core::message::FeatureMessage;
use failsafe_core::peer::PeerDirectory;
use failsafe_core::registry::FeatureRegistry;
use failsafe_transport::blobs::MockBlobTransfer;
use failsafe_transport::mock::MockTransport;
use failsafe_transport::transport::Transport;
use uuid::Uuid;

use crate::coordinator::SendCoordinator;
use crate::feature::SendFeature;
use crate::payload::{
    encode_envelope, FileEntry, SendEnvelope, SendPayload, SEND_PAYLOAD_VERSION,
};

#[tokio::test]
async fn send_receive_ack_roundtrip() {
    let (sender_transport, receiver_transport) = MockTransport::pair().await;
    let sender_transport = Arc::new(sender_transport);
    let receiver_transport = Arc::new(receiver_transport);
    let sender_id = sender_transport.local_device_id();
    let receiver_id = receiver_transport.local_device_id();

    let blob_transfer = Arc::new(MockBlobTransfer::new());
    let files = vec![("note.txt".to_owned(), b"synced".to_vec())];
    let hash = blob_transfer
        .store_files(files.clone())
        .await
        .expect("store files");

    let coordinator = SendCoordinator::new();
    let peers = Arc::new(PeerDirectory::new());
    peers.replace_peers([receiver_id]).await;
    peers
        .set_feature_enabled(receiver_id, FeatureId::FileSend, true)
        .await;

    let receiver: Arc<dyn Transport> = receiver_transport.clone();
    let mut receiver_registry = FeatureRegistry::new();
    receiver_registry
        .register(Box::new(SendFeature::new(
            blob_transfer.clone(),
            ClipboardLimits::default(),
            receiver,
            coordinator.clone(),
        )))
        .unwrap();
    receiver_registry.enable(FeatureId::FileSend).unwrap();

    let transfer_id = Uuid::new_v4();
    let payload = SendPayload {
        version: SEND_PAYLOAD_VERSION,
        transfer_id,
        sender_name: "sender".to_owned(),
        collection_hash: hash.as_str().to_owned(),
        entries: vec![FileEntry {
            name: "note.txt".to_owned(),
            size: 6,
        }],
    };

    sender_transport
        .as_ref()
        .send(FeatureMessage::new(
            sender_id,
            receiver_id,
            FeatureId::FileSend,
            encode_envelope(&SendEnvelope::Transfer(payload)),
        ))
        .await
        .expect("send transfer");

    let inbound = receiver_transport
        .as_ref()
        .recv()
        .await
        .expect("receive transfer");
    receiver_registry.dispatch(inbound).await.expect("dispatch");

    let ack = sender_transport.as_ref().recv().await.expect("receive ack");
    assert_eq!(ack.feature, FeatureId::FileSend);
    let envelope = crate::payload::decode_envelope(&ack.payload).expect("decode ack");
    assert!(matches!(
        envelope,
        SendEnvelope::Ack(crate::payload::SendAck { ok: true, .. })
    ));
}
