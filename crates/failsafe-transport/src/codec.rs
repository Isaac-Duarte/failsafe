use failsafe_core::message::FeatureMessage;

use crate::transport::TransportError;

pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

pub fn encode(message: &FeatureMessage) -> Result<Vec<u8>, TransportError> {
    let payload =
        serde_json::to_vec(message).map_err(|error| TransportError::Codec(error.to_string()))?;

    if payload.len() > MAX_MESSAGE_SIZE {
        return Err(TransportError::Codec(format!(
            "message exceeds maximum size of {MAX_MESSAGE_SIZE} bytes"
        )));
    }

    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub fn decode(frame: &[u8]) -> Result<FeatureMessage, TransportError> {
    if frame.len() < 4 {
        return Err(TransportError::Codec("frame too short".to_owned()));
    }

    let length = u32::from_be_bytes(frame[..4].try_into().expect("length frame")) as usize;
    if length > MAX_MESSAGE_SIZE {
        return Err(TransportError::Codec(format!(
            "message exceeds maximum size of {MAX_MESSAGE_SIZE} bytes"
        )));
    }

    if frame.len() < 4 + length {
        return Err(TransportError::Codec("incomplete frame".to_owned()));
    }

    serde_json::from_slice(&frame[4..4 + length])
        .map_err(|error| TransportError::Codec(error.to_string()))
}

#[cfg(test)]
mod tests {
    use failsafe_core::device::DeviceId;
    use failsafe_core::feature::FeatureId;

    use super::*;

    #[test]
    fn roundtrips_feature_message() {
        let message = FeatureMessage::new(
            DeviceId::new(),
            DeviceId::new(),
            FeatureId::from_static("clipboard"),
            b"hello",
        );

        let frame = encode(&message).unwrap();
        let decoded = decode(&frame).unwrap();
        assert_eq!(message, decoded);
    }
}
