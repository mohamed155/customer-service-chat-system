//! A conversation's lifecycle status and the rules that change it.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::message::MessageKind;

/// Conversation lifecycle status.
///
/// Variants serialize to their snake_case wire names (`open`, `pending`,
/// `resolved`, `closed`) via the explicit serde rename; the OpenAPI schema
/// surfaces the same names so the documented vocabulary matches the wire
/// format (FR-006 / data-model.md).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationStatus {
    Open,
    Pending,
    Resolved,
    Closed,
}

impl ConversationStatus {
    /// The name the status has in the database and on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Pending => "pending",
            Self::Resolved => "resolved",
            Self::Closed => "closed",
        }
    }

    /// The status a stored name stands for, or `None` when the name is not a status.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "open" => Some(Self::Open),
            "pending" => Some(Self::Pending),
            "resolved" => Some(Self::Resolved),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }

    /// The status of a stored row. A name that is not a status reads as open, as it always has.
    pub fn from_stored(name: &str) -> Self {
        Self::parse(name).unwrap_or(Self::Open)
    }

    /// Resolved and closed conversations are finished; a customer or an agent writing to one
    /// reopens it.
    pub fn is_finished(self) -> bool {
        matches!(self, Self::Resolved | Self::Closed)
    }
}

/// The status a conversation moves to when a message of `kind` is added while it is `current`, or
/// `None` when it stays as it is. A customer message, an agent reply or an AI answer reopens a
/// finished conversation; a note or an automatic acknowledgement does not.
pub fn status_after_message(kind: MessageKind, current: &str) -> Option<ConversationStatus> {
    let finished = ConversationStatus::parse(current).is_some_and(ConversationStatus::is_finished);
    (kind.reopens_conversation() && finished).then_some(ConversationStatus::Open)
}

/// A status change that really changes something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusChange<'a> {
    pub from: &'a str,
    pub to: ConversationStatus,
}

/// What asking for `requested` does to a conversation stored as `prior`: nothing when it is
/// already there.
pub fn status_change(prior: &str, requested: ConversationStatus) -> Option<StatusChange<'_>> {
    (requested.as_str() != prior).then_some(StatusChange {
        from: prior,
        to: requested,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_unknown_names_are_not_statuses() {
        for status in [
            ConversationStatus::Open,
            ConversationStatus::Pending,
            ConversationStatus::Resolved,
            ConversationStatus::Closed,
        ] {
            assert_eq!(ConversationStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(ConversationStatus::parse("escalated"), None);
        assert_eq!(ConversationStatus::parse("OPEN"), None);
    }

    #[test]
    fn a_stored_name_that_is_not_a_status_reads_as_open() {
        assert_eq!(
            ConversationStatus::from_stored("garbage"),
            ConversationStatus::Open
        );
        assert_eq!(
            ConversationStatus::from_stored("closed"),
            ConversationStatus::Closed
        );
    }

    #[test]
    fn only_resolved_and_closed_are_finished() {
        assert!(ConversationStatus::Resolved.is_finished());
        assert!(ConversationStatus::Closed.is_finished());
        assert!(!ConversationStatus::Open.is_finished());
        assert!(!ConversationStatus::Pending.is_finished());
    }

    #[test]
    fn customer_reply_and_ai_messages_reopen_a_finished_conversation() {
        for kind in [MessageKind::Customer, MessageKind::Reply, MessageKind::Ai] {
            for current in ["resolved", "closed"] {
                assert_eq!(
                    status_after_message(kind, current),
                    Some(ConversationStatus::Open),
                    "{kind:?} on {current}"
                );
            }
        }
    }

    #[test]
    fn notes_and_system_messages_never_reopen() {
        for kind in [MessageKind::Note, MessageKind::System] {
            for current in ["resolved", "closed"] {
                assert_eq!(status_after_message(kind, current), None, "{kind:?}");
            }
        }
    }

    #[test]
    fn a_live_conversation_stays_as_it_is_whatever_is_added() {
        for kind in [
            MessageKind::Customer,
            MessageKind::Reply,
            MessageKind::Ai,
            MessageKind::Note,
            MessageKind::System,
        ] {
            for current in ["open", "pending", "something-else"] {
                assert_eq!(status_after_message(kind, current), None);
            }
        }
    }

    #[test]
    fn asking_for_the_status_a_conversation_has_changes_nothing() {
        assert_eq!(status_change("open", ConversationStatus::Open), None);
        assert_eq!(
            status_change("open", ConversationStatus::Resolved),
            Some(StatusChange {
                from: "open",
                to: ConversationStatus::Resolved
            })
        );
    }

    #[test]
    fn a_stored_name_that_is_not_a_status_is_changed_by_any_request() {
        assert!(status_change("escalated", ConversationStatus::Open).is_some());
    }

    #[test]
    fn wire_names_are_snake_case() {
        assert_eq!(
            serde_json::to_value(ConversationStatus::Resolved).unwrap(),
            serde_json::json!("resolved")
        );
        assert!(serde_json::from_value::<ConversationStatus>(serde_json::json!("all")).is_err());
    }
}
