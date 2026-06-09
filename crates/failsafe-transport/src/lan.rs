pub const LAN_HANDSHAKE: &[u8; 4] = b"FSL1";
pub const MAX_LAN_PACKET_SIZE: usize = 65_535;

pub fn is_lan_handshake(header: &[u8]) -> bool {
    header.len() >= 4 && header[..4] == LAN_HANDSHAKE[..]
}

pub fn build_lan_init() -> [u8; 4] {
    let mut buf = [0u8; 4];
    buf.copy_from_slice(LAN_HANDSHAKE);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lan_handshake_is_recognized() {
        let init = build_lan_init();
        assert!(is_lan_handshake(&init));
    }
}
