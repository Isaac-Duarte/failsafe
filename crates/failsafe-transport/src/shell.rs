use failsafe_core::device::DeviceId;

pub const SHELL_HANDSHAKE: &[u8; 4] = b"FSH1";
pub const SHELL_INIT_LEN: usize = 8;

/// Incoming shell session from a remote peer.
pub struct ShellInbound {
    pub from: DeviceId,
    pub rows: u16,
    pub cols: u16,
}

pub fn is_shell_handshake(header: &[u8]) -> bool {
    header.len() >= 4 && header[..4] == SHELL_HANDSHAKE[..]
}

pub fn build_shell_init(rows: u16, cols: u16) -> [u8; SHELL_INIT_LEN] {
    let mut buf = [0u8; SHELL_INIT_LEN];
    buf[..4].copy_from_slice(SHELL_HANDSHAKE);
    buf[4..6].copy_from_slice(&rows.to_be_bytes());
    buf[6..8].copy_from_slice(&cols.to_be_bytes());
    buf
}

pub fn parse_shell_init(bytes: &[u8]) -> Option<(u16, u16)> {
    if bytes.len() < SHELL_INIT_LEN || !is_shell_handshake(bytes) {
        return None;
    }
    let rows = u16::from_be_bytes(bytes[4..6].try_into().ok()?);
    let cols = u16::from_be_bytes(bytes[6..8].try_into().ok()?);
    Some((rows, cols))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_init_roundtrip() {
        let init = build_shell_init(24, 80);
        assert!(is_shell_handshake(&init));
        assert_eq!(parse_shell_init(&init), Some((24, 80)));
    }

    #[test]
    fn feature_length_prefix_is_not_shell_handshake() {
        let frame = [0, 0, 0, 5];
        assert!(!is_shell_handshake(&frame));
    }
}
