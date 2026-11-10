// Billing overview integration tests (spec 031).

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use chrono::{Datelike, NaiveDate, Utc};
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
    AppState {
        config: Arc::new(test_config()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool, &test_config()).unwrap(),
        budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
    }
}

#[allow(dead_code)]
fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

#[allow(dead_code)]
async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            eprintln!("skipping billing overview tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping billing overview tests: DATABASE_URL is unreachable");
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

// ── Seed helpers ────────────────────────────────────────────────────────────

async fn seed_tenant(pool: &sqlx::PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(name)
        .bind(format!("bill-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_customer(pool: &sqlx::PgPool, tenant_id: Uuid, display_name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind(display_name)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Billing Test User")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_member(pool: &sqlx::PgPool, tenant_id: Uuid, email: &str, role: &str) -> Uuid {
    let user_id = seed_user(pool, email).await;
    sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, $3, 'active')",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .execute(pool)
    .await
    .unwrap();
    user_id
}

fn authenticated_request(uri: &str, user_id: Uuid, tenant_id: Uuid) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(Method::GET)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .body(Body::empty())
        .unwrap()
}

async fn subscribe(pool: &sqlx::PgPool, tenant_id: Uuid, plan_id: Uuid) {
    sqlx::query("INSERT INTO tenant_subscriptions (tenant_id, plan_id) VALUES ($1, $2)")
        .bind(tenant_id)
        .bind(plan_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn free_plan_id(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("SELECT id FROM billing_plans WHERE name = 'Free'")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn create_custom_plan(
    pool: &sqlx::PgPool,
    name: &str,
    token_allowance: Option<i64>,
    conversation_allowance: Option<i32>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO billing_plans \
         (name, monthly_price_cents, token_allowance, \
          conversation_allowance, is_default, status) \
         VALUES ($1, 0, $2, $3, false, 'active') RETURNING id",
    )
    .bind(name)
    .bind(token_allowance)
    .bind(conversation_allowance)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn start_of_month() -> chrono::DateTime<Utc> {
    let now = Utc::now();
    NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
}

// ── Tests ───────────────────────────────────────────────────────────────────

const URI: &str = "/api/v1/tenant/billing/overview";

#[tokio::test]
#[serial(billing_db)]
async fn billing_overview_200_for_owner() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let tenant = seed_tenant(&pool, "Owner Billing Tenant").await;
    subscribe(&pool, tenant, free_plan_id(&pool).await).await;
    let user = seed_member(&pool, tenant, "owner@bill-t001.test", "owner").await;
    let response = send(pool.clone(), authenticated_request(URI, user, tenant)).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
#[serial(billing_db)]
async fn billing_overview_200_for_admin() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let tenant = seed_tenant(&pool, "Admin Billing Tenant").await;
    subscribe(&pool, tenant, free_plan_id(&pool).await).await;
    let user = seed_member(&pool, tenant, "admin@bill-t002.test", "admin").await;
    let response = send(pool.clone(), authenticated_request(URI, user, tenant)).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
#[serial(billing_db)]
async fn billing_overview_403_for_manager() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let tenant = seed_tenant(&pool, "Manager Billing Tenant").await;
    subscribe(&pool, tenant, free_plan_id(&pool).await).await;
    let user = seed_member(&pool, tenant, "manager@bill-t003.test", "manager").await;
    let response = send(pool.clone(), authenticated_request(URI, user, tenant)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(billing_db)]
async fn billing_overview_403_for_agent() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let tenant = seed_tenant(&pool, "Agent Billing Tenant").await;
    subscribe(&pool, tenant, free_plan_id(&pool).await).await;
    let user = seed_member(&pool, tenant, "agent@bill-t004.test", "agent").await;
    let response = send(pool.clone(), authenticated_request(URI, user, tenant)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(billing_db)]
async fn billing_overview_403_for_viewer() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let tenant = seed_tenant(&pool, "Viewer Billing Tenant").await;
    subscribe(&pool, tenant, free_plan_id(&pool).await).await;
    let user = seed_member(&pool, tenant, "viewer@bill-t005.test", "viewer").await;
    let response = send(pool.clone(), authenticated_request(URI, user, tenant)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(billing_db)]
async fn billing_overview_no_subscription_returns_free_plan() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let tenant = seed_tenant(&pool, "No Sub Billing Tenant").await;
    let user = seed_member(&pool, tenant, "admin@bill-t006.test", "admin").await;
    let response = send(pool.clone(), authenticated_request(URI, user, tenant)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["plan"]["name"], "Free");
    assert_eq!(body["subscription_status"], "active");
}

#[tokio::test]
#[serial(billing_db)]
async fn billing_overview_zero_usage() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let tenant = seed_tenant(&pool, "Zero Usage Billing Tenant").await;
    subscribe(&pool, tenant, free_plan_id(&pool).await).await;
    let user = seed_member(&pool, tenant, "admin@bill-t007.test", "admin").await;
    let response = send(pool.clone(), authenticated_request(URI, user, tenant)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["usage"]["tokens"]["used"], 0);
    assert_eq!(body["usage"]["conversations"]["used"], 0);
}

#[tokio::test]
#[serial(billing_db)]
async fn billing_overview_token_allowance_null() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let tenant = seed_tenant(&pool, "Null Allowance Tenant").await;
    let plan_id = create_custom_plan(
        &pool,
        &format!("Null Tokens {}", Uuid::new_v4().simple()),
        None,
        Some(100),
    )
    .await;
    subscribe(&pool, tenant, plan_id).await;
    let user = seed_member(&pool, tenant, "admin@bill-t008.test", "admin").await;
    let response = send(pool.clone(), authenticated_request(URI, user, tenant)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(
        body["usage"]["tokens"]["allowance"],
        serde_json::Value::Null
    );
    assert_eq!(body["usage"]["tokens"]["percent"], serde_json::Value::Null);
    assert_eq!(body["usage"]["tokens"]["state"], "normal");
}

#[tokio::test]
#[serial(billing_db)]
async fn billing_overview_tenant_isolation() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let tenant_a = seed_tenant(&pool, "Isolation A Tenant").await;
    let tenant_b = seed_tenant(&pool, "Isolation B Tenant").await;
    let plan = free_plan_id(&pool).await;
    subscribe(&pool, tenant_a, plan).await;
    subscribe(&pool, tenant_b, plan).await;

    let customer_a = seed_customer(&pool, tenant_a, "Isolation Customer A").await;
    let customer_b = seed_customer(&pool, tenant_b, "Isolation Customer B").await;

    let period_start = start_of_month();

    // Tenant A: 5 conversations, 150 tokens
    for _ in 0..5 {
        sqlx::query(
            "INSERT INTO conversations \
             (tenant_id, customer_id, channel, status, created_at, \
              last_activity_at, updated_at) \
             VALUES ($1, $2, 'widget', 'closed', $3, $3, $3)",
        )
        .bind(tenant_a)
        .bind(customer_a)
        .bind(period_start)
        .execute(&pool)
        .await
        .unwrap();
    }

    sqlx::query(
        "INSERT INTO ai_usage_records \
         (tenant_id, provider, model, input_tokens, output_tokens, \
          status, streamed, latency_ms, created_at) \
         VALUES ($1, 'openai', 'gpt-4o', 100, 50, \
          'success', false, 10, $2)",
    )
    .bind(tenant_a)
    .bind(period_start)
    .execute(&pool)
    .await
    .unwrap();

    // Tenant B: 2 conversations, 30 tokens
    for _ in 0..2 {
        sqlx::query(
            "INSERT INTO conversations \
             (tenant_id, customer_id, channel, status, created_at, \
              last_activity_at, updated_at) \
             VALUES ($1, $2, 'widget', 'closed', $3, $3, $3)",
        )
        .bind(tenant_b)
        .bind(customer_b)
        .bind(period_start)
        .execute(&pool)
        .await
        .unwrap();
    }

    sqlx::query(
        "INSERT INTO ai_usage_records \
         (tenant_id, provider, model, input_tokens, output_tokens, \
          status, streamed, latency_ms, created_at) \
         VALUES ($1, 'openai', 'gpt-4o', 20, 10, \
          'success', false, 10, $2)",
    )
    .bind(tenant_b)
    .bind(period_start)
    .execute(&pool)
    .await
    .unwrap();

    let user_a = seed_member(&pool, tenant_a, "admin@bill-t009-a.test", "admin").await;
    let user_b = seed_member(&pool, tenant_b, "admin@bill-t009-b.test", "admin").await;

    let response_a = send(pool.clone(), authenticated_request(URI, user_a, tenant_a)).await;
    assert_eq!(response_a.status(), StatusCode::OK);
    let body_a = body_json(response_a).await;
    assert_eq!(body_a["usage"]["conversations"]["used"], 5);
    assert_eq!(body_a["usage"]["tokens"]["used"], 150);

    let response_b = send(pool.clone(), authenticated_request(URI, user_b, tenant_b)).await;
    assert_eq!(response_b.status(), StatusCode::OK);
    let body_b = body_json(response_b).await;
    assert_eq!(body_b["usage"]["conversations"]["used"], 2);
    assert_eq!(body_b["usage"]["tokens"]["used"], 30);
}
