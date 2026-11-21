use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::time::Instant;
use uuid::Uuid;

static LAST_REFUSAL: LazyLock<Mutex<HashMap<Uuid, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Writes a `security.rate_limit.refused` audit entry for the given tenant.
/// Deduplicates to at most one row per tenant per 60-second window.
pub async fn record_refusal(pool: &PgPool, tenant_id: Uuid) {
    {
        let mut cache = LAST_REFUSAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(last) = cache.get(&tenant_id) {
            if last.elapsed() < std::time::Duration::from_secs(60) {
                return;
            }
        }
        cache.insert(tenant_id, Instant::now());
    }

    audit_log::record(
        pool,
        "security.rate_limit.refused",
        None,
        Some(tenant_id),
        "rate_limit",
        Some(&tenant_id.to_string()),
        &serde_json::json!({}),
    )
    .await;
}
