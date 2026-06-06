use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use failsafe_clipboard::limits::ClipboardLimits;
use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use failsafe_core::message::FeatureMessage;
use failsafe_core::peer::PeerDirectory;
use failsafe_core::registry::FeatureRegistry;
use failsafe_send::{
    decode_envelope, encode_envelope, send_ack_timeout, FileEntry, SendCoordinator, SendEnvelope,
    SendFeature, SendPayload, SEND_PAYLOAD_VERSION,
};
use failsafe_transport::iroh::{IrohConfig, IrohTransport};
use failsafe_transport::transport::Transport;
use failsafe_core::peer_address::PeerAddressBook;
use tempfile::TempDir;
use uuid::Uuid;

async fn wait_for_connection(transport: &IrohTransport, peer: DeviceId) {
    for _ in 0..60 {
        if transport.connected_peers().await.contains(&peer) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    panic!("timed out waiting for connection to {peer}");
}

#[tokio::test]
async fn iroh_send_receive_ack_roundtrip() {
    let temp = TempDir::new().expect("tempdir");
    let device_sender = DeviceId::new();
    let device_receiver = DeviceId::new();

    let transport_sender = IrohTransport::start(IrohConfig {
        device_id: device_sender,
        secret_key_path: temp.path().join("sender.key"),
        blob_store_path: temp.path().join("sender-blobs"),
        address_book: PeerAddressBook::default(),
    })
    .await
    .expect("start sender transport");

    let mut addresses_receiver = HashMap::new();
    addresses_receiver.insert(device_sender, transport_sender.public_key().to_string());

    let transport_receiver = IrohTransport::start(IrohConfig {
        device_id: device_receiver,
        secret_key_path: temp.path().join("receiver.key"),
        blob_store_path: temp.path().join("receiver-blobs"),
        address_book: PeerAddressBook::from_map(addresses_receiver),
    })
    .await
    .expect("start receiver transport");

    let mut addresses_sender = HashMap::new();
    addresses_sender.insert(device_receiver, transport_receiver.public_key().to_string());
    transport_sender
        .update_peers(PeerAddressBook::from_map(addresses_sender))
        .expect("update sender peer addresses");

    wait_for_connection(&transport_sender, device_receiver).await;
    wait_for_connection(&transport_receiver, device_sender).await;

    let file_path = temp.path().join("note.txt");
    std::fs::write(&file_path, b"synced over iroh").expect("write fixture");

    let blob_sender = transport_sender.blob_transfer();
    let (hash, imported) = blob_sender
        .import_sources(
            &[("note.txt".to_owned(), PathBuf::from(file_path))],
            &mut |_| {},
        )
        .await
        .expect("import on sender");

    assert_eq!(imported.len(), 1);
    let file_size = imported[0].size;

    let coordinator = SendCoordinator::new();
    let peers = Arc::new(PeerDirectory::new());
    peers.replace_peers([device_receiver]).await;
    peers
        .set_feature_enabled(device_receiver, FeatureId::FileSend, true)
        .await;

    let blob_receiver = transport_receiver.blob_transfer();
    let receiver_transport: Arc<dyn Transport> = Arc::new(transport_receiver);
    let mut receiver_registry = FeatureRegistry::new();
    receiver_registry
        .register(Box::new(SendFeature::new(
            blob_receiver,
            ClipboardLimits::unlimited(),
            receiver_transport.clone(),
            coordinator.clone(),
        )))
        .expect("register receiver feature");
    receiver_registry
        .enable_and_start(FeatureId::FileSend)
        .await
        .expect("start receiver feature");

    let transfer_id = Uuid::new_v4();
    let payload = SendPayload {
        version: SEND_PAYLOAD_VERSION,
        transfer_id,
        sender_name: "sender".to_owned(),
        collection_hash: hash.as_str().to_owned(),
        entries: vec![FileEntry {
            name: "note.txt".to_owned(),
            size: file_size,
        }],
    };

    let sender_transport: Arc<dyn Transport> = Arc::new(transport_sender);
    sender_transport
        .send(FeatureMessage::new(
            device_sender,
            device_receiver,
            FeatureId::FileSend,
            encode_envelope(&SendEnvelope::Transfer(payload)),
        ))
        .await
        .expect("send transfer metadata");

    let inbound = receiver_transport.recv().await.expect("receive transfer");
    receiver_registry
        .dispatch(inbound)
        .await
        .expect("dispatch transfer");

    let mut ack_message = None;
    for _ in 0..120 {
        if let Ok(Some(message)) = sender_transport.try_recv().await {
            ack_message = Some(message);
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let ack_message = ack_message.expect("timed out waiting for transfer acknowledgement");
    assert_eq!(ack_message.feature, FeatureId::FileSend);
    let envelope = decode_envelope(&ack_message.payload).expect("decode ack envelope");
    assert!(matches!(
        envelope,
        SendEnvelope::Ack(failsafe_send::SendAck { ok: true, .. })
    ));

    // Large transfers should get a proportionally longer acknowledgement window.
    assert!(send_ack_timeout(1024 * 1024 * 1024) > send_ack_timeout(0));
}
