//! The conversation use cases: what happens when a message is added, a conversation is assigned,
//! changed or opened, and how the inbox, the detail and the timeline are put together.
//!
//! Each use case runs on one [`ConversationUnit`], the caller's transaction. It decides with the
//! rules of [`crate::domain`] and does its reads and writes through the ports, so it runs in the
//! unit tests against an in-memory store.

use std::collections::HashMap;

use uuid::Uuid;

use super::audit::AuditEvent;
use super::error::{ConversationError, Outcome, StoreResult};
use super::events::ConversationEvent;
use super::model::{
    AddMessageResponse, Assignee, Conversation, ConversationDetail, ConversationStatusRef,
    CustomerRef, LastMessagePreview, Message, Participant, TenantFeedbackDto, WidgetInstanceRef,
};
use super::ports::{ConversationUnit, MemberDirectory, MessageEnrichment};
use super::records::{DetailRecord, InboxRecord, Member, TimelineRecord};
use crate::domain::{
    assignee_is_told, assignment_change, confidence_band, decode_cursor, decode_timeline_cursor,
    encode_cursor, encode_timeline_cursor, origin_tells_assignee, status_after_message,
    status_change, ConversationStatus, InboxFilter, MessageKind, SenderRole,
};

/// Who performs a conversation write: a staff user, or a widget visitor.
#[derive(Debug, Clone, Copy)]
pub enum ConversationActor {
    Staff { user_id: Uuid, membership_id: Uuid },
    Visitor { customer_id: Uuid },
}

impl ConversationActor {
    fn user_id(self) -> Option<Uuid> {
        match self {
            Self::Staff { user_id, .. } => Some(user_id),
            Self::Visitor { .. } => None,
        }
    }

    fn membership_id(self) -> Option<Uuid> {
        match self {
            Self::Staff { membership_id, .. } => Some(membership_id),
            Self::Visitor { .. } => None,
        }
    }
}

/// One page of the inbox.
#[derive(Debug, Clone)]
pub struct InboxPage {
    pub conversations: Vec<Conversation>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

/// One page of a conversation's messages, newest first.
#[derive(Debug, Clone)]
pub struct TimelinePage {
    pub messages: Vec<Message>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

/// The profiles of `ids` (duplicates and absent ids are fine). Who an assignee, a sender or a
/// logger is belongs to `tenancy` and `identity`, not to this module.
async fn profiles_of<U: MemberDirectory + ?Sized>(
    unit: &mut U,
    tenant_id: Uuid,
    mut ids: Vec<Uuid>,
) -> StoreResult<HashMap<Uuid, Member>> {
    ids.sort_unstable();
    ids.dedup();
    unit.member_profiles(tenant_id, &ids).await
}

fn assignee_of_row(
    membership_id: Option<Uuid>,
    people: &HashMap<Uuid, Member>,
) -> Option<Assignee> {
    membership_id.map(|membership_id| {
        let profile = people.get(&membership_id);
        Assignee {
            membership_id,
            display_name: profile.map(|p| p.display_name.clone()).unwrap_or_default(),
            active: profile.is_some_and(|p| p.active),
        }
    })
}

fn last_message_of(kind: Option<String>, preview: Option<String>) -> Option<LastMessagePreview> {
    kind.map(|kind| LastMessagePreview {
        kind: MessageKind::from_stored(&kind),
        preview: preview.unwrap_or_default(),
    })
}

fn widget_of(id: Option<Uuid>, name: Option<String>) -> Option<WidgetInstanceRef> {
    id.map(|id| WidgetInstanceRef {
        id,
        name: name.unwrap_or_default(),
    })
}

fn inbox_row_to_conversation(row: InboxRecord, people: &HashMap<Uuid, Member>) -> Conversation {
    Conversation {
        id: row.id,
        customer: CustomerRef {
            id: row.customer_id,
            display_name: row.customer_display_name,
        },
        channel: row.channel,
        status: ConversationStatus::from_stored(&row.status),
        assignee: assignee_of_row(row.assigned_membership_id, people),
        last_message: last_message_of(row.last_message_kind, row.last_message_preview),
        last_activity_at: row.last_activity_at,
        created_at: row.created_at,
        ai_handling: row.ai_handling,
        awaiting_ai_decision: false,
        widget_instance: widget_of(row.widget_instance_id, row.widget_instance_name),
        rating: row.feedback_rating,
    }
}

/// One page of the inbox for `filter` (with `assignee=me` already resolved), newest activity first.
/// `cursor` is the opaque cursor of the previous page; one that does not decode starts from the top.
pub async fn inbox<U: ConversationUnit>(
    unit: &mut U,
    tenant_id: Uuid,
    filter: &InboxFilter,
    cursor: Option<&str>,
    limit: i64,
) -> Outcome<InboxPage> {
    let after = cursor.and_then(decode_cursor);
    // Over-fetch by one to learn whether there is another page.
    let mut rows = unit.inbox_rows(tenant_id, filter, after, limit + 1).await?;
    let has_more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let assignee_ids: Vec<Uuid> = rows
        .iter()
        .filter_map(|row| row.assigned_membership_id)
        .collect();
    let people = profiles_of(unit, tenant_id, assignee_ids).await?;
    let conversations: Vec<Conversation> = rows
        .into_iter()
        .map(|row| inbox_row_to_conversation(row, &people))
        .collect();
    let next_cursor = conversations
        .last()
        .filter(|_| has_more)
        .map(|last| encode_cursor(last.last_activity_at, last.id));
    Ok(InboxPage {
        conversations,
        has_more,
        next_cursor,
    })
}

/// The customer plus every distinct agent who has sent or logged a message, agents ordered by user
/// and membership.
async fn participants<U: ConversationUnit>(
    unit: &mut U,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> Outcome<Vec<Participant>> {
    let customer = unit
        .participant_customer(tenant_id, conversation_id)
        .await?;
    let membership_ids = unit
        .participating_memberships(tenant_id, conversation_id)
        .await?;
    let mut agents: Vec<Member> = unit
        .member_profiles(tenant_id, &membership_ids)
        .await?
        .into_values()
        .collect();
    agents.sort_by_key(|member| (member.user_id, member.membership_id));

    let mut all = Vec::with_capacity(1 + agents.len());
    if let Some((id, display_name)) = customer {
        all.push(Participant {
            participant_type: "customer".into(),
            id: Some(id),
            membership_id: None,
            display_name,
            active: None,
        });
    }
    all.extend(agents.into_iter().map(|member| Participant {
        participant_type: "agent".into(),
        id: Some(member.user_id),
        membership_id: Some(member.membership_id),
        display_name: member.display_name,
        active: Some(member.active),
    }));
    Ok(all)
}

/// A conversation's full detail: customer, assignee, latest message, feedback and participants.
/// `None` when the conversation is missing or deleted.
pub async fn detail<U: ConversationUnit>(
    unit: &mut U,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> Outcome<Option<ConversationDetail>> {
    let Some(row) = unit.detail_row(tenant_id, conversation_id).await? else {
        return Ok(None);
    };
    let people = profiles_of(
        unit,
        tenant_id,
        row.assigned_membership_id.into_iter().collect(),
    )
    .await?;
    let participants = participants(unit, tenant_id, conversation_id).await?;
    Ok(Some(detail_from(row, &people, participants)))
}

fn detail_from(
    row: DetailRecord,
    people: &HashMap<Uuid, Member>,
    participants: Vec<Participant>,
) -> ConversationDetail {
    // A rating without a submission time cannot be shown; the schema writes both together.
    let feedback =
        row.feedback_rating
            .zip(row.feedback_submitted_at)
            .map(|(rating, submitted_at)| TenantFeedbackDto {
                rating,
                comment: row.feedback_comment,
                submitted_at,
            });
    ConversationDetail {
        id: row.id,
        customer: CustomerRef {
            id: row.customer_id,
            display_name: row.customer_display_name,
        },
        channel: row.channel,
        status: ConversationStatus::from_stored(&row.status),
        assignee: assignee_of_row(row.assigned_membership_id, people),
        last_message: last_message_of(row.last_message_kind, row.last_message_preview),
        last_activity_at: row.last_activity_at,
        created_at: row.created_at,
        participants,
        ai_handling: row.ai_handling,
        awaiting_ai_decision: false,
        widget_instance: widget_of(row.widget_instance_id, row.widget_instance_name),
        feedback,
    }
}

/// The sender a non-agent message shows, or `None` for a message written by a member.
fn fixed_sender(role: SenderRole, customer: Option<(Option<Uuid>, String)>) -> Option<Participant> {
    match role {
        SenderRole::Customer => {
            let (id, display_name) = customer.unwrap_or_default();
            Some(Participant {
                participant_type: "customer".into(),
                id,
                membership_id: None,
                display_name,
                active: None,
            })
        }
        SenderRole::AiAgent => Some(Participant {
            participant_type: "ai_agent".into(),
            id: None,
            membership_id: None,
            display_name: "<agent name>".into(),
            active: None,
        }),
        SenderRole::System => Some(Participant {
            participant_type: "system".into(),
            id: None,
            membership_id: None,
            display_name: "Automated reply".into(),
            active: None,
        }),
        SenderRole::Agent => None,
    }
}

fn timeline_message(row: TimelineRecord, people: &HashMap<Uuid, Member>) -> Message {
    let kind = MessageKind::from_stored(&row.kind);
    // A sender who is no longer a member keeps an empty name and shows as inactive.
    let sender_profile = row.sender_membership_id.and_then(|id| people.get(&id));
    let sender = fixed_sender(
        kind.sender_role(),
        Some((
            row.customer_id,
            row.customer_display_name.unwrap_or_default(),
        )),
    )
    .unwrap_or_else(|| Participant {
        participant_type: "agent".into(),
        id: sender_profile.map(|p| p.user_id),
        membership_id: row.sender_membership_id,
        display_name: sender_profile
            .map(|p| p.display_name.clone())
            .unwrap_or_default(),
        active: Some(sender_profile.is_some_and(|p| p.active)),
    });
    let logged_by = assignee_of_row(row.logged_by_membership_id, people);
    let confidence = row
        .ai_confidence_score
        .map(|score| super::model::ConfidenceView {
            score,
            band: confidence_band(score).to_string(),
        });
    Message {
        id: row.id,
        kind,
        sender,
        logged_by,
        body: row.body,
        created_at: row.created_at,
        citations: Vec::new(),
        confidence,
        attachments: None,
        delivery: None,
    }
}

/// One page of a conversation's messages, newest first, with their citations, attachments and
/// delivery state. `cursor` is the opaque cursor of the previous page; one that does not decode
/// starts from the newest message.
pub async fn timeline<U: ConversationUnit, E: MessageEnrichment + ?Sized>(
    unit: &mut U,
    enrichment: &E,
    tenant_id: Uuid,
    conversation_id: Uuid,
    cursor: Option<&str>,
    limit: i64,
) -> Outcome<TimelinePage> {
    let before = cursor.and_then(decode_timeline_cursor);
    let mut rows = unit
        .timeline_rows(tenant_id, conversation_id, before, limit + 1)
        .await?;
    let has_more = rows.len() > limit as usize;
    let next_cursor = has_more.then(|| {
        let last = &rows[limit as usize - 1];
        encode_timeline_cursor(last.created_at, last.seq)
    });
    rows.truncate(limit as usize);

    // The sender's and the logger's names come from the `tenancy` directory.
    let member_ids: Vec<Uuid> = rows
        .iter()
        .flat_map(|row| [row.sender_membership_id, row.logged_by_membership_id])
        .flatten()
        .collect();
    let people = profiles_of(unit, tenant_id, member_ids).await?;
    let mut messages: Vec<Message> = rows
        .into_iter()
        .map(|row| timeline_message(row, &people))
        .collect();

    if !messages.is_empty() {
        let ids: Vec<Uuid> = messages.iter().map(|m| m.id).collect();
        let mut citations = enrichment.citations(tenant_id, &ids).await?;
        let mut attachments = enrichment.attachments(tenant_id, &ids).await?;
        let mut deliveries = enrichment.deliveries(tenant_id, &ids).await?;
        for message in &mut messages {
            if let Some(found) = citations.remove(&message.id) {
                message.citations = found;
            }
            if let Some(found) = attachments.remove(&message.id) {
                if !found.is_empty() {
                    message.attachments = Some(found);
                }
            }
            if let Some(found) = deliveries.remove(&message.id) {
                message.delivery = Some(found);
            }
        }
    }
    Ok(TimelinePage {
        messages,
        has_more,
        next_cursor,
    })
}

/// Stores a message, marks the conversation active and reopens it when the message calls for it.
/// Returns the message as the dashboard shows it and the conversation's refreshed status.
#[allow(clippy::too_many_arguments)]
pub async fn add_message<U: ConversationUnit>(
    unit: &mut U,
    tenant_id: Uuid,
    conversation_id: Uuid,
    kind: MessageKind,
    body: &str,
    sender_membership_id: Option<Uuid>,
    logged_by_membership_id: Option<Uuid>,
    actor: ConversationActor,
) -> Outcome<AddMessageResponse> {
    let inserted = unit
        .insert_message(
            tenant_id,
            conversation_id,
            kind.as_str(),
            body,
            sender_membership_id,
            logged_by_membership_id,
        )
        .await?;

    let current_status = unit.touch_and_status(tenant_id, conversation_id).await?;
    let final_status = match status_after_message(kind, &current_status) {
        Some(reopened) => {
            let stored = unit.reopen(tenant_id, conversation_id).await?;
            unit.record(
                actor.user_id(),
                tenant_id,
                conversation_id,
                &AuditEvent::StatusChanged {
                    from: &current_status,
                    to: reopened.as_str(),
                    auto: true,
                },
            )
            .await?;
            stored
        }
        None => current_status,
    };
    let last_activity_at = unit.last_activity_at(tenant_id, conversation_id).await?;

    let (customer_id, customer_name) = unit.customer_of(tenant_id, conversation_id).await?;
    let sender = match fixed_sender(kind.sender_role(), Some((Some(customer_id), customer_name))) {
        Some(sender) => sender,
        None => {
            let profile = profiles_of(unit, tenant_id, sender_membership_id.into_iter().collect())
                .await?
                .into_values()
                .next();
            match profile {
                Some(member) => Participant {
                    participant_type: "agent".into(),
                    id: Some(member.user_id),
                    membership_id: sender_membership_id,
                    display_name: member.display_name,
                    active: Some(member.active),
                },
                None => Participant {
                    participant_type: "agent".into(),
                    id: None,
                    membership_id: sender_membership_id,
                    display_name: "Unknown".into(),
                    active: Some(false),
                },
            }
        }
    };

    // Only a customer message is logged by a member. A directory failure leaves it unattributed
    // rather than failing the message that is already stored.
    let logged_by = match (kind, logged_by_membership_id) {
        (MessageKind::Customer, Some(member_id)) => {
            match profiles_of(unit, tenant_id, vec![member_id]).await {
                Ok(mut people) => people.remove(&member_id).map(|member| Assignee {
                    membership_id: member_id,
                    display_name: member.display_name,
                    active: member.active,
                }),
                Err(_) => None,
            }
        }
        _ => None,
    };

    Ok(AddMessageResponse {
        message: Message {
            id: inserted.id,
            kind,
            sender,
            logged_by,
            body: inserted.body,
            created_at: inserted.created_at,
            citations: Vec::new(),
            confidence: None,
            attachments: None,
            delivery: None,
        },
        conversation: ConversationStatusRef {
            status: ConversationStatus::from_stored(&final_status),
            last_activity_at,
        },
    })
}

/// Assigns, or with `None` unassigns, a conversation. Does nothing when it is already assigned that
/// way. The new assignee must be an active member. Records the change in the audit trail (when a
/// user made it) and in the outbox, and tells the new assignee unless they made the change
/// themselves or the escalation routing did.
///
/// Does not lock the row: a caller that needs the assignment to be exclusive holds
/// `SELECT … FOR UPDATE` itself.
pub async fn assign<U: ConversationUnit>(
    unit: &mut U,
    tenant_id: Uuid,
    conversation_id: Uuid,
    assigned_membership_id: Option<Uuid>,
    actor_user_id: Option<Uuid>,
    origin: &str,
) -> Outcome<()> {
    let prior = unit.assignee_of(tenant_id, conversation_id).await?;
    let Some(change) = assignment_change(prior, assigned_membership_id) else {
        return Ok(());
    };

    if let Some(membership_id) = change.to {
        if !unit.is_active_member(tenant_id, membership_id).await? {
            return Err(ConversationError::MembershipNotActive {
                tenant_id,
                membership_id,
            });
        }
    }

    unit.set_assignee(tenant_id, conversation_id, change.to)
        .await?;

    if let Some(user_id) = actor_user_id {
        unit.record(
            Some(user_id),
            tenant_id,
            conversation_id,
            &AuditEvent::AssignmentChanged {
                from_membership_id: change.from,
                to_membership_id: change.to,
            },
        )
        .await?;
    }

    unit.publish(&ConversationEvent::AssignmentChanged {
        tenant_id,
        conversation_id,
        old_membership_id: change.from,
        new_membership_id: change.to,
        actor_membership_id: actor_user_id,
        origin,
    })
    .await?;

    if let (true, Some(target_membership_id)) = (origin_tells_assignee(origin), change.to) {
        let actor_membership_id = match actor_user_id {
            Some(user_id) => unit
                .active_membership_of_user(tenant_id, user_id)
                .await
                .unwrap_or(None),
            None => None,
        };
        if assignee_is_told(actor_membership_id, target_membership_id) {
            // A failure to queue the notification is logged, not returned, as it always has been.
            // It is the caller's transaction that decides what a failed statement means for the
            // assignment written before it.
            if let Err(error) = unit
                .publish(&ConversationEvent::AssigneeNotification {
                    tenant_id,
                    conversation_id,
                    actor_membership_id,
                    target_membership_id,
                })
                .await
            {
                tracing::error!(
                    %error,
                    %conversation_id,
                    "failed to emit notification for conversation assignment"
                );
            }
        }
    }
    Ok(())
}

/// Changes a conversation's status and/or assignee. Locks the row, skips what is already so,
/// audits and publishes what changed, and returns the refreshed detail.
pub async fn patch<U: ConversationUnit>(
    unit: &mut U,
    tenant_id: Uuid,
    conversation_id: Uuid,
    status: Option<ConversationStatus>,
    assigned_membership_id: Option<Option<Uuid>>,
    actor_user_id: Uuid,
) -> Outcome<ConversationDetail> {
    let (prior_status, _prior_assignee) = unit
        .lock_status_and_assignee(tenant_id, conversation_id)
        .await?;

    if let Some(change) = status.and_then(|requested| status_change(&prior_status, requested)) {
        unit.set_status(tenant_id, conversation_id, change.to.as_str())
            .await?;
        unit.record(
            Some(actor_user_id),
            tenant_id,
            conversation_id,
            &AuditEvent::StatusChanged {
                from: change.from,
                to: change.to.as_str(),
                auto: false,
            },
        )
        .await?;
        unit.publish(&ConversationEvent::StatusChanged {
            tenant_id,
            conversation_id,
            old_status: change.from,
            new_status: change.to.as_str(),
            actor_membership_id: None,
            origin: "conversations",
        })
        .await?;
    }

    if let Some(assignment) = assigned_membership_id {
        assign(
            unit,
            tenant_id,
            conversation_id,
            assignment,
            Some(actor_user_id),
            "conversations",
        )
        .await?;
    }

    detail(unit, tenant_id, conversation_id)
        .await?
        .ok_or(ConversationError::NotFound)
}

/// Opens a conversation for a customer of the tenant, with a first reply from the actor when
/// `body` is not empty (the widget opens a conversation before the visitor has typed anything).
/// Returns the new conversation's detail.
pub async fn create<U: ConversationUnit>(
    unit: &mut U,
    tenant_id: Uuid,
    customer_id: Uuid,
    channel: &str,
    body: &str,
    actor: ConversationActor,
    widget_instance_id: Option<Uuid>,
) -> Outcome<ConversationDetail> {
    if !unit.customer_exists(tenant_id, customer_id).await? {
        return Err(ConversationError::CustomerNotFound {
            tenant_id,
            customer_id,
        });
    }

    let conversation_id = unit
        .insert_conversation(tenant_id, customer_id, channel, widget_instance_id)
        .await?;
    if !body.is_empty() {
        unit.insert_reply(tenant_id, conversation_id, body, actor.membership_id())
            .await?;
    }
    unit.touch_activity(tenant_id, conversation_id).await?;
    unit.record(
        actor.user_id(),
        tenant_id,
        conversation_id,
        &AuditEvent::Created {
            customer_id,
            channel,
        },
    )
    .await?;

    detail(unit, tenant_id, conversation_id)
        .await?
        .ok_or(ConversationError::NotFound)
}

#[cfg(test)]
mod tests;
