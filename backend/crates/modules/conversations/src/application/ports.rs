//! What the conversation use cases need from the outside world.
//!
//! The use cases in [`super::service`] run inside the caller's tenant Unit of Work, so every port
//! method takes the tenant and the implementation filters on it. `infrastructure` implements the
//! ports on one database connection; the unit tests implement them in memory.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::audit::AuditEvent;
use super::error::StoreResult;
use super::events::ConversationEvent;
use super::model::{AttachmentInfo, CitationView, DeliveryInfo};
use super::records::{
    ConversationRecord, DetailRecord, InboxRecord, InsertedMessage, Member, TimelineRecord,
};
use crate::domain::InboxFilter;

/// Reads and writes of conversations and their messages.
#[async_trait]
pub trait ConversationStore: Send {
    /// A live (not deleted) conversation of the tenant.
    async fn find_live(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> StoreResult<Option<ConversationRecord>>;

    /// Who a live conversation is assigned to. Fails when there is no such conversation.
    async fn assignee_of(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> StoreResult<Option<Uuid>>;

    /// A live conversation's status and assignee, holding its row until the transaction ends.
    /// Fails when there is no such conversation.
    async fn lock_status_and_assignee(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> StoreResult<(String, Option<Uuid>)>;

    async fn set_status(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
        status: &str,
    ) -> StoreResult<()>;

    async fn set_assignee(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
        membership_id: Option<Uuid>,
    ) -> StoreResult<()>;

    /// Opens an `open`, unassigned conversation and returns its id.
    async fn insert_conversation(
        &mut self,
        tenant_id: Uuid,
        customer_id: Uuid,
        channel: &str,
        widget_instance_id: Option<Uuid>,
    ) -> StoreResult<Uuid>;

    /// Stores a staff reply (the first message of a conversation opened by staff).
    async fn insert_reply(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
        body: &str,
        sender_membership_id: Option<Uuid>,
    ) -> StoreResult<()>;

    async fn insert_message(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
        kind: &str,
        body: &str,
        sender_membership_id: Option<Uuid>,
        logged_by_membership_id: Option<Uuid>,
    ) -> StoreResult<InsertedMessage>;

    /// Marks the conversation active now.
    async fn touch_activity(&mut self, tenant_id: Uuid, conversation_id: Uuid) -> StoreResult<()>;

    /// Marks a live conversation active now and returns its status. Fails when there is none.
    async fn touch_and_status(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> StoreResult<String>;

    /// Sets the status to `open` and returns the status stored.
    async fn reopen(&mut self, tenant_id: Uuid, conversation_id: Uuid) -> StoreResult<String>;

    async fn last_activity_at(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> StoreResult<DateTime<Utc>>;

    /// The customer a conversation belongs to: id and display name.
    async fn customer_of(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> StoreResult<(Uuid, String)>;
}

/// The read models the dashboard shows.
#[async_trait]
pub trait ConversationReader: Send {
    /// One page of the inbox, newest activity first: the rows after `after` (the sort key of the
    /// last row of the previous page), at most `fetch` of them.
    async fn inbox_rows(
        &mut self,
        tenant_id: Uuid,
        filter: &InboxFilter,
        after: Option<(DateTime<Utc>, Uuid)>,
        fetch: i64,
    ) -> StoreResult<Vec<InboxRecord>>;

    async fn detail_row(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> StoreResult<Option<DetailRecord>>;

    /// One page of a conversation's messages, newest first: the rows before `before` (the sort key
    /// of the last row of the previous page), at most `fetch` of them.
    async fn timeline_rows(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
        before: Option<(DateTime<Utc>, i64)>,
        fetch: i64,
    ) -> StoreResult<Vec<TimelineRecord>>;

    /// The customer a conversation belongs to, as a participant: id and display name.
    async fn participant_customer(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> StoreResult<Option<(Uuid, String)>>;

    /// Every membership that sent or logged a message of the conversation.
    async fn participating_memberships(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> StoreResult<Vec<Uuid>>;
}

/// What `tenancy` knows about members.
#[async_trait]
pub trait MemberDirectory: Send {
    async fn is_active_member(&mut self, tenant_id: Uuid, membership_id: Uuid)
        -> StoreResult<bool>;

    /// The profiles of the given memberships; absent ones are left out.
    async fn member_profiles(
        &mut self,
        tenant_id: Uuid,
        membership_ids: &[Uuid],
    ) -> StoreResult<HashMap<Uuid, Member>>;

    /// The active membership of a user in this tenant.
    async fn active_membership_of_user(
        &mut self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> StoreResult<Option<Uuid>>;
}

/// What `customers` knows.
#[async_trait]
pub trait CustomerDirectory: Send {
    async fn customer_exists(&mut self, tenant_id: Uuid, customer_id: Uuid) -> StoreResult<bool>;
}

/// The transactional outbox: an event is stored in the transaction that caused it and published
/// when it commits.
#[async_trait]
pub trait EventOutbox: Send {
    async fn publish(&mut self, event: &ConversationEvent<'_>) -> StoreResult<()>;
}

/// The audit trail.
#[async_trait]
pub trait AuditTrail: Send {
    async fn record(
        &mut self,
        actor_user_id: Option<Uuid>,
        tenant_id: Uuid,
        conversation_id: Uuid,
        event: &AuditEvent<'_>,
    ) -> StoreResult<()>;
}

/// Everything one transaction of the conversation use cases needs.
pub trait ConversationUnit:
    ConversationStore
    + ConversationReader
    + MemberDirectory
    + CustomerDirectory
    + EventOutbox
    + AuditTrail
    + Send
{
}

impl<T> ConversationUnit for T where
    T: ConversationStore
        + ConversationReader
        + MemberDirectory
        + CustomerDirectory
        + EventOutbox
        + AuditTrail
        + Send
{
}

/// What a message list is enriched with. These reads run in units of work of their own, so the
/// enrichment of a page does not hold the page's transaction open.
#[async_trait]
pub trait MessageEnrichment: Send + Sync {
    async fn citations(
        &self,
        tenant_id: Uuid,
        message_ids: &[Uuid],
    ) -> StoreResult<HashMap<Uuid, Vec<CitationView>>>;

    async fn attachments(
        &self,
        tenant_id: Uuid,
        message_ids: &[Uuid],
    ) -> StoreResult<HashMap<Uuid, Vec<AttachmentInfo>>>;

    async fn deliveries(
        &self,
        tenant_id: Uuid,
        message_ids: &[Uuid],
    ) -> StoreResult<HashMap<Uuid, DeliveryInfo>>;
}
