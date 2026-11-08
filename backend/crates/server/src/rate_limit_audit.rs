use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::time::Instant;
use tracing::error;
use uuid::Uuid;

static LAST_REFUSAL: LazyLock<Mutex<HashMap<Uuid, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Writes a `security.rate_limit.refused` audit entry for the given tenant.
/// Deduplicates to at most one row per tenant per 60-second window.
pub async fn record_refusal(pool: &PgPool, tenant_id: Uuid) {
    {
        let mut cache = LAST_REFUSAL.lock().expect("rate_limit_audit lock poisoned");
        if let Some(last) = cache.get(&tenant_id) {
            if last.elapsed() < std::time::Duration::from_secs(60) {
                return;
            }
        }
        cache.insert(tenant_id, Instant::now());
    }

    let result: sqlx::Result<()> = async {
        let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
        sqlx::query(
            "INSERT INTO audit_logs \
             (actor_user_id, action, resource_type, resource_id, tenant_id, details) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(Option::<Uuid>::None)
        .bind("security.rate_limit.refused")
        .bind("rate_limit")
        .bind(tenant_id.to_string())
        .bind(tenant_id)
        .bind(serde_json::json!({}))
        .execute(uow.conn())
        .await?;
        uow.commit().await
    }
    .await;

    if let Err(e) = result {
        error!(
            tenant_id = %tenant_id,
            error = %e,
            "failed to record rate-limit refusal audit log"
        );
    }
}
