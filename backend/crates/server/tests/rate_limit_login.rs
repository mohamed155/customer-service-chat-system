use std::sync::Arc;
use std::time::Duration;

use argon2::{Argon2, PasswordHasher};
use axum::body::Body;
use axum::http::Request;
use serde_json::json;
use server::router;
use server::state::AppState;
use tower::ServiceExt;

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            assert!(
                !require_db_tests(),
                "REQUIRE_DB_TESTS=1 but DATABASE_URL is not set"
            );
            eprintln!("skipping rate_limit_login test: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        assert!(
            !require_db_tests(),
            "REQUIRE_DB_TESTS=1 but the database is unreachable at {url}"
        );
        eprintln!("skipping rate_limit_login test: could not connect to DATABASE_URL");
        return None;
    }
    Some(pool)
}

async fn get_redis_url() -> Option<String> {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
    let cache = match cache::Cache::new(&url) {
        Ok(c) => c,
        Err(e) => {
            assert!(
                !require_db_tests(),
                "REQUIRE_DB_TESTS=1 but a Redis client could not be created for {url}: {e}"
            );
            return None;
        }
    };
    if cache.ping().await.is_err() {
        assert!(
            !require_db_tests(),
            "REQUIRE_DB_TESTS=1 but Redis is unreachable at {url}"
        );
        eprintln!("skipping rate_limit_login test: Redis unreachable at {url}");
        return None;
    }
    Some(url)
}

fn test_config(redis_url: &str) -> config::AppConfig {
    config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: redis_url.into(),
        auth_jwt_secret: "test-auth-jwt-secret-at-least-32-bytes".into(),
        auth_session_ttl_seconds: 28_800,
        port: 0,
        bind_address: "0.0.0.0".into(),
        environment: config::Environment::Test,
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
        rate_limit_enabled: true,
        rate_limit_login_max: 10,
        rate_limit_login_window_secs: 60,
        rate_limit_max_override: 10000,
        rate_limit_fail_mode: "auth_closed".into(),
        security_headers_enabled: false,
        csp_policy: String::new(),
        csp_report_only: false,
        hsts_max_age: 0,
        otlp_endpoint: None,
        trace_sample_ratio: 0.0,
        sentry_dsn: None,
        release_sha: None,
        ai_key_encryption_key: Some("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=".into()),
        integration_secrets_key: None,
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
        trusted_proxy: false,
    }
}

fn make_state(pool: sqlx::PgPool, redis_url: &str) -> AppState {
    let cfg = test_config(redis_url);
    AppState {
        config: Arc::new(cfg.clone()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new(redis_url).unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool, &cfg).unwrap(),
        budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
    }
}

fn login_request(email: &str, password: &str) -> Request<Body> {
    Request::builder()
        .uri("/api/v1/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "email": email, "password": password }).to_string(),
        ))
        .unwrap()
}

fn hash_password(password: &str) -> String {
    Argon2::default()
        .hash_password(password.as_bytes())
        .expect("hash password")
        .to_string()
}

fn unique_email() -> String {
    format!("rate_limit_{}@example.com", uuid::Uuid::new_v4())
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) {
    let password_hash = hash_password("RealPass1!");
    sqlx::query("INSERT INTO users (email, display_name, password_hash) VALUES ($1, $2, $3)")
        .bind(email)
        .bind("Rate Limit User")
        .bind(&password_hash)
        .execute(pool)
        .await
        .expect("seed user");
}

#[tokio::test]
async fn login_rate_limit_blocks_after_10_failures() {
    let Some(pool) = get_pool().await else { return };
    let Some(redis_url) = get_redis_url().await else {
        return;
    };
    db::testing::migrate().await;

    let email = unique_email();
    seed_user(&pool, &email).await;

    let state = make_state(pool, &redis_url);
    let app = router::app(state);

    for i in 1..=10 {
        let resp = app
            .clone()
            .oneshot(login_request(&email, "wrong-password"))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            401,
            "attempt {i} should return 401 Unauthorized"
        );
    }

    let resp = app
        .clone()
        .oneshot(login_request(&email, "wrong-password"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 429);
    assert!(
        resp.headers().get("retry-after").is_some(),
        "429 response must carry Retry-After header"
    );
}

#[tokio::test]
async fn login_rate_limit_prevents_enumeration() {
    let Some(pool) = get_pool().await else { return };
    let Some(redis_url) = get_redis_url().await else {
        return;
    };
    db::testing::migrate().await;

    let existing = unique_email();
    seed_user(&pool, &existing).await;
    let non_existing = unique_email();

    let state = make_state(pool, &redis_url);
    let app = router::app(state);

    let mut existing_statuses = Vec::new();
    let mut non_existing_statuses = Vec::new();

    for _ in 0..11 {
        let resp = app
            .clone()
            .oneshot(login_request(&existing, "wrong-password"))
            .await
            .unwrap();
        existing_statuses.push(resp.status());
    }

    for _ in 0..11 {
        let resp = app
            .clone()
            .oneshot(login_request(&non_existing, "wrong-password"))
            .await
            .unwrap();
        non_existing_statuses.push(resp.status());
    }

    assert_eq!(
        existing_statuses, non_existing_statuses,
        "status sequences for existing and non-existing accounts must match"
    );
}
