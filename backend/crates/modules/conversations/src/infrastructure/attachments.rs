//! A message's attachments (MOH-194, ADR-017).
//!
//! `message_attachments` is owned by `conversations`. A channel that receives media records the
//! attachment here when the message arrives, a worker later marks it stored or failed once it has
//! fetched the file, and the download route checks that an attachment belongs to the conversation
//! it is asked for.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// A row of `message_attachments`.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AttachmentRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub message_id: Uuid,
    pub kind: String,
    pub status: String,
    pub provider_media_id: Option<String>,
    pub storage_key: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub file_name: Option<String>,
    pub fetch_attempts: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// How many fetch attempts an attachment gets before it is marked failed.
pub const MAX_FETCH_ATTEMPTS: i32 = 5;

/// Records an attachment the channel has announced but whose file is not fetched yet. Returns its ID.
pub async fn insert_pending_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    message_id: Uuid,
    kind: &str,
    provider_media_id: &str,
    file_name: Option<&str>,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
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
    .await
}

/// Marks an attachment as stored, with the file's metadata.
pub async fn mark_stored(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    id: Uuid,
    storage_key: &str,
    mime_type: &str,
    size_bytes: i64,
) -> sqlx::Result<()> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    mark_stored_in_tenant(
        uow.conn(),
        tenant_id,
        id,
        storage_key,
        mime_type,
        size_bytes,
    )
    .await?;
    uow.commit().await
}

pub async fn lock_pending(
    conn: &mut sqlx::PgConnection,
    tenant: Uuid,
    id: Uuid,
) -> sqlx::Result<bool> {
    Ok(sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM message_attachments
         WHERE tenant_id = $1 AND id = $2 AND status = 'pending' FOR UPDATE",
    )
    .bind(tenant)
    .bind(id)
    .fetch_optional(conn)
    .await?
    .is_some())
}

pub async fn mark_stored_in_tenant(
    conn: &mut sqlx::PgConnection,
    tenant: Uuid,
    id: Uuid,
    storage_key: &str,
    mime_type: &str,
    size_bytes: i64,
) -> sqlx::Result<()> {
    let updated = sqlx::query(
        "UPDATE message_attachments
         SET status = 'stored', storage_key = $1, mime_type = $2, size_bytes = $3
         WHERE id = $4 AND tenant_id = $5",
    )
    .bind(storage_key)
    .bind(mime_type)
    .bind(size_bytes)
    .bind(id)
    .bind(tenant)
    .execute(conn)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}

/// Counts one more failed fetch, and marks the attachment failed once it has used
/// [`MAX_FETCH_ATTEMPTS`].
pub async fn mark_failed_attempt(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    id: Uuid,
) -> sqlx::Result<()> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<()> = async {
        sqlx::query(
            "UPDATE message_attachments \
             SET fetch_attempts = fetch_attempts + 1, \
                 status = CASE WHEN fetch_attempts + 1 >= $3 THEN 'failed' ELSE status END \
             WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .bind(MAX_FETCH_ATTEMPTS)
        .execute(uow.conn())
        .await?;
        Ok(())
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

/// A stored attachment, only if its message belongs to `conversation_id`: asking for an attachment
/// through the wrong conversation finds nothing.
pub async fn stored_for_conversation(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    attachment_id: Uuid,
) -> sqlx::Result<Option<AttachmentRow>> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let customer: Option<Uuid> = sqlx::query_scalar(
        "SELECT customer_id FROM conversations WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_optional(uow.conn())
    .await?;
    let Some(customer) = customer else {
        return Ok(None);
    };
    if customers::customer_privacy::lock_customer(uow.conn(), tenant_id, customer).await?
        != Some(false)
    {
        return Ok(None);
    }
    let result: sqlx::Result<Option<AttachmentRow>> = async {
        sqlx::query_as::<_, AttachmentRow>(
            "SELECT a.id, a.tenant_id, a.message_id, a.kind, a.status, a.provider_media_id, \
                    a.storage_key, a.mime_type, a.size_bytes, a.file_name, a.fetch_attempts, \
                    a.created_at, a.updated_at \
             FROM message_attachments a \
             JOIN messages m ON m.id = a.message_id AND m.tenant_id = a.tenant_id \
             WHERE a.tenant_id = $1 AND a.id = $2 AND m.conversation_id = $3 \
               AND a.status = 'stored'",
        )
        .bind(tenant_id)
        .bind(attachment_id)
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
