use std::io;

use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::quality::{ScreenControlMessage, ScreenQualityPreset};

pub const SCREEN_HANDSHAKE: &[u8; 4] = b"FSS1";
pub const SCREEN_INIT_LEN: usize = 4;

pub const PACKET_TAG_FRAME: u8 = 0x01;
pub const PACKET_TAG_CONTROL: u8 = 0x02;

const MAX_PACKET_SIZE: usize = 16 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("packet too large")]
    PacketTooLarge,

    #[error("unknown packet tag: {0}")]
    UnknownTag(u8),

    #[error("invalid control message: {0}")]
    Control(String),
}

pub fn is_screen_handshake(header: &[u8]) -> bool {
    header.len() >= 4 && header[..4] == SCREEN_HANDSHAKE[..]
}

pub fn build_screen_init() -> [u8; SCREEN_INIT_LEN] {
    let mut buf = [0u8; SCREEN_INIT_LEN];
    buf.copy_from_slice(SCREEN_HANDSHAKE);
    buf
}

pub fn encode_tagged_packet(tag: u8, payload: &[u8]) -> Vec<u8> {
    let len = u32::try_from(payload.len()).expect("packet fits in u32");
    let mut packet = vec![tag];
    packet.extend_from_slice(&len.to_be_bytes());
    packet.extend_from_slice(payload);
    packet
}

pub fn encode_frame(payload: &[u8]) -> Vec<u8> {
    encode_tagged_packet(PACKET_TAG_FRAME, payload)
}

pub fn encode_control(message: &ScreenControlMessage) -> Result<Vec<u8>, ProtocolError> {
    let payload = serde_json::to_vec(message)
        .map_err(|error| ProtocolError::Control(error.to_string()))?;
    Ok(encode_tagged_packet(PACKET_TAG_CONTROL, &payload))
}

pub fn encode_set_quality(preset: ScreenQualityPreset) -> Result<Vec<u8>, ProtocolError> {
    encode_control(&ScreenControlMessage::SetQuality { preset })
}

pub async fn read_tagged_packet<R>(reader: &mut R) -> Result<(u8, Vec<u8>), ProtocolError>
where
    R: AsyncRead + Unpin,
{
    let mut tag_buf = [0u8; 1];
    reader.read_exact(&mut tag_buf).await?;
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_PACKET_SIZE {
        return Err(ProtocolError::PacketTooLarge);
    }
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await?;
    Ok((tag_buf[0], payload))
}

pub async fn write_tagged_packet<W>(writer: &mut W, tag: u8, payload: &[u8]) -> Result<(), ProtocolError>
where
    W: AsyncWrite + Unpin,
{
    let packet = encode_tagged_packet(tag, payload);
    writer.write_all(&packet).await?;
    writer.flush().await?;
    Ok(())
}

pub fn decode_control(payload: &[u8]) -> Result<ScreenControlMessage, ProtocolError> {
    serde_json::from_slice(payload).map_err(|error| ProtocolError::Control(error.to_string()))
}

pub fn decode_frame(bytes: &[u8]) -> Option<(usize, &[u8])> {
    if bytes.is_empty() {
        return None;
    }
    if bytes[0] == PACKET_TAG_FRAME {
        if bytes.len() < 5 {
            return None;
        }
        let len = u32::from_be_bytes(bytes[1..5].try_into().ok()?) as usize;
        if bytes.len() < 5 + len {
            return None;
        }
        return Some((len, &bytes[5..5 + len]));
    }

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
    use crate::quality::ScreenSettings;

    #[test]
    fn screen_init_is_handshake() {
        let init = build_screen_init();
        assert!(is_screen_handshake(&init));
    }

    #[test]
    fn tagged_frame_roundtrip() {
        let payload = b"jpeg-data";
        let frame = encode_frame(payload);
        assert_eq!(frame[0], PACKET_TAG_FRAME);
        let (len, decoded) = decode_frame(&frame).expect("decode frame");
        assert_eq!(len, payload.len());
        assert_eq!(decoded, payload);
    }

    #[test]
    fn control_roundtrip() {
        let packet = encode_set_quality(ScreenQualityPreset::P1080).expect("encode");
        assert_eq!(packet[0], PACKET_TAG_CONTROL);
        let payload = &packet[5..];
        let message = decode_control(payload).expect("decode");
        let mut settings = ScreenSettings::default();
        message.apply(&mut settings);
        assert_eq!(settings.max_width, 1920);
    }

    #[tokio::test]
    async fn async_tagged_packet_roundtrip() {
        let payload = b"hello";
        let encoded = encode_tagged_packet(PACKET_TAG_FRAME, payload);
        let mut cursor = io::Cursor::new(encoded);
        let (tag, decoded) = read_tagged_packet(&mut cursor).await.expect("read");
        assert_eq!(tag, PACKET_TAG_FRAME);
        assert_eq!(decoded, payload);
    }
}
