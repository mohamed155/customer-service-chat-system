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
            eprintln!("skipping widget session lifecycle tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping widget session lifecycle tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::testing::migrate().await;
    // Widget rate-limit buckets are keyed by client IP, which is the same
    // ("unknown") for every test request; reset them so tests don't share budget.
    cache::Cache::new(
        &std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into()),
    )
    .expect("redis url")
    .delete_by_prefix("rl:widget_")
    .await
    .expect("reset widget rate-limit buckets");
    sqlx::query(
        "TRUNCATE TABLE widget_sessions, widget_instances, messages, \
         customer_channel_identifiers, customers, conversations, \
         outbox_events, audit_logs, tenant_invitations, \
         tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(&db::testing::owner_pool())
    .await
    .expect("failed to reset test tables");
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

async fn seed_widget_instance(pool: &sqlx::PgPool, tenant_id: Uuid, public_id: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO widget_instances \
         (tenant_id, public_id, name, display_name, enabled, allowed_domains) \
         VALUES ($1, $2, $3, $4, true, '{}') RETURNING id",
    )
    .bind(tenant_id)
    .bind(public_id)
    .bind("Test Widget")
    .bind("Test Widget")
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_customer(pool: &sqlx::PgPool, tenant_id: Uuid, display_name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name, email, phone) \
         VALUES ($1, $2, '', '') RETURNING id",
    )
    .bind(tenant_id)
    .bind(display_name)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap()
}

async fn seed_session_with_customer(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    instance_id: Uuid,
    customer_id: Uuid,
    token_prefix: &str,
    expire_hours: i64,
) -> String {
    let token = format!("{token_prefix}-session-token");
    let token_hash = sha2::Sha256::digest(token.as_bytes()).to_vec();
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(expire_hours);
    sqlx::query(
        "INSERT INTO widget_sessions \
         (tenant_id, widget_instance_id, token_hash, customer_id, expires_at) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(tenant_id)
    .bind(instance_id)
    .bind(&token_hash)
    .bind(customer_id)
    .bind(expires_at)
    .execute(pool)
    .await
    .unwrap();
    token
}

async fn mint_session(pool: &sqlx::PgPool, public_id: &str) -> String {
    let body = serde_json::json!({ "widgetId": public_id });
    let response = send(
        pool.clone(),
        Request::builder()
            .uri("/api/v1/widget/v1/sessions")
            .method(Method::POST)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let json = body_json(response).await;
    json["sessionToken"].as_str().unwrap().to_owned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t001_valid_session_resolves_existing_conversation() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "Lifecycle").await;
    let pub_id = "wgt_lifecycle_001";
    let _instance_id = seed_widget_instance(&pool, tenant_id, pub_id).await;
    let token = mint_session(&pool, pub_id).await;

    let conv_resp = send(
        pool.clone(),
        Request::builder()
            .uri("/api/v1/widget/v1/conversations")
            .method(Method::POST)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({})).unwrap(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(conv_resp.status(), StatusCode::CREATED);

    let get_resp = send(
        pool.clone(),
        Request::builder()
            .uri("/api/v1/widget/v1/conversation")
            .method(Method::GET)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let json = body_json(get_resp).await;
    assert!(
        json["data"]["conversation"]["id"].is_string(),
        "valid session must resolve existing conversation"
    );
}

#[tokio::test]
async fn t002_expired_session_returns_401() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "Lifecycle2").await;
    let pub_id = "wgt_lifecycle_002";
    let instance_id = seed_widget_instance(&pool, tenant_id, pub_id).await;

    let expired_token_hash = sha2::Sha256::digest(b"expired-token").to_vec();
    sqlx::query(
        "INSERT INTO widget_sessions \
         (tenant_id, widget_instance_id, token_hash, expires_at) \
         VALUES ($1, $2, $3, now() - interval '1 hour')",
    )
    .bind(tenant_id)
    .bind(instance_id)
    .bind(&expired_token_hash)
    .execute(&pool)
    .await
    .unwrap();

    let response = send(
        pool.clone(),
        Request::builder()
            .uri("/api/v1/widget/v1/conversation")
            .method(Method::GET)
            .header("authorization", "Bearer expired-token")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let json = body_json(response).await;
    assert_eq!(json["error"]["code"], "session_invalid");
}

#[tokio::test]
async fn t003_authenticated_call_slides_expiry() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "Lifecycle3").await;
    let pub_id = "wgt_lifecycle_003";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let token = mint_session(&pool, pub_id).await;

    let initial_expiry: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT expires_at FROM widget_sessions WHERE token_hash = $1")
            .bind(sha2::Sha256::digest(token.as_bytes()).to_vec())
            .fetch_one(&pool)
            .await
            .unwrap();

    tokio::time::sleep(Duration::from_millis(10)).await;

    let response = send(
        pool.clone(),
        Request::builder()
            .uri("/api/v1/widget/v1/conversation")
            .method(Method::GET)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert!(response.status() == StatusCode::OK);

    let updated_expiry: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT expires_at FROM widget_sessions WHERE token_hash = $1")
            .bind(sha2::Sha256::digest(token.as_bytes()).to_vec())
            .fetch_one(&pool)
            .await
            .unwrap();

    assert!(
        updated_expiry > initial_expiry,
        "expires_at must be extended after an authenticated call"
    );
}

#[tokio::test]
async fn t004_resolved_conversation_not_returned_by_get_conversation() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "Lifecycle4").await;
    let pub_id = "wgt_lifecycle_004";
    let instance_id = seed_widget_instance(&pool, tenant_id, pub_id).await;
    let customer_id = seed_customer(&pool, tenant_id, "Lifecycle Cust 4").await;
    let token =
        seed_session_with_customer(&pool, tenant_id, instance_id, customer_id, "lc4", 24).await;

    sqlx::query(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'widget', 'resolved')",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .execute(&db::testing::seed_pool())
    .await
    .unwrap();

    let response = send(
        pool.clone(),
        Request::builder()
            .uri("/api/v1/widget/v1/conversation")
            .method(Method::GET)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert!(
        json["data"].is_null() || json["data"]["conversation"].is_null(),
        "resolved conversation must not be returned by GET /conversation"
    );
}

#[tokio::test]
async fn t005_posting_to_closed_conversation_returns_409() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "Lifecycle5").await;
    let pub_id = "wgt_lifecycle_005";
    let instance_id = seed_widget_instance(&pool, tenant_id, pub_id).await;
    let customer_id = seed_customer(&pool, tenant_id, "Lifecycle Cust 5").await;
    let token =
        seed_session_with_customer(&pool, tenant_id, instance_id, customer_id, "lc5", 24).await;

    let conv_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'widget', 'closed') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();

    let msg_resp = send(
        pool.clone(),
        Request::builder()
            .uri(format!(
                "/api/v1/widget/v1/conversations/{conv_id}/messages"
            ))
            .method(Method::POST)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({ "body": "Hello" })).unwrap(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(msg_resp.status(), StatusCode::CONFLICT);
}

/// `touch_session` only reports a refresh when it refreshed something: a session of another tenant
/// or one that no longer exists is `RowNotFound`, which `session::authenticate` turns into a 401.
#[tokio::test]
async fn t006_touch_session_does_not_report_a_missing_or_foreign_session_as_refreshed() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "Lifecycle6").await;
    let other_tenant = seed_tenant(&pool, "Lifecycle6 Other").await;
    let instance_id = seed_widget_instance(&pool, tenant_id, "wgt_lifecycle_006").await;
    let customer_id = seed_customer(&pool, tenant_id, "Lifecycle Cust 6").await;
    seed_session_with_customer(&pool, tenant_id, instance_id, customer_id, "lc6", 24).await;
    let session_id: Uuid =
        sqlx::query_scalar("SELECT id FROM widget_sessions WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let later = chrono::Utc::now() + chrono::Duration::hours(48);

    widgets::queries::touch_session(&pool, tenant_id, session_id, later)
        .await
        .expect("the owning tenant refreshes its own session");
    assert!(matches!(
        widgets::queries::touch_session(&pool, other_tenant, session_id, later).await,
        Err(sqlx::Error::RowNotFound)
    ));
    assert!(matches!(
        widgets::queries::touch_session(&pool, tenant_id, Uuid::new_v4(), later).await,
        Err(sqlx::Error::RowNotFound)
    ));
}
