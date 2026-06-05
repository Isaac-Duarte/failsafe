use failsafe_core::feature::{FeatureError, FeatureId};
use serde::{Deserialize, Serialize};

pub const CLIPBOARD_PAYLOAD_VERSION: u32 = 2;
pub const INLINE_HTML_THRESHOLD: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardPayload {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(flatten)]
    pub content: ClipboardContent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClipboardContent {
    Text {
        text: String,
    },
    Html {
        html: String,
        plain: String,
    },
    HtmlBlob {
        hash: String,
        plain: String,
    },
    Image {
        hash: String,
        width: u32,
        height: u32,
        mime: String,
    },
    Files {
        collection_hash: String,
        entries: Vec<FileEntry>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub size: u64,
}

fn default_version() -> u32 {
    CLIPBOARD_PAYLOAD_VERSION
}

pub fn encode(payload: &ClipboardPayload) -> Vec<u8> {
    serde_json::to_vec(payload).expect("clipboard payload is serializable")
}

pub fn decode(bytes: &[u8]) -> Result<ClipboardPayload, FeatureError> {
    if let Ok(payload) = serde_json::from_slice::<ClipboardPayload>(bytes) {
        return Ok(payload);
    }

    #[derive(Deserialize)]
    struct LegacyPayload {
        text: String,
    }

    let legacy: LegacyPayload = serde_json::from_slice(bytes).map_err(|error| {
        FeatureError::Failed(
            FeatureId::Clipboard,
            format!("invalid clipboard payload: {error}"),
        )
    })?;

    Ok(ClipboardPayload {
        version: 1,
        content: ClipboardContent::Text { text: legacy.text },
    })
}

pub fn fingerprint(payload: &ClipboardPayload) -> String {
    hex::encode(blake3::hash(&encode(payload)).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_text() {
        let payload = ClipboardPayload {
            version: CLIPBOARD_PAYLOAD_VERSION,
            content: ClipboardContent::Text {
                text: "hello 🤓".to_owned(),
            },
        };
        let decoded = decode(&encode(&payload)).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn decodes_legacy_text_payload() {
        let bytes = br#"{"text":"legacy"}"#;
        let payload = decode(bytes).unwrap();
        assert_eq!(
            payload.content,
            ClipboardContent::Text {
                text: "legacy".to_owned()
            }
        );
    }

    #[test]
    fn roundtrips_image_metadata() {
        let payload = ClipboardPayload {
            version: CLIPBOARD_PAYLOAD_VERSION,
            content: ClipboardContent::Image {
                hash: "abc123".to_owned(),
                width: 10,
                height: 20,
                mime: "image/png".to_owned(),
            },
        };
        let decoded = decode(&encode(&payload)).unwrap();
        assert_eq!(decoded, payload);
    }
}
