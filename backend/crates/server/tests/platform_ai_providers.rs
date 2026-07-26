use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use config::Environment;
use http_body_util::BodyExt;
use server::router;
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;
use serial_test::serial;

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
            eprintln!("skipping platform AI providers tests: DATABASE_URL not set");
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping platform AI providers tests: DATABASE_URL is unreachable");
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

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug, status) VALUES ($1, $2, 'active') RETURNING id")
        .bind("AI Providers Tenant")
        .bind(format!("aip-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, platform_role: Option<&str>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name, platform_role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(format!("pai_{}@example.com", Uuid::new_v4()))
    .bind("Platform AI Providers User")
    .bind(platform_role)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query("DELETE FROM escalations").execute(pool).await.unwrap();
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

async fn seed_ai_call(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    provider: &str,
    status: &str,
    category: Option<&str>,
    latency: i32,
    minutes_ago: i32,
) {
    sqlx::query(
        "INSERT INTO ai_usage_records (tenant_id, provider, model, status, streamed, error_category, latency_ms, input_tokens, output_tokens, created_at) VALUES ($1, $2, 'gpt-4o', $3, false, $4, $5, 10, 5, now() - make_interval(mins => $6))",
    )
    .bind(tenant_id)
    .bind(provider)
    .bind(status)
    .bind(category)
    .bind(latency)
    .bind(minutes_ago)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
#[serial]
async fn platform_ai_providers_failure_rate_and_verdict() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;

    for _ in 0..75 {
        seed_ai_call(
            &pool,
            tenant_id,
            "openai",
            "success",
            None,
            100,
            120,
        )
        .await;
    }
    for _ in 0..25 {
        seed_ai_call(
            &pool,
            tenant_id,
            "openai",
            "failure",
            Some("rate_limited"),
            200,
            60,
        )
        .await;
    }

    let user_id = seed_user(&pool, Some("super_admin")).await;
    let response = send(
        pool,
        authenticated_request(
            "/api/v1/platform/ai-providers?window=7d",
            Method::GET,
            user_id,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    let providers = body["providers"].as_array().unwrap();
    let openai = providers
        .iter()
        .find(|p| p["provider"] == "openai")
        .unwrap();
    assert_eq!(openai["total_calls"], 100);
    assert_eq!(openai["failures"], 25);
    assert!((openai["failure_rate"].as_f64().unwrap() - 0.25).abs() < 0.001);
    assert_eq!(openai["verdict"], "unhealthy");
}

#[tokio::test]
#[serial]
async fn platform_ai_providers_degraded_verdict() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;

    for _ in 0..96 {
        seed_ai_call(
            &pool,
            tenant_id,
            "openai",
            "success",
            None,
            100,
            120,
        )
        .await;
    }
    for _ in 0..4 {
        seed_ai_call(
            &pool,
            tenant_id,
            "openai",
            "failure",
            Some("timeout"),
            200,
            60,
        )
        .await;
    }

    let user_id = seed_user(&pool, Some("super_admin")).await;
    let response = send(
        pool,
        authenticated_request(
            "/api/v1/platform/ai-providers?window=7d",
            Method::GET,
            user_id,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    let providers = body["providers"].as_array().unwrap();
    let openai = providers
        .iter()
        .find(|p| p["provider"] == "openai")
        .unwrap();
    assert!((openai["failure_rate"].as_f64().unwrap() - 0.04).abs() < 0.001);
    assert_eq!(openai["verdict"], "degraded");
}

#[tokio::test]
#[serial]
async fn platform_ai_providers_categories_sum_to_failures() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;

    let categories = [
        ("authentication", 2),
        ("rate_limited", 3),
        ("unavailable", 1),
        ("timeout", 4),
        ("invalid_request", 0),
    ];
    for _ in 0..90 {
        seed_ai_call(
            &pool,
            tenant_id,
            "openai",
            "success",
            None,
            100,
            120,
        )
        .await;
    }
    for (cat, n) in &categories {
        for _ in 0..*n {
            seed_ai_call(
                &pool,
                tenant_id,
                "openai",
                "failure",
                Some(cat),
                200,
                60,
            )
            .await;
        }
    }

    let user_id = seed_user(&pool, Some("super_admin")).await;
    let response = send(
        pool,
        authenticated_request(
            "/api/v1/platform/ai-providers?window=7d",
            Method::GET,
            user_id,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    let providers = body["providers"].as_array().unwrap();
    let openai = providers
        .iter()
        .find(|p| p["provider"] == "openai")
        .unwrap();
    let cat = &openai["failures_by_category"];
    let sum: i64 = cat["authentication"].as_i64().unwrap()
        + cat["rate_limited"].as_i64().unwrap()
        + cat["unavailable"].as_i64().unwrap()
        + cat["timeout"].as_i64().unwrap()
        + cat["invalid_request"].as_i64().unwrap();
    assert_eq!(sum, openai["failures"].as_i64().unwrap());
}

#[tokio::test]
#[serial]
async fn platform_ai_providers_insufficient_data_and_no_activity() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;

    // 3 calls for anthropic -> insufficient_data
    for _ in 0..3 {
        seed_ai_call(
            &pool,
            tenant_id,
            "anthropic",
            "success",
            None,
            100,
            120,
        )
        .await;
    }
    // 0 calls for gemini -> no_activity

    let user_id = seed_user(&pool, Some("super_admin")).await;
    let response = send(
        pool,
        authenticated_request(
            "/api/v1/platform/ai-providers?window=7d",
            Method::GET,
            user_id,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    let providers = body["providers"].as_array().unwrap();

    let anthropic = providers
        .iter()
        .find(|p| p["provider"] == "anthropic")
        .unwrap();
    assert_eq!(anthropic["verdict"], "insufficient_data");

    let gemini = providers
        .iter()
        .find(|p| p["provider"] == "gemini")
        .unwrap();
    assert_eq!(gemini["verdict"], "no_activity");
    assert!(gemini["failure_rate"].is_null());
    assert!(gemini["p50_latency_ms"].is_null());
    assert!(gemini["p95_latency_ms"].is_null());
}

#[tokio::test]
#[serial]
async fn platform_ai_providers_recent_failures_capped_at_50() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;

    for i in 0..60 {
        seed_ai_call(&pool, tenant_id, "openai", "failure", Some("rate_limited"), 100, i + 1).await;
    }

    let user_id = seed_user(&pool, Some("super_admin")).await;
    let response = send(
        pool,
        authenticated_request(
            "/api/v1/platform/ai-providers?window=7d",
            Method::GET,
            user_id,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    assert!(body["recent_failures"].as_array().unwrap().len() <= 50);
}

#[tokio::test]
#[serial]
async fn platform_ai_providers_invalid_window_returns_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let user_id = seed_user(&pool, Some("super_admin")).await;
    let response = send(
        pool,
        authenticated_request(
            "/api/v1/platform/ai-providers?window=90d",
            Method::GET,
            user_id,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
#[serial]
async fn platform_ai_providers_valid_windows_200() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let user_id = seed_user(&pool, Some("super_admin")).await;

    for w in ["1h", "24h", "7d"] {
        let resp = send(
            pool.clone(),
            authenticated_request(
                &format!("/api/v1/platform/ai-providers?window={w}"),
                Method::GET,
                user_id,
            ),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK, "window={w} should be 200");
    }
}

#[tokio::test]
#[serial]
async fn platform_ai_providers_no_sensitive_content_in_response() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;

    seed_ai_call(
        &pool,
        tenant_id,
        "openai",
        "failure",
        Some("rate_limited"),
        100,
        120,
    )
    .await;

    let user_id = seed_user(&pool, Some("super_admin")).await;
    let response = send(
        pool,
        authenticated_request(
            "/api/v1/platform/ai-providers?window=7d",
            Method::GET,
            user_id,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    let body_str = format!("{}", body);
    assert!(!body_str.contains("request_content"));
    assert!(!body_str.contains("response_content"));
}
