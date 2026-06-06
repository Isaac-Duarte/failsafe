use failsafe_core::device::DeviceId;

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

pub struct ScreenInbound {
    pub from: DeviceId,
}
