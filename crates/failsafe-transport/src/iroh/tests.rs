use std::collections::HashMap;
use std::time::Duration;

use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use failsafe_core::message::FeatureMessage;
use failsafe_core::peer_address::PeerAddressBook;
use tempfile::TempDir;

use super::{IrohConfig, IrohTransport};
use crate::transport::Transport;

async fn wait_for_connection(
    transport: &IrohTransport,
    peer: DeviceId,
) -> Result<(), String> {
    for _ in 0..60 {
        if transport.connected_peers().await.contains(&peer) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(format!("timed out waiting for connection to {peer}"))
}

#[tokio::test]
async fn two_transports_exchange_messages() {
    let temp = TempDir::new().expect("tempdir");
    let device_a = DeviceId::new();
    let device_b = DeviceId::new();

    let key_a = temp.path().join("a.key");
    let key_b = temp.path().join("b.key");

    let mut addresses_a = HashMap::new();
    let mut addresses_b = HashMap::new();

    let transport_a = IrohTransport::start(IrohConfig {
        device_id: device_a,
        secret_key_path: key_a.clone(),
        address_book: PeerAddressBook::default(),
    })
    .await
    .expect("start transport a");

    addresses_b.insert(device_a, transport_a.public_key().to_string());

    let transport_b = IrohTransport::start(IrohConfig {
        device_id: device_b,
        secret_key_path: key_b,
        address_book: PeerAddressBook::from_map(addresses_b),
    })
    .await
    .expect("start transport b");

    addresses_a.insert(device_b, transport_b.public_key().to_string());

    drop(transport_a);

    let transport_a = IrohTransport::start(IrohConfig {
        device_id: device_a,
        secret_key_path: key_a,
        address_book: PeerAddressBook::from_map(addresses_a),
    })
    .await
    .expect("restart transport a with peer address");

    wait_for_connection(&transport_a, device_b)
        .await
        .expect("a connects to b");
    wait_for_connection(&transport_b, device_a)
        .await
        .expect("b connects to a");

    let message = FeatureMessage::new(
        device_a,
        device_b,
        FeatureId::Clipboard,
        b"hello over iroh",
    );

    transport_a
        .send(message.clone())
        .await
        .expect("send message");

    let received = tokio::time::timeout(Duration::from_secs(10), transport_b.recv())
        .await
        .expect("recv timeout")
        .expect("recv message");

    assert_eq!(received, message);
}
