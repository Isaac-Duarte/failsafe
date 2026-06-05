use failsafe_core::feature::{FeatureError, FeatureId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardPayload {
    pub text: String,
}

pub fn encode(text: &str) -> Vec<u8> {
    let payload = ClipboardPayload {
        text: text.to_owned(),
    };

    serde_json::to_vec(&payload).expect("clipboard payload is serializable")
}

pub fn decode(bytes: &[u8]) -> Result<ClipboardPayload, FeatureError> {
    serde_json::from_slice(bytes).map_err(|error| {
        FeatureError::Failed(
            FeatureId::Clipboard,
            format!("invalid clipboard payload: {error}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_text() {
        let bytes = encode("hello 🤓");
        let payload = decode(&bytes).unwrap();
        assert_eq!(payload.text, "hello 🤓");
    }
}
