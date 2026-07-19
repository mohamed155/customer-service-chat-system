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
            eprintln!("skipping widget conversation flow tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping widget conversation flow tests: DATABASE_URL is unreachable");
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

fn authed_request(method: Method, path: &str, token: &str) -> Request<Body> {
    let mut builder = Request::builder().uri(path).method(method);
    if !token.is_empty() {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::empty()).unwrap()
}

fn authed_json_post(path: &str, token: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .uri(path)
        .method(Method::POST)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t001_creating_conversation_persists_with_channel_widget() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "ConvFlow").await;
    let pub_id = "wgt_conv_001";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let token = mint_session(&pool, pub_id).await;

    let response = send(
        pool.clone(),
        authed_json_post(
            "/api/v1/widget/v1/conversations",
            &token,
            serde_json::json!({}),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let json = body_json(response).await;
    let conv_id = json["data"]["conversation"]["id"].as_str().unwrap();

    let row: (String,) = sqlx::query_as("SELECT channel FROM conversations WHERE id = $1")
        .bind(Uuid::parse_str(conv_id).unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.0, "widget");
}

#[tokio::test]
async fn t002_creating_conversation_creates_anonymous_customer() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "ConvFlow2").await;
    let pub_id = "wgt_conv_002";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let token = mint_session(&pool, pub_id).await;

    let response = send(
        pool.clone(),
        authed_json_post(
            "/api/v1/widget/v1/conversations",
            &token,
            serde_json::json!({}),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let json = body_json(response).await;
    let conv_id = Uuid::parse_str(json["data"]["conversation"]["id"].as_str().unwrap()).unwrap();

    let customer_id: Uuid =
        sqlx::query_scalar("SELECT customer_id FROM conversations WHERE id = $1")
            .bind(conv_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let customer_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM customers WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(customer_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(customer_exists, "anonymous customer must exist");

    let channel_identifier_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM customer_channel_identifiers \
         WHERE customer_id = $1 AND channel = 'widget')",
    )
    .bind(customer_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        channel_identifier_exists,
        "customer channel identifier must exist"
    );
}

#[tokio::test]
async fn t003_creating_conversation_has_widget_instance_id() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "ConvFlow3").await;
    let pub_id = "wgt_conv_003";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let token = mint_session(&pool, pub_id).await;

    let response = send(
        pool.clone(),
        authed_json_post(
            "/api/v1/widget/v1/conversations",
            &token,
            serde_json::json!({}),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let json = body_json(response).await;
    let conv_id = Uuid::parse_str(json["data"]["conversation"]["id"].as_str().unwrap()).unwrap();

    let widget_instance_id: Option<Uuid> =
        sqlx::query_scalar("SELECT widget_instance_id FROM conversations WHERE id = $1")
            .bind(conv_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        widget_instance_id.is_some(),
        "widget_instance_id must be non-null"
    );
}

#[tokio::test]
async fn t004_posting_message_stores_with_customer_sender() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "ConvFlow4").await;
    let pub_id = "wgt_conv_004";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let token = mint_session(&pool, pub_id).await;

    let conv_resp = send(
        pool.clone(),
        authed_json_post(
            "/api/v1/widget/v1/conversations",
            &token,
            serde_json::json!({}),
        ),
    )
    .await;
    let conv_id = body_json(conv_resp).await["data"]["conversation"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let msg_resp = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/messages"),
            &token,
            serde_json::json!({ "body": "Hello from widget" }),
        ),
    )
    .await;
    assert_eq!(msg_resp.status(), StatusCode::CREATED);

    let message_kind: String = sqlx::query_scalar(
        "SELECT kind FROM messages WHERE conversation_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(Uuid::parse_str(&conv_id).unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(message_kind, "customer");
}

#[tokio::test]
async fn t005_empty_body_returns_422() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "ConvFlow5").await;
    let pub_id = "wgt_conv_005";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let token = mint_session(&pool, pub_id).await;

    let conv_resp = send(
        pool.clone(),
        authed_json_post(
            "/api/v1/widget/v1/conversations",
            &token,
            serde_json::json!({}),
        ),
    )
    .await;
    let conv_id = body_json(conv_resp).await["data"]["conversation"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let msg_resp = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/messages"),
            &token,
            serde_json::json!({ "body": "" }),
        ),
    )
    .await;
    assert_eq!(msg_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn t006_over_4000_char_body_returns_422() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "ConvFlow6").await;
    let pub_id = "wgt_conv_006";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let token = mint_session(&pool, pub_id).await;

    let conv_resp = send(
        pool.clone(),
        authed_json_post(
            "/api/v1/widget/v1/conversations",
            &token,
            serde_json::json!({}),
        ),
    )
    .await;
    let conv_id = body_json(conv_resp).await["data"]["conversation"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let long_body = "x".repeat(4001);
    let msg_resp = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/messages"),
            &token,
            serde_json::json!({ "body": long_body }),
        ),
    )
    .await;
    assert_eq!(msg_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn t007_posting_to_another_sessions_conversation_returns_404() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "ConvFlow7").await;
    let pub_id = "wgt_conv_007";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let token_a = mint_session(&pool, pub_id).await;
    let token_b = mint_session(&pool, pub_id).await;

    let conv_resp = send(
        pool.clone(),
        authed_json_post(
            "/api/v1/widget/v1/conversations",
            &token_a,
            serde_json::json!({}),
        ),
    )
    .await;
    let conv_id = body_json(conv_resp).await["data"]["conversation"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let msg_resp = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/messages"),
            &token_b,
            serde_json::json!({ "body": "Hello" }),
        ),
    )
    .await;
    assert_eq!(msg_resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn t008_posting_to_resolved_conversation_returns_409() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "ConvFlow8").await;
    let pub_id = "wgt_conv_008";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let token = mint_session(&pool, pub_id).await;

    let conv_resp = send(
        pool.clone(),
        authed_json_post(
            "/api/v1/widget/v1/conversations",
            &token,
            serde_json::json!({}),
        ),
    )
    .await;
    let conv_id_str = body_json(conv_resp).await["data"]["conversation"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let conv_id = Uuid::parse_str(&conv_id_str).unwrap();

    sqlx::query("UPDATE conversations SET status = 'resolved' WHERE id = $1")
        .bind(conv_id)
        .execute(&pool)
        .await
        .unwrap();

    let msg_resp = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/messages"),
            &token,
            serde_json::json!({ "body": "Hello" }),
        ),
    )
    .await;
    assert_eq!(msg_resp.status(), StatusCode::CONFLICT);

    let status: String = sqlx::query_scalar("SELECT status FROM conversations WHERE id = $1")
        .bind(conv_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        status, "resolved",
        "resolved conversation must stay resolved after 409"
    );
}
