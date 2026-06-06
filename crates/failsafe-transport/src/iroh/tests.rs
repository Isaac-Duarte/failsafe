use std::collections::HashMap;
use std::time::Duration;

use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use failsafe_core::message::FeatureMessage;
use failsafe_core::peer_address::PeerAddressBook;
use tempfile::TempDir;

use tokio::sync::mpsc;

use super::{IrohConfig, IrohTransport};
use crate::iroh::stream::read_exact;
use crate::transport::Transport;

async fn wait_for_connection(transport: &IrohTransport, peer: DeviceId) -> Result<(), String> {
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
        blob_store_path: temp.path().join("blobs-a"),
        address_book: PeerAddressBook::default(),
    })
    .await
    .expect("start transport a");

    addresses_b.insert(device_a, transport_a.public_key().to_string());

    let transport_b = IrohTransport::start(IrohConfig {
        device_id: device_b,
        secret_key_path: key_b,
        blob_store_path: temp.path().join("blobs-b"),
        address_book: PeerAddressBook::from_map(addresses_b),
    })
    .await
    .expect("start transport b");

    addresses_a.insert(device_b, transport_b.public_key().to_string());

    drop(transport_a);

    let transport_a = IrohTransport::start(IrohConfig {
        device_id: device_a,
        secret_key_path: key_a,
        blob_store_path: temp.path().join("blobs-a-restart"),
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

    let message = FeatureMessage::new(device_a, device_b, FeatureId::Clipboard, b"hello over iroh");

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

#[tokio::test]
async fn update_peers_connects_to_new_peer() {
    let temp = TempDir::new().expect("tempdir");
    let device_a = DeviceId::new();
    let device_b = DeviceId::new();

    let transport_a = IrohTransport::start(IrohConfig {
        device_id: device_a,
        secret_key_path: temp.path().join("a.key"),
        blob_store_path: temp.path().join("blobs-a-update"),
        address_book: PeerAddressBook::default(),
    })
    .await
    .expect("start transport a");

    let transport_b = IrohTransport::start(IrohConfig {
        device_id: device_b,
        secret_key_path: temp.path().join("b.key"),
        blob_store_path: temp.path().join("blobs-b-update"),
        address_book: PeerAddressBook::default(),
    })
    .await
    .expect("start transport b");

    let mut addresses_a = HashMap::new();
    addresses_a.insert(device_b, transport_b.public_key().to_string());
    transport_a
        .update_peers(PeerAddressBook::from_map(addresses_a))
        .expect("update peer addresses on a");

    let mut addresses_b = HashMap::new();
    addresses_b.insert(device_a, transport_a.public_key().to_string());
    transport_b
        .update_peers(PeerAddressBook::from_map(addresses_b))
        .expect("update peer addresses on b");

    wait_for_connection(&transport_a, device_b)
        .await
        .expect("a connects to b after peer update");
    wait_for_connection(&transport_b, device_a)
        .await
        .expect("b connects to a after peer update");
}

#[tokio::test]
async fn blob_transfer_roundtrips_bytes() {
    let temp = TempDir::new().expect("tempdir");
    let device_a = DeviceId::new();
    let device_b = DeviceId::new();

    let transport_a = IrohTransport::start(IrohConfig {
        device_id: device_a,
        secret_key_path: temp.path().join("blob-a.key"),
        blob_store_path: temp.path().join("blob-store-a"),
        address_book: PeerAddressBook::default(),
    })
    .await
    .expect("start transport a");

    let mut addresses_b = HashMap::new();
    addresses_b.insert(device_a, transport_a.public_key().to_string());

    let transport_b = IrohTransport::start(IrohConfig {
        device_id: device_b,
        secret_key_path: temp.path().join("blob-b.key"),
        blob_store_path: temp.path().join("blob-store-b"),
        address_book: PeerAddressBook::from_map(addresses_b),
    })
    .await
    .expect("start transport b");

    let mut addresses_a = HashMap::new();
    addresses_a.insert(device_b, transport_b.public_key().to_string());
    transport_a
        .update_peers(PeerAddressBook::from_map(addresses_a))
        .expect("update peer addresses on a");

    wait_for_connection(&transport_a, device_b)
        .await
        .expect("a connects to b");
    wait_for_connection(&transport_b, device_a)
        .await
        .expect("b connects to a");

    let blob_a = transport_a.blob_transfer();
    let blob_b = transport_b.blob_transfer();

    let hash = blob_a
        .store_bytes(b"clipboard image bytes".to_vec())
        .await
        .expect("store blob on a");

    let fetched = blob_b
        .fetch_bytes(device_a, &hash)
        .await
        .expect("fetch blob on b");

    assert_eq!(fetched, b"clipboard image bytes");
}

#[tokio::test]
async fn blob_transfer_roundtrips_file_collection() {
    let temp = TempDir::new().expect("tempdir");
    let device_a = DeviceId::new();
    let device_b = DeviceId::new();

    let transport_a = IrohTransport::start(IrohConfig {
        device_id: device_a,
        secret_key_path: temp.path().join("files-a.key"),
        blob_store_path: temp.path().join("files-store-a"),
        address_book: PeerAddressBook::default(),
    })
    .await
    .expect("start transport a");

    let mut addresses_b = HashMap::new();
    addresses_b.insert(device_a, transport_a.public_key().to_string());

    let transport_b = IrohTransport::start(IrohConfig {
        device_id: device_b,
        secret_key_path: temp.path().join("files-b.key"),
        blob_store_path: temp.path().join("files-store-b"),
        address_book: PeerAddressBook::from_map(addresses_b),
    })
    .await
    .expect("start transport b");

    let mut addresses_a = HashMap::new();
    addresses_a.insert(device_b, transport_b.public_key().to_string());
    transport_a
        .update_peers(PeerAddressBook::from_map(addresses_a))
        .expect("update peer addresses on a");

    wait_for_connection(&transport_a, device_b)
        .await
        .expect("a connects to b");
    wait_for_connection(&transport_b, device_a)
        .await
        .expect("b connects to a");

    let blob_a = transport_a.blob_transfer();
    let blob_b = transport_b.blob_transfer();

    let files = vec![
        ("notes.txt".to_owned(), b"hello files".to_vec()),
        ("data.bin".to_owned(), vec![1, 2, 3, 4]),
    ];
    let root = blob_a
        .store_files(files.clone())
        .await
        .expect("store collection on a");

    let fetched = blob_b
        .fetch_collection_files(device_a, &root)
        .await
        .expect("fetch collection on b");

    assert_eq!(fetched, files);
}

#[tokio::test]
async fn shell_stream_opens_between_peers() {
    let temp = TempDir::new().expect("tempdir");
    let device_a = DeviceId::new();
    let device_b = DeviceId::new();

    let transport_a = IrohTransport::start(IrohConfig {
        device_id: device_a,
        secret_key_path: temp.path().join("shell-a.key"),
        blob_store_path: temp.path().join("shell-blobs-a"),
        address_book: PeerAddressBook::default(),
    })
    .await
    .expect("start transport a");

    let mut addresses_b = HashMap::new();
    addresses_b.insert(device_a, transport_a.public_key().to_string());

    let transport_b = IrohTransport::start(IrohConfig {
        device_id: device_b,
        secret_key_path: temp.path().join("shell-b.key"),
        blob_store_path: temp.path().join("shell-blobs-b"),
        address_book: PeerAddressBook::from_map(addresses_b),
    })
    .await
    .expect("start transport b");

    let (acceptor_tx, mut acceptor_rx) = mpsc::channel(1);
    transport_b.set_shell_acceptor(acceptor_tx).await;

    let mut addresses_a = HashMap::new();
    addresses_a.insert(device_b, transport_b.public_key().to_string());
    transport_a
        .update_peers(PeerAddressBook::from_map(addresses_a))
        .expect("update peer addresses on a");

    wait_for_connection(&transport_a, device_b)
        .await
        .expect("a connects to b");
    wait_for_connection(&transport_b, device_a)
        .await
        .expect("b connects to a");

    let mut client_session = transport_a
        .open_shell_stream(device_b, 24, 80)
        .await
        .expect("open shell stream");

    let mut host_session = tokio::time::timeout(Duration::from_secs(10), acceptor_rx.recv())
        .await
        .expect("shell accept timeout")
        .expect("shell accept channel open");

    assert_eq!(host_session.from, device_a);
    assert_eq!(host_session.rows, 24);
    assert_eq!(host_session.cols, 80);

    client_session
        .send
        .write_all(b"ping")
        .await
        .expect("write shell payload");

    let mut host_buf = [0u8; 4];
    read_exact(&mut host_session.recv, &mut host_buf)
        .await
        .expect("read shell payload on host");
    assert_eq!(&host_buf, b"ping");

    host_session
        .send
        .write_all(b"pong")
        .await
        .expect("write shell response");

    let mut client_buf = [0u8; 4];
    read_exact(&mut client_session.recv, &mut client_buf)
        .await
        .expect("read shell response on client");
    assert_eq!(&client_buf, b"pong");
}

#[tokio::test]
async fn concurrent_shell_streams_to_same_peer() {
    let temp = TempDir::new().expect("tempdir");
    let device_a = DeviceId::new();
    let device_b = DeviceId::new();

    let transport_a = IrohTransport::start(IrohConfig {
        device_id: device_a,
        secret_key_path: temp.path().join("concurrent-shell-a.key"),
        blob_store_path: temp.path().join("concurrent-shell-blobs-a"),
        address_book: PeerAddressBook::default(),
    })
    .await
    .expect("start transport a");

    let mut addresses_b = HashMap::new();
    addresses_b.insert(device_a, transport_a.public_key().to_string());

    let transport_b = IrohTransport::start(IrohConfig {
        device_id: device_b,
        secret_key_path: temp.path().join("concurrent-shell-b.key"),
        blob_store_path: temp.path().join("concurrent-shell-blobs-b"),
        address_book: PeerAddressBook::from_map(addresses_b),
    })
    .await
    .expect("start transport b");

    let (acceptor_tx, mut acceptor_rx) = mpsc::channel(2);
    transport_b.set_shell_acceptor(acceptor_tx).await;

    let mut addresses_a = HashMap::new();
    addresses_a.insert(device_b, transport_b.public_key().to_string());
    transport_a
        .update_peers(PeerAddressBook::from_map(addresses_a))
        .expect("update peer addresses on a");

    wait_for_connection(&transport_a, device_b)
        .await
        .expect("a connects to b");
    wait_for_connection(&transport_b, device_a)
        .await
        .expect("b connects to a");

    let (client_one, client_two) = tokio::join!(
        transport_a.open_shell_stream(device_b, 24, 80),
        transport_a.open_shell_stream(device_b, 24, 80),
    );
    let mut client_one = client_one.expect("open first shell stream");
    let mut client_two = client_two.expect("open second shell stream");

    let mut host_one = tokio::time::timeout(Duration::from_secs(10), acceptor_rx.recv())
        .await
        .expect("first shell accept timeout")
        .expect("first shell accept channel open");
    let mut host_two = tokio::time::timeout(Duration::from_secs(10), acceptor_rx.recv())
        .await
        .expect("second shell accept timeout")
        .expect("second shell accept channel open");

    assert_eq!(host_one.from, device_a);
    assert_eq!(host_two.from, device_a);

    client_one
        .send
        .write_all(b"one")
        .await
        .expect("write first shell payload");
    client_two
        .send
        .write_all(b"two")
        .await
        .expect("write second shell payload");

    let mut host_one_buf = [0u8; 3];
    let mut host_two_buf = [0u8; 3];
    read_exact(&mut host_one.recv, &mut host_one_buf)
        .await
        .expect("read first shell payload on host");
    read_exact(&mut host_two.recv, &mut host_two_buf)
        .await
        .expect("read second shell payload on host");
    assert_eq!(&host_one_buf, b"one");
    assert_eq!(&host_two_buf, b"two");

    host_one
        .send
        .write_all(b"1ok")
        .await
        .expect("write first shell response");
    host_two
        .send
        .write_all(b"2ok")
        .await
        .expect("write second shell response");

    let mut client_one_buf = [0u8; 3];
    let mut client_two_buf = [0u8; 3];
    read_exact(&mut client_one.recv, &mut client_one_buf)
        .await
        .expect("read first shell response on client");
    read_exact(&mut client_two.recv, &mut client_two_buf)
        .await
        .expect("read second shell response on client");
    assert_eq!(&client_one_buf, b"1ok");
    assert_eq!(&client_two_buf, b"2ok");
}
