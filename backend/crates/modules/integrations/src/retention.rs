//! 90-day retention sweep for `integration_events` and
//! `integration_webhook_deliveries` (FR-015). Mirrors the notification
//! retention sweeper in shape: one periodic task deletes rows older than
//! 90 days from both tables and returns the total deleted.

use sqlx::PgPool;

/// Delete integration events and accepted deliveries older than 90 days.
/// Returns the sum of rows deleted across both tables, or a `sqlx::Error`
/// if either DELETE fails. Callers (the server's main spawn loop) log the
/// count and continue.
pub async fn sweep_expired(pool: &PgPool) -> sqlx::Result<u64> {
    let events = sqlx::query(
        "DELETE FROM integration_events \
         WHERE created_at < now() - interval '90 days'",
    )
    .execute(pool)
    .await?
    .rows_affected();
    let deliveries = sqlx::query(
        "DELETE FROM integration_webhook_deliveries \
         WHERE received_at < now() - interval '90 days'",
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(events + deliveries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use uuid::Uuid;

    fn require_db_tests() -> bool {
        std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
    }

    async fn get_pool() -> Option<PgPool> {
        let url = match std::env::var("DATABASE_URL") {
            Ok(value) => value,
            Err(_) => {
                eprintln!(
                    "skipping integrations retention live tests: DATABASE_URL not set"
                );
                if require_db_tests() {
                    panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
                }
                return None;
            }
        };
        let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
        if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
            eprintln!(
                "skipping integrations retention live tests: DATABASE_URL is unreachable"
            );
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
            }
            return None;
        }
        Some(pool)
    }

    /// T057 — FR-015 retention sweep. Seed 2 old + 2 new rows in each
    /// table; `sweep_expired` must delete only the old rows.
    #[tokio::test]
    async fn sweep_expired_deletes_only_old_rows() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        // Seed a tenant + catalog connection so the FK on integration_events
        // and integration_webhook_deliveries is satisfied.
        let tenant_id: Uuid =
            sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
                .bind("Integrations Retention Tenant")
                .bind(format!("iret-{}", Uuid::new_v4().simple()))
                .fetch_one(&pool)
                .await
                .unwrap();
        let catalog_id: Uuid =
            sqlx::query_scalar("SELECT id FROM integration_catalog WHERE slug = $1")
                .bind("generic-webhook")
                .fetch_one(&pool)
                .await
                .unwrap();
        let mut token_hash = [0u8; 32];
        let mut token_ciphertext = vec![0u8; 48];
        let mut token_nonce = [0u8; 12];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut token_hash);
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut token_ciphertext);
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut token_nonce);
        let connection_id: Uuid = sqlx::query_scalar(
            "INSERT INTO integration_connections \
             (tenant_id, catalog_id, is_active, config, \
              webhook_token_hash, webhook_token_ciphertext, webhook_token_nonce) \
             VALUES ($1, $2, true, '{}'::jsonb, $3, $4, $5) RETURNING id",
        )
        .bind(tenant_id)
        .bind(catalog_id)
        .bind(token_hash.to_vec())
        .bind(token_ciphertext)
        .bind(token_nonce.to_vec())
        .fetch_one(&pool)
        .await
        .unwrap();

        // 2 old + 2 new events.
        for label in ["old-1", "old-2", "new-1", "new-2"] {
            let is_old = label.starts_with("old-");
            let ts = if is_old {
                "now() - interval '100 days'::timestamptz"
            } else {
                "now() - interval '1 day'::timestamptz"
            };
            sqlx::query(&format!(
                "INSERT INTO integration_events \
                 (tenant_id, connection_id, event_type, outcome, reason, created_at) \
                 VALUES ($1, $2, 'delivery_accepted', 'success', NULL, {ts})"
            ))
            .bind(tenant_id)
            .bind(connection_id)
            .execute(&pool)
            .await
            .unwrap();
        }

        // 2 old + 2 new deliveries.
        for label in ["old-1", "old-2", "new-1", "new-2"] {
            let is_old = label.starts_with("old-");
            let ts = if is_old {
                "now() - interval '100 days'::timestamptz"
            } else {
                "now() - interval '1 day'::timestamptz"
            };
            sqlx::query(&format!(
                "INSERT INTO integration_webhook_deliveries \
                 (tenant_id, connection_id, payload, received_at) \
                 VALUES ($1, $2, '{{}}'::jsonb, {ts})"
            ))
            .bind(tenant_id)
            .bind(connection_id)
            .execute(&pool)
            .await
            .unwrap();
        }

        // Sanity: 4 rows in each table before the sweep.
        let events_before: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM integration_events WHERE connection_id = $1",
        )
        .bind(connection_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(events_before.0, 4, "expected 4 events before sweep");
        let deliveries_before: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM integration_webhook_deliveries WHERE connection_id = $1",
        )
        .bind(connection_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            deliveries_before.0, 4,
            "expected 4 deliveries before sweep"
        );

        let deleted = sweep_expired(&pool)
            .await
            .expect("sweep_expired should succeed");
        assert_eq!(
            deleted, 4,
            "sweep must return the total of rows deleted (2 events + 2 deliveries)"
        );

        let events_after: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM integration_events WHERE connection_id = $1",
        )
        .bind(connection_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            events_after.0, 2,
            "the 2 old events must be deleted, the 2 new ones kept"
        );

        let deliveries_after: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM integration_webhook_deliveries WHERE connection_id = $1",
        )
        .bind(connection_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            deliveries_after.0, 2,
            "the 2 old deliveries must be deleted, the 2 new ones kept"
        );
    }
}
