use std::collections::HashMap;

use crate::device::DeviceId;

/// Maps logical device IDs to opaque network addresses.
///
/// Addresses are transport-specific strings (e.g. Iroh `EndpointAddr`). The
/// registration server will populate this in the future; for now it comes
/// from daemon config.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerAddressBook {
    addresses: HashMap<DeviceId, String>,
}

impl PeerAddressBook {
    pub fn from_map(addresses: HashMap<DeviceId, String>) -> Self {
        Self { addresses }
    }

    pub fn get(&self, device: DeviceId) -> Option<&str> {
        self.addresses.get(&device).map(String::as_str)
    }

    pub fn iter(&self) -> impl Iterator<Item = (DeviceId, &str)> {
        self.addresses
            .iter()
            .map(|(device, address)| (*device, address.as_str()))
    }

    pub fn is_empty(&self) -> bool {
        self.addresses.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_up_configured_address() {
        let device = DeviceId::new();
        let mut addresses = HashMap::new();
        addresses.insert(device, "some-addr".to_owned());

        let book = PeerAddressBook::from_map(addresses);
        assert_eq!(book.get(device), Some("some-addr"));
    }
}
