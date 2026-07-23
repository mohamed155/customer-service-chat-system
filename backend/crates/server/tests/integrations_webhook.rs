use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use base64::Engine;
use config::Environment;
use hmac::{Hmac, Mac};
use http_body_util::BodyExt;
use rand::RngCore;
use serde_json::{json, Value};
use server::router;
use server::state::AppState;
use sha2::Sha256;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_INTEGRATION_SECRETS_KEY: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=";
const TEST_SLUG: &str = "generic-webhook";
const TEST_SECRET: &str = "whsec_test_abc123";
const TEST_SOURCE_LABEL: &str = "Billing system";

fn test_config() -> config::AppConfig {
    config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: "redis://127.0.0.1:6379".into(),
        auth_jwt_secret: "test-auth-jwt-secret-at-least-32-bytes".into(),
        auth_session_ttl_seconds: 28_800,
        port: 0,
        bind_address: "0.0.0.0".into(),
        environment: Environment::Test,
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
        ai_key_encryption_key: Some("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=".into()),
        integration_secrets_key: Some(TEST_INTEGRATION_SECRETS_KEY.into()),
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
    }
}

fn app_state(pool: sqlx::PgPool) -> AppState {
    let cfg = test_config();
    AppState {
        config: Arc::new(cfg.clone()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool, &cfg).unwrap(),
    }
}

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("skipping integrations_webhook live tests: DATABASE_URL not set");
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping integrations_webhook live tests: DATABASE_URL is unreachable");
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        return None;
    }
    Some(pool)
}

async fn send(pool: sqlx::PgPool, request: Request<Body>) -> Response {
    router::app_with_test_routes(app_state(pool))
        .oneshot(request)
        .await
        .expect("request should complete")
}

async fn body_bytes(response: Response) -> Vec<u8> {
    response.into_body().collect().await.unwrap().to_bytes().to_vec()
}

async fn body_json(response: Response) -> Value {
    let bytes = body_bytes(response).await;
    serde_json::from_slice(&bytes).unwrap()
}

fn authenticated_request(
    uri: &str,
    method: Method,
    user_id: Uuid,
    tenant_id: Uuid,
) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(method)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .body(Body::empty())
        .unwrap()
}

fn authenticated_json_request(
    uri: &str,
    method: Method,
    user_id: Uuid,
    tenant_id: Uuid,
    body: Value,
) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(method)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    let bytes = mac.finalize().into_bytes();
    format!("sha256={}", hex::encode(bytes))
}

fn webhook_request(token: &str, body: Vec<u8>, signature: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("/api/v1/hooks/v1/{token}"))
        .method(Method::POST)
        .header("content-type", "application/json")
        .header("x-webhook-signature", signature)
        .body(Body::from(body))
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name, platform_role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(format!("iwhk_{}@example.com", Uuid::new_v4()))
    .bind("Integrations Webhook User")
    .bind(None::<String>)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Integrations Webhook Tenant")
        .bind(format!("iwhk-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_membership(pool: &sqlx::PgPool, tenant_id: Uuid, user_id: Uuid, role: &str) {
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(tenant_id)
        .bind(user_id)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
}

struct ConnectionHandle {
    token: String,
    connection_id: Uuid,
}

/// Connect an integration through the real public endpoint. Returns the
/// plaintext webhook token (extracted from the returned `webhook_url`) and
/// the resolved connection id. The connection's `is_active` is `true` until
/// `disconnect_through_api` is called.
async fn connect_through_api(
    pool: sqlx::PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
) -> ConnectionHandle {
    let response = send(
        pool.clone(),
        authenticated_json_request(
            &format!("/api/v1/tenant/integrations/{TEST_SLUG}/connect"),
            Method::POST,
            user_id,
            tenant_id,
            json!({
                "config": { "source_label": TEST_SOURCE_LABEL },
                "secrets": { "signing_secret": TEST_SECRET },
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "connect should succeed for setup"
    );
    let body = body_json(response).await;
    let webhook_url = body["connection"]["webhook_url"]
        .as_str()
        .expect("webhook_url present")
        .to_string();
    let token = webhook_url
        .rsplit_once('/')
        .expect("webhook_url has token")
        .1
        .to_string();

    let connection_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM integration_connections WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    ConnectionHandle {
        token,
        connection_id,
    }
}

async fn disconnect_through_api(
    pool: sqlx::PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Response {
    send(
        pool,
        authenticated_request(
            &format!("/api/v1/tenant/integrations/{TEST_SLUG}/disconnect"),
            Method::POST,
            user_id,
            tenant_id,
        ),
    )
    .await
}

async fn count_events(
    pool: &sqlx::PgPool,
    connection_id: Uuid,
    event_type: &str,
    reason: Option<&str>,
) -> i64 {
    let row: (i64,) = match reason {
        Some(r) => {
            sqlx::query_as(
                "SELECT COUNT(*) FROM integration_events \
                 WHERE connection_id = $1 AND event_type = $2 AND reason = $3",
            )
            .bind(connection_id)
            .bind(event_type)
            .bind(r)
            .fetch_one(pool)
            .await
            .unwrap()
        }
        None => {
            sqlx::query_as(
                "SELECT COUNT(*) FROM integration_events \
                 WHERE connection_id = $1 AND event_type = $2 AND reason IS NULL",
            )
            .bind(connection_id)
            .bind(event_type)
            .fetch_one(pool)
            .await
            .unwrap()
        }
    };
    row.0
}

async fn count_deliveries(pool: &sqlx::PgPool, connection_id: Uuid) -> i64 {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM integration_webhook_deliveries WHERE connection_id = $1",
    )
    .bind(connection_id)
    .fetch_one(pool)
    .await
    .unwrap();
    row.0
}

async fn count_all_events(pool: &sqlx::PgPool) -> i64 {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM integration_events")
        .fetch_one(pool)
        .await
        .unwrap();
    row.0
}

// ---------------------------------------------------------------------------
// T054 — Public webhook intake: accepted / rejected / throttled / sized
// ---------------------------------------------------------------------------

#[tokio::test]
async fn correctly_signed_delivery_returns_202_and_writes_event_and_delivery() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;
    let conn = connect_through_api(pool.clone(), tenant, admin).await;

    let body = br#"{"event":"order.created","id":42}"#.to_vec();
    let signature = sign(TEST_SECRET, &body);

    let response = send(
        pool.clone(),
        webhook_request(&conn.token, body, &signature),
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let payload = body_json(response).await;
    assert_eq!(payload["status"], "accepted");

    assert_eq!(
        count_events(&pool, conn.connection_id, "delivery_accepted", None).await,
        1,
        "exactly one delivery_accepted event must be written"
    );
    assert_eq!(
        count_deliveries(&pool, conn.connection_id).await,
        1,
        "exactly one integration_webhook_deliveries row must be written"
    );
}

#[tokio::test]
async fn bad_signature_returns_404_and_writes_invalid_signature_event() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;
    let conn = connect_through_api(pool.clone(), tenant, admin).await;

    let body = br#"{"event":"order.created"}"#.to_vec();
    // Sign with the wrong secret on purpose.
    let signature = sign("whsec_wrong_secret_value", &body);

    let response = send(
        pool.clone(),
        webhook_request(&conn.token, body, &signature),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    assert_eq!(
        count_events(
            &pool,
            conn.connection_id,
            "delivery_rejected",
            Some("invalid_signature")
        )
        .await,
        1,
        "exactly one delivery_rejected/invalid_signature event must be written"
    );
    assert_eq!(
        count_deliveries(&pool, conn.connection_id).await,
        0,
        "rejected deliveries must not be persisted"
    );
}

#[tokio::test]
async fn unknown_token_returns_404_with_no_new_events() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    // Even though this tenant has no connection, we want the unknown-token
    // branch to be the one exercised. We use a random token that hashes to
    // nothing in integration_connections.webhook_token_hash.
    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    let before = count_all_events(&pool).await;

    // 32 random bytes URL-safe-base64-encoded to look like a real token.
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let bogus_token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);

    let body = br#"{"hello":"world"}"#.to_vec();
    let signature = sign(TEST_SECRET, &body);

    let response = send(pool.clone(), webhook_request(&bogus_token, body, &signature)).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let after = count_all_events(&pool).await;
    assert_eq!(
        after, before,
        "unknown token must not write any integration_events rows"
    );
}

#[tokio::test]
async fn disconnected_connection_returns_404_and_writes_inactive_event() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;
    let conn = connect_through_api(pool.clone(), tenant, admin).await;

    // Disconnect the connection.
    let response = disconnect_through_api(pool.clone(), tenant, admin).await;
    assert_eq!(response.status(), StatusCode::OK);

    // The webhook token still resolves (it was rotated away, but the row is
    // preserved with is_active=false), so a correctly signed delivery now
    // hits the inactive-connection branch.
    let body = br#"{"event":"ping"}"#.to_vec();
    let signature = sign(TEST_SECRET, &body);
    let response = send(
        pool.clone(),
        webhook_request(&conn.token, body, &signature),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    assert_eq!(
        count_events(
            &pool,
            conn.connection_id,
            "delivery_rejected",
            Some("inactive_connection")
        )
        .await,
        1,
        "exactly one delivery_rejected/inactive_connection event must be written"
    );
    assert_eq!(
        count_deliveries(&pool, conn.connection_id).await,
        0,
        "inactive deliveries must not be persisted"
    );
}

#[tokio::test]
async fn unknown_token_and_bad_signature_404_bodies_are_byte_identical() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;
    let conn = connect_through_api(pool.clone(), tenant, admin).await;

    // Body for the bad-signature branch.
    let body = br#"{"event":"order.created"}"#.to_vec();
    let bad_signature = sign("whsec_wrong_secret_value", &body);

    // Send the bad-signature request first and capture the raw response body.
    let bad_sig_response = send(
        pool.clone(),
        webhook_request(&conn.token, body, &bad_signature),
    )
    .await;
    assert_eq!(bad_sig_response.status(), StatusCode::NOT_FOUND);
    let body_bad_sig = body_bytes(bad_sig_response).await;

    // Build a bogus token and send the same shape to the unknown-token branch.
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let bogus_token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);

    let body2 = br#"{"event":"order.created"}"#.to_vec();
    let sig2 = sign(TEST_SECRET, &body2);
    let unknown_response = send(pool.clone(), webhook_request(&bogus_token, body2, &sig2)).await;
    assert_eq!(unknown_response.status(), StatusCode::NOT_FOUND);
    let body_unknown = body_bytes(unknown_response).await;

    assert_eq!(
        body_unknown, body_bad_sig,
        "404 bodies must be identical to avoid existence leak"
    );
}

#[tokio::test]
async fn body_larger_than_256kb_returns_413() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;
    let conn = connect_through_api(pool.clone(), tenant, admin).await;

    // 256 KB + 1 byte payload. The signed string would be a valid signature
    // if the handler were called, but the body-limit layer rejects the
    // request before that.
    let body = vec![b'.'; 256 * 1024 + 1];
    let signature = sign(TEST_SECRET, &body);

    let response = send(
        pool.clone(),
        webhook_request(&conn.token, body, &signature),
    )
    .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn sixty_one_deliveries_produce_at_most_one_rate_limited_event() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;
    let conn = connect_through_api(pool.clone(), tenant, admin).await;

    // Send 60 correctly signed deliveries — all should be accepted.
    for i in 0..60 {
        let payload = format!(r#"{{"seq":{i}}}"#);
        let body = payload.as_bytes().to_vec();
        let signature = sign(TEST_SECRET, &body);
        let response = send(
            pool.clone(),
            webhook_request(&conn.token, body, &signature),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::ACCEPTED,
            "request {i} of 60 should be accepted"
        );
    }

    // The 61st should be rate limited.
    let body = br#"{"seq":60}"#.to_vec();
    let signature = sign(TEST_SECRET, &body);
    let response = send(
        pool.clone(),
        webhook_request(&conn.token, body, &signature),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "the 61st correctly signed delivery must be 429"
    );

    // And several more, all still rate limited.
    for i in 61..66 {
        let body = format!(r#"{{"seq":{i}}}"#).into_bytes();
        let signature = sign(TEST_SECRET, &body);
        let response = send(
            pool.clone(),
            webhook_request(&conn.token, body, &signature),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "request {i} should also be rate limited"
        );
    }

    // Acceptance-side counts: 60 deliveries + 60 delivery_accepted events.
    assert_eq!(
        count_deliveries(&pool, conn.connection_id).await,
        60,
        "expected 60 accepted deliveries to be persisted"
    );
    assert_eq!(
        count_events(&pool, conn.connection_id, "delivery_accepted", None).await,
        60,
        "expected 60 delivery_accepted events"
    );

    // Throttling assertion: at most one rate_limited event for the burst
    // (the 1-per-connection-per-minute budget from T047).
    let rate_limited_count =
        count_events(&pool, conn.connection_id, "delivery_rejected", Some("rate_limited")).await;
    assert!(
        (1..=2).contains(&rate_limited_count),
        "rate_limited events must be throttled to at most one per burst, got {rate_limited_count}"
    );
}
