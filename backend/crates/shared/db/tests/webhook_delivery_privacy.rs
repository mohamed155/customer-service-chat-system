#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test assertions

use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

async fn insert_id(conn: &mut PgConnection, sql: &'static str, ids: &[Uuid]) -> Uuid {
    let mut query = sqlx::query_scalar(sql);
    for id in ids {
        query = query.bind(id);
    }
    query.fetch_one(conn).await.unwrap()
}

async fn customer(conn: &mut PgConnection, tenant: Uuid) -> Uuid {
    insert_id(
        conn,
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, 'Test') RETURNING id",
        &[tenant],
    )
    .await
}

#[tokio::test]
async fn late_webhook_deliveries_scrub_only_unambiguous_inactive_subjects() {
    if std::env::var("REQUIRE_DB_TESTS").as_deref() != Ok("1")
        && std::env::var("MIGRATION_DATABASE_URL").is_err()
    {
        eprintln!("skipping webhook delivery privacy test: MIGRATION_DATABASE_URL not set");
        return;
    }

    let mut tx = db::testing::owner_tx().await;
    db::testing::become_platform(&mut tx).await;
    let tenant = insert_id(
        &mut tx,
        "INSERT INTO tenants (name, slug) VALUES ('Test', 'webhook-' || gen_random_uuid())
         RETURNING id",
        &[],
    )
    .await;
    let connection = insert_id(
        &mut tx,
        "INSERT INTO integration_connections
             (tenant_id, catalog_id, webhook_token_hash, webhook_token_ciphertext, webhook_token_nonce)
         VALUES ($1, (SELECT id FROM integration_catalog ORDER BY slug LIMIT 1),
                 decode(md5(random()::text), 'hex'), decode('01', 'hex'), decode('02', 'hex'))
         RETURNING id",
        &[tenant],
    )
    .await;
    let active = customer(&mut tx, tenant).await;
    let pending = customer(&mut tx, tenant).await;
    let erased = customer(&mut tx, tenant).await;
    let conversation = insert_id(
        &mut tx,
        "INSERT INTO conversations (tenant_id, customer_id, channel, status)
         VALUES ($1, $2, 'widget', 'open') RETURNING id",
        &[tenant, pending],
    )
    .await;
    let message = insert_id(
        &mut tx,
        "INSERT INTO messages (tenant_id, conversation_id, kind, body)
         VALUES ($1, $2, 'customer', 'hi') RETURNING id",
        &[tenant, conversation],
    )
    .await;
    let foreign_tenant = insert_id(
        &mut tx,
        "INSERT INTO tenants (name, slug) VALUES ('Foreign', 'webhook-' || gen_random_uuid())
         RETURNING id",
        &[],
    )
    .await;
    let foreign = customer(&mut tx, foreign_tenant).await;
    sqlx::query(
        "UPDATE customers SET erasure_pending_at = now()
         WHERE (tenant_id = $1 AND id = $2) OR (tenant_id = $3 AND id = $4)",
    )
    .bind(tenant)
    .bind(pending)
    .bind(foreign_tenant)
    .bind(foreign)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("UPDATE customers SET erased_at = now() WHERE tenant_id = $1 AND id = $2")
        .bind(tenant)
        .bind(erased)
        .execute(&mut *tx)
        .await
        .unwrap();

    db::testing::become_runtime(&mut tx).await;
    sqlx::query("SELECT set_config('app.tenant_id', $1, true)")
        .bind(tenant.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();

    let compact = pending.simple().to_string();
    let malformed = [
        format!("{{{pending}"),
        format!("{pending}}}"),
        format!("{}--{}", &compact[..4], &compact[4..]),
        format!("{pending}-"),
        format!("-{pending}"),
        format!("{}-{}", &compact[..2], &compact[2..]),
    ];
    let cases = [
        (
            json!({"customer_id": pending.to_string().to_uppercase()}),
            true,
        ),
        (json!({"customer_id": compact}), true),
        (json!({"customer_id": format!("{{{pending}}}")}), true),
        (
            json!({"customer_id": format!("{}-{}", &compact[..4], &compact[4..])}),
            true,
        ),
        (json!({"customer_id": pending, "detail": "private"}), true),
        (
            json!({"customerId": erased.to_string().to_uppercase()}),
            true,
        ),
        (json!({"conversation_id": conversation}), true),
        (json!({"conversationId": conversation}), true),
        (json!({"message_id": message}), true),
        (json!({"customerId": pending, "messageId": message}), true),
        (json!({"customer_id": active}), false),
        (json!({"customer_id": foreign}), false),
        (json!({"customer_id": "malformed"}), false),
        (json!({"customer_id": malformed[0].as_str()}), false),
        (json!({"customer_id": malformed[1].as_str()}), false),
        (json!({"customer_id": malformed[2].as_str()}), false),
        (json!({"customer_id": malformed[3].as_str()}), false),
        (json!({"customer_id": malformed[4].as_str()}), false),
        (json!({"customer_id": malformed[5].as_str()}), false),
        (json!({"conversationId": ["malformed"]}), false),
        (json!({"message_id": Uuid::new_v4()}), false),
        (json!({"nested": {"customer_id": pending}}), false),
        (
            json!({"customer_id": active, "conversationId": conversation}),
            false,
        ),
        (
            json!({"customer_id": pending, "customerId": active.simple().to_string()}),
            false,
        ),
        (
            json!({"customer_id": pending, "messageId": "malformed"}),
            false,
        ),
    ];
    for (payload, scrubbed) in cases {
        let stored: Value = sqlx::query_scalar(
            "INSERT INTO integration_webhook_deliveries (tenant_id, connection_id, payload)
             VALUES ($1, $2, $3) RETURNING payload",
        )
        .bind(tenant)
        .bind(connection)
        .bind(&payload)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let expected = if scrubbed { json!({}) } else { payload.clone() };
        assert_eq!(stored, expected, "delivery payload {payload}");
    }

    tx.rollback().await.unwrap();
}
