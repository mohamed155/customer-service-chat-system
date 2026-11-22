//! What a message is, who sent it, and which messages are acceptable.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// The longest message body, counted in bytes as the database constraint counts it.
pub const MAX_BODY_LEN: usize = 10_000;

/// Message kind — the sender role that produced a given message row.
///
/// Variants serialize to their snake_case wire names (`customer`, `reply`,
/// `note`) so the documented vocabulary matches the wire format
/// (FR-006 / data-model.md).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Customer,
    #[serde(rename = "ai")]
    Ai,
    #[serde(rename = "system")]
    System,
    Reply,
    Note,
}

/// Who a message shows as its sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SenderRole {
    Customer,
    AiAgent,
    System,
    Agent,
}

impl MessageKind {
    /// The name the kind has in the database and on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Customer => "customer",
            Self::Ai => "ai",
            Self::System => "system",
            Self::Reply => "reply",
            Self::Note => "note",
        }
    }

    /// The kind a stored name stands for, or `None` when the name is not a kind.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "customer" => Some(Self::Customer),
            "ai" => Some(Self::Ai),
            "system" => Some(Self::System),
            "reply" => Some(Self::Reply),
            "note" => Some(Self::Note),
            _ => None,
        }
    }

    /// The kind of a stored row. A name that is not a kind reads as a reply, as it always has.
    pub fn from_stored(name: &str) -> Self {
        Self::parse(name).unwrap_or(Self::Reply)
    }

    /// A customer message, an agent reply and an AI answer reopen a finished conversation. A note
    /// and an automatic acknowledgement do not.
    pub fn reopens_conversation(self) -> bool {
        matches!(self, Self::Customer | Self::Reply | Self::Ai)
    }

    /// Who the message shows as its sender.
    pub fn sender_role(self) -> SenderRole {
        match self {
            Self::Customer => SenderRole::Customer,
            Self::Ai => SenderRole::AiAgent,
            Self::System => SenderRole::System,
            Self::Reply | Self::Note => SenderRole::Agent,
        }
    }
}

/// The staff member a message is attributed to, as stored on the message row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attribution {
    /// The member who wrote the message (replies and notes).
    pub sender_membership_id: Option<Uuid>,
    /// The member who logged a message on the customer's behalf (customer messages).
    pub logged_by_membership_id: Option<Uuid>,
}

/// Attributes a message that the staff member `member` adds through the dashboard. A customer
/// message is logged by the member; AI and system messages have no staff author; a reply or note
/// is the member's own.
pub fn attribute_staff_message(kind: MessageKind, member: Uuid) -> Attribution {
    match kind {
        MessageKind::Customer => Attribution {
            sender_membership_id: None,
            logged_by_membership_id: Some(member),
        },
        MessageKind::Ai | MessageKind::System => Attribution {
            sender_membership_id: None,
            logged_by_membership_id: None,
        },
        MessageKind::Reply | MessageKind::Note => Attribution {
            sender_membership_id: Some(member),
            logged_by_membership_id: None,
        },
    }
}

/// Why a message body is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyError {
    /// Empty once trimmed.
    Empty,
    /// Longer than [`MAX_BODY_LEN`] bytes.
    TooLong,
}

/// The body to store: trimmed, and between 1 and [`MAX_BODY_LEN`] bytes.
pub fn validate_body(raw: &str) -> Result<String, BodyError> {
    let body = raw.trim();
    if body.is_empty() {
        return Err(BodyError::Empty);
    }
    if body.len() > MAX_BODY_LEN {
        return Err(BodyError::TooLong);
    }
    Ok(body.to_owned())
}

/// The band a model's confidence score falls in. Intentionally the same cut-offs as
/// `ai::confidence`, which this crate cannot depend on.
pub fn confidence_band(score: f32) -> &'static str {
    if score >= 0.70 {
        "high"
    } else if score >= 0.40 {
        "medium"
    } else {
        "low"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [MessageKind; 5] = [
        MessageKind::Customer,
        MessageKind::Ai,
        MessageKind::System,
        MessageKind::Reply,
        MessageKind::Note,
    ];

    #[test]
    fn names_round_trip_and_unknown_names_are_not_kinds() {
        for kind in ALL {
            assert_eq!(MessageKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(MessageKind::parse("whisper"), None);
    }

    #[test]
    fn a_stored_name_that_is_not_a_kind_reads_as_a_reply() {
        assert_eq!(MessageKind::from_stored("whisper"), MessageKind::Reply);
        assert_eq!(MessageKind::from_stored("note"), MessageKind::Note);
    }

    #[test]
    fn wire_names_match_the_stored_names() {
        for kind in ALL {
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                serde_json::json!(kind.as_str())
            );
        }
    }

    #[test]
    fn only_customer_reply_and_ai_messages_reopen() {
        let reopening: Vec<_> = ALL
            .into_iter()
            .filter(|k| k.reopens_conversation())
            .collect();
        assert_eq!(
            reopening,
            vec![MessageKind::Customer, MessageKind::Ai, MessageKind::Reply]
        );
    }

    #[test]
    fn each_kind_shows_its_own_sender() {
        assert_eq!(MessageKind::Customer.sender_role(), SenderRole::Customer);
        assert_eq!(MessageKind::Ai.sender_role(), SenderRole::AiAgent);
        assert_eq!(MessageKind::System.sender_role(), SenderRole::System);
        assert_eq!(MessageKind::Reply.sender_role(), SenderRole::Agent);
        assert_eq!(MessageKind::Note.sender_role(), SenderRole::Agent);
    }

    #[test]
    fn a_staff_member_logs_customer_messages_and_writes_replies_and_notes() {
        let member = Uuid::from_u128(7);
        assert_eq!(
            attribute_staff_message(MessageKind::Customer, member),
            Attribution {
                sender_membership_id: None,
                logged_by_membership_id: Some(member)
            }
        );
        for kind in [MessageKind::Reply, MessageKind::Note] {
            assert_eq!(
                attribute_staff_message(kind, member),
                Attribution {
                    sender_membership_id: Some(member),
                    logged_by_membership_id: None
                }
            );
        }
    }

    #[test]
    fn ai_and_system_messages_have_no_staff_author() {
        let member = Uuid::from_u128(7);
        for kind in [MessageKind::Ai, MessageKind::System] {
            assert_eq!(
                attribute_staff_message(kind, member),
                Attribution {
                    sender_membership_id: None,
                    logged_by_membership_id: None
                }
            );
        }
    }

    #[test]
    fn a_body_is_trimmed_before_it_is_stored() {
        assert_eq!(validate_body("  hello \n").as_deref(), Ok("hello"));
    }

    #[test]
    fn an_empty_or_blank_body_is_refused() {
        assert_eq!(validate_body(""), Err(BodyError::Empty));
        assert_eq!(validate_body(" \t\n "), Err(BodyError::Empty));
    }

    #[test]
    fn the_length_limit_counts_bytes_after_trimming() {
        let at_limit = "a".repeat(MAX_BODY_LEN);
        assert!(validate_body(&at_limit).is_ok());
        assert!(validate_body(&format!("  {at_limit}  ")).is_ok());
        assert_eq!(
            validate_body(&"a".repeat(MAX_BODY_LEN + 1)),
            Err(BodyError::TooLong)
        );
        // Two bytes per character: 5001 characters are 10002 bytes.
        assert_eq!(
            validate_body(&"é".repeat(MAX_BODY_LEN / 2 + 1)),
            Err(BodyError::TooLong)
        );
    }

    #[test]
    fn confidence_bands_cut_at_forty_and_seventy_percent() {
        assert_eq!(confidence_band(1.0), "high");
        assert_eq!(confidence_band(0.70), "high");
        assert_eq!(confidence_band(0.69), "medium");
        assert_eq!(confidence_band(0.40), "medium");
        assert_eq!(confidence_band(0.39), "low");
        assert_eq!(confidence_band(0.0), "low");
    }
}
