use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use failsafe_core::feature::FeatureSpec;
use failsafe_core::outbound::OutboundMessage;

use super::ClipboardFeatureSpec;
use crate::payload;

use super::ClipboardState;
use super::outbound::{content_to_payload, fingerprint_content};

const POLL_INTERVAL: Duration = Duration::from_millis(300);

pub(super) async fn watch_clipboard(state: Arc<ClipboardState>) {
    let mut interval = tokio::time::interval(POLL_INTERVAL);

    loop {
        interval.tick().await;

        if state.applying_remote.load(Ordering::SeqCst) {
            continue;
        }

        let content = match state.clipboard.read().await {
            Ok(Some(content)) => content,
            Ok(None) => continue,
            Err(error) => {
                eprintln!("clipboard read failed: {error}");
                continue;
            }
        };

        let content_fingerprint = fingerprint_content(&content);
        let payload =
            match content_to_payload(&content, state.blob_transfer.clone(), state.limits).await {
                Ok(payload) => {
                    *state.last_failed.lock().await = None;
                    payload
                }
                Err(error) => {
                    let mut last_failed = state.last_failed.lock().await;
                    if last_failed.as_deref() == Some(content_fingerprint.as_str()) {
                        continue;
                    }
                    *last_failed = Some(content_fingerprint);
                    eprintln!("clipboard payload build failed: {error}");
                    continue;
                }
            };

        let fingerprint = payload::fingerprint(&payload);
        {
            let last_emitted = state.last_emitted.lock().await;
            if last_emitted.as_deref() == Some(fingerprint.as_str()) {
                continue;
            }
        }

        *state.last_emitted.lock().await = Some(fingerprint);

        let outbound = OutboundMessage::new(
            ClipboardFeatureSpec::feature_id(),
            payload::encode(&payload),
        );

        if let Err(error) = state.publisher.publish(outbound).await {
            eprintln!("clipboard publish failed: {error}");
        }
    }
}
