//! Opaque keyset-pagination cursors.
//!
//! A cursor is the hex of `"<timestamp>|<tiebreaker>"`: the sort key of the last row of a page. The
//! inbox is ordered on `(last_activity_at, id)` and a conversation's messages on `(created_at,
//! seq)`.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Encode a `(DateTime<Utc>, Uuid)` pair as an opaque hex string suitable
/// for use as a keyset-pagination cursor.
pub fn encode_cursor(last_activity_at: DateTime<Utc>, id: Uuid) -> String {
    let payload = format!("{}|{}", last_activity_at.to_rfc3339(), id);
    hex::encode(payload.as_bytes())
}

/// Decode a cursor string produced by [`encode_cursor`] back into the
/// original `(DateTime<Utc>, Uuid)` pair. Returns `None` on malformed input.
pub fn decode_cursor(cursor: &str) -> Option<(DateTime<Utc>, Uuid)> {
    let bytes = hex::decode(cursor).ok()?;
    let decoded = String::from_utf8(bytes).ok()?;
    let (ts, id) = decoded.split_once('|')?;
    let ts = DateTime::parse_from_rfc3339(ts).ok()?.with_timezone(&Utc);
    let id = Uuid::parse_str(id).ok()?;
    Some((ts, id))
}

/// Encode a `(DateTime<Utc>, i64)` pair as an opaque hex string suitable
/// for use as a timeline keyset-pagination cursor.
pub fn encode_timeline_cursor(created_at: DateTime<Utc>, seq: i64) -> String {
    let payload = format!("{}|{}", created_at.to_rfc3339(), seq);
    hex::encode(payload.as_bytes())
}

/// Decode a cursor string produced by [`encode_timeline_cursor`] back into
/// the original `(DateTime<Utc>, i64)` pair. Returns `None` on malformed input.
pub fn decode_timeline_cursor(cursor: &str) -> Option<(DateTime<Utc>, i64)> {
    let bytes = hex::decode(cursor).ok()?;
    let decoded = String::from_utf8(bytes).ok()?;
    let (ts, seq) = decoded.split_once('|')?;
    let ts = DateTime::parse_from_rfc3339(ts).ok()?.with_timezone(&Utc);
    let seq = seq.parse().ok()?;
    Some((ts, seq))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 13, 9, 30, 0).unwrap()
    }

    #[test]
    fn an_inbox_cursor_round_trips() {
        let id = Uuid::from_u128(42);
        assert_eq!(decode_cursor(&encode_cursor(at(), id)), Some((at(), id)));
    }

    #[test]
    fn a_timeline_cursor_round_trips() {
        assert_eq!(
            decode_timeline_cursor(&encode_timeline_cursor(at(), 17)),
            Some((at(), 17))
        );
    }

    #[test]
    fn a_cursor_is_hex_of_the_documented_payload() {
        let id = Uuid::from_u128(1);
        let expected = hex::encode(format!("{}|{}", at().to_rfc3339(), id));
        assert_eq!(encode_cursor(at(), id), expected);
    }

    #[test]
    fn malformed_cursors_decode_to_nothing() {
        for bad in ["", "zz", "not hex at all", &hex::encode("no-separator")] {
            assert_eq!(decode_cursor(bad), None, "{bad}");
            assert_eq!(decode_timeline_cursor(bad), None, "{bad}");
        }
        assert_eq!(
            decode_cursor(&hex::encode("2026-07-13T09:30:00+00:00|not-a-uuid")),
            None
        );
        assert_eq!(decode_timeline_cursor(&hex::encode("not-a-time|3")), None);
        assert_eq!(
            decode_timeline_cursor(&hex::encode("2026-07-13T09:30:00+00:00|x")),
            None
        );
    }

    #[test]
    fn an_inbox_cursor_is_not_a_timeline_cursor() {
        assert_eq!(
            decode_timeline_cursor(&encode_cursor(at(), Uuid::from_u128(1))),
            None
        );
    }
}
