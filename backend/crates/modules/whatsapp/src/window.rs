use chrono::{DateTime, Duration, Utc};

/// The 24-hour customer-service messaging window enforced by Meta.
pub const WINDOW: Duration = Duration::hours(24);

/// Check whether the messaging window is open for a conversation.
/// Returns `true` if the last customer message was within the last 24 hours.
/// Returns `false` if there is no customer message yet or the window has expired.
pub fn window_open(last_customer_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    match last_customer_at {
        Some(ts) => now.signed_duration_since(ts) < WINDOW,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_open_within_24h() {
        let now = Utc::now();
        let within = now - Duration::hours(23) - Duration::minutes(59);
        assert!(window_open(Some(within), now));
    }

    #[test]
    fn test_window_open_exactly_24h() {
        let now = Utc::now();
        let exactly = now - Duration::hours(24);
        // 24h exactly is NOT open (strictly less than)
        assert!(!window_open(Some(exactly), now));
    }

    #[test]
    fn test_window_open_after_24h() {
        let now = Utc::now();
        let after = now - Duration::hours(24) - Duration::minutes(1);
        assert!(!window_open(Some(after), now));
    }

    #[test]
    fn test_window_open_no_customer_message() {
        let now = Utc::now();
        assert!(!window_open(None, now));
    }
}
