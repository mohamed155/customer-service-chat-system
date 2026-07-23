use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use chrono::Utc;
use http_body_util::BodyExt;
use integrations::crypto::{aad, hint, seal, MasterKey};
use integrations::webhook::hash_token;
use serde_json::json;
use server::router;
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_MASTER_KEY_B64: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=";
const WHATSAPP_SLUG: &str = "whatsapp";
const FIELD_ACCESS_TOKEN: &str = "access_token";
const FIELD_VERIFY_TOKEN: &str = "verify_token";
const FIELD_WEBHOOK_TOKEN: &str = "__webhook_token";
const FIELD_APP_SECRET: &str = "app_secret";
const ACCESS_TOKEN_VALUE: &str = "test_access_token_reply";
const VERIFY_TOKEN_VALUE: &str = "test_verify_token_reply";
const APP_SECRET_VALUE: &str = "test_app_secret_reply";
const PHONE_NUMBER_ID: &str = "987654321";

const TEST_ENV: config::Environment = config::Environment::Test;

fn test_config() -> config::AppConfig {
    config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: "redis://127.0.0.1:6379".into(),
        auth_jwt_secret: "test-auth-jwt-secret-at-least-32-bytes".into(),
        auth_session_ttl_seconds: 28_800,
        port: 0,
        bind_address: "0.0.0.0".into(),
        environment: TEST_ENV,
        cors_allowed_origins: vec![],
        log_format: config::LogFormat::Pretty,
        smtp_url: None,
        smtp_from: None,
        public_dashboard_url: "http://localhost:4200".into(),
        db_max_connections: 2,
        db_acquire_timeout_ms: 5000,
        ready_probe_timeout_ms: 5000,
        shutdown_grace_seconds: 1,
        docs_enabled: false,
        ai_key_encryption_key: None,
        integration_secrets_key: None,
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
    }
}

fn app_state(pool: sqlx::PgPool) -> AppState {
    AppState {
        config: Arc::new(test_config()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool, &test_config()).unwrap(),
    }
}

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            eprintln!("skipping whatsapp reply guard tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping whatsapp reply guard tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query(
        "TRUNCATE TABLE \
         messages, customer_channel_identifiers, customers, conversations, \
         outbox_events, audit_logs, tenant_invitations, tenant_memberships, \
         tenants, users, \
         integration_events, integration_webhook_deliveries, \
         integration_secrets, integration_connections \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("failed to reset test tables");
}

async fn send(pool: sqlx::PgPool, request: Request<Body>) -> axum::response::Response {
    router::app_with_test_routes(app_state(pool))
        .oneshot(request)
        .await
        .expect("request should complete")
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn json_post(uri: &str, user_id: Uuid, tenant_id: Uuid, body: serde_json::Value) -> Request<Body> {
    let bytes = serde_json::to_vec(&body).unwrap();
    Request::builder()
        .uri(uri)
        .method(Method::POST)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("content-type", "application/json")
        .body(Body::from(bytes))
        .unwrap()
}

fn master_key() -> Arc<MasterKey> {
    Arc::new(MasterKey::from_base64(TEST_MASTER_KEY_B64).unwrap())
}

async fn whatsapp_catalog_id(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("SELECT id FROM integration_catalog WHERE slug = $1")
        .bind(WHATSAPP_SLUG)
        .fetch_one(pool)
        .await
        .expect("whatsapp catalog entry must exist \u{2014} run migration 0057")
}

/// Set up an active WhatsApp connection with encrypted secrets.
async fn setup_whatsapp_connection(
    pool: &sqlx::PgPool,
    mk: &MasterKey,
    tenant_id: Uuid,
) -> (Uuid, String) {
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

    (connection_id, webhook_token)
}

// ---------------------------------------------------------------------------
// Seed helpers
// ---------------------------------------------------------------------------

async fn seed_tenant(pool: &sqlx::PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(name)
        .bind(format!("wa-rg-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("WA Reply Guard Test User")
        .fetch_one(pool)
        .await
        .unwrap()
}

struct SeededMembership {
    user_id: Uuid,
    membership_id: Uuid,
}

async fn seed_admin(pool: &sqlx::PgPool, tenant_id: Uuid, email: &str) -> SeededMembership {
    let user_id = seed_user(pool, email).await;
    let membership_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, 'admin', 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap();
    SeededMembership {
        user_id,
        membership_id,
    }
}

async fn seed_customer(pool: &sqlx::PgPool, tenant_id: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) \
         VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_conversation(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    customer_id: Uuid,
    channel: &str,
    status: &str,
    last_activity_at: chrono::DateTime<chrono::Utc>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, last_activity_at) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(channel)
    .bind(status)
    .bind(last_activity_at)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_customer_message(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    body: &str,
    created_at: chrono::DateTime<chrono::Utc>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body, created_at) \
         VALUES ($1, $2, 'customer', $3, $4) RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(body)
    .bind(created_at)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_reply_message(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    body: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'reply', $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(body)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(conversations_db)]
async fn reply_whatsapp_window_expired_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let tenant_id = seed_tenant(&pool, "WA Window Expired").await;
    let admin = seed_admin(&pool, tenant_id, "wa-window-expired@example.com").await;
    setup_whatsapp_connection(&pool, &mk, tenant_id).await;
    let customer_id = seed_customer(&pool, tenant_id, "WA Window Cust").await;

    // Customer message > 24h ago
    let old_time = Utc::now() - chrono::Duration::hours(25);
    let conv_id = seed_conversation(&pool, tenant_id, customer_id, "whatsapp", "open", old_time).await;
    seed_customer_message(&pool, tenant_id, conv_id, "Old customer message", old_time).await;

    let payload = json!({"kind": "reply", "body": "This should be blocked"});
    let response = send(
        pool.clone(),
        json_post(
            &format!("/api/v1/tenant/conversations/{conv_id}/messages"),
            admin.user_id,
            tenant_id,
            payload,
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(response).await;
    assert_eq!(
        body["error"]["code"], "whatsapp_window_expired",
        "should return whatsapp_window_expired code, got: {:?}",
        body["error"]["code"]
    );
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
async fn reply_whatsapp_disconnected_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    // No mk needed since we're not setting up a connection
    let tenant_id = seed_tenant(&pool, "WA Disconnected").await;
    let admin = seed_admin(&pool, tenant_id, "wa-disconnected@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "WA Disc Cust").await;

    // Conversation with whatsapp channel, but NO active connection
    let now = Utc::now();
    let conv_id = seed_conversation(&pool, tenant_id, customer_id, "whatsapp", "open", now).await;
    seed_customer_message(&pool, tenant_id, conv_id, "Customer message", now).await;

    let payload = json!({"kind": "reply", "body": "Should be blocked"});
    let response = send(
        pool.clone(),
        json_post(
            &format!("/api/v1/tenant/conversations/{conv_id}/messages"),
            admin.user_id,
            tenant_id,
            payload,
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(response).await;
    assert_eq!(
        body["error"]["code"], "whatsapp_channel_disconnected",
        "should return whatsapp_channel_disconnected code, got: {:?}",
        body["error"]["code"]
    );
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
async fn reply_whatsapp_body_too_long_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let tenant_id = seed_tenant(&pool, "WA Body Too Long").await;
    let admin = seed_admin(&pool, tenant_id, "wa-body-too-long@example.com").await;
    setup_whatsapp_connection(&pool, &mk, tenant_id).await;
    let customer_id = seed_customer(&pool, tenant_id, "WA Body Cust").await;

    let now = Utc::now();
    let conv_id = seed_conversation(&pool, tenant_id, customer_id, "whatsapp", "open", now).await;
    seed_customer_message(&pool, tenant_id, conv_id, "Recent customer msg", now).await;

    // Body > 4096 chars
    let long_body = "x".repeat(4097);
    let payload = json!({"kind": "reply", "body": long_body});
    let response = send(
        pool.clone(),
        json_post(
            &format!("/api/v1/tenant/conversations/{conv_id}/messages"),
            admin.user_id,
            tenant_id,
            payload,
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(response).await;
    assert_eq!(
        body["error"]["code"], "whatsapp_body_too_long",
        "should return whatsapp_body_too_long code, got: {:?}",
        body["error"]["code"]
    );
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
async fn reply_whatsapp_success_emits_outbound() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let tenant_id = seed_tenant(&pool, "WA Reply Success").await;
    let admin = seed_admin(&pool, tenant_id, "wa-reply-success@example.com").await;
    setup_whatsapp_connection(&pool, &mk, tenant_id).await;
    let customer_id = seed_customer(&pool, tenant_id, "WA Success Cust").await;

    let now = Utc::now();
    let conv_id = seed_conversation(&pool, tenant_id, customer_id, "whatsapp", "open", now).await;
    seed_customer_message(&pool, tenant_id, conv_id, "Recent customer message", now).await;

    let payload = json!({"kind": "reply", "body": "Valid reply message"});
    let response = send(
        pool.clone(),
        json_post(
            &format!("/api/v1/tenant/conversations/{conv_id}/messages"),
            admin.user_id,
            tenant_id,
            payload,
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK, "reply should succeed");

    // Verify outbox event was emitted
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
        "whatsapp.outbound_message outbox event should be emitted"
    );

    // Verify the payload contains the conversation_id
    let payload_conv_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT (payload->>'conversationId')::uuid FROM outbox_events \
         WHERE tenant_id = $1 AND event_type = 'whatsapp.outbound_message'",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(payload_conv_id, Some(conv_id), "outbox payload should reference the conversation");
}
