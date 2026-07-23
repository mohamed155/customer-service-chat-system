use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use http_body_util::BodyExt;
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
            eprintln!("skipping widget tenant isolation tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping widget tenant isolation tests: DATABASE_URL is unreachable");
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

#[tokio::test]
async fn t001_two_tenants_instances_dont_cross() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_a = seed_tenant(&pool, "IsolationA").await;
    let tenant_b = seed_tenant(&pool, "IsolationB").await;
    let pub_a = "wgt_isolation_a";
    let pub_b = "wgt_isolation_b";
    seed_widget_instance(&pool, tenant_a, pub_a).await;
    seed_widget_instance(&pool, tenant_b, pub_b).await;

    let response_a = send(
        pool.clone(),
        Request::builder()
            .uri(format!("/api/v1/widget/v1/config?widget_id={pub_a}"))
            .method(Method::GET)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response_a.status(), StatusCode::OK);
    let body_a = body_json(response_a).await;
    assert_eq!(body_a["widgetId"], pub_a);

    let response_b = send(
        pool.clone(),
        Request::builder()
            .uri(format!("/api/v1/widget/v1/config?widget_id={pub_b}"))
            .method(Method::GET)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response_b.status(), StatusCode::OK);
    let body_b = body_json(response_b).await;
    assert_eq!(body_b["widgetId"], pub_b);

    let response_a_to_b = send(
        pool.clone(),
        Request::builder()
            .uri(format!("/api/v1/widget/v1/config?widget_id={pub_b}"))
            .method(Method::GET)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response_a_to_b.status(), StatusCode::OK);
    let body_a_to_b = body_json(response_a_to_b).await;
    assert_eq!(body_a_to_b["widgetId"], pub_b);
}

#[tokio::test]
async fn t002_config_never_contains_tenant_id() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "NoTenantId").await;
    let pub_id = "wgt_notenant";
    seed_widget_instance(&pool, tenant_id, pub_id).await;

    let response = send(
        pool.clone(),
        Request::builder()
            .uri(format!("/api/v1/widget/v1/config?widget_id={pub_id}"))
            .method(Method::GET)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert!(
        body.get("tenant_id").is_none(),
        "config must not contain tenant_id"
    );
    assert!(
        body.get("id").is_none(),
        "config must not contain internal id"
    );
}

#[tokio::test]
async fn t003_session_from_tenant_a_cannot_access_tenant_b_conversations() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_a = seed_tenant(&pool, "AccessA").await;
    let tenant_b = seed_tenant(&pool, "AccessB").await;
    let pub_a = "wgt_access_a";
    let pub_b = "wgt_access_b";
    seed_widget_instance(&pool, tenant_a, pub_a).await;
    seed_widget_instance(&pool, tenant_b, pub_b).await;

    let body_a = serde_json::json!({ "widgetId": pub_a });
    let resp_a = send(
        pool.clone(),
        Request::builder()
            .uri("/api/v1/widget/v1/sessions")
            .method(Method::POST)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body_a).unwrap()))
            .unwrap(),
    )
    .await;
    let token_a = body_json(resp_a).await["sessionToken"]
        .as_str()
        .unwrap()
        .to_owned();

    let body_b = serde_json::json!({ "widgetId": pub_b });
    let resp_b = send(
        pool.clone(),
        Request::builder()
            .uri("/api/v1/widget/v1/sessions")
            .method(Method::POST)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body_b).unwrap()))
            .unwrap(),
    )
    .await;
    let token_b = body_json(resp_b).await["sessionToken"]
        .as_str()
        .unwrap()
        .to_owned();

    let conv_resp = send(
        pool.clone(),
        Request::builder()
            .uri("/api/v1/widget/v1/conversations")
            .method(Method::POST)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token_a}"))
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({})).unwrap(),
            ))
            .unwrap(),
    )
    .await;
    let conv_id = body_json(conv_resp).await["data"]["conversation"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let msg_resp = send(
        pool.clone(),
        Request::builder()
            .uri(format!(
                "/api/v1/widget/v1/conversations/{conv_id}/messages"
            ))
            .method(Method::POST)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token_b}"))
            .body(Body::from(
                serde_json::to_vec(&serde_json::json!({ "body": "cross-tenant test" })).unwrap(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(
        msg_resp.status(),
        StatusCode::NOT_FOUND,
        "session from tenant B must get 404 when posting to tenant A's conversation"
    );
}
