use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use axum::Router;
use http_body_util::BodyExt;
use integrations::crypto::{aad, hint, seal, MasterKey};
use integrations::webhook::hash_token;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_MASTER_KEY_B64: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=";
const WHATSAPP_SLUG: &str = "whatsapp";
const FIELD_VERIFY_TOKEN: &str = "verify_token";
const FIELD_WEBHOOK_TOKEN: &str = "__webhook_token";
const VERIFY_TOKEN_VALUE: &str = "my_test_verify_token";
const CHALLENGE_VALUE: &str = "abc123";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn master_key() -> Arc<MasterKey> {
    Arc::new(MasterKey::from_base64(TEST_MASTER_KEY_B64).unwrap())
}

fn test_app(pool: sqlx::PgPool, mk: Arc<MasterKey>) -> Router {
    whatsapp::routes::public_router()
        .layer(axum::extract::Extension(mk))
        .with_state(pool)
}

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("skipping handshake tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping handshake tests: DATABASE_URL is unreachable");
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
         tenant_memberships, tenants \
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
    .bind("WhatsApp Handshake Test Tenant")
    .bind(format!("wht-{}", Uuid::new_v4().simple()))
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
}

async fn setup_connection(
    pool: &sqlx::PgPool,
    mk: &MasterKey,
    is_active: bool,
) -> ConnectionFixture {
    let tenant_id = seed_tenant(pool).await;
    let catalog_id = whatsapp_catalog_id(pool).await;

    // Encrypt verify_token for integration_secrets.
    let verify_aad = aad(tenant_id, WHATSAPP_SLUG, FIELD_VERIFY_TOKEN);
    let (verify_ct, verify_nonce) = seal(mk, &verify_aad, VERIFY_TOKEN_VALUE).unwrap();

    // Generate a webhook token and encrypt it for the connection row.
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

    ConnectionFixture {
        token: webhook_token,
        connection_id,
        verify_token: VERIFY_TOKEN_VALUE.to_string(),
        tenant_id,
    }
}

async fn handshake_request(
    app: &mut Router,
    token: &str,
    mode: &str,
    verify_token: &str,
    challenge: &str,
) -> Response {
    let uri = format!(
        "/integrations/whatsapp/webhook/{token}?hub.mode={mode}&hub.verify_token={verify_token}&hub.challenge={challenge}"
    );
    let req = Request::builder()
        .uri(&uri)
        .method(Method::GET)
        .body(Body::empty())
        .unwrap();
    app.oneshot(req).await.unwrap()
}

async fn response_bytes(resp: Response) -> Vec<u8> {
    resp.into_body().collect().await.unwrap().to_bytes().to_vec()
}

async fn response_text(resp: Response) -> String {
    String::from_utf8(response_bytes(resp).await).unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn handshake_echoes_challenge_on_correct_verify_token() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let conn = setup_connection(&pool, &mk, true).await;
    let mut app = test_app(pool, mk);

    let resp = handshake_request(
        &mut app,
        &conn.token,
        "subscribe",
        &conn.verify_token,
        CHALLENGE_VALUE,
    )
    .await;

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(response_text(resp).await, CHALLENGE_VALUE);
}

#[tokio::test]
async fn wrong_verify_token_returns_404_with_identical_body() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let conn = setup_connection(&pool, &mk, true).await;
    let mut app = test_app(pool, mk);

    let resp = handshake_request(
        &mut app,
        &conn.token,
        "subscribe",
        "wrong_verify_token",
        CHALLENGE_VALUE,
    )
    .await;

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = response_bytes(resp).await;
    assert_eq!(body, br#"{"error":"not found"}"#);
}

#[tokio::test]
async fn wrong_mode_returns_404() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let conn = setup_connection(&pool, &mk, true).await;
    let mut app = test_app(pool, mk);

    let resp = handshake_request(
        &mut app,
        &conn.token,
        "unsubscribe",
        &conn.verify_token,
        CHALLENGE_VALUE,
    )
    .await;

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = response_bytes(resp).await;
    assert_eq!(body, br#"{"error":"not found"}"#);
}

#[tokio::test]
async fn unknown_token_returns_404() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    let conn = setup_connection(&pool, &mk, true).await;
    let mut app = test_app(pool, mk.clone());

    // Use a different connection's token (doesn't exist in this test's setup)
    // or simply a random UUID that won't match any connection.
    let bogus_token = "bogus-token-that-hashes-to-nothing".to_string();

    let resp = handshake_request(
        &mut app,
        &bogus_token,
        "subscribe",
        &conn.verify_token,
        CHALLENGE_VALUE,
    )
    .await;

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = response_bytes(resp).await;
    assert_eq!(body, br#"{"error":"not found"}"#);
}

#[tokio::test]
async fn disconnected_connection_returns_404() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let mk = master_key();
    // Create connection with is_active = false.
    let conn = setup_connection(&pool, &mk, false).await;
    let mut app = test_app(pool, mk);

    let resp = handshake_request(
        &mut app,
        &conn.token,
        "subscribe",
        &conn.verify_token,
        CHALLENGE_VALUE,
    )
    .await;

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = response_bytes(resp).await;
    assert_eq!(body, br#"{"error":"not found"}"#);
}
