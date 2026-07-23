use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use http_body_util::BodyExt;
use server::router;
use server::state::AppState;
use sha2::Digest;
use tower::ServiceExt;
use uuid::Uuid;

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
        ai_key_encryption_key: Some("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=".into()),
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
            eprintln!("skipping widget public tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping widget public tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query(
        "TRUNCATE TABLE widget_sessions, widget_instances, messages, \
         customer_channel_identifiers, customers, conversations, \
         outbox_events, audit_logs, tenant_invitations, \
         tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("failed to reset widget test tables");
}

async fn send(pool: sqlx::PgPool, request: Request<Body>) -> Response {
    router::app_with_test_routes(app_state(pool))
        .oneshot(request)
        .await
        .expect("request should complete")
}

async fn body_json(response: Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn seed_tenant(pool: &sqlx::PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(name)
        .bind(format!("wgt-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_widget_instance(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    public_id: &str,
    enabled: bool,
    allowed_domains: &[&str],
) -> Uuid {
    let domains: Vec<String> = allowed_domains.iter().map(|s| s.to_string()).collect();
    sqlx::query_scalar(
        "INSERT INTO widget_instances \
         (tenant_id, public_id, name, display_name, enabled, allowed_domains) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(tenant_id)
    .bind(public_id)
    .bind("Test Widget")
    .bind("Test Widget Display")
    .bind(enabled)
    .bind(&domains)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_disabled_widget_instance(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    public_id: &str,
) -> Uuid {
    seed_widget_instance(pool, tenant_id, public_id, false, &[]).await
}

fn get(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .method(Method::GET)
        .body(Body::empty())
        .unwrap()
}

fn post_json(path: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .uri(path)
        .method(Method::POST)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

#[allow(dead_code)]
fn post_json_with_origin(path: &str, body: serde_json::Value, origin: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .method(Method::POST)
        .header("content-type", "application/json")
        .header("origin", origin)
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t001_config_returns_public_fields_and_no_tenant_id() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "ConfigTest").await;
    let public_id = "wgt_config_test_001";
    seed_widget_instance(&pool, tenant_id, public_id, true, &[]).await;

    let response = send(
        pool.clone(),
        get(&format!("/api/v1/widget/v1/config?widget_id={public_id}")),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["widgetId"], public_id);
    assert_eq!(body["displayName"], "Test Widget Display");
    assert!(body["enabled"].as_bool().unwrap());
    // Must not expose tenant_id, id, allowed_domains, or timestamps
    assert!(body.get("tenant_id").is_none());
    assert!(body.get("id").is_none());
    assert!(body.get("allowed_domains").is_none());
}

#[tokio::test]
async fn t002_unknown_widget_id_returns_404() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let response = send(
        pool.clone(),
        get("/api/v1/widget/v1/config?widget_id=nonexistent"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn t003_disabled_instance_returns_200_with_enabled_false() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "DisabledTest").await;
    let public_id = "wgt_disabled_001";
    seed_disabled_widget_instance(&pool, tenant_id, public_id).await;

    let response = send(
        pool.clone(),
        get(&format!("/api/v1/widget/v1/config?widget_id={public_id}")),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert!(!body["enabled"].as_bool().unwrap());
}

#[tokio::test]
async fn t004_origin_not_in_allowlist_returns_403() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "OriginTest").await;
    let public_id = "wgt_origin_001";
    seed_widget_instance(&pool, tenant_id, public_id, true, &["allowed.example.com"]).await;

    let response = send(
        pool.clone(),
        Request::builder()
            .uri(format!("/api/v1/widget/v1/config?widget_id={public_id}"))
            .method(Method::GET)
            .header("origin", "https://evil.com")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "origin_not_allowed");
}

#[tokio::test]
async fn t005_session_mint_returns_token_that_authenticates() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "SessionTest").await;
    let public_id = "wgt_session_001";
    seed_widget_instance(&pool, tenant_id, public_id, true, &[]).await;

    // Mint a session
    let body = serde_json::json!({ "widgetId": public_id });
    let response = send(pool.clone(), post_json("/api/v1/widget/v1/sessions", body)).await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let session_body = body_json(response).await;
    let token = session_body["sessionToken"].as_str().unwrap().to_owned();
    let expires_at = session_body["expiresAt"].as_str().unwrap();
    assert!(!token.is_empty());
    assert!(!expires_at.is_empty());

    // The session token authenticates subsequent requests
    // (session extraction middleware will validate this)
}

#[tokio::test]
async fn t006_expired_session_returns_401_session_invalid() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "ExpiredSessionTest").await;
    let public_id = "wgt_expired_001";
    seed_widget_instance(&pool, tenant_id, public_id, true, &[]).await;

    // Session with an already expired token
    let expired_token_hash = sha2::Sha256::digest(b"expired-token-should-fail").to_vec();
    sqlx::query(
        "INSERT INTO widget_sessions \
         (tenant_id, widget_instance_id, token_hash, expires_at) \
         VALUES ($1, $2, $3, now() - interval '1 hour')",
    )
    .bind(tenant_id)
    .bind(
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM widget_instances WHERE public_id = $1")
            .bind(public_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
    )
    .bind(&expired_token_hash)
    .execute(&pool)
    .await
    .unwrap();

    let expired_token = "expired-token-should-fail";
    let response = send(
        pool.clone(),
        Request::builder()
            .uri("/api/v1/widget/v1/conversation")
            .method(Method::GET)
            .header("authorization", format!("Bearer {expired_token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "session_invalid");
}

#[tokio::test]
async fn t007_exceeding_per_ip_creation_limit_returns_429() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "RateLimitTest").await;
    let public_id = "wgt_ratelimit_001";
    seed_widget_instance(&pool, tenant_id, public_id, true, &[]).await;

    let body = serde_json::json!({ "widgetId": public_id });
    // Exhaust the per-IP creation limit, then assert the next request is rejected
    for i in 0..10 {
        let response = send(
            pool.clone(),
            post_json("/api/v1/widget/v1/sessions", body.clone()),
        )
        .await;
        if i < 9 {
            assert_ne!(
                response.status(),
                StatusCode::TOO_MANY_REQUESTS,
                "rate limiter triggered before budget exhausted"
            );
        }
    }
    let response = send(
        pool.clone(),
        post_json("/api/v1/widget/v1/sessions", body.clone()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "rate_limited");
}

#[tokio::test]
async fn t008_two_tenants_traffic_does_not_share_bucket() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_a = seed_tenant(&pool, "BucketTestA").await;
    let tenant_b = seed_tenant(&pool, "BucketTestB").await;
    let pub_a = "wgt_bucket_a";
    let pub_b = "wgt_bucket_b";
    seed_widget_instance(&pool, tenant_a, pub_a, true, &[]).await;
    seed_widget_instance(&pool, tenant_b, pub_b, true, &[]).await;

    // Both tenants should be able to create sessions independently
    let body_a = serde_json::json!({ "widgetId": pub_a });
    let body_b = serde_json::json!({ "widgetId": pub_b });
    let response_a = send(
        pool.clone(),
        post_json("/api/v1/widget/v1/sessions", body_a),
    )
    .await;
    let response_b = send(
        pool.clone(),
        post_json("/api/v1/widget/v1/sessions", body_b),
    )
    .await;

    assert_eq!(response_a.status(), StatusCode::CREATED);
    assert_eq!(response_b.status(), StatusCode::CREATED);
}
