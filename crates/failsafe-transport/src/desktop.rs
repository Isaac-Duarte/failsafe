use failsafe_core::device::DeviceId;

pub const DESKTOP_HANDSHAKE: &[u8; 4] = b"FDT1";
pub const DESKTOP_INIT_LEN: usize = 9;

pub const INPUT_HANDSHAKE: &[u8; 4] = b"FDI1";

pub fn is_desktop_handshake(header: &[u8]) -> bool {
    header.len() >= 4 && header[..4] == DESKTOP_HANDSHAKE[..]
}

pub fn is_input_handshake(header: &[u8]) -> bool {
    header.len() >= 4 && header[..4] == INPUT_HANDSHAKE[..]
}

pub fn build_desktop_init(view_only: bool, display_index: u32) -> [u8; DESKTOP_INIT_LEN] {
    let mut buf = [0u8; DESKTOP_INIT_LEN];
    buf[..4].copy_from_slice(DESKTOP_HANDSHAKE);
    buf[4] = u8::from(view_only);
    buf[5..9].copy_from_slice(&display_index.to_be_bytes());
    buf
}

pub fn parse_desktop_init(bytes: &[u8]) -> Option<(bool, u32)> {
    if bytes.len() < DESKTOP_INIT_LEN || !is_desktop_handshake(bytes) {
        return None;
    }
    let view_only = bytes[4] != 0;
    let display_index = u32::from_be_bytes(bytes[5..9].try_into().ok()?);
    Some((view_only, display_index))
}

pub fn build_input_init() -> [u8; 4] {
    let mut buf = [0u8; 4];
    buf.copy_from_slice(INPUT_HANDSHAKE);
    buf
}

/// Incoming desktop view/control session from a remote peer.
pub struct DesktopInbound {
    pub from: DeviceId,
    pub view_only: bool,
    pub display_index: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_init_roundtrip() {
        let init = build_desktop_init(false, 2);
        assert!(is_desktop_handshake(&init));
        assert_eq!(parse_desktop_init(&init), Some((false, 2)));
    }

    #[test]
    fn input_handshake_is_distinct() {
        let init = build_input_init();
        assert!(is_input_handshake(&init));
        assert!(!is_desktop_handshake(&init));
    }
}
