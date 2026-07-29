use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use config::Environment;
use http_body_util::BodyExt;
use serial_test::serial;
use server::router;
use server::state::AppState;
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
        rate_limit_enabled: true,
        rate_limit_login_max: 10,
        rate_limit_login_window_secs: 60,
        rate_limit_max_override: 10000,
        rate_limit_fail_mode: "auth_closed".into(),
        security_headers_enabled: true,
        csp_policy:
            "default-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'".into(),
        csp_report_only: true,
        hsts_max_age: 31536000,
        otlp_endpoint: None,
        trace_sample_ratio: 1.0,
        sentry_dsn: None,
        release_sha: None,
        trusted_proxy: false,
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
        budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
    }
}

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("skipping platform health tests: DATABASE_URL not set");
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping platform health tests: DATABASE_URL is unreachable");
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

async fn body_json(response: Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn session_cookie(user_id: Uuid) -> String {
    let config = test_config();
    let (token, _, _) = identity::session::issue_token(
        &config.auth_jwt_secret,
        config.auth_session_ttl_seconds,
        user_id,
    )
    .unwrap();
    format!("app_session={token}")
}

fn authenticated_request(uri: &str, method: Method, user_id: Uuid) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(method)
        .header("cookie", session_cookie(user_id))
        .body(Body::empty())
        .unwrap()
}

fn tenant_request(uri: &str, method: Method, user_id: Uuid, tenant_id: Uuid) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(method)
        .header("cookie", session_cookie(user_id))
        .header("X-Tenant-ID", tenant_id.to_string())
        .body(Body::empty())
        .unwrap()
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Health Test Tenant")
        .bind(format!("ht-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, platform_role: Option<&str>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name, platform_role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(format!("ph_{}@example.com", Uuid::new_v4()))
    .bind("Platform Health User")
    .bind(platform_role)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_membership(pool: &sqlx::PgPool, tenant_id: Uuid, user_id: Uuid) {
    sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, 'admin')",
    )
    .bind(tenant_id)
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
}

async fn seed_outbox_event(pool: &sqlx::PgPool, created_at_minutes_ago: i64, attempts: i32) {
    let created = chrono::Utc::now() - chrono::Duration::minutes(created_at_minutes_ago);
    sqlx::query("INSERT INTO outbox_events (id, event_type, aggregate_type, aggregate_id, payload, created_at, attempts) VALUES ($1, 'test.event', 'test', '00000000-0000-0000-0000-000000000000', '{}', $2, $3)")
        .bind(Uuid::new_v4())
        .bind(created)
        .bind(attempts)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
#[serial(platform_health)]
async fn platform_health_empty_queue_reachable_deps() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;

    let user_id = seed_user(&pool, Some("super_admin")).await;
    let response = send(
        pool,
        authenticated_request("/api/v1/platform/health", Method::GET, user_id),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    assert!(body["dependencies"].as_array().unwrap().len() >= 2);
    assert!(body["read_at"].is_string());
}

#[tokio::test]
#[serial(platform_health)]
async fn platform_health_queue_degraded_at_150_pending_12m_old() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;

    for _ in 0..150 {
        seed_outbox_event(&pool, 12, 0).await;
    }

    let user_id = seed_user(&pool, Some("super_admin")).await;
    let response = send(
        pool,
        authenticated_request("/api/v1/platform/health", Method::GET, user_id),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    let deps = body["dependencies"].as_array().unwrap();
    let queue = deps.iter().find(|d| d["name"] == "queue").unwrap();
    assert_eq!(queue["verdict"], "degraded");
    assert_eq!(queue["detail"]["pending_count"], 150);
}

#[tokio::test]
#[serial(platform_health)]
async fn platform_health_queue_unhealthy_at_1000_pending() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;

    for _ in 0..1000 {
        seed_outbox_event(&pool, 5, 0).await;
    }

    let user_id = seed_user(&pool, Some("super_admin")).await;
    let response = send(
        pool,
        authenticated_request("/api/v1/platform/health", Method::GET, user_id),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    let deps = body["dependencies"].as_array().unwrap();
    let queue = deps.iter().find(|d| d["name"] == "queue").unwrap();
    assert_eq!(queue["verdict"], "unhealthy");
}

#[tokio::test]
#[serial(platform_health)]
async fn platform_health_queue_degraded_with_stuck_retries() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;

    seed_outbox_event(&pool, 1, 6).await;

    let user_id = seed_user(&pool, Some("super_admin")).await;
    let response = send(
        pool,
        authenticated_request("/api/v1/platform/health", Method::GET, user_id),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    let deps = body["dependencies"].as_array().unwrap();
    let queue = deps.iter().find(|d| d["name"] == "queue").unwrap();
    assert_eq!(queue["verdict"], "degraded");
}

#[tokio::test]
#[serial(platform_health)]
async fn platform_health_analytics_view_user_gets_403() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let user_id = seed_user(&pool, Some("sales")).await;
    let response = send(
        pool,
        authenticated_request("/api/v1/platform/health", Method::GET, user_id),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(platform_health)]
async fn platform_health_tenant_user_gets_403() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, None).await;
    seed_membership(&pool, tenant_id, user_id).await;
    let response = send(
        pool,
        tenant_request("/api/v1/platform/health", Method::GET, user_id, tenant_id),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(platform_health)]
async fn platform_health_response_has_no_sensitive_content() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;

    let user_id = seed_user(&pool, Some("super_admin")).await;
    let response = send(
        pool,
        authenticated_request("/api/v1/platform/health", Method::GET, user_id),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    let body_str = format!("{}", body);
    assert!(!body_str.contains("request_content"));
    assert!(!body_str.contains("response_content"));
}
