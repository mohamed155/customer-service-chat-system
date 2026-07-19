use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use serde_json::Value;

use crate::model::{
    Assignee, CitationToInsert, CitationView, ConversationDetail, ConversationStatus,
    ConversationStatusRef, CustomerRef, LastMessagePreview, Message, MessageKind, Participant,
};
use std::collections::HashMap;

/// Actor who performs a conversation write — either a staff user or a widget visitor.
#[derive(Debug, Clone, Copy)]
pub enum ConversationActor {
    Staff { user_id: Uuid, membership_id: Uuid },
    Visitor { customer_id: Uuid },
}

/// Raw row from the `conversations` table, used by `conversation_row_in_tx`
/// for 404-safe reuse across detail / patch / add-message handlers.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ConversationRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub customer_id: Uuid,
    pub channel: String,
    pub status: String,
    pub assigned_membership_id: Option<Uuid>,
    pub last_activity_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Fetch one conversation scoped to `(tenant_id, id)` where `deleted_at IS NULL`.
/// Returns `None` when the row does not exist or is soft-deleted, enabling
/// 404-safe reuse across handlers.
pub async fn conversation_row_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    id: Uuid,
) -> sqlx::Result<Option<ConversationRow>> {
    sqlx::query_as::<_, ConversationRow>(
        "SELECT id, tenant_id, customer_id, channel, status, \
                assigned_membership_id, last_activity_at, created_at \
         FROM conversations \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
}

/// Check whether a tenant-membership is tenant-scoped and `status = 'active'`
/// (and not soft-deleted). Returns `false` if the row is missing, disabled,
/// or deleted.
pub async fn active_membership_exists_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    membership_id: Uuid,
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS( \
         SELECT 1 FROM tenant_memberships \
         WHERE id = $1 AND tenant_id = $2 AND status = 'active' AND deleted_at IS NULL \
         )",
    )
    .bind(membership_id)
    .bind(tenant_id)
    .fetch_one(&mut **tx)
    .await
}

/// Fetch all participants for a conversation: the owning customer plus every
/// distinct agent who has sent or logged a message. Agent display names are
/// resolved through `tenant_memberships → users`.
pub async fn participants_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Vec<Participant>> {
    let customer_participant = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT c.id, c.display_name \
         FROM conversations cv \
         JOIN customers c ON c.id = cv.customer_id AND c.tenant_id = cv.tenant_id \
         WHERE cv.tenant_id = $1 AND cv.id = $2",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_optional(&mut **tx)
    .await?;

    let agent_participants = sqlx::query_as::<_, (Uuid, Uuid, String, bool)>(
        "SELECT DISTINCT u.id, m.id AS membership_id, u.display_name, m.status = 'active' AS active \
         FROM ( \
             SELECT DISTINCT sender_membership_id AS membership_id \
             FROM messages \
             WHERE tenant_id = $1 AND conversation_id = $2 AND sender_membership_id IS NOT NULL \
             UNION \
             SELECT DISTINCT logged_by_membership_id AS membership_id \
             FROM messages \
             WHERE tenant_id = $1 AND conversation_id = $2 AND logged_by_membership_id IS NOT NULL \
         ) ms \
         JOIN tenant_memberships m ON m.id = ms.membership_id AND m.tenant_id = $1 AND m.deleted_at IS NULL \
         JOIN users u ON u.id = m.user_id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&mut **tx)
    .await?;

    let customer = customer_participant.map(|(id, display_name)| Participant {
        participant_type: "customer".into(),
        id: Some(id),
        membership_id: None,
        display_name,
        active: None,
    });

    let agents: Vec<Participant> = agent_participants
        .into_iter()
        .map(
            |(user_id, membership_id, display_name, active)| Participant {
                participant_type: "agent".into(),
                id: Some(user_id),
                membership_id: Some(membership_id),
                display_name,
                active: Some(active),
            },
        )
        .collect();

    let mut all = Vec::with_capacity(1 + agents.len());
    if let Some(c) = customer {
        all.push(c);
    }
    all.extend(agents);
    Ok(all)
}

// ---------------------------------------------------------------------------
// Cursor helpers (keyset pagination on (last_activity_at DESC, id DESC))
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Timeline cursor helpers (keyset pagination on (created_at DESC, seq DESC))
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Inbox row type
// ---------------------------------------------------------------------------

/// Raw row produced by the inbox query, combining the conversation, customer,
/// assignee, and latest-message preview into a single result set.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InboxRow {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub customer_display_name: String,
    pub channel: String,
    pub status: String,
    pub assigned_membership_id: Option<Uuid>,
    pub assignee_display_name: Option<String>,
    pub assignee_active: Option<bool>,
    pub last_message_kind: Option<String>,
    pub last_message_preview: Option<String>,
    pub last_activity_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub ai_handling: Option<String>,
    pub widget_instance_id: Option<Uuid>,
    pub widget_instance_name: Option<String>,
}

// ---------------------------------------------------------------------------
// Detail row type (T026)
// ---------------------------------------------------------------------------

/// Raw row produced by the detail query, combining the conversation, customer,
/// assignee, and latest-message preview into a single result set.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DetailRow {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub customer_display_name: String,
    pub channel: String,
    pub status: String,
    pub assigned_membership_id: Option<Uuid>,
    pub assignee_display_name: Option<String>,
    pub assignee_active: bool,
    pub last_message_kind: Option<String>,
    pub last_message_preview: Option<String>,
    pub last_activity_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub ai_handling: Option<String>,
    pub widget_instance_id: Option<Uuid>,
    pub widget_instance_name: Option<String>,
}

// ---------------------------------------------------------------------------
// Timeline row type (T027)
// ---------------------------------------------------------------------------

/// Raw row produced by the timeline query, joining the message with the
/// customer (for kind='customer') or agent sender info (for kind='reply'/'note')
/// and the agent who logged customer messages.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TimelineRow {
    pub id: Uuid,
    pub kind: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub seq: i64,
    pub customer_id: Option<Uuid>,
    pub customer_display_name: Option<String>,
    pub agent_user_id: Option<Uuid>,
    pub sender_membership_id: Option<Uuid>,
    pub agent_display_name: Option<String>,
    pub agent_active: Option<bool>,
    pub logged_by_membership_id: Option<Uuid>,
    pub logged_by_user_id: Option<Uuid>,
    pub logged_by_display_name: Option<String>,
    pub logged_by_active: Option<bool>,
    pub ai_confidence_score: Option<f32>,
}

// ---------------------------------------------------------------------------
// Inbox query (T012)
// ---------------------------------------------------------------------------

/// Fetch a keyset-paginated page of active (non-deleted) conversations for
/// the given tenant, applying optional `status`/`assignee`/`channel` filters.
///
/// `acting_membership_id` is the caller's own membership id — used to resolve
/// `assignee=me`.
///
/// The `status` parameter defaults to `"open"` when `None`, and `"all"`
/// means no status filter.  The `assignee` parameter accepts `"me"`,
/// `"unassigned"`, or a specific membership UUID (the caller must have
/// validated UUID format already).  `channel` is a literal channel value.
///
/// Returns `(rows, has_more)` where `rows` is at most `limit` items, and
/// `has_more` signals whether additional rows exist beyond this page.
#[allow(clippy::too_many_arguments)]
pub async fn inbox_query(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    acting_membership_id: Uuid,
    status: Option<String>,
    assignee: Option<String>,
    channel: Option<String>,
    escalated: Option<bool>,
    cursor: Option<String>,
    limit: i64,
) -> sqlx::Result<(Vec<InboxRow>, bool)> {
    let decoded_cursor = match cursor {
        Some(c) => decode_cursor(&c),
        None => None,
    };

    let mut sql = String::from(
        "SELECT c.id, c.customer_id, \
                cu.display_name AS customer_display_name, \
                c.channel, c.status, c.assigned_membership_id, \
                mu.display_name AS assignee_display_name, \
                (mu.status = 'active') AS assignee_active, \
                preview.kind AS last_message_kind, \
                LEFT(preview.body, 140) AS last_message_preview, \
                c.last_activity_at, c.created_at, \
                c.ai_handling, \
                wi.id AS widget_instance_id, \
                wi.name AS widget_instance_name \
         FROM conversations c \
         JOIN customers cu \
           ON cu.id = c.customer_id AND cu.tenant_id = c.tenant_id AND cu.deleted_at IS NULL \
         LEFT JOIN tenant_memberships tm \
           ON tm.id = c.assigned_membership_id AND tm.tenant_id = c.tenant_id AND tm.deleted_at IS NULL \
         LEFT JOIN users mu ON mu.id = tm.user_id \
         LEFT JOIN widget_instances wi \
           ON wi.id = c.widget_instance_id AND wi.deleted_at IS NULL \
         LEFT JOIN LATERAL ( \
             SELECT kind, body FROM messages \
             WHERE tenant_id = c.tenant_id AND conversation_id = c.id \
             ORDER BY created_at DESC, seq DESC \
             LIMIT 1 \
         ) preview ON TRUE",
    );

    let mut next_bind = 2u16;

    // ---- status filter ----
    let default_open = status.is_none();
    let status_val = status.as_deref().unwrap_or("open");

    match status_val {
        "all" => {}
        _ => {
            sql.push_str(&format!(" AND c.status = ${next_bind}"));
            next_bind += 1;
        }
    }

    // ---- assignee filter ----
    let assignee_bind_uuid: Option<Uuid> = match assignee.as_deref() {
        Some("me") => {
            sql.push_str(&format!(" AND c.assigned_membership_id = ${next_bind}"));
            next_bind += 1;
            Some(acting_membership_id)
        }
        Some("unassigned") => {
            sql.push_str(" AND c.assigned_membership_id IS NULL");
            None
        }
        Some(uuid_str) => {
            sql.push_str(&format!(" AND c.assigned_membership_id = ${next_bind}"));
            next_bind += 1;
            Some(Uuid::parse_str(uuid_str).unwrap_or(Uuid::nil()))
        }
        None => None,
    };

    // ---- escalated filter ----
    match escalated {
        Some(true) => sql.push_str(" AND c.escalated_at IS NOT NULL"),
        Some(false) => sql.push_str(" AND c.escalated_at IS NULL"),
        None => {}
    }

    // ---- channel filter ----
    if channel.is_some() {
        sql.push_str(&format!(" AND c.channel = ${next_bind}"));
        next_bind += 1;
    }

    // ---- keyset cursor ----
    let (cursor_ts, cursor_id) = match decoded_cursor {
        Some((ts, id)) => {
            sql.push_str(&format!(
                " AND (c.last_activity_at, c.id) < (${a}::timestamptz, ${b}::uuid)",
                a = next_bind,
                b = next_bind + 1
            ));
            next_bind += 2;
            (Some(ts), Some(id))
        }
        None => (None, None),
    };

    sql.push_str(&format!(
        " ORDER BY c.last_activity_at DESC, c.id DESC LIMIT ${next_bind}"
    ));

    let mut query = sqlx::query_as::<_, InboxRow>(&sql).bind(tenant_id);

    // Bind status
    match status_val {
        "all" => {}
        _ => {
            if default_open {
                query = query.bind("open");
            } else {
                query = query.bind(status_val);
            }
        }
    }

    // Bind assignee
    if let Some(mid) = assignee_bind_uuid {
        query = query.bind(mid);
    }

    // Bind channel
    if let Some(ref ch) = channel {
        query = query.bind(ch);
    }

    // Bind cursor components
    if let (Some(ts), Some(id)) = (cursor_ts, cursor_id) {
        query = query.bind(ts).bind(id);
    }

    // Over-fetch by one to determine has_more
    query = query.bind(limit + 1);

    let rows = query.fetch_all(&mut **tx).await?;
    let has_more = rows.len() > limit as usize;
    let data: Vec<InboxRow> = rows.into_iter().take(limit as usize).collect();

    Ok((data, has_more))
}

// ---------------------------------------------------------------------------
// Timeline row → Message conversion
// ---------------------------------------------------------------------------

fn timeline_row_to_message(row: TimelineRow) -> Message {
    let kind: MessageKind =
        serde_json::from_value(Value::String(row.kind)).unwrap_or(MessageKind::Reply);

    let sender = match kind {
        MessageKind::Customer => Participant {
            participant_type: "customer".into(),
            id: row.customer_id,
            membership_id: None,
            display_name: row.customer_display_name.unwrap_or_default(),
            active: None,
        },
        MessageKind::Ai => Participant {
            participant_type: "ai_agent".into(),
            id: None,
            membership_id: None,
            display_name: "<agent name>".into(),
            active: None,
        },
        MessageKind::System => Participant {
            participant_type: "system".into(),
            id: None,
            membership_id: None,
            display_name: "Automated reply".into(),
            active: None,
        },
        _ => Participant {
            participant_type: "agent".into(),
            id: row.agent_user_id,
            membership_id: row.sender_membership_id,
            display_name: row.agent_display_name.unwrap_or_default(),
            active: row.agent_active,
        },
    };

    let logged_by = row.logged_by_membership_id.map(|mid| Assignee {
        membership_id: mid,
        display_name: row.logged_by_display_name.unwrap_or_default(),
        active: row.logged_by_active.unwrap_or(false),
    });

    let confidence = row
        .ai_confidence_score
        .map(|score| crate::model::ConfidenceView {
            score,
            band: crate::model::confidence_band(score).to_string(),
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
    }
}

// ---------------------------------------------------------------------------
// T026 — Detail query (fetch conversation + participants)
// ---------------------------------------------------------------------------

/// Fetch a conversation's full detail including customer, assignee, last
/// message preview, and participants, scoped to `(tenant_id, id)`.
/// Returns `None` when the conversation is missing or soft-deleted.
pub async fn detail_query_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    id: Uuid,
) -> sqlx::Result<Option<ConversationDetail>> {
    let row = sqlx::query_as::<_, DetailRow>(
        "SELECT cv.id, cv.customer_id, \
                c.display_name AS customer_display_name, \
                cv.channel, cv.status, cv.assigned_membership_id, \
                mu.display_name AS assignee_display_name, \
                COALESCE(tm.status = 'active', false) AS assignee_active, \
                preview.kind AS last_message_kind, \
                LEFT(preview.body, 140) AS last_message_preview, \
                cv.last_activity_at, cv.created_at, \
                cv.ai_handling, \
                wi.id AS widget_instance_id, \
                wi.name AS widget_instance_name \
         FROM conversations cv \
         JOIN customers c \
           ON c.id = cv.customer_id AND c.tenant_id = cv.tenant_id AND c.deleted_at IS NULL \
         LEFT JOIN tenant_memberships tm \
           ON tm.id = cv.assigned_membership_id AND tm.tenant_id = cv.tenant_id AND tm.deleted_at IS NULL \
         LEFT JOIN users mu ON mu.id = tm.user_id \
         LEFT JOIN widget_instances wi \
           ON wi.id = cv.widget_instance_id AND wi.deleted_at IS NULL \
         LEFT JOIN LATERAL ( \
             SELECT kind, body FROM messages \
             WHERE tenant_id = cv.tenant_id AND conversation_id = cv.id \
             ORDER BY created_at DESC, seq DESC \
             LIMIT 1 \
         ) preview ON TRUE \
         WHERE cv.tenant_id = $1 AND cv.id = $2 AND cv.deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?;

    let Some(row) = row else { return Ok(None) };

    let participants = participants_in_tx(tx, tenant_id, id).await?;

    let status: ConversationStatus =
        serde_json::from_value(Value::String(row.status)).unwrap_or(ConversationStatus::Open);

    let assignee = row.assigned_membership_id.map(|mid| Assignee {
        membership_id: mid,
        display_name: row.assignee_display_name.unwrap_or_default(),
        active: row.assignee_active,
    });

    let last_message = row.last_message_kind.map(|kind| {
        let preview = row.last_message_preview.unwrap_or_default();
        let kind: MessageKind =
            serde_json::from_value(Value::String(kind)).unwrap_or(MessageKind::Reply);
        LastMessagePreview { kind, preview }
    });

    let widget_instance = row
        .widget_instance_id
        .map(|id| crate::model::WidgetInstanceRef {
            id,
            name: row.widget_instance_name.unwrap_or_default(),
        });

    Ok(Some(ConversationDetail {
        id: row.id,
        customer: CustomerRef {
            id: row.customer_id,
            display_name: row.customer_display_name,
        },
        channel: row.channel,
        status,
        assignee,
        last_message,
        last_activity_at: row.last_activity_at,
        created_at: row.created_at,
        participants,
        ai_handling: row.ai_handling,
        awaiting_ai_decision: false,
        widget_instance,
    }))
}

// ---------------------------------------------------------------------------
// T027 — Timeline query (keyset-paginated messages)
// ---------------------------------------------------------------------------

/// Fetch a keyset-paginated page of messages for a conversation, ordered by
/// `created_at DESC, seq DESC`. Includes sender info via joins (customer for
/// kind='customer', agent for kind='reply'/'note'). Uses an opaque hex cursor
/// encoding `(created_at, seq)`. Over-fetches by one to determine `has_more`.
pub async fn timeline_query_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    cursor: Option<String>,
    limit: i64,
) -> sqlx::Result<(Vec<Message>, bool, Option<String>)> {
    let decoded_cursor = cursor.as_ref().and_then(|c| decode_timeline_cursor(c));

    let mut sql = String::from(
        "SELECT m.id, m.kind, m.body, m.created_at, m.seq, \
                c.id AS customer_id, c.display_name AS customer_display_name, \
                u.id AS agent_user_id, m.sender_membership_id, \
                COALESCE(u.display_name, '') AS agent_display_name, \
                COALESCE(tm.status = 'active', false) AS agent_active, \
                m.logged_by_membership_id, \
                lbu.id AS logged_by_user_id, \
                COALESCE(lbu.display_name, '') AS logged_by_display_name, \
                COALESCE(lbtm.status = 'active', false) AS logged_by_active, \
                m.ai_confidence_score \
         FROM messages m \
         JOIN conversations cv \
           ON cv.id = m.conversation_id AND cv.tenant_id = m.tenant_id \
         JOIN customers c \
           ON c.id = cv.customer_id AND c.tenant_id = cv.tenant_id \
         LEFT JOIN tenant_memberships tm \
           ON tm.id = m.sender_membership_id AND tm.tenant_id = m.tenant_id AND tm.deleted_at IS NULL \
         LEFT JOIN users u ON u.id = tm.user_id \
         LEFT JOIN tenant_memberships lbtm \
           ON lbtm.id = m.logged_by_membership_id AND lbtm.tenant_id = m.tenant_id AND lbtm.deleted_at IS NULL \
         LEFT JOIN users lbu ON lbu.id = lbtm.user_id \
         WHERE m.tenant_id = $1 AND m.conversation_id = $2",
    );

    let mut next_bind = 3u16;

    if let Some((_ts, _seq)) = decoded_cursor {
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

    let mut query = sqlx::query_as::<_, TimelineRow>(&sql)
        .bind(tenant_id)
        .bind(conversation_id);

    if let Some((ts, seq)) = decoded_cursor {
        query = query.bind(ts).bind(seq);
    }

    query = query.bind(limit + 1);

    let rows = query.fetch_all(&mut **tx).await?;
    let has_more = rows.len() > limit as usize;
    let next_cursor = has_more.then(|| {
        let last = &rows[limit as usize - 1];
        encode_timeline_cursor(last.created_at, last.seq)
    });
    let data: Vec<Message> = rows
        .into_iter()
        .take(limit as usize)
        .map(timeline_row_to_message)
        .collect();

    // T034: Batch-load citations for the returned messages
    let message_ids: Vec<Uuid> = data.iter().map(|m| m.id).collect();
    let mut citation_map = if message_ids.is_empty() {
        HashMap::new()
    } else {
        load_citations_for_messages(pool, tenant_id, &message_ids).await?
    };
    let mut data_with_citations: Vec<Message> = Vec::with_capacity(data.len());
    for mut message in data {
        if let Some(citations) = citation_map.remove(&message.id) {
            message.citations = citations;
        }
        data_with_citations.push(message);
    }

    Ok((data_with_citations, has_more, next_cursor))
}

// ---------------------------------------------------------------------------
// T039 — Add message (insert message, bump last_activity_at, auto-reopen)
// ---------------------------------------------------------------------------

/// Insert a message into the conversation, bump `last_activity_at`, and
/// auto-reopen when kind is customer/reply and status is resolved/closed.
/// Returns the inserted message and the updated conversation status ref.
#[allow(clippy::too_many_arguments)]
pub async fn add_message_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    kind: &str,
    body: &str,
    sender_membership_id: Option<Uuid>,
    logged_by_membership_id: Option<Uuid>,
    actor: ConversationActor,
) -> sqlx::Result<(Message, ConversationStatusRef)> {
    let actor_user_id: Option<Uuid> = match actor {
        ConversationActor::Staff { user_id, .. } => Some(user_id),
        ConversationActor::Visitor { .. } => None,
    };

    // 1. Insert the message
    let inserted = sqlx::query_as::<_, (Uuid, String, String, DateTime<Utc>, i64)>(
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
    .fetch_one(&mut **tx)
    .await?;

    // 2. Bump last_activity_at and read current status
    let current_status: String = sqlx::query_scalar(
        "UPDATE conversations SET last_activity_at = now() \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL \
         RETURNING status",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&mut **tx)
    .await?;

    // 3. Auto-reopen when kind is customer/reply and status is resolved/closed
    let final_status = if matches!(kind, "customer" | "reply" | "ai")
        && matches!(current_status.as_str(), "resolved" | "closed")
    {
        let new_status: String = sqlx::query_scalar(
            "UPDATE conversations SET status = 'open' \
             WHERE tenant_id = $1 AND id = $2 \
             RETURNING status",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_one(&mut **tx)
        .await?;

        crate::audit::record_status_changed(
            tx,
            actor_user_id,
            tenant_id,
            conversation_id,
            &current_status,
            "open",
            true,
        )
        .await?;

        new_status
    } else {
        current_status
    };

    let updated_at: DateTime<Utc> = sqlx::query_scalar(
        "SELECT last_activity_at FROM conversations WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&mut **tx)
    .await?;

    // 4. Build the Message response with sender/logger info
    let customer_info = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT c.id, c.display_name \
         FROM conversations cv \
         JOIN customers c ON c.id = cv.customer_id AND c.tenant_id = cv.tenant_id \
         WHERE cv.tenant_id = $1 AND cv.id = $2",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&mut **tx)
    .await?;

    let kind_enum: MessageKind =
        serde_json::from_value(Value::String(kind.to_string())).unwrap_or(MessageKind::Reply);

    let sender = match kind_enum {
        MessageKind::Customer => Participant {
            participant_type: "customer".into(),
            id: Some(customer_info.0),
            membership_id: None,
            display_name: customer_info.1,
            active: None,
        },
        MessageKind::Ai => Participant {
            participant_type: "ai_agent".into(),
            id: None,
            membership_id: None,
            display_name: "<agent name>".into(),
            active: None,
        },
        MessageKind::System => Participant {
            participant_type: "system".into(),
            id: None,
            membership_id: None,
            display_name: "Automated reply".into(),
            active: None,
        },
        _ => {
            let agent_info = sqlx::query_as::<_, (Uuid, String, bool)>(
                "SELECT u.id, u.display_name, tm.status = 'active' AS active \
                 FROM tenant_memberships tm \
                 JOIN users u ON u.id = tm.user_id \
                 WHERE tm.id = $1 AND tm.tenant_id = $2",
            )
            .bind(sender_membership_id)
            .bind(tenant_id)
            .fetch_optional(&mut **tx)
            .await?;

            match agent_info {
                Some((user_id, display_name, active)) => Participant {
                    participant_type: "agent".into(),
                    id: Some(user_id),
                    membership_id: sender_membership_id,
                    display_name,
                    active: Some(active),
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

    let logged_by = if kind_enum == MessageKind::Customer {
        match logged_by_membership_id {
            Some(mid) => {
                match sqlx::query_as::<_, (String, bool)>(
                    "SELECT u.display_name, tm.status = 'active' AS active \
                     FROM tenant_memberships tm \
                     JOIN users u ON u.id = tm.user_id \
                     WHERE tm.id = $1 AND tm.tenant_id = $2",
                )
                .bind(mid)
                .bind(tenant_id)
                .fetch_optional(&mut **tx)
                .await
                {
                    Ok(Some((display_name, active))) => Some(Assignee {
                        membership_id: mid,
                        display_name,
                        active,
                    }),
                    _ => None,
                }
            }
            None => None,
        }
    } else {
        None
    };

    let status_enum: ConversationStatus =
        serde_json::from_value(Value::String(final_status)).unwrap_or(ConversationStatus::Open);

    Ok((
        Message {
            id: inserted.0,
            kind: kind_enum,
            sender,
            logged_by,
            body: inserted.2,
            created_at: inserted.3,
            citations: Vec::new(),
            confidence: None,
        },
        ConversationStatusRef {
            status: status_enum,
            last_activity_at: updated_at,
        },
    ))
}

// ---------------------------------------------------------------------------
// T008 — assign_in_tx: extract-only assignment, used by escalations routing
// ---------------------------------------------------------------------------

/// Assign (or unassign) a conversation's membership. Validates target via
/// `active_membership_exists_in_tx`. Writes audit and outbox. Does not
/// lock the row — caller must hold `SELECT ... FOR UPDATE` if needed.
/// Used by the escalations routing engine (origin = "escalations") and
/// by `patch_conversation_in_tx` for the assignment half.
pub async fn assign_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    assigned_membership_id: Option<Uuid>,
    actor_user_id: Option<Uuid>,
    origin: &str,
) -> sqlx::Result<()> {
    let prior: (Option<Uuid>,) = sqlx::query_as(
        "SELECT assigned_membership_id FROM conversations \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&mut **tx)
    .await?;

    if prior.0 == assigned_membership_id {
        return Ok(());
    }

    if let Some(mid) = assigned_membership_id {
        if !active_membership_exists_in_tx(tx, tenant_id, mid).await? {
            return Err(sqlx::Error::Protocol(format!(
                "Membership {mid} is not active in tenant {tenant_id}"
            )));
        }
    }

    sqlx::query(
        "UPDATE conversations SET assigned_membership_id = $1 \
         WHERE tenant_id = $2 AND id = $3",
    )
    .bind(assigned_membership_id)
    .bind(tenant_id)
    .bind(conversation_id)
    .execute(&mut **tx)
    .await?;

    if let Some(uid) = actor_user_id {
        crate::audit::record_assignment_changed(
            tx,
            uid,
            tenant_id,
            conversation_id,
            prior.0,
            assigned_membership_id,
        )
        .await?;
    }

    crate::outbox::emit_assignment_changed_in_tx(
        tx,
        tenant_id,
        conversation_id,
        prior.0,
        assigned_membership_id,
        actor_user_id,
        origin,
    )
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// T008 — set_escalated_in_tx: maintains conversations.escalated_at column
// ---------------------------------------------------------------------------

/// Set or clear the `escalated_at` flag. Written only by the escalations
/// module via this public interface (module ownership, R5).
pub async fn set_escalated_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    escalated_at: Option<chrono::DateTime<chrono::Utc>>,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE conversations SET escalated_at = $1 \
         WHERE tenant_id = $2 AND id = $3",
    )
    .bind(escalated_at)
    .bind(tenant_id)
    .bind(conversation_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// T049 — Patch conversation (lock + update status/assignment)
// ---------------------------------------------------------------------------

/// Lock the conversation row (SELECT FOR UPDATE), read prior values, and
/// apply status and/or assignment changes. Validates assignment target via
/// `active_membership_exists_in_tx`. Skips no-op audits. Writes audit rows
/// and outbox events for changes. Returns the updated `ConversationDetail`.
pub async fn patch_conversation_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    id: Uuid,
    status: Option<&str>,
    assigned_membership_id: Option<Option<Uuid>>,
    actor_user_id: Uuid,
) -> sqlx::Result<ConversationDetail> {
    // 1. Lock and read prior values
    let prior = sqlx::query_as::<_, (String, Option<Uuid>)>(
        "SELECT status, assigned_membership_id \
         FROM conversations \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL \
         FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(id)
    .fetch_one(&mut **tx)
    .await?;

    // 2. Apply status change
    if let Some(new_status) = status {
        if new_status != prior.0 {
            sqlx::query(
                "UPDATE conversations SET status = $1 \
                 WHERE tenant_id = $2 AND id = $3",
            )
            .bind(new_status)
            .bind(tenant_id)
            .bind(id)
            .execute(&mut **tx)
            .await?;

            crate::audit::record_status_changed(
                tx,
                Some(actor_user_id),
                tenant_id,
                id,
                &prior.0,
                new_status,
                false,
            )
            .await?;

            crate::outbox::emit_status_changed_in_tx(
                tx,
                tenant_id,
                id,
                &prior.0,
                new_status,
                None,
                "conversations",
            )
            .await?;
        }
    }

    // 3. Apply assignment change (delegate to assign_in_tx)
    if let Some(new_assignment) = assigned_membership_id {
        let prior_assignment = prior.1;
        if new_assignment != prior_assignment {
            assign_in_tx(
                tx,
                tenant_id,
                id,
                new_assignment,
                Some(actor_user_id),
                "conversations",
            )
            .await?;
        }
    }

    // 4. Fetch and return the updated detail
    let detail = detail_query_in_tx(tx, tenant_id, id)
        .await?
        .expect("just-updated conversation must exist");

    Ok(detail)
}

// ---------------------------------------------------------------------------
// T058 — Create conversation (insert conversation + first message + audit)
// ---------------------------------------------------------------------------

/// Create a conversation with its first (reply) message. Checks customer
/// existence first. Inserts conversation row (status='open',
/// assigned_membership_id=NULL), inserts the first message (kind='reply',
/// sender=actor), bumps last_activity_at, and writes a `conversation.created`
/// audit row. Returns the new `ConversationDetail`.
pub async fn create_conversation_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    customer_id: Uuid,
    channel: &str,
    body: &str,
    actor: ConversationActor,
    widget_instance_id: Option<Uuid>,
) -> sqlx::Result<ConversationDetail> {
    // 1. Check customer exists
    let customer_ok = customers::customer_exists_in_tx(tx, tenant_id, customer_id).await?;
    if !customer_ok {
        return Err(sqlx::Error::Protocol(format!(
            "Customer {customer_id} not found in tenant {tenant_id}"
        )));
    }

    let (actor_user_id, actor_membership_id) = match actor {
        ConversationActor::Staff {
            user_id,
            membership_id,
        } => (Some(user_id), Some(membership_id)),
        ConversationActor::Visitor { .. } => (None, None),
    };

    // 2. Insert conversation (optionally with widget_instance_id)
    let conv_id: Uuid = if let Some(wid) = widget_instance_id {
        sqlx::query_scalar(
            "INSERT INTO conversations (tenant_id, customer_id, channel, status, widget_instance_id) \
             VALUES ($1, $2, $3, 'open', $4) \
             RETURNING id",
        )
        .bind(tenant_id)
        .bind(customer_id)
        .bind(channel)
        .bind(wid)
        .fetch_one(&mut **tx)
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
        .fetch_one(&mut **tx)
        .await?
    };

    // 3. Insert first message (kind='reply', sender=actor)
    sqlx::query(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body, sender_membership_id) \
         VALUES ($1, $2, 'reply', $3, $4)",
    )
    .bind(tenant_id)
    .bind(conv_id)
    .bind(body)
    .bind(actor_membership_id)
    .execute(&mut **tx)
    .await?;

    // 4. Bump last_activity_at
    sqlx::query(
        "UPDATE conversations SET last_activity_at = now() WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant_id)
    .bind(conv_id)
    .execute(&mut **tx)
    .await?;

    // 5. Write audit
    crate::audit::record_conversation_created(
        tx,
        actor_user_id,
        tenant_id,
        conv_id,
        customer_id,
        channel,
    )
    .await?;

    // 6. Return the full detail
    let detail = detail_query_in_tx(tx, tenant_id, conv_id)
        .await?
        .expect("just-created conversation must exist");

    Ok(detail)
}

// ---------------------------------------------------------------------------
// AI Agent responder helpers
// ---------------------------------------------------------------------------

pub async fn conversation_ai_state(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Option<(String, Option<String>)>> {
    sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status, ai_handling \
         FROM conversations \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_optional(pool)
    .await
}

pub async fn has_system_message(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS( \
         SELECT 1 FROM messages \
         WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'system' \
         )",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(pool)
    .await
}

pub async fn has_system_message_batch(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_ids: &[Uuid],
) -> sqlx::Result<Vec<(Uuid, bool)>> {
    if conversation_ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<(Uuid, bool)> = sqlx::query_as(
        "SELECT conversation_id, EXISTS( \
         SELECT 1 FROM messages m \
         WHERE m.tenant_id = $1 \
         AND m.conversation_id = ANY($2) \
         AND m.kind = 'system' \
         ) AS has_ack \
         FROM unnest($2::uuid[]) AS conversation_id",
    )
    .bind(tenant_id)
    .bind(conversation_ids)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn insert_auto_ack_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    body: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'system', $3)",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(body)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "UPDATE conversations SET last_activity_at = now() \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub async fn insert_ai_reply_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    body: &str,
    confidence_score: Option<f32>,
) -> sqlx::Result<Uuid> {
    let message_id: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body, ai_confidence_score) \
         VALUES ($1, $2, 'ai', $3, $4) \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(body)
    .bind(confidence_score)
    .fetch_one(&mut **tx)
    .await?;

    sqlx::query(
        "UPDATE conversations SET last_activity_at = now() \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .execute(&mut **tx)
    .await?;

    Ok(message_id)
}

pub async fn insert_citations_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    message_id: Uuid,
    citations: &[CitationToInsert],
) -> sqlx::Result<()> {
    if citations.is_empty() {
        return Ok(());
    }
    for citation in citations {
        sqlx::query(
            "INSERT INTO message_citations \
                (tenant_id, message_id, knowledge_item_id, item_title, \
                 passage_text, relevance_score, ordinal) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(tenant_id)
        .bind(message_id)
        .bind(citation.knowledge_item_id)
        .bind(&citation.item_title)
        .bind(&citation.passage_text)
        .bind(citation.relevance_score)
        .bind(citation.ordinal)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub async fn load_citations_for_messages(
    pool: &PgPool,
    tenant_id: Uuid,
    message_ids: &[Uuid],
) -> sqlx::Result<HashMap<Uuid, Vec<CitationView>>> {
    if message_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows: Vec<(Uuid, Uuid, String, String, f32)> = sqlx::query_as(
        "SELECT message_id, knowledge_item_id, item_title, passage_text, relevance_score \
         FROM message_citations \
         WHERE tenant_id = $1 AND message_id = ANY($2) \
         ORDER BY ordinal ASC",
    )
    .bind(tenant_id)
    .bind(message_ids)
    .fetch_all(pool)
    .await?;

    let mut item_ids: Vec<Uuid> = rows.iter().map(|r| r.1).collect();
    item_ids.sort();
    item_ids.dedup();

    let available: std::collections::HashSet<Uuid> = if item_ids.is_empty() {
        std::collections::HashSet::new()
    } else {
        let existing: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM knowledge_items \
             WHERE id = ANY($1) AND deleted_at IS NULL",
        )
        .bind(&item_ids)
        .fetch_all(pool)
        .await?;
        existing.into_iter().map(|r| r.0).collect()
    };

    let mut map: HashMap<Uuid, Vec<CitationView>> = HashMap::new();
    for (message_id, knowledge_item_id, item_title, passage_text, relevance_score) in rows {
        let item_available = available.contains(&knowledge_item_id);
        map.entry(message_id).or_default().push(CitationView {
            knowledge_item_id,
            item_title,
            passage_text,
            relevance_score,
            item_available,
        });
    }
    Ok(map)
}

pub async fn has_ai_reply_since(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    since_message_id: Uuid,
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS( \
         SELECT 1 FROM messages \
         WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai' \
         AND created_at > (SELECT created_at FROM messages WHERE id = $3 AND tenant_id = $1) \
         )",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(since_message_id)
    .fetch_one(pool)
    .await
}

pub async fn customer_display_name(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT c.display_name FROM conversations conv \
         JOIN customers c ON c.id = conv.customer_id \
         WHERE conv.tenant_id = $1 AND conv.id = $2 AND conv.deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_optional(pool)
    .await
}

pub async fn message_body(
    pool: &PgPool,
    tenant_id: Uuid,
    message_id: Uuid,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT body FROM messages WHERE tenant_id = $1 AND id = $2")
        .bind(tenant_id)
        .bind(message_id)
        .fetch_optional(pool)
        .await
}

pub async fn set_ai_handling_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    mode: &str,
) -> sqlx::Result<bool> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "UPDATE conversations SET ai_handling = $1 \
         WHERE tenant_id = $2 AND id = $3 AND ai_handling IS DISTINCT FROM 'human' \
         RETURNING id",
    )
    .bind(mode)
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.is_some())
}

pub async fn recent_history(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    limit: i64,
) -> sqlx::Result<Vec<(String, String)>> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT kind, body FROM messages \
         WHERE tenant_id = $1 AND conversation_id = $2 AND kind IN ('customer', 'reply', 'ai') \
         ORDER BY created_at ASC, seq ASC \
         LIMIT $3",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn insert_fallback_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
    body: &str,
) -> sqlx::Result<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'system', $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(body)
    .fetch_one(&mut **tx)
    .await?;

    sqlx::query(
        "UPDATE conversations SET last_activity_at = now() \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .execute(&mut **tx)
    .await?;

    Ok(id)
}

pub async fn summary_history(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    limit: i64,
) -> sqlx::Result<Vec<(String, String)>> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT kind, body FROM messages \
         WHERE tenant_id = $1 AND conversation_id = $2 AND kind IN ('customer', 'ai', 'reply', 'system') \
         ORDER BY created_at ASC, seq ASC \
         LIMIT $3",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(limit)
    .fetch_all(pool)
    .await
}

pub async fn customer_id_for_conversation(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "SELECT customer_id FROM conversations \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| sqlx::Error::Protocol("Conversation not found or soft-deleted".into()))
}

pub async fn has_customer_message_after(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    after_message_id: Uuid,
) -> sqlx::Result<bool> {
    let created_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT created_at FROM messages WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id)
            .bind(after_message_id)
            .fetch_optional(pool)
            .await?;

    let after = match created_at {
        Some(t) => t,
        None => return Ok(false),
    };

    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS ( \
         SELECT 1 FROM messages \
         WHERE tenant_id = $1 AND conversation_id = $2 \
         AND kind = 'customer' \
         AND created_at > $3 \
         LIMIT 1 \
         )",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(after)
    .fetch_one(pool)
    .await
}
