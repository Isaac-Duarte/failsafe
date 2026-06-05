use std::path::PathBuf;

use failsafe_core::device::DeviceId;
use failsafe_core::peer_address::PeerAddressBook;

pub const FAILSAFE_ALPN: &[u8] = b"failsafe/1";

#[derive(Debug, Clone)]
pub struct IrohConfig {
    pub device_id: DeviceId,
    pub secret_key_path: PathBuf,
    pub blob_store_path: PathBuf,
    pub address_book: PeerAddressBook,
}
