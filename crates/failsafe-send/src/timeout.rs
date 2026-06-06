use std::time::Duration;

/// Minimum wait before the sender treats a transfer as timed out.
const MIN_ACK_TIMEOUT_SECS: u64 = 300;

/// Additional seconds allowed per mebibyte of declared transfer size.
const ACK_TIMEOUT_SECS_PER_MIB: u64 = 120;

/// How long the sender waits for a receiver acknowledgement after dispatching metadata.
///
/// Scales with transfer size so large sends are not capped by a fixed deadline.
pub fn send_ack_timeout(bytes_total: u64) -> Duration {
    let mib = bytes_total / (1024 * 1024);
    Duration::from_secs(
        MIN_ACK_TIMEOUT_SECS.saturating_add(mib.saturating_mul(ACK_TIMEOUT_SECS_PER_MIB)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimum_timeout_for_small_transfers() {
        assert_eq!(send_ack_timeout(0), Duration::from_secs(300));
        assert_eq!(send_ack_timeout(1024), Duration::from_secs(300));
    }

    #[test]
    fn scales_with_size() {
        let one_gib = 1024 * 1024 * 1024;
        assert_eq!(
            send_ack_timeout(one_gib),
            Duration::from_secs(300 + 1024 * 120)
        );
    }
}
