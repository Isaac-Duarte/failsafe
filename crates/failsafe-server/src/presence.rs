use chrono::{DateTime, Duration, Utc};

pub const ONLINE_THRESHOLD_SECS: i64 = 90;

pub fn is_online(last_seen: Option<DateTime<Utc>>) -> bool {
    let Some(last_seen) = last_seen else {
        return false;
    };

    Utc::now().signed_duration_since(last_seen) <= Duration::seconds(ONLINE_THRESHOLD_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_last_seen_is_online() {
        assert!(is_online(Some(Utc::now())));
        assert!(is_online(Some(Utc::now() - Duration::seconds(60))));
    }

    #[test]
    fn stale_last_seen_is_offline() {
        assert!(!is_online(None));
        assert!(!is_online(Some(Utc::now() - Duration::seconds(91))));
    }
}
