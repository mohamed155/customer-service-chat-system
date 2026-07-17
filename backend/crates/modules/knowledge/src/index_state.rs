use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub enum IndexStatus {
    NotIndexed,
    Pending,
    Indexing,
    Indexed,
    Failed,
    NotIndexable,
}

impl fmt::Display for IndexStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            IndexStatus::NotIndexed => "not_indexed",
            IndexStatus::Pending => "pending",
            IndexStatus::Indexing => "indexing",
            IndexStatus::Indexed => "indexed",
            IndexStatus::Failed => "failed",
            IndexStatus::NotIndexable => "not_indexable",
        };
        f.write_str(s)
    }
}

impl FromStr for IndexStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "not_indexed" => Ok(IndexStatus::NotIndexed),
            "pending" => Ok(IndexStatus::Pending),
            "indexing" => Ok(IndexStatus::Indexing),
            "indexed" => Ok(IndexStatus::Indexed),
            "failed" => Ok(IndexStatus::Failed),
            "not_indexable" => Ok(IndexStatus::NotIndexable),
            _ => Err(format!("unknown index status: {s}")),
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IndexState {
    pub item_id: Uuid,
    pub tenant_id: Uuid,
    pub status: String,
    pub failure_reason: Option<String>,
    pub attempts: i32,
    pub indexed_content_hash: Option<String>,
    pub chunk_count: i32,
    pub last_indexed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

pub async fn get(
    pool: &PgPool,
    tenant_id: Uuid,
    item_id: Uuid,
) -> Result<Option<IndexState>, sqlx::Error> {
    sqlx::query_as::<_, IndexState>(
        "SELECT * FROM knowledge_index_state WHERE item_id = $1 AND tenant_id = $2",
    )
    .bind(item_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
}

pub async fn get_many(
    pool: &PgPool,
    item_ids: &[Uuid],
) -> Result<HashMap<Uuid, IndexState>, sqlx::Error> {
    if item_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<IndexState> = sqlx::query_as::<_, IndexState>(
        "SELECT * FROM knowledge_index_state WHERE item_id = ANY($1)",
    )
    .bind(item_ids)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|row| (row.item_id, row)).collect())
}

pub async fn upsert_status(
    pool: &PgPool,
    tenant_id: Uuid,
    item_id: Uuid,
    status: &IndexStatus,
) -> Result<(), sqlx::Error> {
    let status_str = status.to_string();
    sqlx::query(
        "INSERT INTO knowledge_index_state (item_id, tenant_id, status) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (item_id) DO UPDATE SET status = $3, updated_at = now()",
    )
    .bind(item_id)
    .bind(tenant_id)
    .bind(&status_str)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_failed(pool: &PgPool, item_id: Uuid, reason: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE knowledge_index_state SET status = 'failed', failure_reason = $1, updated_at = now() \
         WHERE item_id = $2",
    )
    .bind(reason)
    .bind(item_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_indexed(
    pool: &PgPool,
    item_id: Uuid,
    content_hash: &str,
    chunk_count: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE knowledge_index_state \
         SET status = 'indexed', indexed_content_hash = $1, chunk_count = $2, \
             last_indexed_at = now(), failure_reason = NULL, updated_at = now() \
         WHERE item_id = $3",
    )
    .bind(content_hash)
    .bind(chunk_count)
    .bind(item_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn increment_attempts(pool: &PgPool, item_id: Uuid) -> Result<i32, sqlx::Error> {
    let row: (i32,) = sqlx::query_as(
        "UPDATE knowledge_index_state SET attempts = attempts + 1, updated_at = now() \
         WHERE item_id = $1 RETURNING attempts",
    )
    .bind(item_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub async fn set_not_indexable(
    pool: &PgPool,
    item_id: Uuid,
    reason: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE knowledge_index_state SET status = 'not_indexable', failure_reason = $1, updated_at = now() \
         WHERE item_id = $2",
    )
    .bind(reason)
    .bind(item_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_not_indexed() {
        assert_eq!(IndexStatus::NotIndexed.to_string(), "not_indexed");
    }

    #[test]
    fn display_pending() {
        assert_eq!(IndexStatus::Pending.to_string(), "pending");
    }

    #[test]
    fn display_indexing() {
        assert_eq!(IndexStatus::Indexing.to_string(), "indexing");
    }

    #[test]
    fn display_indexed() {
        assert_eq!(IndexStatus::Indexed.to_string(), "indexed");
    }

    #[test]
    fn display_failed() {
        assert_eq!(IndexStatus::Failed.to_string(), "failed");
    }

    #[test]
    fn display_not_indexable() {
        assert_eq!(IndexStatus::NotIndexable.to_string(), "not_indexable");
    }

    #[test]
    fn from_str_valid() {
        assert_eq!(
            IndexStatus::from_str("not_indexed").unwrap(),
            IndexStatus::NotIndexed
        );
        assert_eq!(
            IndexStatus::from_str("pending").unwrap(),
            IndexStatus::Pending
        );
        assert_eq!(
            IndexStatus::from_str("indexing").unwrap(),
            IndexStatus::Indexing
        );
        assert_eq!(
            IndexStatus::from_str("indexed").unwrap(),
            IndexStatus::Indexed
        );
        assert_eq!(
            IndexStatus::from_str("failed").unwrap(),
            IndexStatus::Failed
        );
        assert_eq!(
            IndexStatus::from_str("not_indexable").unwrap(),
            IndexStatus::NotIndexable
        );
    }

    #[test]
    fn from_str_invalid() {
        assert!(IndexStatus::from_str("unknown").is_err());
        assert!(IndexStatus::from_str("").is_err());
        assert!(IndexStatus::from_str("INDEXED").is_err());
    }
}
