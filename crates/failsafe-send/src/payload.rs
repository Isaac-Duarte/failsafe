use failsafe_core::control::SendPhase;
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
pub struct SendTransferHeader {
    pub version: u32,
    pub transfer_id: Uuid,
    pub sender_name: String,
    pub collection_hash: String,
    pub bytes_total: u64,
    pub entry_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendTransferChunk {
    pub transfer_id: Uuid,
    pub chunk_index: u32,
    pub entries: Vec<FileEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendTransferEnd {
    pub transfer_id: Uuid,
    #[serde(default)]
    pub chunk_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendAck {
    pub transfer_id: Uuid,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendProgress {
    pub transfer_id: Uuid,
    pub phase: SendPhase,
    pub bytes_done: u64,
    pub bytes_total: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SendEnvelope {
    Transfer(SendPayload),
    TransferHeader(SendTransferHeader),
    TransferChunk(SendTransferChunk),
    TransferEnd(SendTransferEnd),
    Ack(SendAck),
    Progress(SendProgress),
}

pub fn encode_envelope(envelope: &SendEnvelope) -> Vec<u8> {
    serde_json::to_vec(envelope).expect("send envelope is serializable")
}

pub fn parse_ack(payload: &[u8]) -> Option<SendAck> {
    let envelope = serde_json::from_slice::<SendEnvelope>(payload).ok()?;
    match envelope {
        SendEnvelope::Ack(ack) => Some(ack),
        SendEnvelope::Transfer(_)
        | SendEnvelope::TransferHeader(_)
        | SendEnvelope::TransferChunk(_)
        | SendEnvelope::TransferEnd(_)
        | SendEnvelope::Progress(_) => None,
    }
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

    #[test]
    fn roundtrips_progress_envelope() {
        let envelope = SendEnvelope::Progress(SendProgress {
            transfer_id: Uuid::new_v4(),
            phase: SendPhase::WaitingForAck,
            bytes_done: 7,
            bytes_total: 42,
            current_file: Some("doc.txt".to_owned()),
        });
        let decoded = decode_envelope(&encode_envelope(&envelope)).unwrap();
        assert_eq!(decoded, envelope);
    }
}
