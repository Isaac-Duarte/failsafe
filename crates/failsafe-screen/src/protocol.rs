pub const SCREEN_HANDSHAKE: &[u8; 4] = b"FSS1";
pub const SCREEN_INIT_LEN: usize = 4;

pub fn is_screen_handshake(header: &[u8]) -> bool {
    header.len() >= 4 && header[..4] == SCREEN_HANDSHAKE[..]
}

pub fn build_screen_init() -> [u8; SCREEN_INIT_LEN] {
    let mut buf = [0u8; SCREEN_INIT_LEN];
    buf.copy_from_slice(SCREEN_HANDSHAKE);
    buf
}

pub fn encode_frame(payload: &[u8]) -> Vec<u8> {
    let len = u32::try_from(payload.len()).expect("frame fits in u32");
    let mut frame = len.to_be_bytes().to_vec();
    frame.extend_from_slice(payload);
    frame
}

pub fn decode_frame(bytes: &[u8]) -> Option<(usize, &[u8])> {
    if bytes.len() < 4 {
        return None;
    }
    let len = u32::from_be_bytes(bytes[..4].try_into().ok()?) as usize;
    if bytes.len() < 4 + len {
        return None;
    }
    Some((len, &bytes[4..4 + len]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_init_is_handshake() {
        let init = build_screen_init();
        assert!(is_screen_handshake(&init));
    }

    #[test]
    fn frame_roundtrip() {
        let payload = b"jpeg-data";
        let frame = encode_frame(payload);
        let (len, decoded) = decode_frame(&frame).expect("decode frame");
        assert_eq!(len, payload.len());
        assert_eq!(decoded, payload);
    }
}
