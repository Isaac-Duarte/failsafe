use failsafe_core::peer_address::PeerAddressBook;

/// Allows the daemon to refresh peer network addresses without restarting transport.
pub trait PeerAddressUpdater: Send + Sync {
    fn update_peer_addresses(&self, book: PeerAddressBook);
}
