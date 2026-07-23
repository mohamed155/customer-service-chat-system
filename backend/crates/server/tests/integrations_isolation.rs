use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use config::Environment;
use http_body_util::BodyExt;
use rand::RngCore;
use serde_json::Value;
use server::router;
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;

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
        integration_secrets_key: None,
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
            eprintln!("skipping integrations_isolation live tests: DATABASE_URL not set");
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping integrations_isolation live tests: DATABASE_URL is unreachable");
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
    .bind(format!("iiso_{}@example.com", Uuid::new_v4()))
    .bind("Integrations Isolation User")
    .bind(None::<String>)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Integrations Isolation Tenant")
        .bind(format!("iiso-{}", Uuid::new_v4().simple()))
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

async fn seed_connection_with_secrets(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    catalog_slug: &str,
    config: &str,
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

    let connection_id: Uuid = sqlx::query_scalar(
        "INSERT INTO integration_connections \
         (tenant_id, catalog_id, is_active, config, \
          webhook_token_hash, webhook_token_ciphertext, webhook_token_nonce) \
         VALUES ($1, $2, true, $3::jsonb, $4, $5, $6) RETURNING id",
    )
    .bind(tenant_id)
    .bind(catalog_id)
    .bind(config)
    .bind(token_hash.to_vec())
    .bind(token_ciphertext)
    .bind(token_nonce.to_vec())
    .fetch_one(pool)
    .await
    .unwrap();

    // Insert an encrypted-secret row so the detail response would carry
    // the field if isolation were broken.
    let mut secret_ciphertext = vec![0u8; 48];
    let mut secret_nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut secret_ciphertext);
    rand::rngs::OsRng.fill_bytes(&mut secret_nonce);
    sqlx::query(
        "INSERT INTO integration_secrets \
         (tenant_id, connection_id, field_key, ciphertext, nonce, hint) \
         VALUES ($1, $2, 'signing_secret', $3, $4, '****')",
    )
    .bind(tenant_id)
    .bind(connection_id)
    .bind(secret_ciphertext)
    .bind(secret_nonce.to_vec())
    .execute(pool)
    .await
    .unwrap();

    connection_id
}

#[tokio::test]
async fn tenant_b_does_not_see_tenant_a_connection_in_list() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;

    let admin_a = seed_user(&pool).await;
    seed_membership(&pool, tenant_a, admin_a, "admin").await;
    let admin_b = seed_user(&pool).await;
    seed_membership(&pool, tenant_b, admin_b, "admin").await;

    seed_connection_with_secrets(
        &pool,
        tenant_a,
        "generic-webhook",
        r#"{"source_label":"A only"}"#,
    )
    .await;

    // Tenant A's list reflects its own connection.
    let response_a = send(
        pool.clone(),
        authenticated_request("/api/v1/tenant/integrations", Method::GET, admin_a, tenant_a),
    )
    .await;
    assert_eq!(response_a.status(), StatusCode::OK);
    let json_a = body_json(response_a).await;
    let entry_a = json_a["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["slug"] == "generic-webhook")
        .expect("generic-webhook present in A's list");
    assert_eq!(entry_a["status"], "connected");

    // Tenant B's list must not see tenant A's connection.
    let response_b = send(
        pool.clone(),
        authenticated_request("/api/v1/tenant/integrations", Method::GET, admin_b, tenant_b),
    )
    .await;
    assert_eq!(response_b.status(), StatusCode::OK);
    let json_b = body_json(response_b).await;
    let entry_b = json_b["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["slug"] == "generic-webhook")
        .expect("generic-webhook present in B's list");
    assert_eq!(entry_b["status"], "not_connected");
}

#[tokio::test]
async fn tenant_b_detail_returns_null_connection() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;

    let admin_a = seed_user(&pool).await;
    seed_membership(&pool, tenant_a, admin_a, "admin").await;
    let admin_b = seed_user(&pool).await;
    seed_membership(&pool, tenant_b, admin_b, "admin").await;

    seed_connection_with_secrets(
        &pool,
        tenant_a,
        "generic-webhook",
        r#"{"source_label":"A only"}"#,
    )
    .await;

    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations/generic-webhook",
            Method::GET,
            admin_b,
            tenant_b,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["slug"], "generic-webhook");
    assert_eq!(json["status"], "not_connected");
    assert!(
        json["connection"].is_null(),
        "tenant B must see connection: null when A is the only connected tenant"
    );
}

#[tokio::test]
async fn tenant_b_detail_does_not_expose_tenant_a_secrets_or_config() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;

    let admin_a = seed_user(&pool).await;
    seed_membership(&pool, tenant_a, admin_a, "admin").await;
    let admin_b = seed_user(&pool).await;
    seed_membership(&pool, tenant_b, admin_b, "admin").await;

    // Tenant A has a connection with config and an encrypted secret.
    seed_connection_with_secrets(
        &pool,
        tenant_a,
        "generic-webhook",
        r#"{"source_label":"A-secret-label"}"#,
    )
    .await;

    // Sanity check: tenant A sees its own connection.
    let a_detail = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations/generic-webhook",
            Method::GET,
            admin_a,
            tenant_a,
        ),
    )
    .await;
    assert_eq!(a_detail.status(), StatusCode::OK);
    let a_json = body_json(a_detail).await;
    assert_eq!(a_json["status"], "connected");
    assert!(a_json["connection"].is_object());
    assert_eq!(
        a_json["connection"]["config"]["source_label"], "A-secret-label"
    );
    assert_eq!(a_json["connection"]["secrets"].as_array().unwrap().len(), 1);

    // Tenant B's detail must contain no trace of A's connection.
    let b_detail = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations/generic-webhook",
            Method::GET,
            admin_b,
            tenant_b,
        ),
    )
    .await;
    assert_eq!(b_detail.status(), StatusCode::OK);
    let b_json = body_json(b_detail).await;
    assert_eq!(b_json["status"], "not_connected");
    assert!(b_json["connection"].is_null());
    let serialized = b_json.to_string();
    assert!(
        !serialized.contains("A-secret-label"),
        "tenant B's response must not contain tenant A's config values: {serialized}"
    );
    assert!(
        !serialized.contains("signing_secret"),
        "tenant B's response must not expose secret field keys from tenant A: {serialized}"
    );
}
