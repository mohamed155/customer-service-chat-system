//! What WhatsApp allows a reply to say and when.
//!
//! WhatsApp only lets a business message a customer inside a 24 hour window that opens when the
//! customer last wrote, and caps a message at 4096 characters.

use chrono::{DateTime, Duration, Utc};

/// How long after the customer's last message a reply may still be sent.
pub fn reply_window() -> Duration {
    Duration::hours(24)
}

/// The longest message WhatsApp accepts, in characters.
pub const MAX_MESSAGE_CHARS: usize = 4096;

/// Whether a reply may still be sent, given when the customer last wrote. A customer who never
/// wrote has no window.
pub fn reply_window_is_open(
    last_customer_message_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> bool {
    last_customer_message_at.is_some_and(|last| now.signed_duration_since(last) < reply_window())
}

/// Whether `body` is short enough for WhatsApp.
pub fn fits_in_a_message(body: &str) -> bool {
    body.chars().count() <= MAX_MESSAGE_CHARS
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 13, 12, 0, 0).unwrap()
    }

    #[test]
    fn a_customer_who_wrote_within_a_day_leaves_the_window_open() {
        assert!(reply_window_is_open(
            Some(now() - Duration::hours(23)),
            now()
        ));
        assert!(reply_window_is_open(Some(now()), now()));
    }

    #[test]
    fn a_day_after_the_last_customer_message_the_window_has_closed() {
        assert!(!reply_window_is_open(
            Some(now() - Duration::hours(24)),
            now()
        ));
        assert!(!reply_window_is_open(
            Some(now() - Duration::days(3)),
            now()
        ));
    }

    #[test]
    fn a_customer_who_never_wrote_has_no_window() {
        assert!(!reply_window_is_open(None, now()));
    }

    #[test]
    fn the_limit_counts_characters_not_bytes() {
        assert!(fits_in_a_message(&"a".repeat(MAX_MESSAGE_CHARS)));
        assert!(fits_in_a_message(&"é".repeat(MAX_MESSAGE_CHARS)));
        assert!(!fits_in_a_message(&"a".repeat(MAX_MESSAGE_CHARS + 1)));
    }
}
