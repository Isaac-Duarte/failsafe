use failsafe_core::feature::{FeatureError, FeatureId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const SEND_PAYLOAD_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendPayload {
    pub version: u32,
    pub transfer_id: Uuid,
    pub sender_name: String,
    pub collection_hash: String,
    pub entries: Vec<FileEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendAck {
    pub transfer_id: Uuid,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SendEnvelope {
    Transfer(SendPayload),
    Ack(SendAck),
}

pub fn encode_envelope(envelope: &SendEnvelope) -> Vec<u8> {
    serde_json::to_vec(envelope).expect("send envelope is serializable")
}

pub fn decode_envelope(bytes: &[u8]) -> Result<SendEnvelope, FeatureError> {
    serde_json::from_slice(bytes).map_err(|error| {
        FeatureError::Failed(
            FeatureId::FileSend,
            format!("invalid send envelope: {error}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_transfer_envelope() {
        let envelope = SendEnvelope::Transfer(SendPayload {
            version: SEND_PAYLOAD_VERSION,
            transfer_id: Uuid::new_v4(),
            sender_name: "laptop".to_owned(),
            collection_hash: "abc123".to_owned(),
            entries: vec![FileEntry {
                name: "doc.txt".to_owned(),
                size: 42,
            }],
        });
        let decoded = decode_envelope(&encode_envelope(&envelope)).unwrap();
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn roundtrips_ack_envelope() {
        let envelope = SendEnvelope::Ack(SendAck {
            transfer_id: Uuid::new_v4(),
            ok: true,
            error: None,
        });
        let decoded = decode_envelope(&encode_envelope(&envelope)).unwrap();
        assert_eq!(decoded, envelope);
    }
}
