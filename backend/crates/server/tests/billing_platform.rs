// Platform billing plan + subscription integration tests (spec 031 / T045).

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use http_body_util::BodyExt;
use serde_json::{json, Value};
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
            eprintln!("skipping billing platform tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping billing platform tests: DATABASE_URL is unreachable");
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

fn platform_request(
    uri: &str,
    method: Method,
    user_id: Uuid,
    body: Option<Value>,
) -> Request<Body> {
    let builder = Request::builder()
        .uri(uri)
        .method(method)
        .header("X-Dev-User-Id", user_id.to_string());
    if let Some(body) = body {
        builder
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    }
}

async fn seed_user(pool: &sqlx::PgPool, platform_role: Option<&str>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name, platform_role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(format!(
        "billing_platform_{}@test.example",
        Uuid::new_v4().simple()
    ))
    .bind("Billing Platform Test User")
    .bind(platform_role)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_tenant(pool: &sqlx::PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(name)
        .bind(format!("bp-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn free_plan_id(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("SELECT id FROM billing_plans WHERE is_default = true")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn create_plan_via_api(pool: &sqlx::PgPool, user_id: Uuid, name: &str) -> Uuid {
    let unique_name = format!("{} {}", name, Uuid::new_v4().simple());
    let body = json!({
        "name": unique_name,
        "monthly_price_cents": 0,
        "token_allowance": null,
        "conversation_allowance": null,
    });
    let response = send(
        pool.clone(),
        platform_request(
            "/api/v1/platform/billing/plans",
            Method::POST,
            user_id,
            Some(body),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "create_plan_via_api({name}) failed"
    );
    body_json(response).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

const LIST_PLANS: &str = "/api/v1/platform/billing/plans";
const CREATE_PLAN: &str = "/api/v1/platform/billing/plans";
fn update_plan_uri(plan_id: Uuid) -> String {
    format!("/api/v1/platform/billing/plans/{plan_id}")
}
const LIST_SUBS: &str = "/api/v1/platform/billing/subscriptions";
fn put_sub_uri(tenant_id: Uuid) -> String {
    format!("/api/v1/platform/billing/tenants/{tenant_id}/subscription")
}

// ── RBAC: Super Admin and Finance → 200 on all five endpoints ─────────

#[tokio::test]
#[serial(billing_platform)]
async fn super_admin_can_list_plans() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("super_admin")).await;
    let response = send(pool, platform_request(LIST_PLANS, Method::GET, user, None)).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
#[serial(billing_platform)]
async fn finance_can_list_plans() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("finance")).await;
    let response = send(pool, platform_request(LIST_PLANS, Method::GET, user, None)).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
#[serial(billing_platform)]
async fn super_admin_can_create_plan() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("super_admin")).await;
    let body = json!({"name": format!("SA Plan {}", Uuid::new_v4()), "monthly_price_cents": 1000});
    let response = send(
        pool,
        platform_request(CREATE_PLAN, Method::POST, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
#[serial(billing_platform)]
async fn finance_can_create_plan() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("finance")).await;
    let body =
        json!({"name": format!("Finance Plan {}", Uuid::new_v4()), "monthly_price_cents": 500});
    let response = send(
        pool,
        platform_request(CREATE_PLAN, Method::POST, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
#[serial(billing_platform)]
async fn super_admin_can_update_plan() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("super_admin")).await;
    let plan_id = create_plan_via_api(&pool, user, "Updatable SA Plan").await;
    let body = json!({"status": "retired"});
    let response = send(
        pool,
        platform_request(&update_plan_uri(plan_id), Method::PATCH, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
#[serial(billing_platform)]
async fn finance_can_update_plan() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("finance")).await;
    let plan_id = create_plan_via_api(&pool, user, "Updatable Finance Plan").await;
    let body = json!({"status": "retired"});
    let response = send(
        pool,
        platform_request(&update_plan_uri(plan_id), Method::PATCH, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
#[serial(billing_platform)]
async fn super_admin_can_list_subscriptions() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("super_admin")).await;
    let response = send(pool, platform_request(LIST_SUBS, Method::GET, user, None)).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
#[serial(billing_platform)]
async fn finance_can_list_subscriptions() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("finance")).await;
    let response = send(pool, platform_request(LIST_SUBS, Method::GET, user, None)).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
#[serial(billing_platform)]
async fn super_admin_can_put_subscription() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("super_admin")).await;
    let tenant = seed_tenant(&pool, "SA Sub Tenant").await;
    let plan_id = free_plan_id(&pool).await;
    let body = json!({"plan_id": plan_id, "status": "active"});
    let response = send(
        pool,
        platform_request(&put_sub_uri(tenant), Method::PUT, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
#[serial(billing_platform)]
async fn finance_can_put_subscription() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("finance")).await;
    let tenant = seed_tenant(&pool, "Finance Sub Tenant").await;
    let plan_id = free_plan_id(&pool).await;
    let body = json!({"plan_id": plan_id, "status": "active"});
    let response = send(
        pool,
        platform_request(&put_sub_uri(tenant), Method::PUT, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

// ── RBAC: Developer, Sales, Support → 403 on all five endpoints ──────

#[tokio::test]
#[serial(billing_platform)]
async fn developer_gets_403_on_list_plans() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("developer")).await;
    let response = send(pool, platform_request(LIST_PLANS, Method::GET, user, None)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(billing_platform)]
async fn sales_gets_403_on_list_plans() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("sales")).await;
    let response = send(pool, platform_request(LIST_PLANS, Method::GET, user, None)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(billing_platform)]
async fn support_gets_403_on_list_plans() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("support")).await;
    let response = send(pool, platform_request(LIST_PLANS, Method::GET, user, None)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(billing_platform)]
async fn developer_gets_403_on_create_plan() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("developer")).await;
    let body = json!({"name": "Dev Plan", "monthly_price_cents": 0});
    let response = send(
        pool,
        platform_request(CREATE_PLAN, Method::POST, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(billing_platform)]
async fn sales_gets_403_on_create_plan() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("sales")).await;
    let body = json!({"name": "Sales Plan", "monthly_price_cents": 0});
    let response = send(
        pool,
        platform_request(CREATE_PLAN, Method::POST, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(billing_platform)]
async fn support_gets_403_on_create_plan() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("support")).await;
    let body = json!({"name": "Support Plan", "monthly_price_cents": 0});
    let response = send(
        pool,
        platform_request(CREATE_PLAN, Method::POST, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(billing_platform)]
async fn developer_gets_403_on_update_plan() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("developer")).await;
    let free_id = free_plan_id(&pool).await;
    let body = json!({"status": "retired"});
    let response = send(
        pool,
        platform_request(&update_plan_uri(free_id), Method::PATCH, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(billing_platform)]
async fn sales_gets_403_on_update_plan() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("sales")).await;
    let free_id = free_plan_id(&pool).await;
    let body = json!({"status": "retired"});
    let response = send(
        pool,
        platform_request(&update_plan_uri(free_id), Method::PATCH, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(billing_platform)]
async fn support_gets_403_on_update_plan() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("support")).await;
    let free_id = free_plan_id(&pool).await;
    let body = json!({"status": "retired"});
    let response = send(
        pool,
        platform_request(&update_plan_uri(free_id), Method::PATCH, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(billing_platform)]
async fn developer_gets_403_on_list_subscriptions() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("developer")).await;
    let response = send(pool, platform_request(LIST_SUBS, Method::GET, user, None)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(billing_platform)]
async fn sales_gets_403_on_list_subscriptions() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("sales")).await;
    let response = send(pool, platform_request(LIST_SUBS, Method::GET, user, None)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(billing_platform)]
async fn support_gets_403_on_list_subscriptions() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("support")).await;
    let response = send(pool, platform_request(LIST_SUBS, Method::GET, user, None)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(billing_platform)]
async fn developer_gets_403_on_put_subscription() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("developer")).await;
    let tenant = seed_tenant(&pool, "Dev Sub").await;
    let plan_id = free_plan_id(&pool).await;
    let body = json!({"plan_id": plan_id, "status": "active"});
    let response = send(
        pool,
        platform_request(&put_sub_uri(tenant), Method::PUT, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(billing_platform)]
async fn sales_gets_403_on_put_subscription() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("sales")).await;
    let tenant = seed_tenant(&pool, "Sales Sub").await;
    let plan_id = free_plan_id(&pool).await;
    let body = json!({"plan_id": plan_id, "status": "active"});
    let response = send(
        pool,
        platform_request(&put_sub_uri(tenant), Method::PUT, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial(billing_platform)]
async fn support_gets_403_on_put_subscription() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("support")).await;
    let tenant = seed_tenant(&pool, "Support Sub").await;
    let plan_id = free_plan_id(&pool).await;
    let body = json!({"plan_id": plan_id, "status": "active"});
    let response = send(
        pool,
        platform_request(&put_sub_uri(tenant), Method::PUT, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// ── Validation / conflict tests ──────────────────────────────────────

#[tokio::test]
#[serial(billing_platform)]
async fn duplicate_plan_name_returns_409() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("super_admin")).await;
    let unique_name = format!("UniqueConflict {}", Uuid::new_v4().simple());
    let body = json!({"name": unique_name, "monthly_price_cents": 0});
    let r1 = send(
        pool.clone(),
        platform_request(CREATE_PLAN, Method::POST, user, Some(body.clone())),
    )
    .await;
    assert_eq!(r1.status(), StatusCode::CREATED);
    let r2 = send(
        pool.clone(),
        platform_request(CREATE_PLAN, Method::POST, user, Some(body)),
    )
    .await;
    assert_eq!(r2.status(), StatusCode::CONFLICT);
    let err = body_json(r2).await;
    assert_eq!(err["error"]["code"], "conflict");
}

#[tokio::test]
#[serial(billing_platform)]
async fn empty_name_returns_422() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("super_admin")).await;
    let body = json!({"name": "", "monthly_price_cents": 0});
    let response = send(
        pool,
        platform_request(CREATE_PLAN, Method::POST, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
#[serial(billing_platform)]
async fn negative_price_returns_422() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("super_admin")).await;
    let body = json!({"name": "NegPrice", "monthly_price_cents": -1});
    let response = send(
        pool,
        platform_request(CREATE_PLAN, Method::POST, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
#[serial(billing_platform)]
async fn retiring_default_plan_returns_409() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("super_admin")).await;
    let default_id = free_plan_id(&pool).await;
    let body = json!({"status": "retired"});
    let response = send(
        pool,
        platform_request(
            &update_plan_uri(default_id),
            Method::PATCH,
            user,
            Some(body),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let err = body_json(response).await;
    assert_eq!(err["error"]["code"], "conflict");
}

#[tokio::test]
#[serial(billing_platform)]
async fn assigning_unknown_plan_returns_404() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("super_admin")).await;
    let tenant = seed_tenant(&pool, "Unknown Plan Tenant").await;
    let unknown_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let body = json!({"plan_id": unknown_id, "status": "active"});
    let response = send(
        pool,
        platform_request(&put_sub_uri(tenant), Method::PUT, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[serial(billing_platform)]
async fn assigning_retired_plan_to_new_tenant_returns_409() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("super_admin")).await;
    // Create a plan, retire it
    let plan_id = create_plan_via_api(&pool, user, "RetiredPlanTest").await;
    let retire_body = json!({"status": "retired"});
    let resp = send(
        pool.clone(),
        platform_request(
            &update_plan_uri(plan_id),
            Method::PATCH,
            user,
            Some(retire_body),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    // Try assigning it to a new tenant
    let tenant = seed_tenant(&pool, "New Tenant Retired").await;
    let body = json!({"plan_id": plan_id, "status": "active"});
    let response = send(
        pool,
        platform_request(&put_sub_uri(tenant), Method::PUT, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
#[serial(billing_platform)]
async fn changing_only_status_on_retired_plan_returns_200() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("super_admin")).await;
    // Create a plan, assign it, retire it, then change status
    let plan_id = create_plan_via_api(&pool, user, "RetiredThenChangeStatus").await;
    let tenant = seed_tenant(&pool, "Retired Status Change").await;
    // Assign the new plan first
    let body = json!({"plan_id": plan_id, "status": "active"});
    let r = send(
        pool.clone(),
        platform_request(&put_sub_uri(tenant), Method::PUT, user, Some(body)),
    )
    .await;
    assert_eq!(r.status(), StatusCode::OK);
    // Retire the plan
    let retire_body = json!({"status": "retired"});
    let r = send(
        pool.clone(),
        platform_request(
            &update_plan_uri(plan_id),
            Method::PATCH,
            user,
            Some(retire_body),
        ),
    )
    .await;
    assert_eq!(r.status(), StatusCode::OK);
    // Change only status while on retired plan → should work
    let body = json!({"plan_id": plan_id, "status": "past_due"});
    let response = send(
        pool,
        platform_request(&put_sub_uri(tenant), Method::PUT, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
#[serial(billing_platform)]
async fn same_put_body_twice_is_idempotent() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("super_admin")).await;
    let tenant = seed_tenant(&pool, "Idempotent Tenant").await;
    let plan_id = free_plan_id(&pool).await;
    let body = json!({"plan_id": plan_id, "status": "active"});
    let r1 = send(
        pool.clone(),
        platform_request(&put_sub_uri(tenant), Method::PUT, user, Some(body.clone())),
    )
    .await;
    assert_eq!(r1.status(), StatusCode::OK);
    let r2 = send(
        pool.clone(),
        platform_request(&put_sub_uri(tenant), Method::PUT, user, Some(body)),
    )
    .await;
    assert_eq!(r2.status(), StatusCode::OK);
}

// ── Default plan / unassigned tenant ─────────────────────────────────

#[tokio::test]
#[serial(billing_platform)]
async fn list_subscriptions_shows_default_plan_for_unassigned_tenant() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("super_admin")).await;
    let tenant = seed_tenant(&pool, "Unassigned Default Tenant").await;
    let response = send(pool, platform_request(LIST_SUBS, Method::GET, user, None)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let subs = body["subscriptions"].as_array().unwrap();
    let entry = subs
        .iter()
        .find(|s| s["tenant_id"] == json!(tenant.to_string()))
        .unwrap();
    assert_eq!(entry["explicit"], false);
    assert_eq!(entry["plan_name"], "Free");
    assert_eq!(entry["status"], "active");
}

// ── Audit assertions ─────────────────────────────────────────────────

#[tokio::test]
#[serial(billing_platform)]
async fn create_plan_writes_audit_log() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("super_admin")).await;
    let unique_name = format!("AuditPlan {}", Uuid::new_v4().simple());
    let body = json!({"name": unique_name, "monthly_price_cents": 2000});
    let response = send(
        pool.clone(),
        platform_request(CREATE_PLAN, Method::POST, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let plan: Value = body_json(response).await;
    let plan_id = plan["id"].as_str().unwrap();
    let row: Value = sqlx::query_scalar(
        "SELECT row_to_json(a.*) FROM audit_logs a WHERE a.action = 'billing_plan.created' AND a.resource_id = $1 ORDER BY a.created_at DESC LIMIT 1",
    )
    .bind(plan_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row["actor_user_id"], json!(user.to_string()));
    assert_eq!(row["resource_type"], "billing_plan");
    assert_eq!(row["details"]["name"], unique_name);
}

#[tokio::test]
#[serial(billing_platform)]
async fn update_plan_writes_audit_log() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("super_admin")).await;
    let plan_id = create_plan_via_api(&pool, user, "Audit Status Plan").await;
    let body = json!({"status": "retired"});
    let response = send(
        pool.clone(),
        platform_request(&update_plan_uri(plan_id), Method::PATCH, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    // Verify the audit row
    let row: Value = sqlx::query_scalar(
        "SELECT row_to_json(a.*) FROM audit_logs a WHERE a.action = 'billing_plan.status_changed' AND a.resource_id = $1 ORDER BY a.created_at DESC LIMIT 1",
    )
    .bind(plan_id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row["actor_user_id"], json!(user.to_string()));
    assert_eq!(row["details"]["from"], "active");
    assert_eq!(row["details"]["to"], "retired");
}

#[tokio::test]
#[serial(billing_platform)]
async fn put_subscription_writes_audit_log() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("super_admin")).await;
    let tenant = seed_tenant(&pool, "Audit Sub Tenant").await;
    let plan_id = free_plan_id(&pool).await;
    let body = json!({"plan_id": plan_id, "status": "active"});
    let response = send(
        pool.clone(),
        platform_request(&put_sub_uri(tenant), Method::PUT, user, Some(body)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    // Verify the audit row
    let row: Value = sqlx::query_scalar(
        "SELECT row_to_json(a.*) FROM audit_logs a WHERE a.action = 'billing_subscription.changed' AND a.tenant_id = $1 ORDER BY a.created_at DESC LIMIT 1",
    )
    .bind(tenant)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row["actor_user_id"], json!(user.to_string()));
    assert_eq!(row["resource_type"], "billing_subscription");
    assert!(row["details"]["previous_plan_id"].is_null());
    assert_eq!(row["details"]["plan_id"], json!(plan_id.to_string()));
}

// ── FR-011 / SC-007: one row per non-deleted tenant ──────────────────

#[tokio::test]
#[serial(billing_platform)]
async fn list_subscriptions_returns_one_row_per_tenant() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let user = seed_user(&pool, Some("super_admin")).await;

    // Create a few tenants — including a deleted one
    let t1 = seed_tenant(&pool, "Row Count T1").await;
    let t2 = seed_tenant(&pool, "Row Count T2").await;
    let _t3 = seed_tenant(&pool, "Row Count T3").await;
    let _deleted = seed_tenant(&pool, "Row Count Deleted").await;
    sqlx::query("UPDATE tenants SET deleted_at = now() WHERE name = 'Row Count Deleted'")
        .execute(&pool)
        .await
        .unwrap();

    // Assign explicit subscription to t2
    let plan_id = free_plan_id(&pool).await;
    let body = json!({"plan_id": plan_id, "status": "active"});
    let r = send(
        pool.clone(),
        platform_request(&put_sub_uri(t2), Method::PUT, user, Some(body)),
    )
    .await;
    assert_eq!(r.status(), StatusCode::OK);

    let response = send(pool, platform_request(LIST_SUBS, Method::GET, user, None)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let subs = body["subscriptions"].as_array().unwrap();

    // Our 3 tenants must each have exactly one row (FR-011 / SC-007)
    for (tid, should_be_explicit) in [(t1, false), (t2, true), (_t3, false)] {
        let entries: Vec<&Value> = subs
            .iter()
            .filter(|s| s["tenant_id"] == json!(tid.to_string()))
            .collect();
        assert_eq!(
            entries.len(),
            1,
            "expected exactly one row for tenant {tid} (FR-011 / SC-007)"
        );
        assert_eq!(entries[0]["explicit"], should_be_explicit);
    }
}
