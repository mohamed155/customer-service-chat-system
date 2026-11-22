//! The channels a conversation can happen on.

/// Every channel a conversation may be opened on (the `conversations.channel` constraint, plus the
/// embedded widget).
pub const CHANNELS: [&str; 6] = [
    "email", "phone", "web_chat", "whatsapp", "telegram", "widget",
];

/// Whether `channel` is one conversations are opened or filtered on.
pub fn is_known_channel(channel: &str) -> bool {
    CHANNELS.contains(&channel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_channel_is_known() {
        for channel in CHANNELS {
            assert!(is_known_channel(channel));
        }
    }

    #[test]
    fn the_embedded_widget_is_a_channel() {
        assert!(is_known_channel("widget"));
    }

    #[test]
    fn anything_else_is_not() {
        assert!(!is_known_channel("fax"));
        assert!(!is_known_channel(""));
        assert!(!is_known_channel("Email"));
    }
}
