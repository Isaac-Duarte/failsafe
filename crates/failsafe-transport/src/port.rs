use failsafe_core::control::PortProtocol;

pub const PORT_HANDSHAKE: &[u8; 4] = b"FSP1";
pub const PORT_INIT_LEN: usize = 7;

pub const PORT_PROTOCOL_TCP: u8 = 0;

pub fn is_port_handshake(header: &[u8]) -> bool {
    header.len() >= 4 && header[..4] == PORT_HANDSHAKE[..]
}

pub fn build_port_init(remote_port: u16, protocol: PortProtocol) -> [u8; PORT_INIT_LEN] {
    let mut buf = [0u8; PORT_INIT_LEN];
    buf[..4].copy_from_slice(PORT_HANDSHAKE);
    buf[4..6].copy_from_slice(&remote_port.to_be_bytes());
    buf[6] = match protocol {
        PortProtocol::Tcp => PORT_PROTOCOL_TCP,
    };
    buf
}

pub fn parse_port_init(bytes: &[u8]) -> Option<(u16, PortProtocol)> {
    if bytes.len() < PORT_INIT_LEN || !is_port_handshake(bytes) {
        return None;
    }
    let remote_port = u16::from_be_bytes(bytes[4..6].try_into().ok()?);
    let protocol = match bytes[6] {
        PORT_PROTOCOL_TCP => PortProtocol::Tcp,
        _ => return None,
    };
    Some((remote_port, protocol))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_init_roundtrip() {
        let init = build_port_init(3000, PortProtocol::Tcp);
        assert!(is_port_handshake(&init));
        assert_eq!(parse_port_init(&init), Some((3000, PortProtocol::Tcp)));
    }

    #[test]
    fn feature_length_prefix_is_not_port_handshake() {
        let frame = [0, 0, 0, 5];
        assert!(!is_port_handshake(&frame));
    }
}
