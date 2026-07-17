use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use sqlx::PgPool;
use storage::ObjectStorage;
use uuid::Uuid;

use crate::chunking;
use crate::index_state::{self, IndexStatus};
use crate::store;

/// Embedding provider abstraction for the knowledge indexer.
/// Implemented by the `ai` crate's `AiService`.
#[async_trait::async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(
        &self,
        tenant_id: Uuid,
        texts: Vec<String>,
        request_id: String,
    ) -> Result<Vec<Vec<f32>>, EmbedError>;
}

#[derive(Debug, Clone)]
pub enum EmbedError {
    NotConfigured,
    Retriable(String),
    Permanent(String),
}

const MAX_RETRIES: i32 = 5;
const POLL_DELAY_IDLE: Duration = Duration::from_millis(100);
const POLL_DELAY_ERROR: Duration = Duration::from_secs(5);

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct ClaimedEvent {
    id: Uuid,
    tenant_id: Option<String>,
    aggregate_id: String,
    payload: serde_json::Value,
    attempts: i32,
}

pub async fn run_knowledge_indexer_worker(
    pool: PgPool,
    embedder: Arc<dyn Embedder>,
    storage: Arc<dyn ObjectStorage>,
) -> ! {
    loop {
        if let Err(e) = process_one_event(&pool, &*embedder, &*storage).await {
            tracing::error!(error = %e, "knowledge indexer: event processing failed");
            tokio::time::sleep(POLL_DELAY_ERROR).await;
        } else {
            tokio::time::sleep(POLL_DELAY_IDLE).await;
        }
    }
}

async fn process_one_event(
    pool: &PgPool,
    embedder: &dyn Embedder,
    storage: &dyn ObjectStorage,
) -> Result<bool, String> {
    // 1. Claim one outbox event with FOR UPDATE SKIP LOCKED
    let claim_token = Uuid::new_v4();
    let claimed = sqlx::query_as::<_, ClaimedEvent>(
        "UPDATE outbox_events \
         SET claimed_at = now(), claim_token = $1, attempts = attempts + 1 \
         WHERE id = ( \
             SELECT id FROM outbox_events \
             WHERE event_type = 'knowledge.index_requested' \
               AND claimed_at IS NULL \
               AND available_at <= now() \
               AND dead_lettered_at IS NULL \
             ORDER BY created_at ASC \
             LIMIT 1 \
             FOR UPDATE SKIP LOCKED \
         ) \
         RETURNING id, tenant_id, aggregate_id, payload, attempts",
    )
    .bind(claim_token)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("claim outbox event: {e}"))?;

    let event = match claimed {
        Some(ev) => ev,
        None => return Ok(false),
    };

    // 2. Parse payload
    let item_id: Uuid = event.payload["itemId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| format!("missing itemId in outbox event {}", event.id))?;

    let tenant_id: Uuid = event.payload["tenantId"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| format!("missing tenantId in outbox event {}", event.id))?;

    // 3. Read the knowledge item
    let item = match store::get_item(pool, tenant_id, item_id)
        .await
        .map_err(|e| format!("get_item: {e}"))?
    {
        Some(item) => item,
        None => {
            sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                .bind(event.id)
                .execute(pool)
                .await
                .map_err(|e| format!("delete orphaned outbox event: {e}"))?;
            return Ok(true);
        }
    };

    // 4. Mark as indexing
    index_state::upsert_status(pool, tenant_id, item_id, &IndexStatus::Indexing)
        .await
        .map_err(|e| format!("upsert index status: {e}"))?;

    // 5. Extract source text
    let source_text = if item.item_type == "document" {
        let doc = match store::get_document(pool, tenant_id, item_id)
            .await
            .map_err(|e| format!("get_document: {e}"))?
        {
            Some(d) => d,
            None => {
                index_state::set_not_indexable(pool, item_id, "Document record not found")
                    .await
                    .map_err(|e| format!("set_not_indexable: {e}"))?;
                sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                    .bind(event.id)
                    .execute(pool)
                    .await
                    .map_err(|e| format!("delete outbox: {e}"))?;
                return Ok(true);
            }
        };

        let (body_bytes, content_type) = match storage.get(&doc.storage_key).await {
            Ok(result) => result,
            Err(storage::StorageError::NotFound) => {
                index_state::set_not_indexable(
                    pool,
                    item_id,
                    "Document file not found in storage",
                )
                .await
                .map_err(|e| format!("set_not_indexable: {e}"))?;
                sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                    .bind(event.id)
                    .execute(pool)
                    .await
                    .map_err(|e| format!("delete outbox: {e}"))?;
                return Ok(true);
            }
            Err(e) => {
                return Err(format!("storage.get error for item {item_id}: {e}"));
            }
        };

        match chunking::extract_text(&content_type, &body_bytes) {
            Some(text) => text,
            None => {
                let reason = format!("No extractable text from content type: {content_type}");
                index_state::set_not_indexable(pool, item_id, &reason)
                    .await
                    .map_err(|e| format!("set_not_indexable: {e}"))?;
                sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                    .bind(event.id)
                    .execute(pool)
                    .await
                    .map_err(|e| format!("delete outbox: {e}"))?;
                return Ok(true);
            }
        }
    } else {
        let body = item.body.as_deref().unwrap_or("");
        format!("{}\n\n{}", item.title, body)
    };

    // 6. Chunk and check indexability
    let chunk_result = chunking::chunk_text(&source_text);

    if chunk_result.not_indexable {
        index_state::set_not_indexable(pool, item_id, "No extractable text content")
            .await
            .map_err(|e| format!("set_not_indexable: {e}"))?;
        sqlx::query("DELETE FROM outbox_events WHERE id = $1")
            .bind(event.id)
            .execute(pool)
            .await
            .map_err(|e| format!("delete outbox: {e}"))?;
        return Ok(true);
    }

    // 7. Early skip if content hash matches already-indexed state
    if let Some(ref state) = index_state::get(pool, tenant_id, item_id)
        .await
        .map_err(|e| format!("get index state: {e}"))?
    {
        if state.status == "indexed" {
            if let Some(ref indexed_hash) = state.indexed_content_hash {
                if *indexed_hash == chunk_result.content_hash {
                    sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                        .bind(event.id)
                        .execute(pool)
                        .await
                        .map_err(|e| format!("delete outbox: {e}"))?;
                    return Ok(true);
                }
            }
        }
    }

    // 8. Embed all chunk contents in one batch
    let texts: Vec<String> = chunk_result
        .chunks
        .iter()
        .map(|c| c.content.clone())
        .collect();
    let chunk_count = texts.len() as i32;

    match embedder.embed(tenant_id, texts, format!("knowledge-indexer/{item_id}")).await {
        // 9a. Success — atomic replace
        Ok(embeddings) => {
            let mut tx = pool
                .begin()
                .await
                .map_err(|e| format!("begin tx: {e}"))?;

            sqlx::query("DELETE FROM knowledge_chunks WHERE item_id = $1")
                .bind(item_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("delete chunks: {e}"))?;

            for (i, chunk) in chunk_result.chunks.iter().enumerate() {
                let emb = &embeddings[i];
                let emb_str = format!(
                    "[{}]",
                    emb.iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                );

                sqlx::query(
                    "INSERT INTO knowledge_chunks \
                     (item_id, tenant_id, ordinal, content, embedding, content_hash) \
                     VALUES ($1, $2, $3, $4, $5::vector, $6)",
                )
                .bind(item_id)
                .bind(tenant_id)
                .bind(chunk.ordinal as i32)
                .bind(&chunk.content)
                .bind(&emb_str)
                .bind(&chunk_result.content_hash)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("insert chunk {i}: {e}"))?;
            }

            sqlx::query(
                "UPDATE knowledge_index_state \
                 SET status = 'indexed', indexed_content_hash = $1, chunk_count = $2, \
                     last_indexed_at = now(), failure_reason = NULL, \
                     attempts = 0, updated_at = now() \
                 WHERE item_id = $3",
            )
            .bind(&chunk_result.content_hash)
            .bind(chunk_count)
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("update index state: {e}"))?;

            sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                .bind(event.id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("delete outbox: {e}"))?;

            tx.commit()
                .await
                .map_err(|e| format!("commit tx: {e}"))?;

            tracing::info!(
                tenant_id = %tenant_id,
                item_id = %item_id,
                chunk_count = chunk_count,
                outcome = "indexed",
                "rag.index"
            );
        }

        // 9b. Retriable provider error — exponential backoff
        Err(EmbedError::Retriable(reason)) => {
            let attempts = index_state::increment_attempts(pool, item_id)
                .await
                .map_err(|e| format!("increment attempts: {e}"))?;

            if attempts >= MAX_RETRIES {
                let reason = format!(
                    "embedding failed after {MAX_RETRIES} attempts: {reason}",
                );
                index_state::set_failed(pool, item_id, &reason)
                    .await
                    .map_err(|e| format!("set_failed: {e}"))?;
                sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                    .bind(event.id)
                    .execute(pool)
                    .await
                    .map_err(|e| format!("delete outbox: {e}"))?;

                tracing::info!(
                    tenant_id = %tenant_id,
                    item_id = %item_id,
                    outcome = "failed",
                    "rag.index"
                );
            } else {
                let backoff_secs =
                    (5i64 * 2i64.pow(attempts as u32 - 1)).min(300);
                let available_at = Utc::now()
                    + chrono::Duration::seconds(backoff_secs);

                sqlx::query(
                    "UPDATE outbox_events \
                     SET claimed_at = NULL, claim_token = NULL, \
                         available_at = $1, last_error = $2 \
                     WHERE id = $3 AND claim_token = $4",
                )
                .bind(available_at)
                .bind(format!("retriable: {reason}"))
                .bind(event.id)
                .bind(claim_token)
                .execute(pool)
                .await
                .map_err(|e| format!("reschedule outbox: {e}"))?;

                tracing::info!(
                    tenant_id = %tenant_id,
                    item_id = %item_id,
                    attempt = attempts,
                    backoff_secs = backoff_secs,
                    outcome = "retry",
                    "rag.index"
                );
            }
        }

        // 9c. Permanent/non-retriable provider error
        Err(EmbedError::Permanent(reason)) => {
            let reason = format!("embedding error: {reason}");
            index_state::set_failed(pool, item_id, &reason)
                .await
                .map_err(|e| format!("set_failed: {e}"))?;
            sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                .bind(event.id)
                .execute(pool)
                .await
                .map_err(|e| format!("delete outbox: {e}"))?;

            tracing::info!(
                tenant_id = %tenant_id,
                item_id = %item_id,
                outcome = "failed",
                "rag.index"
            );
        }

        // 9d. Not configured
        Err(EmbedError::NotConfigured) => {
            let reason = "Platform AI not configured for embeddings";
            index_state::set_failed(pool, item_id, reason)
                .await
                .map_err(|e| format!("set_failed: {e}"))?;
            sqlx::query("DELETE FROM outbox_events WHERE id = $1")
                .bind(event.id)
                .execute(pool)
                .await
                .map_err(|e| format!("delete outbox: {e}"))?;

            tracing::warn!(
                tenant_id = %tenant_id,
                item_id = %item_id,
                "embedding not configured"
            );
        }
    }

    Ok(true)
}
