//! Desktop stream framing over Iroh bi-directional streams.

pub const FRAME_CONFIG: u8 = 0;
pub const FRAME_JPEG: u8 = 1;

pub const INPUT_MOUSE_MOVE: u8 = 0;
pub const INPUT_MOUSE_BUTTON: u8 = 1;
pub const INPUT_KEY: u8 = 2;

pub fn encode_config(width: u32, height: u32) -> Vec<u8> {
    let mut frame = Vec::with_capacity(1 + 8);
    frame.push(FRAME_CONFIG);
    frame.extend_from_slice(&width.to_be_bytes());
    frame.extend_from_slice(&height.to_be_bytes());
    frame
}

pub fn encode_jpeg(jpeg: &[u8]) -> Vec<u8> {
    let len = u32::try_from(jpeg.len()).expect("jpeg frame fits in u32");
    let mut frame = Vec::with_capacity(5 + jpeg.len());
    frame.push(FRAME_JPEG);
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(jpeg);
    frame
}

pub fn encode_length_prefixed(payload: &[u8]) -> Vec<u8> {
    let len = u32::try_from(payload.len()).expect("payload fits in u32");
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    Config { width: u32, height: u32 },
    Jpeg,
    Unknown(u8),
}

pub fn parse_frame_kind(header: &[u8]) -> FrameKind {
    match header.first().copied() {
        Some(FRAME_CONFIG) if header.len() >= 9 => FrameKind::Config {
            width: u32::from_be_bytes(header[1..5].try_into().expect("width")),
            height: u32::from_be_bytes(header[5..9].try_into().expect("height")),
        },
        Some(FRAME_JPEG) => FrameKind::Jpeg,
        Some(other) => FrameKind::Unknown(other),
        None => FrameKind::Unknown(0),
    }
}

pub fn encode_mouse_move(x: i32, y: i32) -> [u8; 9] {
    let mut buf = [0u8; 9];
    buf[0] = INPUT_MOUSE_MOVE;
    buf[1..5].copy_from_slice(&x.to_be_bytes());
    buf[5..9].copy_from_slice(&y.to_be_bytes());
    buf
}

pub fn encode_mouse_button(button: u8, pressed: bool) -> [u8; 3] {
    [INPUT_MOUSE_BUTTON, button, u8::from(pressed)]
}

pub fn encode_key(key: u32, pressed: bool) -> [u8; 6] {
    let mut buf = [0u8; 6];
    buf[0] = INPUT_KEY;
    buf[1..5].copy_from_slice(&key.to_be_bytes());
    buf[5] = u8::from(pressed);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrip() {
        let payload = encode_config(1920, 1080);
        match parse_frame_kind(&payload) {
            FrameKind::Config { width, height } => {
                assert_eq!(width, 1920);
                assert_eq!(height, 1080);
            }
            other => panic!("expected config, got {other:?}"),
        }
    }
}
