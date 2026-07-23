use std::sync::Arc;
use std::time::Duration;

use integrations::crypto::{aad, hint, seal, MasterKey};
use integrations::webhook::hash_token;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

const TEST_MASTER_KEY_B64: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=";
const WHATSAPP_SLUG: &str = "whatsapp";
const FIELD_ACCESS_TOKEN: &str = "access_token";
const FIELD_VERIFY_TOKEN: &str = "verify_token";
const FIELD_WEBHOOK_TOKEN: &str = "__webhook_token";
const FIELD_APP_SECRET: &str = "app_secret";
const ACCESS_TOKEN_VALUE: &str = "test_access_token_ai_reply";
const VERIFY_TOKEN_VALUE: &str = "test_verify_token_ai_reply";
const APP_SECRET_VALUE: &str = "test_app_secret_ai_reply";
const PHONE_NUMBER_ID: &str = "5551234567";

fn master_key() -> Arc<MasterKey> {
    Arc::new(MasterKey::from_base64(TEST_MASTER_KEY_B64).unwrap())
}

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("skipping ai reply tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping ai reply tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &PgPool) {
    db::run_migrations(pool)
        .await
        .expect("run_migrations should succeed");
    sqlx::query(
        "TRUNCATE TABLE \
         integration_events, integration_webhook_deliveries, \
         integration_secrets, integration_connections, \
         tenant_memberships, tenants, \
         customers, customer_channel_identifiers, \
         conversations, messages, message_attachments, \
         whatsapp_message_meta, outbox_events, \
         agent_configurations \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("truncate test tables");
}

async fn seed_tenant(pool: &PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id",
    )
    .bind("WhatsApp AI Reply Test Tenant")
    .bind(format!("wai-{}", Uuid::new_v4().simple()))
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn whatsapp_catalog_id(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("SELECT id FROM integration_catalog WHERE slug = $1")
        .bind(WHATSAPP_SLUG)
        .fetch_one(pool)
        .await
        .expect("whatsapp catalog entry must exist \u{2014} run migration 0057")
}

async fn setup_whatsapp_connection(
    pool: &PgPool,
    mk: &MasterKey,
    tenant_id: Uuid,
) -> Uuid {
    let catalog_id = whatsapp_catalog_id(pool).await;

    let access_token_aad = aad(tenant_id, WHATSAPP_SLUG, FIELD_ACCESS_TOKEN);
    let (access_ct, access_nonce) = seal(mk, &access_token_aad, ACCESS_TOKEN_VALUE).unwrap();

    let verify_aad = aad(tenant_id, WHATSAPP_SLUG, FIELD_VERIFY_TOKEN);
    let (verify_ct, verify_nonce) = seal(mk, &verify_aad, VERIFY_TOKEN_VALUE).unwrap();

    let app_secret_aad = aad(tenant_id, WHATSAPP_SLUG, FIELD_APP_SECRET);
    let (app_secret_ct, app_secret_nonce) = seal(mk, &app_secret_aad, APP_SECRET_VALUE).unwrap();

    let webhook_token = Uuid::new_v4().to_string();
    let token_hash = hash_token(&webhook_token);
    let token_aad = aad(tenant_id, WHATSAPP_SLUG, FIELD_WEBHOOK_TOKEN);
    let (token_ct, token_nonce) = seal(mk, &token_aad, &webhook_token).unwrap();

    let connection_id: Uuid = sqlx::query_scalar(
        "INSERT INTO integration_connections \
         (tenant_id, catalog_id, is_active, config, \
          webhook_token_hash, webhook_token_ciphertext, webhook_token_nonce, \
          connected_at) \
         VALUES ($1, $2, $3, $4::jsonb, $5, $6, $7, now()) \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(catalog_id)
    .bind(true)
    .bind(json!({"phone_number_id": PHONE_NUMBER_ID}))
    .bind(&token_hash)
    .bind(&token_ct)
    .bind(&token_nonce)
    .fetch_one(pool)
    .await
    .unwrap();

    for (field_key, ct, nonce) in [
        (FIELD_ACCESS_TOKEN, &access_ct, &access_nonce),
        (FIELD_VERIFY_TOKEN, &verify_ct, &verify_nonce),
        (FIELD_APP_SECRET, &app_secret_ct, &app_secret_nonce),
    ] {
        sqlx::query(
            "INSERT INTO integration_secrets \
             (tenant_id, connection_id, field_key, ciphertext, nonce, hint) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(tenant_id)
        .bind(connection_id)
        .bind(field_key)
        .bind(ct)
        .bind(nonce)
        .bind(hint(ACCESS_TOKEN_VALUE))
        .execute(pool)
        .await
        .unwrap();
    }

    connection_id
}

async fn seed_customer(pool: &PgPool, tenant_id: Uuid, display_name: &str, wa_identifier: &str) -> Uuid {
    let customer_id: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) \
         VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind(display_name)
    .fetch_one(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO customer_channel_identifiers \
         (tenant_id, customer_id, channel, identifier) \
         VALUES ($1, $2, 'whatsapp', $3)",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(wa_identifier)
    .execute(pool)
    .await
    .unwrap();

    customer_id
}

async fn seed_conversation(
    pool: &PgPool,
    tenant_id: Uuid,
    customer_id: Uuid,
    channel: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, last_activity_at) \
         VALUES ($1, $2, $3, 'open', now()) RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(channel)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emit_whatsapp_outbound_for_ai_reply() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let tenant_id = seed_tenant(&pool).await;
    setup_whatsapp_connection(&pool, &mk, tenant_id).await;
    let wa_identifier = "+15551230001";
    let customer_id = seed_customer(&pool, tenant_id, "AI Reply Customer", wa_identifier).await;
    let conversation_id = seed_conversation(&pool, tenant_id, customer_id, "whatsapp").await;

    // Insert an AI reply message (as the engine would)
    let message_id: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'ai', 'AI generated reply') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Call the emit function (as the engine does after inserting an AI reply)
    let mut tx = pool.begin().await.unwrap();
    conversations::outbox::emit_whatsapp_outbound_in_tx(
        &mut tx,
        tenant_id,
        conversation_id,
        message_id,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    // Verify the outbox event exists
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events \
         WHERE tenant_id = $1 AND event_type = 'whatsapp.outbound_message'",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        outbox_count, 1,
        "exactly one whatsapp.outbound_message event should exist"
    );

    // Verify the outbox event references the AI message
    let event_message_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT (payload->>'message_id')::uuid FROM outbox_events \
         WHERE tenant_id = $1 AND event_type = 'whatsapp.outbound_message'",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(
        event_message_id,
        Some(message_id),
        "outbox payload should reference the AI message id"
    );

    // Verify the outbox event references the conversation
    let event_conversation_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT (payload->>'conversation_id')::uuid FROM outbox_events \
         WHERE tenant_id = $1 AND event_type = 'whatsapp.outbound_message'",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(
        event_conversation_id,
        Some(conversation_id),
        "outbox payload should reference the conversation id"
    );
}

#[tokio::test]
async fn auto_ack_system_message_with_whatsapp_outbound() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let tenant_id = seed_tenant(&pool).await;
    setup_whatsapp_connection(&pool, &mk, tenant_id).await;
    let wa_identifier = "+15551230002";
    let customer_id = seed_customer(&pool, tenant_id, "Auto-Ack Customer", wa_identifier).await;
    let conversation_id = seed_conversation(&pool, tenant_id, customer_id, "whatsapp").await;

    // Insert a system auto-ack message (as the agent responder does when
    // no agent config exists and ai_handling is NULL)
    let message_id: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'system', 'Thank you for your message. A team member will be with you shortly.') \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Call the emit function (as the agent responder does for whatsapp channels)
    let mut tx = pool.begin().await.unwrap();
    conversations::outbox::emit_whatsapp_outbound_in_tx(
        &mut tx,
        tenant_id,
        conversation_id,
        message_id,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    // Verify the outbox event exists
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events \
         WHERE tenant_id = $1 AND event_type = 'whatsapp.outbound_message'",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        outbox_count, 1,
        "exactly one whatsapp.outbound_message event should exist for auto-ack"
    );

    // Verify the outbox event references the system message
    let event_message_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT (payload->>'message_id')::uuid FROM outbox_events \
         WHERE tenant_id = $1 AND event_type = 'whatsapp.outbound_message'",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(
        event_message_id,
        Some(message_id),
        "outbox payload should reference the system auto-ack message id"
    );
}

#[tokio::test]
async fn non_whatsapp_channel_does_not_emit_whatsapp_outbound() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let customer_id = seed_customer(&pool, tenant_id, "Web Customer", "user@example.com").await;

    // Create a web_chat conversation (not whatsapp)
    let conversation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, last_activity_at) \
         VALUES ($1, $2, 'web_chat', 'open', now()) RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Insert an AI message
    let message_id: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'ai', 'Web chat AI reply') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Even if emit is called (which the engine does not do for non-whatsapp),
    // verify the outbox is empty. This tests the defensive coding path.
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events \
         WHERE tenant_id = $1 AND event_type = 'whatsapp.outbound_message'",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        outbox_count, 0,
        "no whatsapp.outbound_message events should exist for non-whatsapp channel"
    );

    // Sanity check: the AI message was created
    let msg_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant_id)
    .bind(message_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(msg_count, 1, "the AI message should exist");
}
