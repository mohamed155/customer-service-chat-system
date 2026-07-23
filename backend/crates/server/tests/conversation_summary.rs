use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use server::router;
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::matchers::{header as wm_header, method as wm_method, path as wm_path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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

fn master_key() -> ai::crypto::MasterKey {
    ai::crypto::MasterKey::from_base64("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=").unwrap()
}

fn wiremock_state(pool: sqlx::PgPool, openai_uri: &str) -> AppState {
    let mut cfg = test_config();
    cfg.ai_openai_base_url = Some(openai_uri.to_string());
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
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            eprintln!("skipping conversation summary tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping conversation summary tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query(
        "TRUNCATE TABLE ai_usage_records, ai_credentials, ai_configurations, \
         agent_configurations, agent_prompts, agent_prompt_versions, agent_avatar_uploads, \
         messages, customer_channel_identifiers, customers, conversations, \
         outbox_events, audit_logs, tenant_invitations, tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("failed to reset test tables");
}

async fn send(state: &AppState, request: Request<Body>) -> axum::response::Response {
    router::app_with_test_routes(state.clone())
        .oneshot(request)
        .await
        .expect("request should complete")
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn auth_post_empty(uri: &str, user_id: Uuid, tenant_id: Uuid) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(Method::POST)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({})).unwrap(),
        ))
        .unwrap()
}

fn json_put(uri: &str, user_id: Uuid, tenant_id: Uuid, body: serde_json::Value) -> Request<Body> {
    let bytes = serde_json::to_vec(&body).unwrap();
    Request::builder()
        .uri(uri)
        .method(Method::PUT)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("content-type", "application/json")
        .body(Body::from(bytes))
        .unwrap()
}

// Seed helpers

async fn seed_tenant(pool: &sqlx::PgPool, name: &str) -> Uuid {
    let slug = format!("cs-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(name)
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Summary Test User")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_membership(pool: &sqlx::PgPool, tenant_id: Uuid, user_id: Uuid, role: &str) {
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(tenant_id)
        .bind(user_id)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
}

async fn seed_ai_config(
    pool: &sqlx::PgPool,
    tenant_id: Option<Uuid>,
    provider: &str,
    model: &str,
    fallbacks: serde_json::Value,
) {
    sqlx::query(
        "INSERT INTO ai_configurations (tenant_id, provider, model, fallbacks) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(tenant_id)
    .bind(provider)
    .bind(model)
    .bind(fallbacks)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_ai_credential(
    pool: &sqlx::PgPool,
    tenant_id: Option<Uuid>,
    provider: &str,
    api_key: &str,
    master: &ai::crypto::MasterKey,
) {
    let aad = ai::crypto::aad(tenant_id, provider);
    let (ciphertext, nonce) = ai::crypto::seal(master, &aad, api_key).unwrap();
    let hint = ai::crypto::hint(api_key);
    sqlx::query(
        "INSERT INTO ai_credentials (tenant_id, provider, ciphertext, nonce, key_hint) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(tenant_id)
    .bind(provider)
    .bind(ciphertext)
    .bind(nonce)
    .bind(hint)
    .execute(pool)
    .await
    .unwrap();
}

fn openai_response(content: &str) -> serde_json::Value {
    serde_json::json!({
        "choices": [{"message": {"content": content}, "finish_reason": "stop"}],
        "model": "gpt-4",
        "usage": {"prompt_tokens": 10, "completion_tokens": 5}
    })
}

async fn mock_openai(mock: &MockServer, api_key: &str, body: serde_json::Value) {
    Mock::given(wm_method("POST"))
        .and(wm_path("/v1/chat/completions"))
        .and(wm_header("Authorization", format!("Bearer {}", api_key)))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(mock)
        .await;
}

// ─────────────────────────────────────────────────────────────────────────────
// T036 — Conversation Summary endpoint
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
#[serial_test::serial(conversation_summary_db)]
async fn populated_conversation_returns_summary() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool, "Summary Tenant").await;
    let user_id = seed_user(&pool, "summary-test@example.com").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-summary-test-key", &master).await;

    let agent_payload = serde_json::json!({
        "name": "SummaryBot",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": { "provider": "openai", "model": "gpt-4" },
    });
    let agent_resp = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, agent_payload),
    )
    .await;
    assert_eq!(agent_resp.status(), StatusCode::CREATED);

    mock_openai(
        &openai_mock,
        "sk-summary-test-key",
        openai_response("Customer needs help with billing for order #1234."),
    )
    .await;

    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Summary Customer")
    .fetch_one(&pool)
    .await
    .unwrap();

    let conversation_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let now = chrono::Utc::now();
    for (i, (kind, body)) in [
        ("customer", "I need help with my order"),
        ("ai", "Let me check your order details"),
        ("customer", "Order #1234 hasn't arrived"),
        ("reply", "I've looked into it and it should arrive tomorrow"),
    ]
    .into_iter()
    .enumerate()
    {
        sqlx::query(
            "INSERT INTO messages (tenant_id, conversation_id, kind, body, created_at) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(tenant_id)
        .bind(conversation_id)
        .bind(kind)
        .bind(body)
        .bind(now - chrono::Duration::minutes((4 - i as i64) * 5))
        .execute(&pool)
        .await
        .unwrap();
    }

    let response = send(
        &state,
        auth_post_empty(
            &format!("/api/v1/tenant/conversations/{conversation_id}/summary"),
            user_id,
            tenant_id,
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;

    let summary = body["summary"].as_str().expect("summary must be a string");
    assert!(!summary.is_empty(), "summary must be non-empty");

    let generated_at = body["generatedAt"]
        .as_str()
        .expect("generatedAt must be a string");
    assert!(
        !generated_at.is_empty(),
        "generatedAt must be a non-empty date string"
    );
    assert!(
        chrono::DateTime::parse_from_rfc3339(generated_at).is_ok(),
        "generatedAt must be a valid RFC 3339 date"
    );

    let message_count = body["messageCount"]
        .as_u64()
        .expect("messageCount must be a number");
    assert!(
        message_count > 0,
        "messageCount must be > 0 for a populated conversation"
    );
    assert_eq!(
        message_count, 4,
        "messageCount should match the number of messages inserted"
    );
}

#[tokio::test]
#[serial_test::serial(conversation_summary_db)]
async fn empty_conversation_returns_422() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool, "Empty Summary Tenant").await;
    let user_id = seed_user(&pool, "empty-summary@example.com").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-empty-summary-key", &master).await;

    let agent_payload = serde_json::json!({
        "name": "EmptyBot",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "friendly",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": { "provider": "openai", "model": "gpt-4" },
    });
    send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, agent_payload),
    )
    .await;

    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Empty Customer")
    .fetch_one(&pool)
    .await
    .unwrap();

    let conversation_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let response = send(
        &state,
        auth_post_empty(
            &format!("/api/v1/tenant/conversations/{conversation_id}/summary"),
            user_id,
            tenant_id,
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "validation_failed");
}

#[tokio::test]
#[serial_test::serial(conversation_summary_db)]
async fn cross_tenant_conversation_returns_404() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_a = seed_tenant(&pool, "Summary Tenant A").await;
    let tenant_b = seed_tenant(&pool, "Summary Tenant B").await;
    let admin_b = seed_user(&pool, "summary-cross-b@example.com").await;
    seed_membership(&pool, tenant_b, admin_b, "admin").await;

    let customer_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_a)
    .bind("Cross Tenant Customer A")
    .fetch_one(&pool)
    .await
    .unwrap();

    let conversation_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_a)
    .bind(customer_a)
    .fetch_one(&pool)
    .await
    .unwrap();

    let response = send(
        &state,
        auth_post_empty(
            &format!("/api/v1/tenant/conversations/{conversation_a}/summary"),
            admin_b,
            tenant_b,
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
#[serial_test::serial(conversation_summary_db)]
async fn disabled_membership_returns_403() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool, "Disabled Summary Tenant").await;
    let disabled_user = seed_user(&pool, "disabled-summary@example.com").await;

    sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, 'viewer', 'disabled')",
    )
    .bind(tenant_id)
    .bind(disabled_user)
    .execute(&pool)
    .await
    .unwrap();

    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Disabled Customer")
    .fetch_one(&pool)
    .await
    .unwrap();

    let conversation_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let response = send(
        &state,
        auth_post_empty(
            &format!("/api/v1/tenant/conversations/{conversation_id}/summary"),
            disabled_user,
            tenant_id,
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
