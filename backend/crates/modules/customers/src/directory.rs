//! What other modules may ask `customers` (MOH-194, ADR-017).
//!
//! `customers` owns `customers` and `customer_channel_identifiers`. A channel module that needs to
//! know how to reach a customer (their WhatsApp number, say) asks here instead of reading the
//! table. Every function takes the tenant ID and filters on it, and runs on the caller's executor,
//! so it stays inside the caller's tenant Unit of Work.

use std::collections::HashMap;

use sqlx::PgExecutor;
use uuid::Uuid;

/// The customer's identifier on `channel` (a phone number for `whatsapp`), when they have one that
/// has not been removed.
pub async fn channel_identifier<'e>(
    executor: impl PgExecutor<'e>,
    tenant_id: Uuid,
    customer_id: Uuid,
    channel: &str,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT identifier FROM customer_channel_identifiers \
         WHERE tenant_id = $1 AND customer_id = $2 AND channel = $3 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(channel)
    .fetch_optional(executor)
    .await
}

/// The display name of each of `customer_ids` that exists in this tenant. IDs of another tenant's
/// customers and unknown IDs are left out.
pub async fn display_names<'e>(
    executor: impl PgExecutor<'e>,
    tenant_id: Uuid,
    customer_ids: &[Uuid],
) -> sqlx::Result<HashMap<Uuid, String>> {
    if customer_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, display_name FROM customers WHERE tenant_id = $1 AND id = ANY($2)",
    )
    .bind(tenant_id)
    .bind(customer_ids)
    .fetch_all(executor)
    .await?;
    Ok(rows.into_iter().collect())
}
