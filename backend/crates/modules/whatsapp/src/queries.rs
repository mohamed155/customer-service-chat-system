use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::model::MessageAttachmentRow;

/// Insert inbound message meta. Returns false when the wamid already exists
/// (deduplication signal). Pre-checks existence, then relies on the unique
/// index for race safety.
pub async fn insert_inbound_meta_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    message_id: Uuid,
    conversation_id: Uuid,
    wamid: &str,
    provider_timestamp: Option<DateTime<Utc>>,
) -> sqlx::Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM whatsapp_message_meta WHERE tenant_id = $1 AND wamid = $2)",
    )
    .bind(tenant_id)
    .bind(wamid)
    .fetch_one(&mut **tx)
    .await?;

    if exists {
        return Ok(false);
    }

    sqlx::query(
        "INSERT INTO whatsapp_message_meta (tenant_id, message_id, conversation_id, direction, wamid, provider_timestamp) \
         VALUES ($1, $2, $3, 'inbound', $4, $5)",
    )
    .bind(tenant_id)
    .bind(message_id)
    .bind(conversation_id)
    .bind(wamid)
    .bind(provider_timestamp)
    .execute(&mut **tx)
    .await?;

    Ok(true)
}

/// Insert outbound message meta with pending delivery status.
pub async fn insert_outbound_meta_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    message_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO whatsapp_message_meta (tenant_id, message_id, conversation_id, direction, delivery_status) \
         VALUES ($1, $2, $3, 'outbound', 'pending')",
    )
    .bind(tenant_id)
    .bind(message_id)
    .bind(conversation_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Set outbound message as sent (with the provider wamid).
pub async fn set_outbound_sent_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    message_id: Uuid,
    wamid: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE whatsapp_message_meta \
         SET delivery_status = 'sent', wamid = $1 \
         WHERE tenant_id = $2 AND message_id = $3",
    )
    .bind(wamid)
    .bind(tenant_id)
    .bind(message_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Mark outbound message as failed with a reason.
pub async fn set_outbound_failed_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    message_id: Uuid,
    reason: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE whatsapp_message_meta \
         SET delivery_status = 'failed', failure_reason = $1 \
         WHERE tenant_id = $2 AND message_id = $3",
    )
    .bind(reason)
    .bind(tenant_id)
    .bind(message_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Update delivery status by wamid. Only performs the update if the new
/// status has a strictly higher rank than the current status (monotonicity
/// enforcement). Returns `Some((conversation_id, message_id))` when an update
/// was applied, or `None` when no row matches, the status is invalid, or the
/// change is not a monotonic upgrade.
pub async fn update_status_by_wamid_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    wamid: &str,
    new_status: &str,
    failure_reason: Option<&str>,
) -> sqlx::Result<Option<(Uuid, Uuid)>> {
    use crate::model::DeliveryStatus;

    let new_rank = match new_status.parse::<DeliveryStatus>() {
        Ok(s) => s.rank(),
        Err(_) => return Ok(None),
    };

    let current: Option<(String, Uuid, Uuid)> = sqlx::query_as(
        "SELECT COALESCE(delivery_status, 'pending'), message_id, conversation_id \
         FROM whatsapp_message_meta \
         WHERE tenant_id = $1 AND wamid = $2",
    )
    .bind(tenant_id)
    .bind(wamid)
    .fetch_optional(&mut **tx)
    .await?;

    let (current_status_str, message_id, conversation_id) = match current {
        Some(row) => row,
        None => return Ok(None),
    };

    let current_rank = match current_status_str.parse::<DeliveryStatus>() {
        Ok(s) => s.rank(),
        Err(_) => return Ok(None),
    };

    if new_rank <= current_rank {
        return Ok(None);
    }

    let reason = failure_reason.filter(|_| new_status == "failed");

    sqlx::query(
        "UPDATE whatsapp_message_meta \
         SET delivery_status = $1, failure_reason = $2 \
         WHERE tenant_id = $3 AND wamid = $4",
    )
    .bind(new_status)
    .bind(reason)
    .bind(tenant_id)
    .bind(wamid)
    .execute(&mut **tx)
    .await?;

    Ok(Some((conversation_id, message_id)))
}

/// Insert a new attachment record. Returns the generated id.
pub async fn insert_attachment_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    message_id: Uuid,
    kind: &str,
    provider_media_id: &str,
    file_name: Option<&str>,
) -> sqlx::Result<Uuid> {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO message_attachments (tenant_id, message_id, kind, provider_media_id, file_name) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(message_id)
    .bind(kind)
    .bind(provider_media_id)
    .bind(file_name)
    .fetch_one(&mut **tx)
    .await?;
    Ok(row.0)
}

/// Claim the next pending attachment for processing (FOR UPDATE SKIP LOCKED).
/// Only claims attachments with fewer than 5 fetch attempts.
pub async fn claim_pending_attachment(
    pool: &sqlx::PgPool,
) -> sqlx::Result<Option<MessageAttachmentRow>> {
    sqlx::query_as::<_, MessageAttachmentRow>(
        "SELECT id, tenant_id, message_id, kind, status, provider_media_id, \
                storage_key, mime_type, size_bytes, file_name, fetch_attempts, \
                created_at, updated_at \
         FROM message_attachments \
         WHERE status = 'pending' AND fetch_attempts < 5 \
         ORDER BY created_at ASC \
         LIMIT 1 \
         FOR UPDATE SKIP LOCKED",
    )
    .fetch_optional(pool)
    .await
}

/// Mark an attachment as stored with its file metadata.
pub async fn mark_attachment_stored(
    pool: &sqlx::PgPool,
    id: Uuid,
    storage_key: &str,
    mime_type: &str,
    size_bytes: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE message_attachments \
         SET status = 'stored', storage_key = $1, mime_type = $2, size_bytes = $3 \
         WHERE id = $4",
    )
    .bind(storage_key)
    .bind(mime_type)
    .bind(size_bytes)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Increment fetch_attempts and auto-fail if threshold reached.
pub async fn mark_attachment_failed_attempt(
    pool: &sqlx::PgPool,
    id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE message_attachments \
         SET fetch_attempts = fetch_attempts + 1, \
             status = CASE WHEN fetch_attempts + 1 >= 5 THEN 'failed' ELSE status END \
         WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch a stored attachment by id, verifying that it belongs to the right
/// conversation via a join on the messages table.
pub async fn attachment_for_download(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    attachment_id: Uuid,
) -> sqlx::Result<Option<MessageAttachmentRow>> {
    sqlx::query_as::<_, MessageAttachmentRow>(
        "SELECT a.id, a.tenant_id, a.message_id, a.kind, a.status, a.provider_media_id, \
                a.storage_key, a.mime_type, a.size_bytes, a.file_name, a.fetch_attempts, \
                a.created_at, a.updated_at \
         FROM message_attachments a \
         JOIN messages m ON m.id = a.message_id AND m.tenant_id = a.tenant_id \
         WHERE a.tenant_id = $1 AND a.id = $2 AND m.conversation_id = $3 AND a.status = 'stored'",
    )
    .bind(tenant_id)
    .bind(attachment_id)
    .bind(conversation_id)
    .fetch_optional(pool)
    .await
}

/// Retrieve the WhatsApp identifier for a customer (from customer_channel_identifiers).
pub async fn customer_whatsapp_identifier(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    customer_id: Uuid,
) -> sqlx::Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT identifier FROM customer_channel_identifiers \
         WHERE tenant_id = $1 AND customer_id = $2 AND channel = 'whatsapp' AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|r| r.0))
}

/// Fetch the body of an outbound message (must be reply or ai kind).
pub async fn message_body_for_outbound(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    message_id: Uuid,
) -> sqlx::Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT body FROM messages WHERE tenant_id = $1 AND id = $2 AND kind IN ('reply', 'ai')",
    )
    .bind(tenant_id)
    .bind(message_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.0))
}
