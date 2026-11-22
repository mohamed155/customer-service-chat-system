//! Single-statement reads and writes of conversations and messages that other modules call
//! (the AI responder, the WhatsApp sender, the widget, escalations) and that carry no rule of
//! their own. Each runs in the tenant Unit of Work the caller holds, or opens one for the tenant.

use std::collections::HashMap;

use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::application::model::{AttachmentInfo, CitationToInsert, CitationView, DeliveryInfo};

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
// AI Agent responder helpers
// ---------------------------------------------------------------------------

pub async fn conversation_ai_state(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Option<(String, Option<String>)>> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<Option<(String, Option<String>)>> = async {
        sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT status, ai_handling \
         FROM conversations \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_optional(uow.conn())
        .await
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

pub async fn has_system_message(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<bool> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<bool> = async {
        sqlx::query_scalar(
            "SELECT EXISTS( \
         SELECT 1 FROM messages \
         WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'system' \
         )",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_one(uow.conn())
        .await
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

pub async fn has_system_message_batch(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_ids: &[Uuid],
) -> sqlx::Result<Vec<(Uuid, bool)>> {
    if conversation_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<Vec<(Uuid, bool)>> = async {
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
        .fetch_all(uow.conn())
        .await?;
        Ok(rows)
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

pub async fn insert_auto_ack_in_tx(
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
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<HashMap<Uuid, Vec<CitationView>>> = async {
        let rows: Vec<(Uuid, Uuid, String, String, f32)> = sqlx::query_as(
            "SELECT message_id, knowledge_item_id, item_title, passage_text, relevance_score \
         FROM message_citations \
         WHERE tenant_id = $1 AND message_id = ANY($2) \
         ORDER BY ordinal ASC",
        )
        .bind(tenant_id)
        .bind(message_ids)
        .fetch_all(uow.conn())
        .await?;

        let mut item_ids: Vec<Uuid> = rows.iter().map(|r| r.1).collect();
        item_ids.sort();
        item_ids.dedup();

        let available: std::collections::HashSet<Uuid> = if item_ids.is_empty() {
            std::collections::HashSet::new()
        } else {
            let existing: Vec<(Uuid,)> = sqlx::query_as(
                "SELECT id FROM knowledge_items \
             WHERE id = ANY($1) AND status = 'published'",
            )
            .bind(&item_ids)
            .fetch_all(uow.conn())
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
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

pub async fn has_ai_reply_since(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    since_message_id: Uuid,
) -> sqlx::Result<bool> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<bool> = async {
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
        .fetch_one(uow.conn())
        .await
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

pub async fn customer_display_name(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Option<String>> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<Option<String>> = async {
        sqlx::query_scalar(
            "SELECT c.display_name FROM conversations conv \
         JOIN customers c ON c.id = conv.customer_id \
         WHERE conv.tenant_id = $1 AND conv.id = $2 AND conv.deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_optional(uow.conn())
        .await
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

pub async fn message_body(
    pool: &PgPool,
    tenant_id: Uuid,
    message_id: Uuid,
) -> sqlx::Result<Option<String>> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<Option<String>> = async {
        sqlx::query_scalar("SELECT body FROM messages WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id)
            .bind(message_id)
            .fetch_optional(uow.conn())
            .await
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
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
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<Vec<(String, String)>> = async {
        sqlx::query_as::<_, (String, String)>(
            "SELECT kind, body FROM messages \
         WHERE tenant_id = $1 AND conversation_id = $2 AND kind IN ('customer', 'reply', 'ai') \
         ORDER BY created_at ASC, seq ASC \
         LIMIT $3",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .bind(limit)
        .fetch_all(uow.conn())
        .await
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
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
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<Vec<(String, String)>> = async {
    sqlx::query_as::<_, (String, String)>(
        "SELECT kind, body FROM messages \
         WHERE tenant_id = $1 AND conversation_id = $2 AND kind IN ('customer', 'ai', 'reply', 'system') \
         ORDER BY created_at ASC, seq ASC \
         LIMIT $3",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(limit)
    .fetch_all(uow.conn())
    .await
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

pub async fn customer_id_for_conversation(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Uuid> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<Uuid> = async {
        sqlx::query_scalar(
            "SELECT customer_id FROM conversations \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_optional(uow.conn())
        .await?
        .ok_or_else(|| sqlx::Error::Protocol("Conversation not found or soft-deleted".into()))
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

pub async fn find_open_conversation_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    customer_id: Uuid,
    channel: &str,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "SELECT id FROM conversations \
         WHERE tenant_id = $1 AND customer_id = $2 AND channel = $3 \
           AND status IN ('open', 'escalated') \
           AND deleted_at IS NULL \
         ORDER BY last_activity_at DESC \
         LIMIT 1",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(channel)
    .fetch_optional(&mut **tx)
    .await
}

pub async fn last_customer_message_at(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Option<chrono::DateTime<chrono::Utc>>> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<Option<chrono::DateTime<chrono::Utc>>> = async {
        sqlx::query_scalar(
            "SELECT MAX(created_at) FROM messages \
         WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'customer'",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .fetch_one(uow.conn())
        .await
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

pub async fn load_attachments_for_messages(
    pool: &PgPool,
    tenant_id: Uuid,
    message_ids: &[Uuid],
) -> sqlx::Result<HashMap<Uuid, Vec<AttachmentInfo>>> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<HashMap<Uuid, Vec<AttachmentInfo>>> = async {
        #[allow(clippy::type_complexity)]
        let rows: Vec<(
            Uuid,
            Uuid,
            String,
            String,
            Option<String>,
            Option<i64>,
            Option<String>,
            Option<String>,
        )> = sqlx::query_as(
            "SELECT id, message_id, kind, status, mime_type, size_bytes, file_name, storage_key \
         FROM message_attachments WHERE tenant_id = $1 AND message_id = ANY($2)",
        )
        .bind(tenant_id)
        .bind(message_ids)
        .fetch_all(uow.conn())
        .await?;

        let mut map: HashMap<Uuid, Vec<AttachmentInfo>> = HashMap::new();
        for (id, message_id, kind, status, mime_type, size_bytes, file_name, storage_key) in rows {
            let has_storage = storage_key.is_some();
            map.entry(message_id).or_default().push(AttachmentInfo {
                id,
                kind,
                status,
                mime_type,
                size_bytes,
                file_name,
                url: if has_storage {
                    Some(String::new())
                } else {
                    None
                },
            });
        }
        Ok(map)
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

pub async fn load_whatsapp_meta_for_messages(
    pool: &PgPool,
    tenant_id: Uuid,
    message_ids: &[Uuid],
) -> sqlx::Result<HashMap<Uuid, DeliveryInfo>> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<HashMap<Uuid, DeliveryInfo>> = async {
        let rows: Vec<(Uuid, String, Option<String>)> = sqlx::query_as(
            "SELECT message_id, COALESCE(delivery_status, 'pending'), failure_reason \
         FROM whatsapp_message_meta \
         WHERE tenant_id = $1 AND message_id = ANY($2) AND direction = 'outbound'",
        )
        .bind(tenant_id)
        .bind(message_ids)
        .fetch_all(uow.conn())
        .await?;

        let mut map = HashMap::new();
        for (message_id, status, failure_reason) in rows {
            map.insert(
                message_id,
                DeliveryInfo {
                    status,
                    failure_reason,
                },
            );
        }
        Ok(map)
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

pub async fn has_customer_message_after(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    after_message_id: Uuid,
) -> sqlx::Result<bool> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<bool> = async {
        let created_at: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT created_at FROM messages WHERE tenant_id = $1 AND id = $2")
                .bind(tenant_id)
                .bind(after_message_id)
                .fetch_optional(uow.conn())
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
        .fetch_one(uow.conn())
        .await
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}
