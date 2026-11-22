//! The use cases against an in-memory store: no database, no HTTP.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;

use super::*;
use crate::application::error::{StoreError, StoreResult};
use crate::application::model::{AttachmentInfo, CitationView, DeliveryInfo};
use crate::application::ports::{
    AuditTrail, ConversationReader, ConversationStore, CustomerDirectory, EventOutbox,
    MemberDirectory, MessageEnrichment,
};
use crate::application::records::{ConversationRecord, InsertedMessage};
use crate::domain::{AssigneeFilter, StatusFilter};

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn at(minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 7, 13, 9, minute, 0).unwrap()
}

const TENANT: u128 = 1;
const CONVERSATION: u128 = 2;
const CUSTOMER: u128 = 3;
const AGENT_USER: u128 = 10;
const AGENT_MEMBER: u128 = 11;
const OTHER_MEMBER: u128 = 21;

#[derive(Debug)]
struct Disconnected;

impl std::fmt::Display for Disconnected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("store disconnected")
    }
}

impl std::error::Error for Disconnected {}

fn down() -> StoreError {
    StoreError::new(Disconnected)
}

/// A store that keeps everything in memory and remembers what was written to it.
#[derive(Default)]
struct FakeUnit {
    status: String,
    assignee: Option<Uuid>,
    active_members: HashSet<Uuid>,
    members: HashMap<Uuid, Member>,
    user_membership: HashMap<Uuid, Uuid>,
    /// Customers by `(tenant, customer)`, so a lookup in the wrong tenant finds nothing.
    customers: HashSet<(Uuid, Uuid)>,
    customer_name: String,
    detail_row: Option<DetailRecord>,
    inbox: Vec<InboxRecord>,
    timeline: Vec<TimelineRecord>,
    participants: Vec<Uuid>,
    // What the store was asked to do.
    asked_inbox_after: Option<Option<(DateTime<Utc>, Uuid)>>,
    asked_inbox_fetch: Option<i64>,
    asked_timeline_fetch: Option<i64>,
    asked_timeline_before: Option<Option<(DateTime<Utc>, i64)>>,
    writes: Vec<String>,
    events: Vec<(&'static str, Value)>,
    audits: Vec<(Option<Uuid>, &'static str, Value)>,
    // Failures to inject.
    fail_notification: bool,
    fail_member_profiles: bool,
}

impl FakeUnit {
    fn with_conversation(status: &str) -> Self {
        let mut unit = Self {
            status: status.to_owned(),
            customer_name: "Ada".to_owned(),
            ..Self::default()
        };
        unit.customers.insert((id(TENANT), id(CUSTOMER)));
        unit.add_member(AGENT_MEMBER, AGENT_USER, "Grace", true);
        unit
    }

    fn add_member(&mut self, membership: u128, user: u128, name: &str, active: bool) {
        self.members.insert(
            id(membership),
            Member {
                user_id: id(user),
                membership_id: id(membership),
                display_name: name.to_owned(),
                active,
            },
        );
        if active {
            self.active_members.insert(id(membership));
            self.user_membership.insert(id(user), id(membership));
        }
    }
}

#[async_trait]
impl ConversationStore for FakeUnit {
    async fn find_live(&mut self, _: Uuid, _: Uuid) -> StoreResult<Option<ConversationRecord>> {
        unreachable!("not used by these tests")
    }

    async fn assignee_of(&mut self, _: Uuid, _: Uuid) -> StoreResult<Option<Uuid>> {
        Ok(self.assignee)
    }

    async fn lock_status_and_assignee(
        &mut self,
        _: Uuid,
        _: Uuid,
    ) -> StoreResult<(String, Option<Uuid>)> {
        self.writes.push("lock".into());
        Ok((self.status.clone(), self.assignee))
    }

    async fn set_status(&mut self, _: Uuid, _: Uuid, status: &str) -> StoreResult<()> {
        self.writes.push(format!("set_status {status}"));
        self.status = status.to_owned();
        Ok(())
    }

    async fn set_assignee(&mut self, _: Uuid, _: Uuid, member: Option<Uuid>) -> StoreResult<()> {
        self.writes.push(format!("set_assignee {member:?}"));
        self.assignee = member;
        Ok(())
    }

    async fn insert_conversation(
        &mut self,
        _: Uuid,
        _: Uuid,
        channel: &str,
        widget: Option<Uuid>,
    ) -> StoreResult<Uuid> {
        self.writes
            .push(format!("insert_conversation {channel} {widget:?}"));
        Ok(id(CONVERSATION))
    }

    async fn insert_reply(
        &mut self,
        _: Uuid,
        _: Uuid,
        body: &str,
        sender: Option<Uuid>,
    ) -> StoreResult<()> {
        self.writes.push(format!("insert_reply {body} {sender:?}"));
        Ok(())
    }

    async fn insert_message(
        &mut self,
        _: Uuid,
        _: Uuid,
        kind: &str,
        body: &str,
        sender: Option<Uuid>,
        logged_by: Option<Uuid>,
    ) -> StoreResult<InsertedMessage> {
        self.writes
            .push(format!("insert_message {kind} {sender:?} {logged_by:?}"));
        Ok(InsertedMessage {
            id: id(100),
            body: body.to_owned(),
            created_at: at(5),
        })
    }

    async fn touch_activity(&mut self, _: Uuid, _: Uuid) -> StoreResult<()> {
        self.writes.push("touch".into());
        Ok(())
    }

    async fn touch_and_status(&mut self, _: Uuid, _: Uuid) -> StoreResult<String> {
        self.writes.push("touch".into());
        Ok(self.status.clone())
    }

    async fn reopen(&mut self, _: Uuid, _: Uuid) -> StoreResult<String> {
        self.writes.push("reopen".into());
        self.status = "open".into();
        Ok(self.status.clone())
    }

    async fn last_activity_at(&mut self, _: Uuid, _: Uuid) -> StoreResult<DateTime<Utc>> {
        Ok(at(6))
    }

    async fn customer_of(&mut self, _: Uuid, _: Uuid) -> StoreResult<(Uuid, String)> {
        Ok((id(CUSTOMER), self.customer_name.clone()))
    }
}

#[async_trait]
impl ConversationReader for FakeUnit {
    async fn inbox_rows(
        &mut self,
        _: Uuid,
        _: &InboxFilter,
        after: Option<(DateTime<Utc>, Uuid)>,
        fetch: i64,
    ) -> StoreResult<Vec<InboxRecord>> {
        self.asked_inbox_after = Some(after);
        self.asked_inbox_fetch = Some(fetch);
        Ok(self.inbox.clone())
    }

    async fn detail_row(&mut self, _: Uuid, _: Uuid) -> StoreResult<Option<DetailRecord>> {
        Ok(self.detail_row.clone())
    }

    async fn timeline_rows(
        &mut self,
        _: Uuid,
        _: Uuid,
        before: Option<(DateTime<Utc>, i64)>,
        fetch: i64,
    ) -> StoreResult<Vec<TimelineRecord>> {
        self.asked_timeline_before = Some(before);
        self.asked_timeline_fetch = Some(fetch);
        Ok(self.timeline.clone())
    }

    async fn participant_customer(
        &mut self,
        _: Uuid,
        _: Uuid,
    ) -> StoreResult<Option<(Uuid, String)>> {
        Ok(Some((id(CUSTOMER), self.customer_name.clone())))
    }

    async fn participating_memberships(&mut self, _: Uuid, _: Uuid) -> StoreResult<Vec<Uuid>> {
        Ok(self.participants.clone())
    }
}

#[async_trait]
impl MemberDirectory for FakeUnit {
    async fn is_active_member(&mut self, _: Uuid, membership_id: Uuid) -> StoreResult<bool> {
        Ok(self.active_members.contains(&membership_id))
    }

    async fn member_profiles(
        &mut self,
        _: Uuid,
        ids: &[Uuid],
    ) -> StoreResult<HashMap<Uuid, Member>> {
        if self.fail_member_profiles {
            return Err(down());
        }
        Ok(ids
            .iter()
            .filter_map(|id| self.members.get(id).map(|m| (*id, m.clone())))
            .collect())
    }

    async fn active_membership_of_user(
        &mut self,
        _: Uuid,
        user_id: Uuid,
    ) -> StoreResult<Option<Uuid>> {
        Ok(self.user_membership.get(&user_id).copied())
    }
}

#[async_trait]
impl CustomerDirectory for FakeUnit {
    async fn customer_exists(&mut self, tenant_id: Uuid, customer_id: Uuid) -> StoreResult<bool> {
        Ok(self.customers.contains(&(tenant_id, customer_id)))
    }
}

#[async_trait]
impl EventOutbox for FakeUnit {
    async fn publish(&mut self, event: &ConversationEvent<'_>) -> StoreResult<()> {
        if self.fail_notification && matches!(event, ConversationEvent::AssigneeNotification { .. })
        {
            return Err(down());
        }
        self.events.push((event.event_type(), event.payload()));
        Ok(())
    }
}

#[async_trait]
impl AuditTrail for FakeUnit {
    async fn record(
        &mut self,
        actor: Option<Uuid>,
        _: Uuid,
        _: Uuid,
        event: &AuditEvent<'_>,
    ) -> StoreResult<()> {
        self.audits.push((actor, event.action(), event.details()));
        Ok(())
    }
}

fn event_types(unit: &FakeUnit) -> Vec<&'static str> {
    unit.events.iter().map(|(kind, _)| *kind).collect()
}

// ---------------------------------------------------------------------------------------------
// assign
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn assigning_the_current_assignee_writes_nothing() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.assignee = Some(id(AGENT_MEMBER));

    assign(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        Some(id(AGENT_MEMBER)),
        Some(id(AGENT_USER)),
        "conversations",
    )
    .await
    .unwrap();

    assert!(unit.writes.is_empty());
    assert!(unit.events.is_empty());
    assert!(unit.audits.is_empty());
}

#[tokio::test]
async fn a_member_who_is_not_active_cannot_be_assigned() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.add_member(OTHER_MEMBER, 20, "Left the team", false);

    let error = assign(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        Some(id(OTHER_MEMBER)),
        Some(id(AGENT_USER)),
        "conversations",
    )
    .await
    .unwrap_err();

    assert!(matches!(
        error,
        ConversationError::MembershipNotActive { membership_id, tenant_id }
            if membership_id == id(OTHER_MEMBER) && tenant_id == id(TENANT)
    ));
    assert_eq!(
        error.to_string(),
        format!(
            "Membership {} is not active in tenant {}",
            id(OTHER_MEMBER),
            id(TENANT)
        )
    );
    assert!(unit.writes.is_empty());
    assert!(unit.events.is_empty());
    assert!(unit.audits.is_empty());
}

#[tokio::test]
async fn an_assignment_is_stored_audited_published_and_told_to_the_assignee() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.add_member(OTHER_MEMBER, 20, "Linus", true);

    assign(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        Some(id(OTHER_MEMBER)),
        Some(id(AGENT_USER)),
        "conversations",
    )
    .await
    .unwrap();

    assert_eq!(unit.assignee, Some(id(OTHER_MEMBER)));
    assert_eq!(unit.audits.len(), 1);
    let (actor, action, details) = &unit.audits[0];
    assert_eq!(*actor, Some(id(AGENT_USER)));
    assert_eq!(*action, "conversation.assignment_changed");
    assert!(details["from_membership_id"].is_null());
    assert_eq!(details["to_membership_id"], id(OTHER_MEMBER).to_string());

    assert_eq!(
        event_types(&unit),
        vec!["conversation.assignment_changed", "notification.requested"]
    );
    // The acting user's id travels in the actor field of the assignment event, as it always has.
    assert_eq!(
        unit.events[0].1["actorMembershipId"],
        Value::String(id(AGENT_USER).to_string())
    );
    assert_eq!(unit.events[0].1["origin"], "conversations");
    let notification = &unit.events[1].1;
    assert_eq!(
        notification["targetMembershipId"],
        Value::String(id(OTHER_MEMBER).to_string())
    );
    assert_eq!(
        notification["actorMembershipId"],
        Value::String(id(AGENT_MEMBER).to_string())
    );
}

#[tokio::test]
async fn assigning_yourself_notifies_nobody() {
    let mut unit = FakeUnit::with_conversation("open");

    assign(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        Some(id(AGENT_MEMBER)),
        Some(id(AGENT_USER)),
        "conversations",
    )
    .await
    .unwrap();

    assert_eq!(event_types(&unit), vec!["conversation.assignment_changed"]);
}

#[tokio::test]
async fn the_routing_engine_assigns_without_a_notification_of_its_own() {
    let mut unit = FakeUnit::with_conversation("open");

    assign(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        Some(id(AGENT_MEMBER)),
        None,
        "escalations",
    )
    .await
    .unwrap();

    assert_eq!(event_types(&unit), vec!["conversation.assignment_changed"]);
    assert_eq!(unit.events[0].1["origin"], "escalations");
    // No user acted, so there is no audit record either.
    assert!(unit.audits.is_empty());
}

#[tokio::test]
async fn an_assignment_without_an_acting_user_is_still_announced_to_the_assignee() {
    let mut unit = FakeUnit::with_conversation("open");

    assign(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        Some(id(AGENT_MEMBER)),
        None,
        "tools",
    )
    .await
    .unwrap();

    assert_eq!(
        event_types(&unit),
        vec!["conversation.assignment_changed", "notification.requested"]
    );
    assert!(unit.events[1].1["actorMembershipId"].is_null());
    assert!(unit.audits.is_empty());
}

#[tokio::test]
async fn unassigning_is_published_and_announced_to_nobody() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.assignee = Some(id(AGENT_MEMBER));

    assign(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        None,
        Some(id(AGENT_USER)),
        "conversations",
    )
    .await
    .unwrap();

    assert_eq!(unit.assignee, None);
    assert_eq!(event_types(&unit), vec!["conversation.assignment_changed"]);
    assert!(unit.events[0].1["newMembershipId"].is_null());
}

#[tokio::test]
async fn a_notification_that_cannot_be_queued_is_logged_and_does_not_fail_the_assignment() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.add_member(OTHER_MEMBER, 20, "Linus", true);
    unit.fail_notification = true;

    assign(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        Some(id(OTHER_MEMBER)),
        Some(id(AGENT_USER)),
        "conversations",
    )
    .await
    .unwrap();

    assert_eq!(unit.assignee, Some(id(OTHER_MEMBER)));
    assert_eq!(event_types(&unit), vec!["conversation.assignment_changed"]);
}

// ---------------------------------------------------------------------------------------------
// patch
// ---------------------------------------------------------------------------------------------

fn detail_record(status: &str, assignee: Option<Uuid>) -> DetailRecord {
    DetailRecord {
        id: id(CONVERSATION),
        customer_id: id(CUSTOMER),
        customer_display_name: "Ada".into(),
        channel: "web_chat".into(),
        status: status.into(),
        assigned_membership_id: assignee,
        last_message_kind: Some("customer".into()),
        last_message_preview: Some("hello".into()),
        last_activity_at: at(1),
        created_at: at(0),
        ai_handling: None,
        widget_instance_id: None,
        widget_instance_name: None,
        feedback_rating: None,
        feedback_comment: None,
        feedback_submitted_at: None,
    }
}

#[tokio::test]
async fn patching_to_the_current_status_changes_nothing_but_still_returns_the_detail() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.detail_row = Some(detail_record("open", None));

    let detail = patch(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        Some(ConversationStatus::Open),
        None,
        id(AGENT_USER),
    )
    .await
    .unwrap();

    assert_eq!(detail.status, ConversationStatus::Open);
    assert_eq!(unit.writes, vec!["lock"]);
    assert!(unit.events.is_empty());
    assert!(unit.audits.is_empty());
}

#[tokio::test]
async fn a_status_change_is_stored_audited_and_published() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.detail_row = Some(detail_record("resolved", None));

    let detail = patch(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        Some(ConversationStatus::Resolved),
        None,
        id(AGENT_USER),
    )
    .await
    .unwrap();

    assert_eq!(detail.status, ConversationStatus::Resolved);
    assert_eq!(unit.writes, vec!["lock", "set_status resolved"]);
    let (actor, action, details) = &unit.audits[0];
    assert_eq!(*actor, Some(id(AGENT_USER)));
    assert_eq!(*action, "conversation.status_changed");
    assert_eq!(details["from"], "open");
    assert_eq!(details["to"], "resolved");
    assert_eq!(details["auto"], false);
    assert_eq!(event_types(&unit), vec!["conversation.status_changed"]);
    assert_eq!(unit.events[0].1["oldStatus"], "open");
    assert_eq!(unit.events[0].1["newStatus"], "resolved");
    assert!(unit.events[0].1["actorMembershipId"].is_null());
    assert_eq!(unit.events[0].1["origin"], "conversations");
}

#[tokio::test]
async fn patching_the_assignee_goes_through_the_assignment_rules() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.detail_row = Some(detail_record("open", Some(id(AGENT_MEMBER))));

    let detail = patch(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        None,
        Some(Some(id(AGENT_MEMBER))),
        id(AGENT_USER),
    )
    .await
    .unwrap();

    assert_eq!(unit.assignee, Some(id(AGENT_MEMBER)));
    assert_eq!(unit.events[0].1["origin"], "conversations");
    assert_eq!(detail.assignee.unwrap().display_name, "Grace");
}

#[tokio::test]
async fn patching_to_an_inactive_assignee_is_refused() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.add_member(OTHER_MEMBER, 20, "Left", false);

    let error = patch(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        None,
        Some(Some(id(OTHER_MEMBER))),
        id(AGENT_USER),
    )
    .await
    .unwrap_err();

    assert!(matches!(
        error,
        ConversationError::MembershipNotActive { .. }
    ));
}

#[tokio::test]
async fn a_conversation_that_vanishes_during_the_patch_is_not_found() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.detail_row = None;

    let error = patch(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        Some(ConversationStatus::Pending),
        None,
        id(AGENT_USER),
    )
    .await
    .unwrap_err();

    assert!(matches!(error, ConversationError::NotFound));
}

// ---------------------------------------------------------------------------------------------
// create
// ---------------------------------------------------------------------------------------------

fn staff() -> ConversationActor {
    ConversationActor::Staff {
        user_id: id(AGENT_USER),
        membership_id: id(AGENT_MEMBER),
    }
}

#[tokio::test]
async fn a_conversation_cannot_be_opened_for_a_customer_of_another_tenant() {
    let mut unit = FakeUnit::with_conversation("open");
    // The customer exists, in a different tenant.
    unit.customers.insert((id(TENANT + 1), id(999)));

    let error = create(
        &mut unit,
        id(TENANT),
        id(999),
        "email",
        "hello",
        staff(),
        None,
    )
    .await
    .unwrap_err();

    assert!(
        matches!(error, ConversationError::CustomerNotFound { customer_id, .. } if customer_id == id(999))
    );
    assert!(error.to_string().contains("not found in tenant"));
    assert!(unit.writes.is_empty());
    assert!(unit.audits.is_empty());
}

#[tokio::test]
async fn staff_open_a_conversation_with_a_first_reply_and_an_audit_record() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.detail_row = Some(detail_record("open", None));

    let detail = create(
        &mut unit,
        id(TENANT),
        id(CUSTOMER),
        "email",
        "hello",
        staff(),
        None,
    )
    .await
    .unwrap();

    assert_eq!(detail.id, id(CONVERSATION));
    assert_eq!(
        unit.writes,
        vec![
            "insert_conversation email None".to_owned(),
            format!("insert_reply hello Some({})", id(AGENT_MEMBER)),
            "touch".to_owned(),
        ]
    );
    let (actor, action, details) = &unit.audits[0];
    assert_eq!(*actor, Some(id(AGENT_USER)));
    assert_eq!(*action, "conversation.created");
    assert_eq!(details["customer_id"], id(CUSTOMER).to_string());
    assert_eq!(details["channel"], "email");
}

#[tokio::test]
async fn a_visitor_opens_a_conversation_with_no_first_message_and_no_acting_user() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.detail_row = Some(detail_record("open", None));
    let widget = id(77);

    create(
        &mut unit,
        id(TENANT),
        id(CUSTOMER),
        "widget",
        "",
        ConversationActor::Visitor {
            customer_id: id(CUSTOMER),
        },
        Some(widget),
    )
    .await
    .unwrap();

    assert_eq!(
        unit.writes,
        vec![
            format!("insert_conversation widget Some({widget})"),
            "touch".to_owned()
        ]
    );
    assert_eq!(unit.audits[0].0, None);
}

// ---------------------------------------------------------------------------------------------
// add_message
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_agent_reply_reopens_a_closed_conversation_and_says_so_in_the_audit_trail() {
    let mut unit = FakeUnit::with_conversation("closed");

    let response = add_message(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        MessageKind::Reply,
        "back again",
        Some(id(AGENT_MEMBER)),
        None,
        staff(),
    )
    .await
    .unwrap();

    assert_eq!(response.conversation.status, ConversationStatus::Open);
    assert_eq!(response.conversation.last_activity_at, at(6));
    assert_eq!(unit.status, "open");
    let (actor, action, details) = &unit.audits[0];
    assert_eq!(*actor, Some(id(AGENT_USER)));
    assert_eq!(*action, "conversation.status_changed");
    assert_eq!(details["from"], "closed");
    assert_eq!(details["to"], "open");
    assert_eq!(details["auto"], true);
    assert_eq!(response.message.sender.participant_type, "agent");
    assert_eq!(response.message.sender.display_name, "Grace");
    assert_eq!(response.message.sender.active, Some(true));
}

#[tokio::test]
async fn a_note_leaves_a_resolved_conversation_resolved() {
    let mut unit = FakeUnit::with_conversation("resolved");

    let response = add_message(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        MessageKind::Note,
        "internal",
        Some(id(AGENT_MEMBER)),
        None,
        staff(),
    )
    .await
    .unwrap();

    assert_eq!(response.conversation.status, ConversationStatus::Resolved);
    assert!(unit.audits.is_empty());
    assert!(!unit.writes.contains(&"reopen".to_owned()));
}

#[tokio::test]
async fn a_visitors_message_reopens_without_an_acting_user() {
    let mut unit = FakeUnit::with_conversation("resolved");

    add_message(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        MessageKind::Customer,
        "one more thing",
        None,
        None,
        ConversationActor::Visitor {
            customer_id: id(CUSTOMER),
        },
    )
    .await
    .unwrap();

    assert_eq!(unit.audits[0].0, None);
    assert_eq!(unit.audits[0].2["auto"], true);
}

#[tokio::test]
async fn a_message_logged_for_a_customer_shows_the_customer_and_who_logged_it() {
    let mut unit = FakeUnit::with_conversation("open");

    let response = add_message(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        MessageKind::Customer,
        "phoned in",
        None,
        Some(id(AGENT_MEMBER)),
        staff(),
    )
    .await
    .unwrap();

    assert_eq!(response.message.sender.participant_type, "customer");
    assert_eq!(response.message.sender.id, Some(id(CUSTOMER)));
    assert_eq!(response.message.sender.display_name, "Ada");
    let logged_by = response.message.logged_by.unwrap();
    assert_eq!(logged_by.membership_id, id(AGENT_MEMBER));
    assert_eq!(logged_by.display_name, "Grace");
    assert!(logged_by.active);
}

#[tokio::test]
async fn only_customer_messages_have_a_logger() {
    let mut unit = FakeUnit::with_conversation("open");

    let response = add_message(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        MessageKind::Reply,
        "hi",
        Some(id(AGENT_MEMBER)),
        Some(id(AGENT_MEMBER)),
        staff(),
    )
    .await
    .unwrap();

    assert!(response.message.logged_by.is_none());
}

#[tokio::test]
async fn a_reply_from_someone_the_directory_does_not_know_shows_as_unknown_and_inactive() {
    let mut unit = FakeUnit::with_conversation("open");

    let response = add_message(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        MessageKind::Reply,
        "hi",
        Some(id(404)),
        None,
        staff(),
    )
    .await
    .unwrap();

    let sender = response.message.sender;
    assert_eq!(sender.display_name, "Unknown");
    assert_eq!(sender.membership_id, Some(id(404)));
    assert_eq!(sender.id, None);
    assert_eq!(sender.active, Some(false));
}

#[tokio::test]
async fn ai_and_automatic_messages_show_their_own_senders() {
    let mut unit = FakeUnit::with_conversation("open");
    for (kind, participant_type, name) in [
        (MessageKind::Ai, "ai_agent", "<agent name>"),
        (MessageKind::System, "system", "Automated reply"),
    ] {
        let response = add_message(
            &mut unit,
            id(TENANT),
            id(CONVERSATION),
            kind,
            "text",
            None,
            None,
            ConversationActor::Visitor {
                customer_id: id(CUSTOMER),
            },
        )
        .await
        .unwrap();
        assert_eq!(response.message.sender.participant_type, participant_type);
        assert_eq!(response.message.sender.display_name, name);
    }
}

#[tokio::test]
async fn a_directory_failure_leaves_the_logger_out_but_keeps_the_message() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.fail_member_profiles = true;

    let response = add_message(
        &mut unit,
        id(TENANT),
        id(CONVERSATION),
        MessageKind::Customer,
        "phoned in",
        None,
        Some(id(AGENT_MEMBER)),
        staff(),
    )
    .await
    .unwrap();

    assert!(response.message.logged_by.is_none());
    assert_eq!(response.message.body, "phoned in");
}

// ---------------------------------------------------------------------------------------------
// detail
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_missing_conversation_has_no_detail() {
    let mut unit = FakeUnit::with_conversation("open");
    assert!(detail(&mut unit, id(TENANT), id(CONVERSATION))
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn the_detail_lists_the_customer_then_the_agents_in_user_order() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.add_member(OTHER_MEMBER, 5, "Linus", true);
    unit.participants = vec![id(AGENT_MEMBER), id(OTHER_MEMBER)];
    unit.detail_row = Some(detail_record("open", Some(id(AGENT_MEMBER))));

    let detail = detail(&mut unit, id(TENANT), id(CONVERSATION))
        .await
        .unwrap()
        .unwrap();

    let names: Vec<_> = detail
        .participants
        .iter()
        .map(|p| (p.participant_type.as_str(), p.display_name.as_str()))
        .collect();
    assert_eq!(
        names,
        vec![("customer", "Ada"), ("agent", "Linus"), ("agent", "Grace")]
    );
    assert_eq!(detail.assignee.unwrap().display_name, "Grace");
    assert_eq!(detail.last_message.unwrap().preview, "hello");
    assert!(!detail.awaiting_ai_decision);
}

#[tokio::test]
async fn an_assignee_who_left_shows_with_an_empty_name_and_inactive() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.detail_row = Some(detail_record("open", Some(id(404))));

    let detail = detail(&mut unit, id(TENANT), id(CONVERSATION))
        .await
        .unwrap()
        .unwrap();

    let assignee = detail.assignee.unwrap();
    assert_eq!(assignee.display_name, "");
    assert!(!assignee.active);
}

#[tokio::test]
async fn feedback_shows_only_with_both_a_rating_and_a_submission_time() {
    let mut unit = FakeUnit::with_conversation("open");
    let mut row = detail_record("open", None);
    row.feedback_rating = Some(5);
    row.feedback_comment = Some("great".into());
    row.feedback_submitted_at = None;
    unit.detail_row = Some(row.clone());
    let without = detail(&mut unit, id(TENANT), id(CONVERSATION))
        .await
        .unwrap()
        .unwrap();
    assert!(without.feedback.is_none());

    row.feedback_submitted_at = Some(at(9));
    unit.detail_row = Some(row);
    let with = detail(&mut unit, id(TENANT), id(CONVERSATION))
        .await
        .unwrap()
        .unwrap();
    let feedback = with.feedback.unwrap();
    assert_eq!(feedback.rating, 5);
    assert_eq!(feedback.comment.as_deref(), Some("great"));
    assert_eq!(feedback.submitted_at, at(9));
}

#[tokio::test]
async fn a_stored_status_that_is_not_a_status_reads_as_open_in_the_detail() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.detail_row = Some(detail_record("mystery", None));
    let detail = detail(&mut unit, id(TENANT), id(CONVERSATION))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.status, ConversationStatus::Open);
}

// ---------------------------------------------------------------------------------------------
// inbox
// ---------------------------------------------------------------------------------------------

fn inbox_record(n: u128, minute: u32, assignee: Option<Uuid>) -> InboxRecord {
    InboxRecord {
        id: id(n),
        customer_id: id(CUSTOMER),
        customer_display_name: "Ada".into(),
        channel: "email".into(),
        status: "open".into(),
        assigned_membership_id: assignee,
        last_message_kind: Some("reply".into()),
        last_message_preview: Some("preview".into()),
        last_activity_at: at(minute),
        created_at: at(0),
        ai_handling: Some("human".into()),
        widget_instance_id: Some(id(77)),
        widget_instance_name: None,
        feedback_rating: Some(4),
    }
}

fn open_filter() -> InboxFilter {
    InboxFilter {
        status: StatusFilter::Only(ConversationStatus::Open),
        assignee: AssigneeFilter::Any,
        channel: None,
        escalated: None,
    }
}

#[tokio::test]
async fn a_full_inbox_page_reports_more_and_points_at_the_last_row_it_kept() {
    let mut unit = FakeUnit::with_conversation("open");
    // The store was asked for limit + 1 rows and found three.
    unit.inbox = vec![
        inbox_record(30, 30, Some(id(AGENT_MEMBER))),
        inbox_record(20, 20, None),
        inbox_record(10, 10, None),
    ];

    let page = inbox(&mut unit, id(TENANT), &open_filter(), None, 2)
        .await
        .unwrap();

    assert_eq!(
        unit.asked_inbox_fetch,
        Some(3),
        "the store is asked for one row more than the page"
    );
    assert!(page.has_more);
    assert_eq!(page.conversations.len(), 2);
    assert_eq!(
        page.next_cursor,
        Some(encode_cursor(at(20), id(20))),
        "the cursor is the sort key of the last row kept"
    );
    let first = &page.conversations[0];
    assert_eq!(first.assignee.as_ref().unwrap().display_name, "Grace");
    assert_eq!(first.rating, Some(4));
    assert_eq!(first.widget_instance.as_ref().unwrap().name, "");
    assert_eq!(first.ai_handling.as_deref(), Some("human"));
    assert!(!first.awaiting_ai_decision);
}

#[tokio::test]
async fn the_last_inbox_page_has_no_cursor() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.inbox = vec![inbox_record(30, 30, None)];

    let page = inbox(&mut unit, id(TENANT), &open_filter(), None, 2)
        .await
        .unwrap();

    assert!(!page.has_more);
    assert_eq!(page.next_cursor, None);
}

#[tokio::test]
async fn the_inbox_continues_after_the_cursor_and_starts_over_for_a_bad_one() {
    let mut unit = FakeUnit::with_conversation("open");
    let cursor = encode_cursor(at(20), id(20));

    inbox(&mut unit, id(TENANT), &open_filter(), Some(&cursor), 2)
        .await
        .unwrap();
    assert_eq!(unit.asked_inbox_after, Some(Some((at(20), id(20)))));

    inbox(&mut unit, id(TENANT), &open_filter(), Some("garbage"), 2)
        .await
        .unwrap();
    assert_eq!(unit.asked_inbox_after, Some(None));
}

// ---------------------------------------------------------------------------------------------
// timeline
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct FakeEnrichment {
    citations: HashMap<Uuid, Vec<CitationView>>,
    attachments: HashMap<Uuid, Vec<AttachmentInfo>>,
    deliveries: HashMap<Uuid, DeliveryInfo>,
}

#[async_trait]
impl MessageEnrichment for FakeEnrichment {
    async fn citations(
        &self,
        _: Uuid,
        ids: &[Uuid],
    ) -> StoreResult<HashMap<Uuid, Vec<CitationView>>> {
        Ok(ids
            .iter()
            .filter_map(|id| self.citations.get(id).map(|c| (*id, c.clone())))
            .collect())
    }

    async fn attachments(
        &self,
        _: Uuid,
        ids: &[Uuid],
    ) -> StoreResult<HashMap<Uuid, Vec<AttachmentInfo>>> {
        Ok(ids
            .iter()
            .filter_map(|id| self.attachments.get(id).map(|a| (*id, a.clone())))
            .collect())
    }

    async fn deliveries(&self, _: Uuid, ids: &[Uuid]) -> StoreResult<HashMap<Uuid, DeliveryInfo>> {
        Ok(ids
            .iter()
            .filter_map(|id| self.deliveries.get(id).map(|d| (*id, d.clone())))
            .collect())
    }
}

fn timeline_record(n: u128, kind: &str, minute: u32, seq: i64) -> TimelineRecord {
    TimelineRecord {
        id: id(n),
        kind: kind.into(),
        body: format!("message {n}"),
        created_at: at(minute),
        seq,
        customer_id: Some(id(CUSTOMER)),
        customer_display_name: Some("Ada".into()),
        sender_membership_id: None,
        logged_by_membership_id: None,
        ai_confidence_score: None,
    }
}

#[tokio::test]
async fn a_timeline_page_points_at_the_last_message_it_kept() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.timeline = vec![
        timeline_record(3, "customer", 30, 3),
        timeline_record(2, "customer", 20, 2),
        timeline_record(1, "customer", 10, 1),
    ];

    let page = timeline(
        &mut unit,
        &FakeEnrichment::default(),
        id(TENANT),
        id(CONVERSATION),
        None,
        2,
    )
    .await
    .unwrap();

    assert_eq!(
        unit.asked_timeline_fetch,
        Some(3),
        "the store is asked for one row more than the page"
    );
    assert!(page.has_more);
    assert_eq!(page.messages.len(), 2);
    assert_eq!(page.next_cursor, Some(encode_timeline_cursor(at(20), 2)));
}

#[tokio::test]
async fn a_timeline_continues_before_its_cursor() {
    let mut unit = FakeUnit::with_conversation("open");
    let cursor = encode_timeline_cursor(at(20), 2);

    timeline(
        &mut unit,
        &FakeEnrichment::default(),
        id(TENANT),
        id(CONVERSATION),
        Some(&cursor),
        50,
    )
    .await
    .unwrap();

    assert_eq!(unit.asked_timeline_before, Some(Some((at(20), 2))));
}

#[tokio::test]
async fn a_timeline_shows_each_sender_by_kind() {
    let mut unit = FakeUnit::with_conversation("open");
    let mut reply = timeline_record(1, "reply", 10, 1);
    reply.sender_membership_id = Some(id(AGENT_MEMBER));
    let mut gone = timeline_record(2, "note", 11, 2);
    gone.sender_membership_id = Some(id(404));
    let mut logged = timeline_record(3, "customer", 12, 3);
    logged.logged_by_membership_id = Some(id(AGENT_MEMBER));
    let mut ai = timeline_record(4, "ai", 13, 4);
    ai.ai_confidence_score = Some(0.75);
    let system = timeline_record(5, "system", 14, 5);
    unit.timeline = vec![reply, gone, logged, ai, system];

    let page = timeline(
        &mut unit,
        &FakeEnrichment::default(),
        id(TENANT),
        id(CONVERSATION),
        None,
        50,
    )
    .await
    .unwrap();

    let [reply, gone, logged, ai, system] = &page.messages[..] else {
        panic!("five messages expected");
    };
    assert_eq!(reply.sender.display_name, "Grace");
    assert_eq!(reply.sender.id, Some(id(AGENT_USER)));
    assert_eq!(reply.sender.active, Some(true));
    assert_eq!(gone.sender.display_name, "");
    assert_eq!(gone.sender.active, Some(false));
    assert_eq!(gone.sender.membership_id, Some(id(404)));
    assert_eq!(logged.sender.participant_type, "customer");
    assert_eq!(logged.sender.display_name, "Ada");
    assert_eq!(logged.logged_by.as_ref().unwrap().display_name, "Grace");
    assert_eq!(ai.sender.participant_type, "ai_agent");
    assert_eq!(ai.confidence.as_ref().unwrap().band, "high");
    assert_eq!(system.sender.display_name, "Automated reply");
    assert!(!page.has_more);
}

#[tokio::test]
async fn a_timeline_attaches_citations_attachments_and_delivery_state() {
    let mut unit = FakeUnit::with_conversation("open");
    unit.timeline = vec![
        timeline_record(1, "ai", 10, 1),
        timeline_record(2, "reply", 11, 2),
    ];
    let mut enrichment = FakeEnrichment::default();
    enrichment.citations.insert(
        id(1),
        vec![CitationView {
            knowledge_item_id: id(50),
            item_title: "Returns".into(),
            passage_text: "30 days".into(),
            relevance_score: 0.9,
            item_available: true,
        }],
    );
    enrichment.attachments.insert(id(1), Vec::new());
    enrichment.attachments.insert(
        id(2),
        vec![AttachmentInfo {
            id: id(60),
            kind: "image".into(),
            status: "stored".into(),
            mime_type: None,
            size_bytes: None,
            file_name: None,
            url: None,
        }],
    );
    enrichment.deliveries.insert(
        id(2),
        DeliveryInfo {
            status: "delivered".into(),
            failure_reason: None,
        },
    );

    let page = timeline(
        &mut unit,
        &enrichment,
        id(TENANT),
        id(CONVERSATION),
        None,
        50,
    )
    .await
    .unwrap();

    let [first, second] = &page.messages[..] else {
        panic!("two messages expected");
    };
    assert_eq!(first.citations.len(), 1);
    assert!(
        first.attachments.is_none(),
        "an empty attachment list is left out of the message"
    );
    assert!(first.delivery.is_none());
    assert!(second.citations.is_empty());
    assert_eq!(second.attachments.as_ref().unwrap().len(), 1);
    assert_eq!(second.delivery.as_ref().unwrap().status, "delivered");
}
