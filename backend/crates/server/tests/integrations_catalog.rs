use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
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

const TEST_ENV: Environment = Environment::Test;

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
            eprintln!("skipping integrations_catalog live tests: DATABASE_URL not set");
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping integrations_catalog live tests: DATABASE_URL is unreachable");
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

async fn body_json(response: Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
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

async fn seed_user(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name, platform_role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(format!("icat_{}@example.com", Uuid::new_v4()))
    .bind("Integrations Catalog User")
    .bind(None::<String>)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Integrations Catalog Tenant")
        .bind(format!("icat-{}", Uuid::new_v4().simple()))
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

async fn seed_connection(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    catalog_slug: &str,
    is_active: bool,
) -> Uuid {
    let catalog_id: Uuid =
        sqlx::query_scalar("SELECT id FROM integration_catalog WHERE slug = $1")
            .bind(catalog_slug)
            .fetch_one(pool)
            .await
            .unwrap();

    let mut token_hash = [0u8; 32];
    let mut token_ciphertext = vec![0u8; 48];
    let mut token_nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut token_hash);
    rand::rngs::OsRng.fill_bytes(&mut token_ciphertext);
    rand::rngs::OsRng.fill_bytes(&mut token_nonce);

    sqlx::query_scalar(
        "INSERT INTO integration_connections \
         (tenant_id, catalog_id, is_active, config, \
          webhook_token_hash, webhook_token_ciphertext, webhook_token_nonce) \
         VALUES ($1, $2, $3, '{}'::jsonb, $4, $5, $6) RETURNING id",
    )
    .bind(tenant_id)
    .bind(catalog_id)
    .bind(is_active)
    .bind(token_hash.to_vec())
    .bind(token_ciphertext)
    .bind(token_nonce.to_vec())
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn list_returns_all_seeded_catalog_entries() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations",
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let entries = json["data"].as_array().unwrap();
    let slugs: Vec<&str> = entries
        .iter()
        .map(|e| e["slug"].as_str().unwrap())
        .collect();
    assert_eq!(entries.len(), 4);
    assert!(slugs.contains(&"generic-webhook"));
    assert!(slugs.contains(&"slack"));
    assert!(slugs.contains(&"microsoft-teams"));
    assert!(slugs.contains(&"crm"));
}

#[tokio::test]
async fn unavailable_entries_have_is_available_false() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations",
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let entries = json["data"].as_array().unwrap();
    let slack = entries
        .iter()
        .find(|e| e["slug"] == "slack")
        .expect("slack entry present");
    assert_eq!(slack["is_available"], false);
    assert_eq!(slack["status"], "not_connected");
}

#[tokio::test]
async fn tenant_with_no_connections_sees_not_connected_everywhere() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations",
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let entries = json["data"].as_array().unwrap();
    assert_eq!(entries.len(), 4);
    for entry in entries {
        assert_eq!(
            entry["status"], "not_connected",
            "fresh tenant entry {} should be not_connected",
            entry["slug"]
        );
    }
}

#[tokio::test]
async fn active_connection_yields_connected_status() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;
    seed_connection(&pool, tenant, "generic-webhook", true).await;

    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations",
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let entries = json["data"].as_array().unwrap();
    let entry = entries
        .iter()
        .find(|e| e["slug"] == "generic-webhook")
        .expect("generic-webhook entry present");
    assert_eq!(entry["status"], "connected");
}

#[tokio::test]
async fn inactive_connection_yields_disconnected_status() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;
    seed_connection(&pool, tenant, "generic-webhook", false).await;

    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations",
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let entries = json["data"].as_array().unwrap();
    let entry = entries
        .iter()
        .find(|e| e["slug"] == "generic-webhook")
        .expect("generic-webhook entry present");
    assert_eq!(entry["status"], "disconnected");
}

#[tokio::test]
async fn detail_returns_404_for_unknown_slug() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations/does-not-exist",
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn retired_entry_keeps_existing_connection_working() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;
    seed_connection(&pool, tenant, "generic-webhook", true).await;

    // Retire the catalog entry: existing connections must keep working
    // (FR-001: retiring is a forward-looking flag — the connection row
    // already on disk is the source of truth for the tenant's view).
    sqlx::query("UPDATE integration_catalog SET is_available = false WHERE slug = $1")
        .bind("generic-webhook")
        .execute(&pool)
        .await
        .unwrap();

    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations/generic-webhook",
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["slug"], "generic-webhook");
    assert_eq!(json["is_available"], false);
    assert_eq!(json["status"], "connected");
    assert!(
        json["connection"].is_object(),
        "retired entry must still show existing connection"
    );
}

// ---------------------------------------------------------------------------
// T056 — Status derivation: 3 rejected deliveries flip to "error",
// a subsequent accepted delivery flips back to "connected" (SC-006).
// ---------------------------------------------------------------------------

const T056_SLUG: &str = "generic-webhook";
const T056_SECRET: &str = "whsec_status_derive_xx";

fn sign_body(secret: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    let bytes = mac.finalize().into_bytes();
    format!("sha256={}", hex::encode(bytes))
}

fn webhook_request_for(token: &str, body: Vec<u8>, signature: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("/api/v1/hooks/v1/{token}"))
        .method(Method::POST)
        .header("content-type", "application/json")
        .header("x-webhook-signature", signature)
        .body(Body::from(body))
        .unwrap()
}

fn authenticated_json_request_local(
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

async fn count_recent_invalid_signature_events(
    pool: &sqlx::PgPool,
    connection_id: Uuid,
) -> i64 {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM integration_events \
         WHERE connection_id = $1 AND event_type = 'delivery_rejected' \
           AND reason = 'invalid_signature'",
    )
    .bind(connection_id)
    .fetch_one(pool)
    .await
    .unwrap();
    row.0
}

#[tokio::test]
async fn three_rejected_deliveries_yield_error_status() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    // Connect through the real API so we have a valid signing secret and
    // webhook token.
    let response = send(
        pool.clone(),
        authenticated_json_request_local(
            &format!("/api/v1/tenant/integrations/{T056_SLUG}/connect"),
            Method::POST,
            admin,
            tenant,
            json!({
                "config": { "source_label": "Status test" },
                "secrets": { "signing_secret": T056_SECRET },
            }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
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
    let connection_id: Uuid =
        sqlx::query_scalar("SELECT id FROM integration_connections WHERE tenant_id = $1")
            .bind(tenant)
            .fetch_one(&pool)
            .await
            .unwrap();

    // Three deliveries with a wrong signature: each returns 404 and writes
    // an `invalid_signature` event. `invalid_signature` is NOT throttled, so
    // all three rows are persisted.
    for i in 0..3 {
        let body = format!(r#"{{"seq":{i}}}"#).into_bytes();
        // Sign with a key that does not match the stored secret.
        let signature = sign_body("whsec_definitely_wrong", &body);
        let response = send(
            pool.clone(),
            webhook_request_for(&token, body, &signature),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "bad signature #{i} should be rejected with 404"
        );
    }

    assert_eq!(
        count_recent_invalid_signature_events(&pool, connection_id).await,
        3,
        "all three invalid_signature events must be written"
    );

    // List endpoint must now report status = "error".
    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations",
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let entries = body["data"].as_array().unwrap();
    let entry = entries
        .iter()
        .find(|e| e["slug"] == T056_SLUG)
        .expect("generic-webhook entry present");
    assert_eq!(
        entry["status"], "error",
        "list must report error after 3 consecutive failures (SC-006)"
    );

    // Detail endpoint must agree.
    let response = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/integrations/{T056_SLUG}"),
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(
        body["status"], "error",
        "detail must report error after 3 consecutive failures (SC-006)"
    );
}

#[tokio::test]
async fn accepted_delivery_after_failures_returns_connected_status() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    // Connect.
    let response = send(
        pool.clone(),
        authenticated_json_request_local(
            &format!("/api/v1/tenant/integrations/{T056_SLUG}/connect"),
            Method::POST,
            admin,
            tenant,
            json!({
                "config": { "source_label": "Recovery test" },
                "secrets": { "signing_secret": T056_SECRET },
            }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
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

    // Three bad-signature deliveries.
    for i in 0..3 {
        let body = format!(r#"{{"seq":{i}}}"#).into_bytes();
        let signature = sign_body("whsec_definitely_wrong", &body);
        let response = send(
            pool.clone(),
            webhook_request_for(&token, body, &signature),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    // Sanity check: list now says "error".
    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations",
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    let body = body_json(response).await;
    let entry = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["slug"] == T056_SLUG)
        .expect("generic-webhook entry present");
    assert_eq!(entry["status"], "error");

    // One correctly signed delivery: list/detail flip back to "connected".
    let body = br#"{"event":"recovered"}"#.to_vec();
    let signature = sign_body(T056_SECRET, &body);
    let response = send(
        pool.clone(),
        webhook_request_for(&token, body, &signature),
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations",
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    let body = body_json(response).await;
    let entry = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["slug"] == T056_SLUG)
        .expect("generic-webhook entry present");
    assert_eq!(
        entry["status"], "connected",
        "list must report connected after a successful delivery"
    );

    let response = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/integrations/{T056_SLUG}"),
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    let body = body_json(response).await;
    assert_eq!(
        body["status"], "connected",
        "detail must report connected after a successful delivery"
    );
}
