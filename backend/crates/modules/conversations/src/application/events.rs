//! The domain events `conversations` publishes to the transactional outbox.
//!
//! Other modules consume these by `event_type` and parse the payload, so the names and the keys
//! below are a contract. They are not consistent (the customer-message event is snake_case, the
//! others camelCase) and must not be "tidied" without moving every consumer with them.

use serde_json::{json, Value};
use uuid::Uuid;

/// An event of the conversations module, ready to be written to the outbox in the transaction that
/// caused it.
#[derive(Debug, Clone, PartialEq)]
pub enum ConversationEvent<'a> {
    /// `conversation.status_changed`
    StatusChanged {
        tenant_id: Uuid,
        conversation_id: Uuid,
        old_status: &'a str,
        new_status: &'a str,
        actor_membership_id: Option<Uuid>,
        origin: &'a str,
    },
    /// `conversation.assignment_changed`. `actor_membership_id` carries the acting *user* when the
    /// assignment was made by one, as it always has; consumers read it as the actor's id.
    AssignmentChanged {
        tenant_id: Uuid,
        conversation_id: Uuid,
        old_membership_id: Option<Uuid>,
        new_membership_id: Option<Uuid>,
        actor_membership_id: Option<Uuid>,
        origin: &'a str,
    },
    /// `conversation.customer_message`
    CustomerMessage {
        tenant_id: Uuid,
        conversation_id: Uuid,
        message_id: Uuid,
        channel: &'a str,
    },
    /// `ai.tool_decision`
    ToolDecision {
        tenant_id: Uuid,
        conversation_id: Uuid,
        tool_request_id: Uuid,
        outcome: &'a str,
    },
    /// `whatsapp.outbound_message`
    WhatsappOutbound {
        tenant_id: Uuid,
        conversation_id: Uuid,
        message_id: Uuid,
    },
    /// `notification.requested`: tells a member a conversation was assigned to them.
    AssigneeNotification {
        tenant_id: Uuid,
        conversation_id: Uuid,
        actor_membership_id: Option<Uuid>,
        target_membership_id: Uuid,
    },
}

impl ConversationEvent<'_> {
    pub fn tenant_id(&self) -> Uuid {
        match self {
            Self::StatusChanged { tenant_id, .. }
            | Self::AssignmentChanged { tenant_id, .. }
            | Self::CustomerMessage { tenant_id, .. }
            | Self::ToolDecision { tenant_id, .. }
            | Self::WhatsappOutbound { tenant_id, .. }
            | Self::AssigneeNotification { tenant_id, .. } => *tenant_id,
        }
    }

    /// The conversation every event is about; it is the outbox aggregate.
    pub fn conversation_id(&self) -> Uuid {
        match self {
            Self::StatusChanged {
                conversation_id, ..
            }
            | Self::AssignmentChanged {
                conversation_id, ..
            }
            | Self::CustomerMessage {
                conversation_id, ..
            }
            | Self::ToolDecision {
                conversation_id, ..
            }
            | Self::WhatsappOutbound {
                conversation_id, ..
            }
            | Self::AssigneeNotification {
                conversation_id, ..
            } => *conversation_id,
        }
    }

    pub fn aggregate_type(&self) -> &'static str {
        match self {
            Self::AssigneeNotification { .. } => "notification",
            _ => "conversation",
        }
    }

    pub fn event_type(&self) -> &'static str {
        match self {
            Self::StatusChanged { .. } => "conversation.status_changed",
            Self::AssignmentChanged { .. } => "conversation.assignment_changed",
            Self::CustomerMessage { .. } => "conversation.customer_message",
            Self::ToolDecision { .. } => "ai.tool_decision",
            Self::WhatsappOutbound { .. } => "whatsapp.outbound_message",
            Self::AssigneeNotification { .. } => "notification.requested",
        }
    }

    pub fn payload(&self) -> Value {
        match self {
            Self::StatusChanged {
                tenant_id,
                conversation_id,
                old_status,
                new_status,
                actor_membership_id,
                origin,
            } => json!({
                "tenantId": tenant_id,
                "conversationId": conversation_id,
                "oldStatus": old_status,
                "newStatus": new_status,
                "actorMembershipId": actor_membership_id,
                "origin": origin,
            }),
            Self::AssignmentChanged {
                tenant_id,
                conversation_id,
                old_membership_id,
                new_membership_id,
                actor_membership_id,
                origin,
            } => json!({
                "tenantId": tenant_id,
                "conversationId": conversation_id,
                "oldMembershipId": old_membership_id,
                "newMembershipId": new_membership_id,
                "actorMembershipId": actor_membership_id,
                "origin": origin,
            }),
            Self::CustomerMessage {
                conversation_id,
                message_id,
                channel,
                ..
            } => json!({
                "conversation_id": conversation_id,
                "message_id": message_id,
                "channel": channel,
            }),
            Self::ToolDecision {
                tenant_id,
                conversation_id,
                tool_request_id,
                outcome,
            } => json!({
                "tenantId": tenant_id,
                "conversationId": conversation_id,
                "toolRequestId": tool_request_id,
                "outcome": outcome,
            }),
            Self::WhatsappOutbound {
                conversation_id,
                message_id,
                ..
            } => json!({
                "conversationId": conversation_id,
                "messageId": message_id,
            }),
            Self::AssigneeNotification {
                tenant_id,
                conversation_id,
                actor_membership_id,
                target_membership_id,
            } => json!({
                "tenantId": tenant_id,
                "kind": "conversation.assigned",
                "subjectType": "conversation",
                "subjectId": conversation_id,
                "actorMembershipId": actor_membership_id,
                "targetMembershipId": target_membership_id,
                "dedupeKey": format!("assigned:{conversation_id}:{target_membership_id}"),
                "title": "Conversation assigned",
                "body": null,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn status_changed_keeps_its_camel_case_contract() {
        let event = ConversationEvent::StatusChanged {
            tenant_id: id(1),
            conversation_id: id(2),
            old_status: "open",
            new_status: "resolved",
            actor_membership_id: Some(id(3)),
            origin: "conversations",
        };
        assert_eq!(event.event_type(), "conversation.status_changed");
        assert_eq!(event.aggregate_type(), "conversation");
        assert_eq!(event.conversation_id(), id(2));
        assert_eq!(event.tenant_id(), id(1));
        assert_eq!(
            event.payload(),
            json!({
                "tenantId": id(1),
                "conversationId": id(2),
                "oldStatus": "open",
                "newStatus": "resolved",
                "actorMembershipId": id(3),
                "origin": "conversations",
            })
        );
    }

    #[test]
    fn status_changed_without_an_actor_has_a_null_actor() {
        let event = ConversationEvent::StatusChanged {
            tenant_id: id(1),
            conversation_id: id(2),
            old_status: "open",
            new_status: "closed",
            actor_membership_id: None,
            origin: "conversations",
        };
        assert!(event.payload()["actorMembershipId"].is_null());
    }

    #[test]
    fn assignment_changed_keeps_its_camel_case_contract() {
        let event = ConversationEvent::AssignmentChanged {
            tenant_id: id(1),
            conversation_id: id(2),
            old_membership_id: None,
            new_membership_id: Some(id(4)),
            actor_membership_id: Some(id(5)),
            origin: "escalations",
        };
        assert_eq!(event.event_type(), "conversation.assignment_changed");
        assert_eq!(event.aggregate_type(), "conversation");
        assert_eq!(
            event.payload(),
            json!({
                "tenantId": id(1),
                "conversationId": id(2),
                "oldMembershipId": null,
                "newMembershipId": id(4),
                "actorMembershipId": id(5),
                "origin": "escalations",
            })
        );
    }

    #[test]
    fn the_customer_message_event_is_snake_case_and_has_no_tenant_key() {
        let event = ConversationEvent::CustomerMessage {
            tenant_id: id(1),
            conversation_id: id(2),
            message_id: id(3),
            channel: "web_chat",
        };
        assert_eq!(event.event_type(), "conversation.customer_message");
        assert_eq!(
            event.payload(),
            json!({
                "conversation_id": id(2),
                "message_id": id(3),
                "channel": "web_chat",
            })
        );
    }

    #[test]
    fn the_tool_decision_event_is_published_under_the_ai_namespace() {
        let event = ConversationEvent::ToolDecision {
            tenant_id: id(1),
            conversation_id: id(2),
            tool_request_id: id(3),
            outcome: "approved",
        };
        assert_eq!(event.event_type(), "ai.tool_decision");
        assert_eq!(event.aggregate_type(), "conversation");
        assert_eq!(
            event.payload(),
            json!({
                "tenantId": id(1),
                "conversationId": id(2),
                "toolRequestId": id(3),
                "outcome": "approved",
            })
        );
    }

    #[test]
    fn the_whatsapp_outbound_event_names_the_conversation_and_message() {
        let event = ConversationEvent::WhatsappOutbound {
            tenant_id: id(1),
            conversation_id: id(2),
            message_id: id(3),
        };
        assert_eq!(event.event_type(), "whatsapp.outbound_message");
        assert_eq!(
            event.payload(),
            json!({ "conversationId": id(2), "messageId": id(3) })
        );
    }

    #[test]
    fn the_assignee_notification_is_a_notification_aggregate_with_a_dedupe_key() {
        let event = ConversationEvent::AssigneeNotification {
            tenant_id: id(1),
            conversation_id: id(2),
            actor_membership_id: None,
            target_membership_id: id(6),
        };
        assert_eq!(event.event_type(), "notification.requested");
        assert_eq!(event.aggregate_type(), "notification");
        assert_eq!(event.conversation_id(), id(2));
        assert_eq!(
            event.payload(),
            json!({
                "tenantId": id(1),
                "kind": "conversation.assigned",
                "subjectType": "conversation",
                "subjectId": id(2),
                "actorMembershipId": null,
                "targetMembershipId": id(6),
                "dedupeKey": format!("assigned:{}:{}", id(2), id(6)),
                "title": "Conversation assigned",
                "body": null,
            })
        );
    }
}
