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
const FIELD_ACCESS_TOKEN: &str = "access_token";
const FIELD_VERIFY_TOKEN: &str = "verify_token";
const FIELD_WEBHOOK_TOKEN: &str = "__webhook_token";
const FIELD_APP_SECRET: &str = "app_secret";
const ACCESS_TOKEN_VALUE: &str = "test_access_token_123";
const VERIFY_TOKEN_VALUE: &str = "test_verify_token";
const APP_SECRET_VALUE: &str = "test_app_secret";
const PHONE_NUMBER_ID: &str = "123456789";

fn master_key() -> Arc<MasterKey> {
    Arc::new(MasterKey::from_base64(TEST_MASTER_KEY_B64).unwrap())
}

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("skipping sender tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping sender tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

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
    .bind("WhatsApp Sender Test Tenant")
    .bind(format!("wst-{}", Uuid::new_v4().simple()))
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn whatsapp_catalog_id(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("SELECT id FROM integration_catalog WHERE slug = $1")
        .bind(WHATSAPP_SLUG)
        .fetch_one(pool)
        .await
        .expect("whatsapp catalog entry must exist \u{2014} run migration 0057")
}

struct SenderFixture {
    #[allow(dead_code)]
    tenant_id: Uuid,
    #[allow(dead_code)]
    connection_id: Uuid,
    conversation_id: Uuid,
    message_id: Uuid,
    customer_id: Uuid,
    token: String,
    app_secret: String,
}

async fn setup_sender_fixture(
    pool: &sqlx::PgPool,
    mk: &MasterKey,
    with_customer_message: bool,
    customer_message_age_secs: i64,
) -> SenderFixture {
    let tenant_id = seed_tenant(pool).await;
    let catalog_id = whatsapp_catalog_id(pool).await;

    // Encrypt secrets
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

    // Insert connection
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

    // Insert secrets: access_token, verify_token, app_secret
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

    // Customer
    let customer_id: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) \
         VALUES ($1, 'Sender Test Customer') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .unwrap();

    // Customer channel identifier (whatsapp)
    let wa_identifier = "+15551234567";
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

    // Conversation
    let conversation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, last_activity_at) \
         VALUES ($1, $2, 'whatsapp', 'open', now()) RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(pool)
    .await
    .unwrap();

    // Customer message (for window check)
    if with_customer_message {
        let customer_msg_time = chrono::Utc::now() - chrono::Duration::seconds(customer_message_age_secs);
        sqlx::query(
            "INSERT INTO messages (tenant_id, conversation_id, kind, body, created_at) \
             VALUES ($1, $2, 'customer', 'I need help', $3)",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .bind(customer_msg_time)
        .execute(pool)
        .await
        .unwrap();

        // Bump last_activity_at to reflect the customer message
        sqlx::query(
            "UPDATE conversations SET last_activity_at = $1 WHERE tenant_id = $2 AND id = $3",
        )
        .bind(customer_msg_time)
        .bind(tenant_id)
        .bind(conversation_id)
        .execute(pool)
        .await
        .unwrap();
    }

    // Outbound message (kind='reply')
    let message_id: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'reply', 'This is a reply') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(pool)
    .await
    .unwrap();

    // Outbox event
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, 'conversation', $2, $3, 'whatsapp.outbound_message', $4, now())",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(tenant_id)
    .bind(json!({
        "tenantId": tenant_id,
        "conversationId": conversation_id,
        "messageId": message_id,
    }))
    .execute(pool)
    .await
    .unwrap();

    SenderFixture {
        tenant_id,
        connection_id,
        conversation_id,
        message_id,
        customer_id,
        token: webhook_token,
        app_secret: APP_SECRET_VALUE.to_string(),
    }
}

async fn response_bytes(resp: Response) -> Vec<u8> {
    resp.into_body().collect().await.unwrap().to_bytes().to_vec()
}

fn sign_body(secret: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    let result = mac.finalize().into_bytes();
    format!("sha256={}", hex::encode(result))
}

fn status_webhook_payload(wamid: &str, status: &str) -> serde_json::Value {
    json!({
        "entry": [{
            "changes": [{
                "value": {
                    "statuses": [{
                        "id": wamid,
                        "status": status,
                        "timestamp": "2026-07-23T12:00:00Z",
                        "errors": []
                    }],
                    "messages": [],
                    "contacts": []
                }
            }]
        }]
    })
}

fn test_app(pool: sqlx::PgPool, mk: Arc<MasterKey>) -> Router {
    whatsapp::routes::public_router()
        .layer(axum::extract::Extension(mk))
        .layer(axum::extract::Extension(Arc::new(InMemoryRateLimitStore::new())))
        .with_state(pool)
}

// ---------------------------------------------------------------------------
// Sender worker tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sender_success_path() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let fixture = setup_sender_fixture(&pool, &mk, true, 3600).await;

    let mock = Arc::new(whatsapp::api::MockWhatsAppApi::new());
    mock.push_send_text(Ok("wamid.test.success".into()));

    whatsapp::sender::process_one(&pool, &*mock, &mk)
        .await
        .expect("process_one should succeed");

    let meta: (String,) = sqlx::query_as(
        "SELECT delivery_status FROM whatsapp_message_meta \
         WHERE tenant_id = $1 AND message_id = $2",
    )
    .bind(fixture.tenant_id)
    .bind(fixture.message_id)
    .fetch_one(&pool)
    .await
    .expect("meta row should exist");

    assert_eq!(meta.0, "sent", "delivery_status should be 'sent'");

    let wamid: String = sqlx::query_scalar(
        "SELECT wamid FROM whatsapp_message_meta \
         WHERE tenant_id = $1 AND message_id = $2",
    )
    .bind(fixture.tenant_id)
    .bind(fixture.message_id)
    .fetch_one(&pool)
    .await
    .expect("wamid should be set");

    assert_eq!(wamid, "wamid.test.success", "wamid should match mock response");
}

#[tokio::test]
async fn sender_window_expired() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let fixture = setup_sender_fixture(&pool, &mk, true, 3600).await;

    let mock = Arc::new(whatsapp::api::MockWhatsAppApi::new());
    mock.push_send_text(Err(whatsapp::api::SendError::WindowExpired));

    whatsapp::sender::process_one(&pool, &*mock, &mk)
        .await
        .expect("process_one should succeed");

    let meta: (String, String) = sqlx::query_as(
        "SELECT delivery_status, failure_reason FROM whatsapp_message_meta \
         WHERE tenant_id = $1 AND message_id = $2",
    )
    .bind(fixture.tenant_id)
    .bind(fixture.message_id)
    .fetch_one(&pool)
    .await
    .expect("meta row should exist");

    assert_eq!(meta.0, "failed", "delivery_status should be 'failed'");
    assert!(
        meta.1.contains("window"),
        "failure_reason should mention window, got: {}",
        meta.1
    );
}

#[tokio::test]
async fn sender_unauthorized() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let fixture = setup_sender_fixture(&pool, &mk, true, 3600).await;

    let mock = Arc::new(whatsapp::api::MockWhatsAppApi::new());
    mock.push_send_text(Err(whatsapp::api::SendError::Unauthorized));

    whatsapp::sender::process_one(&pool, &*mock, &mk)
        .await
        .expect("process_one should succeed");

    let meta: (String, String) = sqlx::query_as(
        "SELECT delivery_status, failure_reason FROM whatsapp_message_meta \
         WHERE tenant_id = $1 AND message_id = $2",
    )
    .bind(fixture.tenant_id)
    .bind(fixture.message_id)
    .fetch_one(&pool)
    .await
    .expect("meta row should exist");

    assert_eq!(meta.0, "failed", "delivery_status should be 'failed'");
    assert!(
        meta.1.contains("credential"),
        "failure_reason should mention credential, got: {}",
        meta.1
    );
}

// ---------------------------------------------------------------------------
// Status webhook monotonicity test
// ---------------------------------------------------------------------------

#[tokio::test]
async fn status_webhook_does_not_downgrade_read_to_delivered() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let fixture = setup_sender_fixture(&pool, &mk, true, 3600).await;
    let wamid = "wamid.monotonicity.test";

    // Set up meta with a 'sent' delivery_status and the target wamid
    sqlx::query(
        "UPDATE whatsapp_message_meta \
         SET delivery_status = 'sent', wamid = $1 \
         WHERE tenant_id = $2 AND message_id = $3",
    )
    .bind(wamid)
    .bind(fixture.tenant_id)
    .bind(fixture.message_id)
    .execute(&pool)
    .await
    .unwrap();

    // 1. POST a 'read' status
    let app = test_app(pool.clone(), mk.clone());
    let payload = status_webhook_payload(wamid, "read");
    let raw_body = serde_json::to_vec(&payload).unwrap();
    let signature = sign_body(&fixture.app_secret, &raw_body);

    let req = Request::builder()
        .uri(&format!("/integrations/whatsapp/webhook/{}", fixture.token))
        .method(Method::POST)
        .header("x-hub-signature-256", &signature)
        .header("content-type", "application/json")
        .body(Body::from(raw_body))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let status_after_read: String = sqlx::query_scalar(
        "SELECT delivery_status FROM whatsapp_message_meta \
         WHERE tenant_id = $1 AND message_id = $2",
    )
    .bind(fixture.tenant_id)
    .bind(fixture.message_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status_after_read, "read", "status should be promoted to 'read'");

    // 2. POST a 'delivered' status (lower rank)
    let app2 = test_app(pool.clone(), mk);
    let payload2 = status_webhook_payload(wamid, "delivered");
    let raw_body2 = serde_json::to_vec(&payload2).unwrap();
    let signature2 = sign_body(&fixture.app_secret, &raw_body2);

    let req2 = Request::builder()
        .uri(&format!("/integrations/whatsapp/webhook/{}", fixture.token))
        .method(Method::POST)
        .header("x-hub-signature-256", &signature2)
        .header("content-type", "application/json")
        .body(Body::from(raw_body2))
        .unwrap();
    let resp2 = app2.oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);

    let status_after_delivered: String = sqlx::query_scalar(
        "SELECT delivery_status FROM whatsapp_message_meta \
         WHERE tenant_id = $1 AND message_id = $2",
    )
    .bind(fixture.tenant_id)
    .bind(fixture.message_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        status_after_delivered, "read",
        "status should NOT be downgraded from 'read' to 'delivered'"
    );
}
