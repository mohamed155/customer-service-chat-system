use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use axum::Router;
use hmac::{Hmac, Mac};
use http_body_util::BodyExt;
use integrations::crypto::{aad, hint, seal, MasterKey};
use integrations::webhook::hash_token;
use kernel::InMemoryRateLimitStore;
use serde_json::json;
use sha2::Sha256;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_MASTER_KEY_B64: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=";
const WHATSAPP_SLUG: &str = "whatsapp";
const FIELD_VERIFY_TOKEN: &str = "verify_token";
const FIELD_WEBHOOK_TOKEN: &str = "__webhook_token";
const FIELD_APP_SECRET: &str = "app_secret";
const VERIFY_TOKEN_VALUE: &str = "my_test_verify_token";
const APP_SECRET_VALUE: &str = "my_app_secret_key";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn master_key() -> Arc<MasterKey> {
    Arc::new(MasterKey::from_base64(TEST_MASTER_KEY_B64).unwrap())
}

fn test_app(pool: sqlx::PgPool, mk: Arc<MasterKey>) -> Router {
    whatsapp::routes::public_router()
        .layer(axum::extract::Extension(mk))
        .layer(axum::extract::Extension(Arc::new(InMemoryRateLimitStore::new())))
        .with_state(pool)
}

fn sign_body(secret: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    let result = mac.finalize().into_bytes();
    format!("sha256={}", hex::encode(result))
}

fn text_message_payload(from: &str, wamid: &str, body: &str) -> serde_json::Value {
    json!({
        "entry": [{
            "changes": [{
                "value": {
                    "messages": [{
                        "from": from,
                        "id": wamid,
                        "timestamp": "2026-07-23T12:00:00Z",
                        "type": "text",
                        "text": { "body": body }
                    }],
                    "contacts": [{
                        "wa_id": from,
                        "profile": { "name": "Test User" }
                    }]
                }
            }]
        }]
    })
}

fn image_message_payload(from: &str, wamid: &str, media_id: &str) -> serde_json::Value {
    json!({
        "entry": [{
            "changes": [{
                "value": {
                    "messages": [{
                        "from": from,
                        "id": wamid,
                        "timestamp": "2026-07-23T12:00:00Z",
                        "type": "image",
                        "image": { "id": media_id, "mime_type": "image/jpeg" }
                    }],
                    "contacts": [{
                        "wa_id": from,
                        "profile": { "name": "Test User" }
                    }]
                }
            }]
        }]
    })
}

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("skipping intake tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping intake tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn response_bytes(resp: Response) -> Vec<u8> {
    resp.into_body().collect().await.unwrap().to_bytes().to_vec()
}

// ---------------------------------------------------------------------------
// Setup helpers
// ---------------------------------------------------------------------------

async fn setup(pool: &sqlx::PgPool) {
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
         whatsapp_message_meta, outbox_events \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("truncate test tables");
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id",
    )
    .bind("WhatsApp Intake Test Tenant")
    .bind(format!("wit-{}", Uuid::new_v4().simple()))
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn whatsapp_catalog_id(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("SELECT id FROM integration_catalog WHERE slug = $1")
        .bind(WHATSAPP_SLUG)
        .fetch_one(pool)
        .await
        .expect("whatsapp catalog entry must exist — run migration 0057")
}

#[allow(dead_code)]
struct ConnectionFixture {
    token: String,
    connection_id: Uuid,
    verify_token: String,
    tenant_id: Uuid,
    app_secret: String,
}

async fn setup_connection(
    pool: &sqlx::PgPool,
    mk: &MasterKey,
    is_active: bool,
) -> ConnectionFixture {
    let tenant_id = seed_tenant(pool).await;
    let catalog_id = whatsapp_catalog_id(pool).await;

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
         VALUES ($1, $2, $3, '{}'::jsonb, $4, $5, $6, now()) \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(catalog_id)
    .bind(is_active)
    .bind(&token_hash)
    .bind(&token_ct)
    .bind(&token_nonce)
    .fetch_one(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO integration_secrets \
         (tenant_id, connection_id, field_key, ciphertext, nonce, hint) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(tenant_id)
    .bind(connection_id)
    .bind(FIELD_VERIFY_TOKEN)
    .bind(&verify_ct)
    .bind(&verify_nonce)
    .bind(hint(VERIFY_TOKEN_VALUE))
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO integration_secrets \
         (tenant_id, connection_id, field_key, ciphertext, nonce, hint) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(tenant_id)
    .bind(connection_id)
    .bind(FIELD_APP_SECRET)
    .bind(&app_secret_ct)
    .bind(&app_secret_nonce)
    .bind(hint(APP_SECRET_VALUE))
    .execute(pool)
    .await
    .unwrap();

    ConnectionFixture {
        token: webhook_token,
        connection_id,
        verify_token: VERIFY_TOKEN_VALUE.to_string(),
        tenant_id,
        app_secret: APP_SECRET_VALUE.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn signed_unknown_number_creates_customer_conversation_message_outbox() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let conn = setup_connection(&pool, &mk, true).await;
    let app = test_app(pool.clone(), mk);

    let from = "15551234567";
    let wamid = "wamid.intake.001";
    let body = "I need help with my order";
    let payload = text_message_payload(from, wamid, body);
    let raw_body = serde_json::to_vec(&payload).unwrap();
    let signature = sign_body(&conn.app_secret, &raw_body);

    let req = Request::builder()
        .uri(&format!("/integrations/whatsapp/webhook/{}", conn.token))
        .method(Method::POST)
        .header("x-hub-signature-256", &signature)
        .header("content-type", "application/json")
        .body(Body::from(raw_body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let normalized = format!("+{from}");

    // Customer + whatsapp identifier
    let customer_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT c.id FROM customers c \
         JOIN customer_channel_identifiers cci ON cci.customer_id = c.id AND cci.tenant_id = c.tenant_id \
         WHERE c.tenant_id = $1 AND cci.channel = 'whatsapp' AND cci.identifier = $2 \
           AND cci.deleted_at IS NULL AND c.deleted_at IS NULL",
    )
    .bind(conn.tenant_id)
    .bind(&normalized)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(customer_id.is_some(), "customer should be created with whatsapp identifier");

    // Open conversation
    let conv_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversations \
         WHERE tenant_id = $1 AND customer_id = $2 AND channel = 'whatsapp' AND status = 'open'",
    )
    .bind(conn.tenant_id)
    .bind(customer_id.unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(conv_count, 1, "one open whatsapp conversation should exist");

    // Customer message
    let msg_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages m \
         JOIN conversations c ON c.id = m.conversation_id \
         WHERE m.tenant_id = $1 AND c.customer_id = $2 AND m.kind = 'customer'",
    )
    .bind(conn.tenant_id)
    .bind(customer_id.unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(msg_count, 1, "one customer message should exist");

    // Outbox event
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events \
         WHERE tenant_id = $1 AND event_type = 'conversation.customer_message'",
    )
    .bind(conn.tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(outbox_count, 1, "one customer_message outbox event should exist");

    // Verify outbox payload contains channel = "whatsapp"
    let channel: Option<String> = sqlx::query_scalar(
        "SELECT payload->>'channel' FROM outbox_events \
         WHERE tenant_id = $1 AND event_type = 'conversation.customer_message'",
    )
    .bind(conn.tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(channel.as_deref(), Some("whatsapp"));
}

#[tokio::test]
async fn second_message_from_same_number_uses_same_conversation() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let conn = setup_connection(&pool, &mk, true).await;
    let from = "15559876543";

    // Message 1
    {
        let app = test_app(pool.clone(), mk.clone());
        let payload = text_message_payload(from, "wamid.intake.002a", "First message");
        let raw_body = serde_json::to_vec(&payload).unwrap();
        let signature = sign_body(&conn.app_secret, &raw_body);
        let req = Request::builder()
            .uri(&format!("/integrations/whatsapp/webhook/{}", conn.token))
            .method(Method::POST)
            .header("x-hub-signature-256", &signature)
            .header("content-type", "application/json")
            .body(Body::from(raw_body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // Message 2
    {
        let app = test_app(pool.clone(), mk);
        let payload = text_message_payload(from, "wamid.intake.002b", "Second message");
        let raw_body = serde_json::to_vec(&payload).unwrap();
        let signature = sign_body(&conn.app_secret, &raw_body);
        let req = Request::builder()
            .uri(&format!("/integrations/whatsapp/webhook/{}", conn.token))
            .method(Method::POST)
            .header("x-hub-signature-256", &signature)
            .header("content-type", "application/json")
            .body(Body::from(raw_body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // Both messages in the same conversation
    let normalized = format!("+{from}");
    let conv_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT m.conversation_id) FROM messages m \
         JOIN conversations c ON c.id = m.conversation_id \
         JOIN customer_channel_identifiers cci ON cci.customer_id = c.customer_id \
         WHERE c.tenant_id = $1 AND cci.channel = 'whatsapp' AND cci.identifier = $2",
    )
    .bind(conn.tenant_id)
    .bind(&normalized)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(conv_count, 1, "both messages should share one conversation");

    // Two message rows
    let msg_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages m \
         JOIN conversations c ON c.id = m.conversation_id \
         WHERE c.tenant_id = $1 AND c.customer_id = ( \
             SELECT customer_id FROM customer_channel_identifiers \
             WHERE tenant_id = $2 AND channel = 'whatsapp' AND identifier = $3 AND deleted_at IS NULL \
         )",
    )
    .bind(conn.tenant_id)
    .bind(conn.tenant_id)
    .bind(&normalized)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(msg_count, 2, "two customer messages should exist");
}

#[tokio::test]
async fn identical_redelivery_creates_nothing_new() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let conn = setup_connection(&pool, &mk, true).await;
    let from = "15551112222";
    let wamid = "wamid.intake.003";

    // First delivery
    {
        let app = test_app(pool.clone(), mk.clone());
        let payload = text_message_payload(from, wamid, "Duplicate message");
        let raw_body = serde_json::to_vec(&payload).unwrap();
        let signature = sign_body(&conn.app_secret, &raw_body);
        let req = Request::builder()
            .uri(&format!("/integrations/whatsapp/webhook/{}", conn.token))
            .method(Method::POST)
            .header("x-hub-signature-256", &signature)
            .header("content-type", "application/json")
            .body(Body::from(raw_body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    let msg_count_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE tenant_id = $1",
    )
    .bind(conn.tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(msg_count_before, 1, "first delivery created one message");

    // Duplicate delivery (same wamid)
    {
        let app = test_app(pool.clone(), mk);
        let payload = text_message_payload(from, wamid, "Duplicate message");
        let raw_body = serde_json::to_vec(&payload).unwrap();
        let signature = sign_body(&conn.app_secret, &raw_body);
        let req = Request::builder()
            .uri(&format!("/integrations/whatsapp/webhook/{}", conn.token))
            .method(Method::POST)
            .header("x-hub-signature-256", &signature)
            .header("content-type", "application/json")
            .body(Body::from(raw_body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // Still only one message
    let msg_count_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE tenant_id = $1",
    )
    .bind(conn.tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(msg_count_after, 1, "dedup should prevent a second message");
}

#[tokio::test]
async fn phone_identifier_attaches_without_new_customer_profile() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let conn = setup_connection(&pool, &mk, true).await;
    let from = "15553334444";
    let normalized = format!("+{from}");

    // Pre-create a customer with a phone identifier
    let existing_customer_id: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) \
         VALUES ($1, 'Phone Customer Pre-existing') RETURNING id",
    )
    .bind(conn.tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO customer_channel_identifiers \
         (tenant_id, customer_id, channel, identifier) \
         VALUES ($1, $2, 'phone', $3)",
    )
    .bind(conn.tenant_id)
    .bind(existing_customer_id)
    .bind(&normalized)
    .execute(&pool)
    .await
    .unwrap();

    let initial_customer_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM customers WHERE tenant_id = $1",
    )
    .bind(conn.tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(initial_customer_count, 1, "one customer pre-exists");

    // Send whatsapp message from the same number
    let app = test_app(pool.clone(), mk);
    let payload = text_message_payload(from, "wamid.intake.004", "Hello from phone match");
    let raw_body = serde_json::to_vec(&payload).unwrap();
    let signature = sign_body(&conn.app_secret, &raw_body);
    let req = Request::builder()
        .uri(&format!("/integrations/whatsapp/webhook/{}", conn.token))
        .method(Method::POST)
        .header("x-hub-signature-256", &signature)
        .header("content-type", "application/json")
        .body(Body::from(raw_body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Still only one customer
    let customer_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM customers WHERE tenant_id = $1",
    )
    .bind(conn.tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(customer_count, 1, "no new customer should be created");

    // Verify the original customer has a whatsapp identifier
    let has_whatsapp_id: bool = sqlx::query_scalar(
        "SELECT EXISTS( \
         SELECT 1 FROM customer_channel_identifiers \
         WHERE tenant_id = $1 AND customer_id = $2 AND channel = 'whatsapp' \
           AND identifier = $3 AND deleted_at IS NULL \
         )",
    )
    .bind(conn.tenant_id)
    .bind(existing_customer_id)
    .bind(&normalized)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(has_whatsapp_id, "whatsapp identifier should be attached to existing customer");

    // Verify the existing customer's display_name is unchanged
    let display_name: String = sqlx::query_scalar(
        "SELECT display_name FROM customers WHERE tenant_id = $1 AND id = $2",
    )
    .bind(conn.tenant_id)
    .bind(existing_customer_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        display_name, "Phone Customer Pre-existing",
        "display name should not change"
    );
}

#[tokio::test]
async fn message_after_conversation_closed_opens_new_conversation() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let conn = setup_connection(&pool, &mk, true).await;
    let from = "15555556666";

    // First message creates customer + conversation
    {
        let app = test_app(pool.clone(), mk.clone());
        let payload = text_message_payload(from, "wamid.intake.005a", "First message");
        let raw_body = serde_json::to_vec(&payload).unwrap();
        let signature = sign_body(&conn.app_secret, &raw_body);
        let req = Request::builder()
            .uri(&format!("/integrations/whatsapp/webhook/{}", conn.token))
            .method(Method::POST)
            .header("x-hub-signature-256", &signature)
            .header("content-type", "application/json")
            .body(Body::from(raw_body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    let normalized = format!("+{from}");

    // Close the conversation
    let _closed_conv_id: Uuid = sqlx::query_scalar(
        "UPDATE conversations SET status = 'closed' \
         WHERE tenant_id = $1 AND customer_id = ( \
             SELECT customer_id FROM customer_channel_identifiers \
             WHERE tenant_id = $2 AND channel = 'whatsapp' AND identifier = $3 AND deleted_at IS NULL \
         ) AND channel = 'whatsapp' \
         RETURNING id",
    )
    .bind(conn.tenant_id)
    .bind(conn.tenant_id)
    .bind(&normalized)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Second message
    {
        let app = test_app(pool.clone(), mk);
        let payload = text_message_payload(from, "wamid.intake.005b", "Second message after close");
        let raw_body = serde_json::to_vec(&payload).unwrap();
        let signature = sign_body(&conn.app_secret, &raw_body);
        let req = Request::builder()
            .uri(&format!("/integrations/whatsapp/webhook/{}", conn.token))
            .method(Method::POST)
            .header("x-hub-signature-256", &signature)
            .header("content-type", "application/json")
            .body(Body::from(raw_body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // Two conversations now: the closed one and a new open one
    let conv_rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, status FROM conversations \
         WHERE tenant_id = $1 AND customer_id = ( \
             SELECT customer_id FROM customer_channel_identifiers \
             WHERE tenant_id = $2 AND channel = 'whatsapp' AND identifier = $3 AND deleted_at IS NULL \
         ) AND channel = 'whatsapp' \
         ORDER BY created_at ASC",
    )
    .bind(conn.tenant_id)
    .bind(conn.tenant_id)
    .bind(&normalized)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(conv_rows.len(), 2, "two conversations should exist");
    assert_eq!(conv_rows[0].1, "closed", "original conversation should be closed");
    assert_eq!(conv_rows[1].1, "open", "new conversation should be open");
    assert_ne!(conv_rows[0].0, conv_rows[1].0, "conversation IDs should differ");

    // New message is in the new conversation
    let msg_conv: Uuid = sqlx::query_scalar(
        "SELECT m.conversation_id FROM messages m \
         JOIN whatsapp_message_meta wmm ON wmm.message_id = m.id \
         WHERE wmm.tenant_id = $1 AND wmm.wamid = 'wamid.intake.005b'",
    )
    .bind(conn.tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(msg_conv, conv_rows[1].0, "second message should be in new conversation");
}

#[tokio::test]
async fn wrong_signature_returns_404_and_writes_nothing() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let conn = setup_connection(&pool, &mk, true).await;
    let app = test_app(pool.clone(), mk);

    let from = "15557778888";
    let payload = text_message_payload(from, "wamid.intake.006", "Should not be processed");
    let raw_body = serde_json::to_vec(&payload).unwrap();
    let bad_signature =
        "sha256=0000000000000000000000000000000000000000000000000000000000000000";

    let req = Request::builder()
        .uri(&format!("/integrations/whatsapp/webhook/{}", conn.token))
        .method(Method::POST)
        .header("x-hub-signature-256", bad_signature)
        .header("content-type", "application/json")
        .body(Body::from(raw_body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = response_bytes(resp).await;
    assert_eq!(body, br#"{"error":"not found"}"#);

    // No rows written
    let customer_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM customers WHERE tenant_id = $1",
    )
    .bind(conn.tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(customer_count, 0, "no customers should be created");

    let conv_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversations WHERE tenant_id = $1",
    )
    .bind(conn.tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(conv_count, 0, "no conversations should be created");

    let msg_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE tenant_id = $1",
    )
    .bind(conn.tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(msg_count, 0, "no messages should be created");
}

#[tokio::test]
async fn image_message_creates_pending_attachment() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let conn = setup_connection(&pool, &mk, true).await;
    let app = test_app(pool.clone(), mk);

    let from = "15559990000";
    let wamid = "wamid.intake.007";
    let media_id = "media.whatsapp.abc123";
    let payload = image_message_payload(from, wamid, media_id);
    let raw_body = serde_json::to_vec(&payload).unwrap();
    let signature = sign_body(&conn.app_secret, &raw_body);

    let req = Request::builder()
        .uri(&format!("/integrations/whatsapp/webhook/{}", conn.token))
        .method(Method::POST)
        .header("x-hub-signature-256", &signature)
        .header("content-type", "application/json")
        .body(Body::from(raw_body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    // Attachment row with status='pending'
    let attachment_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM message_attachments \
         WHERE tenant_id = $1 AND status = 'pending'",
    )
    .bind(conn.tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(attachment_count, 1, "one pending attachment should exist");

    let attachment_kind: String = sqlx::query_scalar(
        "SELECT kind FROM message_attachments \
         WHERE tenant_id = $1 AND status = 'pending'",
    )
    .bind(conn.tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(attachment_kind, "image", "attachment kind should be 'image'");
}
