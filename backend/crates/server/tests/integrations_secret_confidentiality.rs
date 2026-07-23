use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use config::Environment;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use server::router;
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_INTEGRATION_SECRETS_KEY: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=";

// Plaintext under test. Must never appear in any response body, in any
// audit_logs.details row, or in the ciphertext column.
const SECRET_PLAINTEXT: &str = "whsec_supersecret123";
const SECRET_HINT: &str = "t123"; // last 4 chars

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
            eprintln!("skipping integrations_secret_confidentiality live tests: DATABASE_URL not set");
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping integrations_secret_confidentiality live tests: DATABASE_URL is unreachable");
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

async fn seed_user(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name, platform_role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(format!("iconf_{}@example.com", Uuid::new_v4()))
    .bind("Integrations Confidential User")
    .bind(None::<String>)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Integrations Confidential Tenant")
        .bind(format!("iconf-{}", Uuid::new_v4().simple()))
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

fn assert_no_plaintext_in(value: &Value, surface: &str) {
    let serialized = value.to_string();
    assert!(
        !serialized.contains(SECRET_PLAINTEXT),
        "{surface} must not contain plaintext secret: {serialized}"
    );
}

// ---------------------------------------------------------------------------
// T038 — SC-002: secrets never appear in any read surface; ciphertext is
// encrypted at rest; only a masked hint is returned.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn secret_never_appears_in_connect_response() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    let response = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/connect",
            Method::POST,
            admin,
            tenant,
            json!({
                "config": { "source_label": "Billing" },
                "secrets": { "signing_secret": SECRET_PLAINTEXT },
            }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = body_json(response).await;
    assert_no_plaintext_in(&body, "connect response");
    // The detail DTO always carries the plaintext webhook token, but never
    // the signing secret. Confirm the hint is correct.
    let secrets = body["connection"]["secrets"].as_array().unwrap();
    assert_eq!(secrets.len(), 1);
    assert_eq!(secrets[0]["hint"], SECRET_HINT);
}

#[tokio::test]
async fn secret_never_appears_in_detail_response() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    // Connect first.
    let connect = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/connect",
            Method::POST,
            admin,
            tenant,
            json!({
                "config": { "source_label": "Billing" },
                "secrets": { "signing_secret": SECRET_PLAINTEXT },
            }),
        ),
    )
    .await;
    assert_eq!(connect.status(), StatusCode::CREATED);

    // Detail must also exclude the plaintext.
    let detail = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations/generic-webhook",
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(detail.status(), StatusCode::OK);
    let body = body_json(detail).await;
    assert_no_plaintext_in(&body, "detail response");
    // Sanity: the hint survives.
    let secrets = body["connection"]["secrets"].as_array().unwrap();
    assert_eq!(secrets.len(), 1);
    assert_eq!(secrets[0]["hint"], SECRET_HINT);
}

#[tokio::test]
async fn secret_never_appears_in_list_response() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    // Connect first.
    let connect = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/connect",
            Method::POST,
            admin,
            tenant,
            json!({
                "config": { "source_label": "Billing" },
                "secrets": { "signing_secret": SECRET_PLAINTEXT },
            }),
        ),
    )
    .await;
    assert_eq!(connect.status(), StatusCode::CREATED);

    // List must not include the secret either.
    let list = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations",
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(list.status(), StatusCode::OK);
    let body = body_json(list).await;
    assert_no_plaintext_in(&body, "list response");
}

#[tokio::test]
async fn secret_never_appears_in_events_response() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    let connect = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/connect",
            Method::POST,
            admin,
            tenant,
            json!({
                "config": { "source_label": "Billing" },
                "secrets": { "signing_secret": SECRET_PLAINTEXT },
            }),
        ),
    )
    .await;
    assert_eq!(connect.status(), StatusCode::CREATED);

    // Events endpoint must also exclude the secret.
    let events = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations/generic-webhook/events",
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(events.status(), StatusCode::OK);
    let body = body_json(events).await;
    assert_no_plaintext_in(&body, "events response");
}

#[tokio::test]
async fn secret_never_appears_in_audit_logs_details() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    // Connect and then update and disconnect so we exercise every audit
    // action. Each must write details that contain only slug + connectionId.
    let connect = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/connect",
            Method::POST,
            admin,
            tenant,
            json!({
                "config": { "source_label": "Billing" },
                "secrets": { "signing_secret": SECRET_PLAINTEXT },
            }),
        ),
    )
    .await;
    assert_eq!(connect.status(), StatusCode::CREATED);

    let _ = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/config",
            Method::PUT,
            admin,
            tenant,
            json!({
                "config": { "source_label": "Billing 2" },
                "secrets": { "signing_secret": "whsec_rotated_1111" },
            }),
        ),
    )
    .await;

    let _ = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations/generic-webhook/disconnect",
            Method::POST,
            admin,
            tenant,
        ),
    )
    .await;

    // Pull every audit row for this tenant and confirm the plaintext is not
    // anywhere in the details JSON.
    let rows: Vec<(String, Value)> = sqlx::query_as(
        "SELECT action, details FROM audit_logs \
         WHERE tenant_id = $1 AND action LIKE 'integration.%'",
    )
    .bind(tenant)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(!rows.is_empty(), "expected audit rows to be written");
    for (action, details) in &rows {
        let details_str = details.to_string();
        assert!(
            !details_str.contains(SECRET_PLAINTEXT),
            "audit row action={action} must not contain plaintext secret: {details_str}"
        );
        // Confirm only the allowed keys appear in details. (Defence in
        // depth: even if some future change leaks an extra harmless field,
        // a secret would not look like a uuid string.)
        let obj = details.as_object().expect("details must be a JSON object");
        let allowed: std::collections::HashSet<&str> =
            ["connectionId", "slug"].iter().copied().collect();
        for key in obj.keys() {
            assert!(
                allowed.contains(key.as_str()),
                "audit row action={action} has unexpected key in details: {key}"
            );
        }
    }
}

#[tokio::test]
async fn secret_ciphertext_does_not_contain_plaintext_bytes() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    let connect = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/connect",
            Method::POST,
            admin,
            tenant,
            json!({
                "config": { "source_label": "Billing" },
                "secrets": { "signing_secret": SECRET_PLAINTEXT },
            }),
        ),
    )
    .await;
    assert_eq!(connect.status(), StatusCode::CREATED);

    // Pull the stored ciphertext for the signing_secret and verify it does
    // not contain the plaintext byte sequence.
    let ciphertext: Vec<u8> = sqlx::query_scalar(
        "SELECT ciphertext FROM integration_secrets \
         WHERE tenant_id = $1 AND field_key = 'signing_secret'",
    )
    .bind(tenant)
    .fetch_one(&pool)
    .await
    .unwrap();
    let plaintext_bytes = SECRET_PLAINTEXT.as_bytes();
    assert!(
        !ciphertext.windows(plaintext_bytes.len()).any(|w| w == plaintext_bytes),
        "integration_secrets.ciphertext must not contain the plaintext bytes"
    );
    // Also confirm the hint stored alongside the ciphertext equals the
    // last four characters of the plaintext (FR-005: the only readable
    // remnant).
    let stored_hint: String = sqlx::query_scalar(
        "SELECT hint FROM integration_secrets \
         WHERE tenant_id = $1 AND field_key = 'signing_secret'",
    )
    .bind(tenant)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored_hint, SECRET_HINT);
}
