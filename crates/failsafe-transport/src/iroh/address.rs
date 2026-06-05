use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use failsafe_core::device::DeviceId;
use failsafe_core::peer_address::PeerAddressBook;

use crate::transport::TransportError;

#[derive(Debug, Clone)]
pub struct AddressState {
    pub book: PeerAddressBook,
    pub reverse_lookup: HashMap<String, DeviceId>,
}

impl AddressState {
    pub fn from_book(book: PeerAddressBook) -> Result<Self, TransportError> {
        let reverse_lookup = super::transport::build_reverse_lookup(&book)?;
        Ok(Self {
            book,
            reverse_lookup,
        })
    }
}

pub type SharedAddressState = Arc<RwLock<AddressState>>;

pub fn update_address_state(
    state: &SharedAddressState,
    book: PeerAddressBook,
) -> Result<(), TransportError> {
    let next = AddressState::from_book(book)?;
    let mut guard = state
        .write()
        .map_err(|error| TransportError::Codec(format!("address state lock poisoned: {error}")))?;
    *guard = next;
    Ok(())
}
