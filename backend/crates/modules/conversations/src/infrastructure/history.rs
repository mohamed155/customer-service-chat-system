//! A customer's recent conversations, for the customer profile.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// How many conversations the customer profile shows.
pub const HISTORY_PAGE_SIZE: i64 = 20;

/// Per-conversation summary returned by the customer-profile history
/// endpoint.  A narrower projection than `model::Conversation`: no
/// `last_message` or assignee, since the dashboard only needs the metadata
/// to render the list.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct ConversationSummary {
    pub id: Uuid,
    pub channel: String,
    pub status: String,
    pub last_activity_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Fetches up to `HISTORY_PAGE_SIZE + 1` conversations for the supplied
/// customer, ordered by `last_activity_at DESC` (newest first). The
/// over-fetch by one row is what lets the handler report `has_more` without
/// a second count query (mirrors the pattern in `customers::list_customers`).
///
/// Filters out soft-deleted rows and is constrained to the resolved tenant —
/// the caller is responsible for the `customer_exists` check (T028 calls
/// `customers::customer_exists` first so an unknown / cross-tenant id
/// returns `not_found` before this query runs).
pub async fn list_recent_for_customer(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    customer_id: Uuid,
) -> sqlx::Result<Vec<ConversationSummary>> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let result: sqlx::Result<Vec<ConversationSummary>> = async {
        sqlx::query_as::<_, ConversationSummary>(
            "SELECT id, channel, status, last_activity_at, created_at \
         FROM conversations \
         WHERE tenant_id = $1 AND customer_id = $2 AND deleted_at IS NULL \
         ORDER BY last_activity_at DESC \
         LIMIT $3",
        )
        .bind(tenant_id)
        .bind(customer_id)
        .bind(HISTORY_PAGE_SIZE + 1)
        .fetch_all(uow.conn())
        .await
    }
    .await;
    if result.is_ok() {
        uow.commit().await?;
    }
    result
}

/// Transaction-aware variant — same query but runs inside an existing
/// transaction so callers can verify the customer and fetch conversation
/// history from one consistent snapshot.
pub async fn list_recent_for_customer_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    customer_id: Uuid,
) -> sqlx::Result<Vec<ConversationSummary>> {
    sqlx::query_as::<_, ConversationSummary>(
        "SELECT id, channel, status, last_activity_at, created_at \
         FROM conversations \
         WHERE tenant_id = $1 AND customer_id = $2 AND deleted_at IS NULL \
         ORDER BY last_activity_at DESC \
         LIMIT $3",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(HISTORY_PAGE_SIZE + 1)
    .fetch_all(&mut **tx)
    .await
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use serde_json::json;
    use uuid::Uuid;

    use super::ConversationSummary;

    #[test]
    fn conversation_summary_serializes_to_the_contract_representation() {
        let summary = ConversationSummary {
            id: Uuid::parse_str("018f1f4d-5e6a-7b8c-9d0e-1f2a3b4c5d6e").unwrap(),
            channel: "web_chat".to_owned(),
            status: "open".to_owned(),
            last_activity_at: DateTime::parse_from_rfc3339("2026-07-13T09:30:00Z")
                .unwrap()
                .into(),
            created_at: DateTime::parse_from_rfc3339("2026-07-12T14:00:00Z")
                .unwrap()
                .into(),
        };

        assert_eq!(
            serde_json::to_value(summary).unwrap(),
            json!({
                "id": "018f1f4d-5e6a-7b8c-9d0e-1f2a3b4c5d6e",
                "channel": "web_chat",
                "status": "open",
                "last_activity_at": "2026-07-13T09:30:00Z",
                "created_at": "2026-07-12T14:00:00Z"
            })
        );
    }
}
