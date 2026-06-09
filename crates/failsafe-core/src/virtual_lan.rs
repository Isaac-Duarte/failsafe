use std::net::Ipv4Addr;
use std::str::FromStr;

use uuid::Uuid;

const SUBNET_PREFIX: [u8; 2] = [100, 64];

/// Derives the account-scoped /24 subnet base (third and fourth octets of the network address).
pub fn account_subnet_octets(account_id: Uuid) -> (u8, u8) {
    let hash = blake3::hash(account_id.as_bytes());
    let bytes = hash.as_bytes();
    (bytes[0], bytes[1])
}

/// Assigns a deterministic virtual IPv4 for a device within an account subnet.
pub fn assign_virtual_ip(account_id: Uuid, device_id: Uuid) -> Ipv4Addr {
    let (oct3, _) = account_subnet_octets(account_id);
    let hash = blake3::hash(device_id.as_bytes());
    let host = 2 + (hash.as_bytes()[0] as u16 % 253);
    Ipv4Addr::new(SUBNET_PREFIX[0], SUBNET_PREFIX[1], oct3, host as u8)
}

/// Returns the /24 subnet CIDR for an account, e.g. `100.64.12.0/24`.
pub fn account_subnet_cidr(account_id: Uuid) -> String {
    let (oct3, _) = account_subnet_octets(account_id);
    format!("{}.{}.{}.0/24", SUBNET_PREFIX[0], SUBNET_PREFIX[1], oct3)
}

/// Parses a dotted IPv4 string.
pub fn parse_virtual_ip(value: &str) -> Option<Ipv4Addr> {
    Ipv4Addr::from_str(value.trim()).ok()
}

/// Subnet mask for the virtual LAN (/24).
pub fn subnet_mask() -> Ipv4Addr {
    Ipv4Addr::new(255, 255, 255, 0)
}

/// Network address for a device IP on a /24.
pub fn network_address(ip: Ipv4Addr) -> Ipv4Addr {
    let octets = ip.octets();
    Ipv4Addr::new(octets[0], octets[1], octets[2], 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assign_virtual_ip_is_stable() {
        let account = Uuid::from_u128(1);
        let device = Uuid::from_u128(2);
        let first = assign_virtual_ip(account, device);
        let second = assign_virtual_ip(account, device);
        assert_eq!(first, second);
        assert_eq!(first.octets()[0], 100);
        assert_eq!(first.octets()[1], 64);
        assert!(first.octets()[3] >= 2);
    }

    #[test]
    fn account_subnet_cidr_format() {
        let account = Uuid::from_u128(42);
        let cidr = account_subnet_cidr(account);
        assert!(cidr.starts_with("100.64."));
        assert!(cidr.ends_with(".0/24"));
    }
}
