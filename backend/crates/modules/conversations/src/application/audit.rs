//! The audit records conversations write: which action, and what its details say.

use serde_json::{json, Value};
use uuid::Uuid;

/// The audit resource type of every record below.
pub const RESOURCE_TYPE: &str = "conversation";

/// A conversation change worth an audit record.
#[derive(Debug, Clone, PartialEq)]
pub enum AuditEvent<'a> {
    Created {
        customer_id: Uuid,
        channel: &'a str,
    },
    StatusChanged {
        from: &'a str,
        to: &'a str,
        /// The system moved it (a message reopened the conversation), not a person.
        auto: bool,
    },
    AssignmentChanged {
        from_membership_id: Option<Uuid>,
        to_membership_id: Option<Uuid>,
    },
}

impl AuditEvent<'_> {
    pub fn action(&self) -> &'static str {
        match self {
            Self::Created { .. } => "conversation.created",
            Self::StatusChanged { .. } => "conversation.status_changed",
            Self::AssignmentChanged { .. } => "conversation.assignment_changed",
        }
    }

    pub fn details(&self) -> Value {
        match self {
            Self::Created {
                customer_id,
                channel,
            } => json!({
                "customer_id": customer_id.to_string(),
                "channel": channel,
            }),
            Self::StatusChanged { from, to, auto } => json!({
                "from": from,
                "to": to,
                "auto": auto,
            }),
            Self::AssignmentChanged {
                from_membership_id,
                to_membership_id,
            } => json!({
                "from_membership_id": from_membership_id.map(|id| id.to_string()),
                "to_membership_id": to_membership_id.map(|id| id.to_string()),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_names_its_action_and_holds_customer_id_and_channel() {
        let customer_id = Uuid::new_v4();
        let event = AuditEvent::Created {
            customer_id,
            channel: "web",
        };
        assert_eq!(event.action(), "conversation.created");
        let details = event.details();
        assert_eq!(details["customer_id"], customer_id.to_string());
        assert_eq!(details["channel"], "web");
        assert!(details.is_object());
    }

    #[test]
    fn status_changed_holds_from_to_and_auto() {
        let event = AuditEvent::StatusChanged {
            from: "open",
            to: "closed",
            auto: true,
        };
        assert_eq!(event.action(), "conversation.status_changed");
        let details = event.details();
        assert_eq!(details["from"], "open");
        assert_eq!(details["to"], "closed");
        assert_eq!(details["auto"], true);
    }

    #[test]
    fn status_changed_made_by_a_person_is_not_auto() {
        let event = AuditEvent::StatusChanged {
            from: "open",
            to: "pending",
            auto: false,
        };
        assert_eq!(event.details()["auto"], false);
    }

    #[test]
    fn assignment_changed_with_both_ids() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let event = AuditEvent::AssignmentChanged {
            from_membership_id: Some(from),
            to_membership_id: Some(to),
        };
        assert_eq!(event.action(), "conversation.assignment_changed");
        let details = event.details();
        assert_eq!(details["from_membership_id"], from.to_string());
        assert_eq!(details["to_membership_id"], to.to_string());
    }

    #[test]
    fn assignment_changed_with_null_ids() {
        let details = AuditEvent::AssignmentChanged {
            from_membership_id: None,
            to_membership_id: None,
        }
        .details();
        assert!(details["from_membership_id"].is_null());
        assert!(details["to_membership_id"].is_null());
    }

    #[test]
    fn assignment_changed_with_only_from() {
        let from = Uuid::new_v4();
        let details = AuditEvent::AssignmentChanged {
            from_membership_id: Some(from),
            to_membership_id: None,
        }
        .details();
        assert_eq!(details["from_membership_id"], from.to_string());
        assert!(details["to_membership_id"].is_null());
    }

    #[test]
    fn assignment_changed_with_only_to() {
        let to = Uuid::new_v4();
        let details = AuditEvent::AssignmentChanged {
            from_membership_id: None,
            to_membership_id: Some(to),
        }
        .details();
        assert!(details["from_membership_id"].is_null());
        assert_eq!(details["to_membership_id"], to.to_string());
    }
}
