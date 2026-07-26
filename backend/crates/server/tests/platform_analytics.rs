use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use config::Environment;
use http_body_util::BodyExt;
use server::router;
use server::state::AppState;
use serial_test::serial;
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
            eprintln!("skipping platform analytics tests: DATABASE_URL not set");
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping platform analytics tests: DATABASE_URL is unreachable");
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query("DELETE FROM escalations")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "TRUNCATE TABLE conversation_feedback, ai_generations, \
         ai_usage_records, agent_configurations, messages, \
         customer_channel_identifiers, customers, conversations, \
         widget_sessions, widget_instances, outbox_events, audit_logs, \
         tenant_invitations, tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .unwrap();
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

async fn seed_tenant(pool: &sqlx::PgPool, status: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug, status) VALUES ($1, $2, $3) RETURNING id")
        .bind("Test Tenant")
        .bind(format!("tt-{}", Uuid::new_v4().simple()))
        .bind(status)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_soft_deleted_tenant(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO tenants (name, slug, status, deleted_at) VALUES ($1, $2, 'active', now()) RETURNING id",
    )
    .bind("Deleted Tenant")
    .bind(format!("dt-{}", Uuid::new_v4().simple()))
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, platform_role: Option<&str>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name, platform_role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(format!("pa_{}@example.com", Uuid::new_v4()))
    .bind("Platform Analytics User")
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

async fn seed_customer(pool: &sqlx::PgPool, tenant_id: Uuid) -> Uuid {
    sqlx::query_scalar("INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id")
        .bind(tenant_id)
        .bind("Platform Analytics Customer")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_conversation(pool: &sqlx::PgPool, tenant_id: Uuid, customer_id: Uuid, status: &str) {
    sqlx::query(
        "INSERT INTO conversations (tenant_id, customer_id, status, channel) VALUES ($1, $2, $3, 'web_chat')",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_ai_usage(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    status: &str,
    tokens_in: i64,
    tokens_out: i64,
    created_at: &str,
) {
    sqlx::query(
        "INSERT INTO ai_usage_records (tenant_id, provider, model, status, streamed, input_tokens, output_tokens, latency_ms, created_at) VALUES ($1, 'openai', 'gpt-4o', $2, false, $3, $4, 100, $5::timestamptz)",
    )
    .bind(tenant_id)
    .bind(status)
    .bind(tokens_in)
    .bind(tokens_out)
    .bind(created_at)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
#[serial]
async fn platform_analytics_summary_includes_all_tenants_excluding_soft_deleted() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;

    seed_tenant(&pool, "active").await;
    seed_tenant(&pool, "active").await;
    seed_tenant(&pool, "suspended").await;
    seed_soft_deleted_tenant(&pool).await;

    let user_id = seed_user(&pool, Some("super_admin")).await;
    let response = send(
        pool,
        authenticated_request("/api/v1/platform/analytics/summary", Method::GET, user_id),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    assert_eq!(body["tenants"]["total"], 3);
    assert_eq!(body["tenants"]["active"], 2);
    assert_eq!(body["tenants"]["suspended"], 1);
}

#[tokio::test]
#[serial]
async fn platform_analytics_active_conversations_counts_open_and_pending() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;

    let t1 = seed_tenant(&pool, "active").await;
    let t2 = seed_tenant(&pool, "active").await;
    let c1 = seed_customer(&pool, t1).await;
    let c2 = seed_customer(&pool, t2).await;
    seed_conversation(&pool, t1, c1, "open").await;
    seed_conversation(&pool, t1, c1, "open").await;
    seed_conversation(&pool, t2, c2, "pending").await;
    seed_conversation(&pool, t2, c2, "resolved").await;

    let user_id = seed_user(&pool, Some("super_admin")).await;
    let response = send(
        pool,
        authenticated_request("/api/v1/platform/analytics/summary", Method::GET, user_id),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    assert_eq!(body["active_conversations"], 3);
}

#[tokio::test]
#[serial]
async fn platform_analytics_ai_usage_totals_match() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;

    let t1 = seed_tenant(&pool, "active").await;
    let t2 = seed_tenant(&pool, "active").await;
    seed_ai_usage(&pool, t1, "success", 100, 10, "2026-07-20T00:00:00Z").await;
    seed_ai_usage(&pool, t1, "failure", 50, 5, "2026-07-21T00:00:00Z").await;
    seed_ai_usage(&pool, t2, "success", 200, 20, "2026-07-22T00:00:00Z").await;

    let user_id = seed_user(&pool, Some("super_admin")).await;
    let response = send(
        pool,
        authenticated_request(
            "/api/v1/platform/analytics/summary?from=2026-07-19&to=2026-07-23",
            Method::GET,
            user_id,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    assert_eq!(body["ai_usage"]["total_calls"], 3);
    assert_eq!(body["ai_usage"]["successful_calls"], 2);
    assert_eq!(body["ai_usage"]["failed_calls"], 1);
    assert_eq!(body["ai_usage"]["input_tokens"], 350);
    assert_eq!(body["ai_usage"]["output_tokens"], 35);
}

#[tokio::test]
#[serial]
async fn platform_analytics_invalid_date_range_returns_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let user_id = seed_user(&pool, Some("super_admin")).await;

    // from > to
    let resp = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/platform/analytics/summary?from=2026-07-25&to=2026-06-25",
            Method::GET,
            user_id,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // span > 365 days
    let resp = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/platform/analytics/summary?from=2025-01-01&to=2026-07-25",
            Method::GET,
            user_id,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
#[serial]
async fn platform_analytics_defaults_to_last_30_days() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let user_id = seed_user(&pool, Some("super_admin")).await;

    let response = send(
        pool,
        authenticated_request("/api/v1/platform/analytics/summary", Method::GET, user_id),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
#[serial]
async fn platform_analytics_tenant_role_user_gets_403() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "active").await;
    let user_id = seed_user(&pool, None).await;
    seed_membership(&pool, tenant_id, user_id).await;

    let response = send(
        pool,
        tenant_request(
            "/api/v1/platform/analytics/summary",
            Method::GET,
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
