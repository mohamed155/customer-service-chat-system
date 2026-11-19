#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: a failed unwrap is the failed test
//! Inviting someone who is already a member of the workspace is refused with `409` (MOH-193).
//!
//! The check asks `identity` for the accounts with the invited email and then looks for a
//! membership of any of them. A deleted account and a live one can share an address (the unique
//! index covers live accounts only), so looking at one account is not enough: the member may be
//! the live one while the lookup returns the deleted one.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use identity::session;
use serde_json::{json, Value};
use server::router;
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_ORIGIN: &str = "http://localhost:4200";
const JWT_SECRET: &str = "test-auth-jwt-secret-at-least-32-bytes";

fn test_config() -> config::AppConfig {
    config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: "redis://127.0.0.1:6379".into(),
        auth_jwt_secret: "test-auth-jwt-secret-at-least-32-bytes".into(),
        auth_session_ttl_seconds: 28_800,
        port: 0,
        bind_address: "0.0.0.0".into(),
        environment: config::Environment::Test,
        cors_allowed_origins: vec![TEST_ORIGIN.into()],
        log_format: config::LogFormat::Pretty,
        smtp_url: None,
        smtp_from: None,
        public_dashboard_url: "http://localhost:4200".into(),
        db_max_connections: 2,
        db_acquire_timeout_ms: 5000,
        ready_probe_timeout_ms: 5000,
        shutdown_grace_seconds: 1,
        docs_enabled: false,
        rate_limit_enabled: false,
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
        ai_key_encryption_key: Some("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=".into()),
        integration_secrets_key: None,
        secrets_root_key: None,
        secrets_root_key_previous: None,
        auth_mfa_required: false,
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
        trusted_proxy: false,
        outbound_http: config::OutboundHttpConfig::default(),
    }
}

fn state(pool: sqlx::PgPool, config: config::AppConfig) -> AppState {
    AppState {
        config: Arc::new(config.clone()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool, &config).unwrap(),
        budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
    }
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let require = std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1");
    let Ok(url) = std::env::var("DATABASE_URL") else {
        assert!(!require, "REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
        return None;
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        assert!(
            !require,
            "REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable"
        );
        return None;
    }
    Some(pool)
}

async fn seed_user(email: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name, password_hash) VALUES ($1, 'Invitee', $2) \
         RETURNING id",
    )
    .bind(email)
    .bind(identity::password::hash_password("correct-horse-battery-1").unwrap())
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap()
}

async fn add_member(tenant: Uuid, user: Uuid, role: &str) {
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(tenant)
        .bind(user)
        .bind(role)
        .execute(&db::testing::seed_pool())
        .await
        .unwrap();
}

async fn invite(
    app: &axum::Router,
    tenant: Uuid,
    as_user: Uuid,
    email: &str,
) -> (StatusCode, Value) {
    let (jwt, _, _) =
        session::issue_session(JWT_SECRET, 3600, as_user, session::SessionAuth::password())
            .unwrap();
    let request = Request::builder()
        .uri("/api/v1/tenant/members/invitations")
        .method(Method::POST)
        .header("origin", TEST_ORIGIN)
        .header("x-requested-with", "XMLHttpRequest")
        .header("content-type", "application/json")
        .header("x-tenant-id", tenant.to_string())
        .header("cookie", format!("app_session={jwt}"))
        .body(Body::from(
            json!({ "email": email, "role": "agent" }).to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn inviting_a_member_is_refused_even_when_a_deleted_account_shares_the_email() {
    let Some(pool) = get_pool().await else { return };
    db::testing::migrate().await;
    let app = router::app(state(pool, test_config()));
    let seed = db::testing::seed_pool();
    let tag = Uuid::new_v4().simple().to_string();
    let tenant = db::testing::insert_tenant(&seed, "Invitations", &format!("inv-{tag}")).await;
    let owner = seed_user(&format!("owner-{tag}@invitations.test")).await;
    add_member(tenant, owner, "owner").await;

    // A deleted account first, then the live account that is the member: the same address.
    let shared = format!("shared-{tag}@invitations.test");
    let old = seed_user(&shared).await;
    sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
        .bind(old)
        .execute(&seed)
        .await
        .unwrap();
    let live = seed_user(&shared).await;
    add_member(tenant, live, "agent").await;

    let (status, body) = invite(&app, tenant, owner, &shared).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "conflict", "{body}");

    // An address nobody uses is invited as usual.
    let (status, body) = invite(&app, tenant, owner, &format!("new-{tag}@invitations.test")).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
}
