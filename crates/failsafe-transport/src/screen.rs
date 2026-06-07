pub const SCREEN_LIST_HANDSHAKE: &[u8; 4] = b"FSL1";
pub const SCREEN_STREAM_HANDSHAKE: &[u8; 4] = b"FSS1";
pub const SCREEN_LIST_INIT_LEN: usize = 4;
pub const SCREEN_STREAM_INIT_LEN: usize = 8;

pub fn is_screen_list_handshake(header: &[u8]) -> bool {
    header.len() >= 4 && header[..4] == SCREEN_LIST_HANDSHAKE[..]
}

pub fn is_screen_stream_handshake(header: &[u8]) -> bool {
    header.len() >= 4 && header[..4] == SCREEN_STREAM_HANDSHAKE[..]
}

pub fn build_screen_list_init() -> [u8; SCREEN_LIST_INIT_LEN] {
    let mut buf = [0u8; SCREEN_LIST_INIT_LEN];
    buf[..4].copy_from_slice(SCREEN_LIST_HANDSHAKE);
    buf
}

pub fn build_screen_stream_init(screen_id: u32) -> [u8; SCREEN_STREAM_INIT_LEN] {
    let mut buf = [0u8; SCREEN_STREAM_INIT_LEN];
    buf[..4].copy_from_slice(SCREEN_STREAM_HANDSHAKE);
    buf[4..8].copy_from_slice(&screen_id.to_be_bytes());
    buf
}

pub fn parse_screen_stream_init(bytes: &[u8]) -> Option<u32> {
    if bytes.len() < SCREEN_STREAM_INIT_LEN || !is_screen_stream_handshake(bytes) {
        return None;
    }
    Some(u32::from_be_bytes(bytes[4..8].try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_list_init_is_recognized() {
        let init = build_screen_list_init();
        assert!(is_screen_list_handshake(&init));
        assert!(!is_screen_stream_handshake(&init));
    }

    #[test]
    fn screen_stream_init_roundtrip() {
        let init = build_screen_stream_init(2);
        assert!(is_screen_stream_handshake(&init));
        assert_eq!(parse_screen_stream_init(&init), Some(2));
    }

    #[test]
    fn feature_length_prefix_is_not_screen_handshake() {
        let frame = [0, 0, 0, 5];
        assert!(!is_screen_list_handshake(&frame));
        assert!(!is_screen_stream_handshake(&frame));
    }
}
