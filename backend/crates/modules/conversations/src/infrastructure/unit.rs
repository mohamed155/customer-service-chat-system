//! The conversation ports on one database transaction.
//!
//! A [`PgUnit`] borrows the caller's transaction, so everything it reads and writes stays inside
//! the caller's tenant Unit of Work. The SQL is the module's own; the member, customer, audit and
//! outbox ports delegate to the modules and shared crates that own those concerns.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, Postgres, Transaction};
use uuid::Uuid;

use super::outbox;
use crate::application::audit::{AuditEvent, RESOURCE_TYPE};
use crate::application::ports::{
    AuditTrail, ConversationReader, ConversationStore, CustomerDirectory, EventOutbox,
    MemberDirectory,
};
use crate::application::records::{
    ConversationRecord, DetailRecord, InboxRecord, InsertedMessage, Member, TimelineRecord,
};
use crate::application::{ConversationEvent, StoreError, StoreResult};
use crate::domain::{AssigneeFilter, InboxFilter, StatusFilter};

/// The conversation ports on the caller's transaction.
pub struct PgUnit<'c, 't> {
    tx: &'c mut Transaction<'t, Postgres>,
}

impl<'c, 't> PgUnit<'c, 't> {
    pub fn new(tx: &'c mut Transaction<'t, Postgres>) -> Self {
        Self { tx }
    }

    fn conn(&mut self) -> &mut PgConnection {
        self.tx
    }
}

impl From<sqlx::Error> for StoreError {
    fn from(error: sqlx::Error) -> Self {
        Self::new(error)
    }
}

#[derive(sqlx::FromRow)]
struct ConversationRow {
    id: Uuid,
    tenant_id: Uuid,
    customer_id: Uuid,
    channel: String,
    status: String,
    assigned_membership_id: Option<Uuid>,
    last_activity_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct InboxRow {
    id: Uuid,
    customer_id: Uuid,
    customer_display_name: String,
    channel: String,
    status: String,
    assigned_membership_id: Option<Uuid>,
    last_message_kind: Option<String>,
    last_message_preview: Option<String>,
    last_activity_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    ai_handling: Option<String>,
    widget_instance_id: Option<Uuid>,
    widget_instance_name: Option<String>,
    feedback_rating: Option<i16>,
}

#[derive(sqlx::FromRow)]
struct DetailRow {
    id: Uuid,
    customer_id: Uuid,
    customer_display_name: String,
    channel: String,
    status: String,
    assigned_membership_id: Option<Uuid>,
    last_message_kind: Option<String>,
    last_message_preview: Option<String>,
    last_activity_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    ai_handling: Option<String>,
    widget_instance_id: Option<Uuid>,
    widget_instance_name: Option<String>,
    feedback_rating: Option<i16>,
    feedback_comment: Option<String>,
    feedback_submitted_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct TimelineRow {
    id: Uuid,
    kind: String,
    body: String,
    created_at: DateTime<Utc>,
    seq: i64,
    customer_id: Option<Uuid>,
    customer_display_name: Option<String>,
    sender_membership_id: Option<Uuid>,
    logged_by_membership_id: Option<Uuid>,
    ai_confidence_score: Option<f32>,
}

impl From<ConversationRow> for ConversationRecord {
    fn from(row: ConversationRow) -> Self {
        Self {
            id: row.id,
            tenant_id: row.tenant_id,
            customer_id: row.customer_id,
            channel: row.channel,
            status: row.status,
            assigned_membership_id: row.assigned_membership_id,
            last_activity_at: row.last_activity_at,
            created_at: row.created_at,
        }
    }
}

impl From<InboxRow> for InboxRecord {
    fn from(row: InboxRow) -> Self {
        Self {
            id: row.id,
            customer_id: row.customer_id,
            customer_display_name: row.customer_display_name,
            channel: row.channel,
            status: row.status,
            assigned_membership_id: row.assigned_membership_id,
            last_message_kind: row.last_message_kind,
            last_message_preview: row.last_message_preview,
            last_activity_at: row.last_activity_at,
            created_at: row.created_at,
            ai_handling: row.ai_handling,
            widget_instance_id: row.widget_instance_id,
            widget_instance_name: row.widget_instance_name,
            feedback_rating: row.feedback_rating,
        }
    }
}

impl From<DetailRow> for DetailRecord {
    fn from(row: DetailRow) -> Self {
        Self {
            id: row.id,
            customer_id: row.customer_id,
            customer_display_name: row.customer_display_name,
            channel: row.channel,
            status: row.status,
            assigned_membership_id: row.assigned_membership_id,
            last_message_kind: row.last_message_kind,
            last_message_preview: row.last_message_preview,
            last_activity_at: row.last_activity_at,
            created_at: row.created_at,
            ai_handling: row.ai_handling,
            widget_instance_id: row.widget_instance_id,
            widget_instance_name: row.widget_instance_name,
            feedback_rating: row.feedback_rating,
            feedback_comment: row.feedback_comment,
            feedback_submitted_at: row.feedback_submitted_at,
        }
    }
}

impl From<TimelineRow> for TimelineRecord {
    fn from(row: TimelineRow) -> Self {
        Self {
            id: row.id,
            kind: row.kind,
            body: row.body,
            created_at: row.created_at,
            seq: row.seq,
            customer_id: row.customer_id,
            customer_display_name: row.customer_display_name,
            sender_membership_id: row.sender_membership_id,
            logged_by_membership_id: row.logged_by_membership_id,
            ai_confidence_score: row.ai_confidence_score,
        }
    }
}

#[async_trait]
impl ConversationStore for PgUnit<'_, '_> {
    async fn find_live(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> StoreResult<Option<ConversationRecord>> {
        let row = sqlx::query_as::<_, ConversationRow>(
            "SELECT id, tenant_id, customer_id, channel, status, \
                    assigned_membership_id, last_activity_at, created_at \
             FROM conversations \
             WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_optional(self.conn())
        .await?;
        Ok(row.map(ConversationRecord::from))
    }

    async fn assignee_of(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> StoreResult<Option<Uuid>> {
        let prior: (Option<Uuid>,) = sqlx::query_as(
            "SELECT assigned_membership_id FROM conversations \
             WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_one(self.conn())
        .await?;
        Ok(prior.0)
    }

    async fn lock_status_and_assignee(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> StoreResult<(String, Option<Uuid>)> {
        Ok(sqlx::query_as::<_, (String, Option<Uuid>)>(
            "SELECT status, assigned_membership_id \
             FROM conversations \
             WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL \
             FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_one(self.conn())
        .await?)
    }

    async fn set_status(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
        status: &str,
    ) -> StoreResult<()> {
        sqlx::query(
            "UPDATE conversations SET status = $1 \
             WHERE tenant_id = $2 AND id = $3",
        )
        .bind(status)
        .bind(tenant_id)
        .bind(conversation_id)
        .execute(self.conn())
        .await?;
        Ok(())
    }

    async fn set_assignee(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
        membership_id: Option<Uuid>,
    ) -> StoreResult<()> {
        sqlx::query(
            "UPDATE conversations SET assigned_membership_id = $1 \
             WHERE tenant_id = $2 AND id = $3",
        )
        .bind(membership_id)
        .bind(tenant_id)
        .bind(conversation_id)
        .execute(self.conn())
        .await?;
        Ok(())
    }

    async fn insert_conversation(
        &mut self,
        tenant_id: Uuid,
        customer_id: Uuid,
        channel: &str,
        widget_instance_id: Option<Uuid>,
    ) -> StoreResult<Uuid> {
        let id = if let Some(widget_instance_id) = widget_instance_id {
            sqlx::query_scalar(
                "INSERT INTO conversations (tenant_id, customer_id, channel, status, widget_instance_id) \
                 VALUES ($1, $2, $3, 'open', $4) \
                 RETURNING id",
            )
            .bind(tenant_id)
            .bind(customer_id)
            .bind(channel)
            .bind(widget_instance_id)
            .fetch_one(self.conn())
            .await?
        } else {
            sqlx::query_scalar(
                "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
                 VALUES ($1, $2, $3, 'open') \
                 RETURNING id",
            )
            .bind(tenant_id)
            .bind(customer_id)
            .bind(channel)
            .fetch_one(self.conn())
            .await?
        };
        Ok(id)
    }

    async fn insert_reply(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
        body: &str,
        sender_membership_id: Option<Uuid>,
    ) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO messages (tenant_id, conversation_id, kind, body, sender_membership_id) \
             VALUES ($1, $2, 'reply', $3, $4)",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .bind(body)
        .bind(sender_membership_id)
        .execute(self.conn())
        .await?;
        Ok(())
    }

    async fn insert_message(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
        kind: &str,
        body: &str,
        sender_membership_id: Option<Uuid>,
        logged_by_membership_id: Option<Uuid>,
    ) -> StoreResult<InsertedMessage> {
        let (id, _kind, body, created_at, _seq) =
            sqlx::query_as::<_, (Uuid, String, String, DateTime<Utc>, i64)>(
                "INSERT INTO messages (tenant_id, conversation_id, kind, body, \
                        sender_membership_id, logged_by_membership_id) \
                 VALUES ($1, $2, $3, $4, $5, $6) \
                 RETURNING id, kind, body, created_at, seq",
            )
            .bind(tenant_id)
            .bind(conversation_id)
            .bind(kind)
            .bind(body)
            .bind(sender_membership_id)
            .bind(logged_by_membership_id)
            .fetch_one(self.conn())
            .await?;
        Ok(InsertedMessage {
            id,
            body,
            created_at,
        })
    }

    async fn touch_activity(&mut self, tenant_id: Uuid, conversation_id: Uuid) -> StoreResult<()> {
        sqlx::query(
            "UPDATE conversations SET last_activity_at = now() WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .execute(self.conn())
        .await?;
        Ok(())
    }

    async fn touch_and_status(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> StoreResult<String> {
        Ok(sqlx::query_scalar(
            "UPDATE conversations SET last_activity_at = now() \
             WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL \
             RETURNING status",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_one(self.conn())
        .await?)
    }

    async fn reopen(&mut self, tenant_id: Uuid, conversation_id: Uuid) -> StoreResult<String> {
        Ok(sqlx::query_scalar(
            "UPDATE conversations SET status = 'open' \
             WHERE tenant_id = $1 AND id = $2 \
             RETURNING status",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_one(self.conn())
        .await?)
    }

    async fn last_activity_at(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> StoreResult<DateTime<Utc>> {
        Ok(sqlx::query_scalar(
            "SELECT last_activity_at FROM conversations WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_one(self.conn())
        .await?)
    }

    async fn customer_of(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> StoreResult<(Uuid, String)> {
        Ok(sqlx::query_as::<_, (Uuid, String)>(
            "SELECT c.id, c.display_name \
             FROM conversations cv \
             JOIN customers c ON c.id = cv.customer_id AND c.tenant_id = cv.tenant_id \
             WHERE cv.tenant_id = $1 AND cv.id = $2",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_one(self.conn())
        .await?)
    }
}

#[async_trait]
impl ConversationReader for PgUnit<'_, '_> {
    async fn inbox_rows(
        &mut self,
        tenant_id: Uuid,
        filter: &InboxFilter,
        after: Option<(DateTime<Utc>, Uuid)>,
        fetch: i64,
    ) -> StoreResult<Vec<InboxRecord>> {
        let mut sql = String::from(
            "SELECT c.id, c.customer_id, \
                    cu.display_name AS customer_display_name, \
                    c.channel, c.status, c.assigned_membership_id, \
                    preview.kind AS last_message_kind, \
                    LEFT(preview.body, 140) AS last_message_preview, \
                    c.last_activity_at, c.created_at, \
                    c.ai_handling, \
                    wi.id AS widget_instance_id, \
                     wi.name AS widget_instance_name, \
                     f.rating AS feedback_rating \
             FROM conversations c \
             JOIN customers cu \
               ON cu.id = c.customer_id AND cu.tenant_id = c.tenant_id AND cu.deleted_at IS NULL \
                  AND cu.erasure_pending_at IS NULL AND cu.erased_at IS NULL \
             LEFT JOIN widget_instances wi \
               ON wi.id = c.widget_instance_id AND wi.deleted_at IS NULL \
             LEFT JOIN conversation_feedback f \
               ON f.conversation_id = c.id AND f.tenant_id = c.tenant_id \
             LEFT JOIN LATERAL ( \
                 SELECT kind, body FROM messages \
                 WHERE tenant_id = c.tenant_id AND conversation_id = c.id \
                 ORDER BY created_at DESC, seq DESC \
                 LIMIT 1 \
             ) preview ON TRUE \
             WHERE c.tenant_id = $1 AND c.deleted_at IS NULL",
        );

        let mut next_bind = 2u16;

        // ---- status filter ----
        let status = match filter.status {
            StatusFilter::All => None,
            StatusFilter::Only(status) => {
                sql.push_str(&format!(" AND c.status = ${next_bind}"));
                next_bind += 1;
                Some(status)
            }
        };

        // ---- assignee filter ----
        let assignee = match filter.assignee {
            AssigneeFilter::Any => None,
            AssigneeFilter::Unassigned => {
                sql.push_str(" AND c.assigned_membership_id IS NULL");
                None
            }
            // `me` is resolved to the asking member before the query; one that was not matches
            // nobody rather than everybody.
            AssigneeFilter::Me => {
                sql.push_str(&format!(" AND c.assigned_membership_id = ${next_bind}"));
                next_bind += 1;
                Some(Uuid::nil())
            }
            AssigneeFilter::Member(member) => {
                sql.push_str(&format!(" AND c.assigned_membership_id = ${next_bind}"));
                next_bind += 1;
                Some(member)
            }
        };

        // ---- escalated filter ----
        match filter.escalated {
            Some(true) => sql.push_str(" AND c.escalated_at IS NOT NULL"),
            Some(false) => sql.push_str(" AND c.escalated_at IS NULL"),
            None => {}
        }

        // ---- channel filter ----
        if filter.channel.is_some() {
            sql.push_str(&format!(" AND c.channel = ${next_bind}"));
            next_bind += 1;
        }

        // ---- keyset cursor ----
        if after.is_some() {
            sql.push_str(&format!(
                " AND (c.last_activity_at, c.id) < (${a}::timestamptz, ${b}::uuid)",
                a = next_bind,
                b = next_bind + 1
            ));
            next_bind += 2;
        }

        sql.push_str(&format!(
            " ORDER BY c.last_activity_at DESC, c.id DESC LIMIT ${next_bind}"
        ));

        let mut query = sqlx::query_as::<_, InboxRow>(sqlx::AssertSqlSafe(sql)).bind(tenant_id);
        if let Some(status) = status {
            query = query.bind(status.as_str());
        }
        if let Some(member) = assignee {
            query = query.bind(member);
        }
        if let Some(channel) = &filter.channel {
            query = query.bind(channel.clone());
        }
        if let Some((last_activity_at, id)) = after {
            query = query.bind(last_activity_at).bind(id);
        }
        query = query.bind(fetch);

        let rows = query.fetch_all(self.conn()).await?;
        Ok(rows.into_iter().map(InboxRecord::from).collect())
    }

    async fn detail_row(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> StoreResult<Option<DetailRecord>> {
        let row = sqlx::query_as::<_, DetailRow>(
            "SELECT cv.id, cv.customer_id, \
                    c.display_name AS customer_display_name, \
                    cv.channel, cv.status, cv.assigned_membership_id, \
                    preview.kind AS last_message_kind, \
                    LEFT(preview.body, 140) AS last_message_preview, \
                    cv.last_activity_at, cv.created_at, \
                    cv.ai_handling, \
                     wi.id AS widget_instance_id, \
                     wi.name AS widget_instance_name, \
                     f.rating AS feedback_rating, \
                     f.comment AS feedback_comment, \
                     f.submitted_at AS feedback_submitted_at \
              FROM conversations cv \
              JOIN customers c \
                ON c.id = cv.customer_id AND c.tenant_id = cv.tenant_id AND c.deleted_at IS NULL \
               AND c.erasure_pending_at IS NULL AND c.erased_at IS NULL \
              LEFT JOIN widget_instances wi \
                ON wi.id = cv.widget_instance_id AND wi.deleted_at IS NULL \
              LEFT JOIN conversation_feedback f \
                ON f.conversation_id = cv.id AND f.tenant_id = cv.tenant_id \
              LEFT JOIN LATERAL ( \
                  SELECT kind, body FROM messages \
                  WHERE tenant_id = cv.tenant_id AND conversation_id = cv.id \
                  ORDER BY created_at DESC, seq DESC \
                  LIMIT 1 \
              ) preview ON TRUE \
              WHERE cv.tenant_id = $1 AND cv.id = $2 AND cv.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_optional(self.conn())
        .await?;
        Ok(row.map(DetailRecord::from))
    }

    async fn timeline_rows(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
        before: Option<(DateTime<Utc>, i64)>,
        fetch: i64,
    ) -> StoreResult<Vec<TimelineRecord>> {
        let mut sql = String::from(
            "SELECT m.id, m.kind, m.body, m.created_at, m.seq, \
                    c.id AS customer_id, c.display_name AS customer_display_name, \
                    m.sender_membership_id, \
                    m.logged_by_membership_id, \
                    m.ai_confidence_score \
             FROM messages m \
             JOIN conversations cv \
               ON cv.id = m.conversation_id AND cv.tenant_id = m.tenant_id \
             JOIN customers c \
               ON c.id = cv.customer_id AND c.tenant_id = cv.tenant_id \
             WHERE m.tenant_id = $1 AND m.conversation_id = $2 \
                 AND c.erasure_pending_at IS NULL AND c.erased_at IS NULL",
        );

        let mut next_bind = 3u16;
        if before.is_some() {
            sql.push_str(&format!(
                " AND (m.created_at, m.seq) < (${a}::timestamptz, ${b}::bigint)",
                a = next_bind,
                b = next_bind + 1
            ));
            next_bind += 2;
        }
        sql.push_str(&format!(
            " ORDER BY m.created_at DESC, m.seq DESC LIMIT ${next_bind}"
        ));

        let mut query = sqlx::query_as::<_, TimelineRow>(sqlx::AssertSqlSafe(sql))
            .bind(tenant_id)
            .bind(conversation_id);
        if let Some((created_at, seq)) = before {
            query = query.bind(created_at).bind(seq);
        }
        query = query.bind(fetch);

        let rows = query.fetch_all(self.conn()).await?;
        Ok(rows.into_iter().map(TimelineRecord::from).collect())
    }

    async fn participant_customer(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> StoreResult<Option<(Uuid, String)>> {
        Ok(sqlx::query_as::<_, (Uuid, String)>(
            "SELECT c.id, c.display_name \
             FROM conversations cv \
             JOIN customers c ON c.id = cv.customer_id AND c.tenant_id = cv.tenant_id \
             WHERE cv.tenant_id = $1 AND cv.id = $2",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_optional(self.conn())
        .await?)
    }

    async fn participating_memberships(
        &mut self,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> StoreResult<Vec<Uuid>> {
        Ok(sqlx::query_scalar(
            "SELECT DISTINCT sender_membership_id AS membership_id \
             FROM messages \
             WHERE tenant_id = $1 AND conversation_id = $2 AND sender_membership_id IS NOT NULL \
             UNION \
             SELECT DISTINCT logged_by_membership_id AS membership_id \
             FROM messages \
             WHERE tenant_id = $1 AND conversation_id = $2 AND logged_by_membership_id IS NOT NULL",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_all(self.conn())
        .await?)
    }
}

#[async_trait]
impl MemberDirectory for PgUnit<'_, '_> {
    async fn is_active_member(
        &mut self,
        tenant_id: Uuid,
        membership_id: Uuid,
    ) -> StoreResult<bool> {
        Ok(tenancy::directory::is_active_member(self.conn(), tenant_id, membership_id).await?)
    }

    async fn member_profiles(
        &mut self,
        tenant_id: Uuid,
        membership_ids: &[Uuid],
    ) -> StoreResult<HashMap<Uuid, Member>> {
        let profiles =
            tenancy::directory::member_profiles(self.conn(), tenant_id, membership_ids).await?;
        Ok(profiles
            .into_iter()
            .map(|(id, profile)| {
                (
                    id,
                    Member {
                        user_id: profile.user_id,
                        membership_id: profile.membership_id,
                        display_name: profile.display_name,
                        active: profile.active,
                    },
                )
            })
            .collect())
    }

    async fn active_membership_of_user(
        &mut self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> StoreResult<Option<Uuid>> {
        Ok(
            tenancy::directory::active_membership_id_of_user(self.conn(), tenant_id, user_id)
                .await?,
        )
    }
}

#[async_trait]
impl CustomerDirectory for PgUnit<'_, '_> {
    async fn customer_exists(&mut self, tenant_id: Uuid, customer_id: Uuid) -> StoreResult<bool> {
        Ok(customers::customer_exists_in_tx(self.tx, tenant_id, customer_id).await?)
    }
}

#[async_trait]
impl EventOutbox for PgUnit<'_, '_> {
    async fn publish(&mut self, event: &ConversationEvent<'_>) -> StoreResult<()> {
        Ok(outbox::publish(self.conn(), event).await?)
    }
}

#[async_trait]
impl AuditTrail for PgUnit<'_, '_> {
    async fn record(
        &mut self,
        actor_user_id: Option<Uuid>,
        tenant_id: Uuid,
        conversation_id: Uuid,
        event: &AuditEvent<'_>,
    ) -> StoreResult<()> {
        Ok(audit_log::record_in_tx(
            self.conn(),
            event.action(),
            actor_user_id,
            Some(tenant_id),
            RESOURCE_TYPE,
            Some(&conversation_id.to_string()),
            &event.details(),
        )
        .await?)
    }
}
