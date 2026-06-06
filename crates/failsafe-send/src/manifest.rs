use failsafe_core::device::DeviceId;
use failsafe_core::feature::FeatureId;
use failsafe_core::message::FeatureMessage;
use failsafe_transport::codec;

use crate::payload::{
    FileEntry, SendEnvelope, SendPayload, SendTransferChunk, SendTransferEnd, SendTransferHeader,
    SEND_PAYLOAD_VERSION, encode_envelope,
};

const INITIAL_CHUNK_ENTRIES: usize = 4_096;

pub fn plan_transfer_envelopes(
    from: DeviceId,
    to: DeviceId,
    payload: SendPayload,
) -> Vec<SendEnvelope> {
    let single = SendEnvelope::Transfer(payload.clone());
    if transfer_envelope_fits(from, to, &single) {
        return vec![single];
    }

    let bytes_total = payload.entries.iter().map(|entry| entry.size).sum();
    let entry_count = u32::try_from(payload.entries.len()).unwrap_or(u32::MAX);
    let transfer_id = payload.transfer_id;

    let mut envelopes = vec![SendEnvelope::TransferHeader(SendTransferHeader {
        version: SEND_PAYLOAD_VERSION,
        transfer_id,
        sender_name: payload.sender_name,
        collection_hash: payload.collection_hash,
        bytes_total,
        entry_count,
    })];

    let mut chunk_index = 0u32;
    let mut offset = 0usize;
    while offset < payload.entries.len() {
        let mut chunk_len = INITIAL_CHUNK_ENTRIES.min(payload.entries.len() - offset);
        while chunk_len > 0 {
            let end = offset + chunk_len;
            let chunk = SendTransferChunk {
                transfer_id,
                chunk_index,
                entries: payload.entries[offset..end].to_vec(),
            };
            let envelope = SendEnvelope::TransferChunk(chunk);
            if transfer_envelope_fits(from, to, &envelope) {
                envelopes.push(envelope);
                offset = end;
                chunk_index = chunk_index.saturating_add(1);
                break;
            }
            chunk_len /= 2;
        }
        if chunk_len == 0 {
            let entry = payload.entries[offset].clone();
            let envelope = SendEnvelope::TransferChunk(SendTransferChunk {
                transfer_id,
                chunk_index,
                entries: vec![entry],
            });
            debug_assert!(transfer_envelope_fits(from, to, &envelope));
            envelopes.push(envelope);
            offset += 1;
            chunk_index = chunk_index.saturating_add(1);
        }
    }

    envelopes.push(SendEnvelope::TransferEnd(SendTransferEnd { transfer_id }));
    envelopes
}

fn transfer_envelope_fits(from: DeviceId, to: DeviceId, envelope: &SendEnvelope) -> bool {
    let message = FeatureMessage::new(
        from,
        to,
        FeatureId::FileSend,
        encode_envelope(envelope),
    );
    codec::encode(&message).is_ok()
}

pub struct ChunkedTransfer {
    header: SendTransferHeader,
    chunks: Vec<Vec<FileEntry>>,
}

impl ChunkedTransfer {
    pub fn new(header: SendTransferHeader) -> Self {
        Self {
            header,
            chunks: Vec::new(),
        }
    }

    pub fn push_chunk(&mut self, chunk_index: u32, entries: Vec<FileEntry>) {
        let index = chunk_index as usize;
        if self.chunks.len() <= index {
            self.chunks.resize(index + 1, Vec::new());
        }
        self.chunks[index] = entries;
    }

    pub fn into_payload(self) -> Result<SendPayload, String> {
        let entries: Vec<FileEntry> = self.chunks.into_iter().flatten().collect();
        if entries.len() != self.header.entry_count as usize {
            return Err(format!(
                "transfer {} manifest incomplete: expected {} entries, got {}",
                self.header.transfer_id,
                self.header.entry_count,
                entries.len()
            ));
        }
        let bytes_total: u64 = entries.iter().map(|entry| entry.size).sum();
        if bytes_total != self.header.bytes_total {
            return Err(format!(
                "transfer {} manifest size mismatch: expected {} bytes, got {}",
                self.header.transfer_id, self.header.bytes_total, bytes_total
            ));
        }
        Ok(SendPayload {
            version: self.header.version,
            transfer_id: self.header.transfer_id,
            sender_name: self.header.sender_name,
            collection_hash: self.header.collection_hash,
            entries,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn sample_entry(index: usize) -> FileEntry {
        FileEntry {
            name: format!("elektrik-backend-rs/target/debug/deps/file_{index}.rlib"),
            size: 12_345_678,
        }
    }

    fn large_payload(entry_count: usize) -> SendPayload {
        SendPayload {
            version: SEND_PAYLOAD_VERSION,
            transfer_id: Uuid::new_v4(),
            sender_name: "laptop".to_owned(),
            collection_hash: "abc123".to_owned(),
            entries: (0..entry_count).map(sample_entry).collect(),
        }
    }

    #[test]
    fn single_envelope_used_for_small_transfer() {
        let from = DeviceId::new();
        let to = DeviceId::new();
        let payload = large_payload(1);
        let envelopes = plan_transfer_envelopes(from, to, payload.clone());
        assert_eq!(envelopes.len(), 1);
        assert!(matches!(envelopes[0], SendEnvelope::Transfer(_)));
    }

    #[test]
    fn large_transfer_is_chunked_and_reassembles() {
        let from = DeviceId::new();
        let to = DeviceId::new();
        let payload = large_payload(98_469);
        let single = SendEnvelope::Transfer(payload.clone());
        assert!(!transfer_envelope_fits(from, to, &single));

        let envelopes = plan_transfer_envelopes(from, to, payload.clone());
        assert!(envelopes.len() > 3);
        assert!(matches!(envelopes[0], SendEnvelope::TransferHeader(_)));
        assert!(matches!(
            envelopes.last(),
            Some(SendEnvelope::TransferEnd(_))
        ));

        let mut chunked = None;
        for envelope in envelopes {
            match envelope {
                SendEnvelope::TransferHeader(header) => {
                    chunked = Some(ChunkedTransfer::new(header));
                }
                SendEnvelope::TransferChunk(chunk) => {
                    let state = chunked.as_mut().expect("header first");
                    state.push_chunk(chunk.chunk_index, chunk.entries);
                }
                SendEnvelope::TransferEnd(_) => {}
                other => panic!("unexpected envelope: {other:?}"),
            }
        }

        let reassembled = chunked.unwrap().into_payload().unwrap();
        assert_eq!(reassembled, payload);
    }

    #[test]
    fn chunked_envelopes_fit_transport_limit() {
        let from = DeviceId::new();
        let to = DeviceId::new();
        let payload = large_payload(98_469);
        for envelope in plan_transfer_envelopes(from, to, payload) {
            assert!(transfer_envelope_fits(from, to, &envelope));
        }
    }
}
