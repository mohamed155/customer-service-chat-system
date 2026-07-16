use std::sync::Arc;
use std::time::Duration;

use ai::crypto::{self, MasterKey};
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use futures::StreamExt;
use http_body_util::BodyExt;
use server::router;
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
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
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
    }
}

fn plain_state(pool: sqlx::PgPool) -> AppState {
    let cfg = test_config();
    AppState {
        config: Arc::new(cfg.clone()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(1)),
        ai: ai::AiService::from_config(pool, &cfg).unwrap(),
    }
}

fn wiremock_state(
    pool: sqlx::PgPool,
    openai_uri: &str,
    anthropic_uri: &str,
    gemini_uri: &str,
) -> AppState {
    let mut cfg = test_config();
    cfg.ai_openai_base_url = Some(openai_uri.to_string());
    cfg.ai_anthropic_base_url = Some(anthropic_uri.to_string());
    cfg.ai_gemini_base_url = Some(gemini_uri.to_string());
    AppState {
        config: Arc::new(cfg.clone()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(1)),
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
            eprintln!("skipping ai live tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping ai live tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query(
        "TRUNCATE TABLE ai_usage_records, ai_credentials, ai_configurations, \
         escalations, agent_availability, agent_skills, skills, \
         messages, customer_channel_identifiers, customers, conversations, \
         outbox_events, audit_logs, tenant_invitations, tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("failed to reset test tables");
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    let slug = format!("ai-tenant-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("AI Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
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
    master: &MasterKey,
) {
    let aad = crypto::aad(tenant_id, provider);
    let (ciphertext, nonce) = crypto::seal(master, &aad, api_key).unwrap();
    let hint = crypto::hint(api_key);
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

fn master_key() -> MasterKey {
    MasterKey::from_base64("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=").unwrap()
}

fn openai_response(content: &str) -> serde_json::Value {
    serde_json::json!({
        "choices": [{"message": {"content": content}, "finish_reason": "stop"}],
        "model": "gpt-4",
        "usage": {"prompt_tokens": 10, "completion_tokens": 5}
    })
}

fn anthropic_response(content: &str) -> serde_json::Value {
    serde_json::json!({
        "content": [{"text": content, "type": "text"}],
        "id": "msg_01",
        "model": "claude-sonnet-4-20250514",
        "role": "assistant",
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 12, "output_tokens": 5}
    })
}

#[allow(dead_code)]
fn gemini_response(content: &str) -> serde_json::Value {
    serde_json::json!({
        "candidates": [{"content": {"parts": [{"text": content}]}, "finishReason": "STOP"}],
        "usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 8}
    })
}

fn ai_input() -> ai::AiInput {
    ai::AiInput {
        system: Some("You are a helpful assistant.".into()),
        messages: vec![ai::Message {
            role: ai::Role::User,
            content: "Hello".into(),
        }],
    }
}

async fn mock_openai(mock: &MockServer, api_key: &str, body: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", format!("Bearer {}", api_key)))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(mock)
        .await;
}

async fn mock_anthropic(mock: &MockServer, api_key: &str, body: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", api_key))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(mock)
        .await;
}

#[allow(dead_code)]
async fn mock_gemini(mock: &MockServer, api_key: &str, body: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-2.0-flash:generateContent"))
        .and(header("x-goog-api-key", api_key))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(mock)
        .await;
}

async fn mock_openai_error(mock: &MockServer, status: u16) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(status))
        .mount(mock)
        .await;
}

async fn mock_anthropic_error(mock: &MockServer, status: u16) {
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(status))
        .mount(mock)
        .await;
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn basic_tenant_served_by_openai() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;
    let master = master_key();
    seed_ai_config(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([]),
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-test-key", &master).await;
    mock_openai(
        &openai_mock,
        "sk-test-key",
        openai_response("Hello, world!"),
    )
    .await;

    let result = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: None,
            },
            ai_input(),
        )
        .await
        .unwrap();

    assert_eq!(result.content, "Hello, world!");
    assert_eq!(result.provider, "openai");
    assert_eq!(result.model, "gpt-4");
    assert_eq!(result.usage.input, Some(10));
    assert_eq!(result.usage.output, Some(5));
}

#[tokio::test]
async fn two_tenants_served_by_different_vendors() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let anthropic_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), &anthropic_mock.uri(), "");

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let master = master_key();

    seed_ai_config(
        &pool,
        Some(tenant_a),
        "openai",
        "gpt-4",
        serde_json::json!([]),
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_a), "openai", "sk-tenant-a", &master).await;
    seed_ai_config(
        &pool,
        Some(tenant_b),
        "anthropic",
        "claude-sonnet-4-20250514",
        serde_json::json!([]),
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_b), "anthropic", "sk-tenant-b", &master).await;

    mock_openai(&openai_mock, "sk-tenant-a", openai_response("openai-reply")).await;
    mock_anthropic(
        &anthropic_mock,
        "sk-tenant-b",
        anthropic_response("anthropic-reply"),
    )
    .await;

    let input = ai_input();

    let result_a = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id: tenant_a,
                request_id: None,
            },
            input.clone(),
        )
        .await
        .unwrap();
    assert_eq!(result_a.content, "openai-reply");
    assert_eq!(result_a.provider, "openai");

    let result_b = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id: tenant_b,
                request_id: None,
            },
            input,
        )
        .await
        .unwrap();
    assert_eq!(result_b.content, "anthropic-reply");
    assert_eq!(result_b.provider, "anthropic");

    let openai_requests = openai_mock.received_requests().await.unwrap();
    let anthropic_requests = anthropic_mock.received_requests().await.unwrap();
    assert_eq!(openai_requests.len(), 1);
    assert_eq!(anthropic_requests.len(), 1);
}

#[tokio::test]
async fn precedence_tenant_config_with_tenant_key() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;
    let master = master_key();

    seed_ai_config(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([]),
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-tenant-key", &master).await;

    mock_openai(&openai_mock, "sk-tenant-key", openai_response("tenant-key")).await;

    let result = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: None,
            },
            ai_input(),
        )
        .await
        .unwrap();
    assert_eq!(result.content, "tenant-key");
}

#[tokio::test]
async fn precedence_tenant_config_platform_key() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;
    let master = master_key();

    seed_ai_config(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([]),
    )
    .await;
    seed_ai_credential(&pool, None, "openai", "sk-platform-key", &master).await;

    mock_openai(
        &openai_mock,
        "sk-platform-key",
        openai_response("platform-key"),
    )
    .await;

    let result = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: None,
            },
            ai_input(),
        )
        .await
        .unwrap();
    assert_eq!(result.content, "platform-key");
}

#[tokio::test]
async fn precedence_platform_config_platform_key() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;
    let master = master_key();

    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-platform-key", &master).await;

    mock_openai(
        &openai_mock,
        "sk-platform-key",
        openai_response("platform-cfg"),
    )
    .await;

    let result = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: None,
            },
            ai_input(),
        )
        .await
        .unwrap();
    assert_eq!(result.content, "platform-cfg");
}

#[tokio::test]
async fn precedence_no_config_returns_not_configured() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;

    let err = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: None,
            },
            ai_input(),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, ai::AiCallError::NotConfigured));
}

#[tokio::test]
async fn config_present_no_credentials_returns_not_configured() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;

    seed_ai_config(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([]),
    )
    .await;

    let err = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: None,
            },
            ai_input(),
        )
        .await
        .unwrap_err();

    assert!(matches!(err, ai::AiCallError::NotConfigured));
}

#[tokio::test]
async fn failover_primary_down_uses_fallback() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let anthropic_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), &anthropic_mock.uri(), "");

    let tenant_id = seed_tenant(&pool).await;
    let master = master_key();

    seed_ai_config(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([{"provider": "anthropic", "model": "claude-sonnet-4-20250514"}]),
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-oa", &master).await;
    seed_ai_credential(&pool, Some(tenant_id), "anthropic", "sk-an", &master).await;

    mock_openai_error(&openai_mock, 503).await;
    mock_anthropic(
        &anthropic_mock,
        "sk-an",
        anthropic_response("fallback-served"),
    )
    .await;

    let result = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: None,
            },
            ai_input(),
        )
        .await
        .unwrap();

    assert_eq!(result.content, "fallback-served");
    assert_eq!(result.provider, "anthropic");

    let openai_requests = openai_mock.received_requests().await.unwrap();
    let anthropic_requests = anthropic_mock.received_requests().await.unwrap();
    assert_eq!(openai_requests.len(), 3); // 1 initial + 2 retries
    assert_eq!(anthropic_requests.len(), 1);
}

#[tokio::test]
async fn all_providers_down_returns_last_error() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let anthropic_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), &anthropic_mock.uri(), "");

    let tenant_id = seed_tenant(&pool).await;
    let master = master_key();

    seed_ai_config(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([{"provider": "anthropic", "model": "claude-sonnet-4-20250514"}]),
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-oa", &master).await;
    seed_ai_credential(&pool, Some(tenant_id), "anthropic", "sk-an", &master).await;

    mock_openai_error(&openai_mock, 503).await;
    mock_anthropic_error(&anthropic_mock, 502).await;

    let err = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: None,
            },
            ai_input(),
        )
        .await
        .unwrap_err();

    match err {
        ai::AiCallError::Provider {
            category, provider, ..
        } => {
            assert_eq!(provider, "anthropic");
            assert!(matches!(category, ai::ErrorCategory::Unavailable));
        }
        _ => panic!("expected Provider error"),
    }
}

#[tokio::test]
async fn authentication_error_aborts_without_fallback() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let anthropic_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), &anthropic_mock.uri(), "");

    let tenant_id = seed_tenant(&pool).await;
    let master = master_key();

    seed_ai_config(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([{"provider": "anthropic", "model": "claude-sonnet-4-20250514"}]),
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-bad", &master).await;
    seed_ai_credential(&pool, Some(tenant_id), "anthropic", "sk-an", &master).await;

    mock_openai_error(&openai_mock, 401).await;
    mock_anthropic(
        &anthropic_mock,
        "sk-an",
        anthropic_response("should-not-be-called"),
    )
    .await;

    let err = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: None,
            },
            ai_input(),
        )
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        ai::AiCallError::Provider {
            category: ai::ErrorCategory::Authentication,
            ..
        }
    ));

    let anthropic_requests = anthropic_mock.received_requests().await.unwrap();
    assert_eq!(anthropic_requests.len(), 0);
}

// ── HTTP Helpers ───────────────────────────────────────────────────────────

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

fn json_delete(uri: &str, user_id: Uuid, tenant_id: Uuid) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(Method::DELETE)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .body(Body::empty())
        .unwrap()
}

fn auth_get(uri: &str, user_id: Uuid, tenant_id: Uuid) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(Method::GET)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .body(Body::empty())
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str, _role: &str) -> Uuid {
    let user_id: Uuid =
        sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
            .bind(email)
            .bind("AI Test User")
            .fetch_one(pool)
            .await
            .unwrap();
    user_id
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

// ── US2: Config CRUD ──────────────────────────────────────────────────────

#[tokio::test]
async fn put_tenant_config_returns_200_with_view() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "ai-config-put@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let payload = serde_json::json!({
        "provider": "openai",
        "model": "gpt-4",
        "max_output_tokens": 2000,
        "temperature": 0.5,
        "fallbacks": [{"provider": "anthropic", "model": "claude-sonnet-4-20250514"}],
        "capture_content": false,
    });

    let response = send(
        &plain_state(pool.clone()),
        json_put("/api/v1/tenant/ai/config", user_id, tenant_id, payload),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["scope"], "tenant");
    assert_eq!(json["provider"], "openai");
    assert_eq!(json["model"], "gpt-4");
    assert_eq!(json["max_output_tokens"], 2000);
    assert_eq!(json["temperature"], 0.5);
    assert_eq!(json["fallbacks"][0]["provider"], "anthropic");
    assert!(json["credential"].is_null());
}

#[tokio::test]
async fn get_tenant_config_falls_back_to_platform_default() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "ai-config-get@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    // Platform config exists, tenant has none
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;

    let response = send(
        &plain_state(pool.clone()),
        auth_get("/api/v1/tenant/ai/config", user_id, tenant_id),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["scope"], "platform_default");
    assert_eq!(json["provider"], "openai");
}

#[tokio::test]
async fn get_tenant_config_none_returns_404() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "ai-config-404@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let response = send(
        &plain_state(pool.clone()),
        auth_get("/api/v1/tenant/ai/config", user_id, tenant_id),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let json = body_json(response).await;
    assert_eq!(json["error"]["code"], "not_found");
}

#[tokio::test]
async fn put_tenant_config_validation_rejects_bad_provider() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "ai-val-prov@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let payload = serde_json::json!({"provider": "unknown-ai", "model": "gpt-4"});
    let response = send(
        &plain_state(pool.clone()),
        json_put("/api/v1/tenant/ai/config", user_id, tenant_id, payload),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn put_tenant_config_validation_rejects_bad_temperature() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "ai-val-temp@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let payload = serde_json::json!({"provider": "openai", "model": "gpt-4", "temperature": 3.0});
    let response = send(
        &plain_state(pool.clone()),
        json_put("/api/v1/tenant/ai/config", user_id, tenant_id, payload),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn put_tenant_config_validation_rejects_too_many_fallbacks() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "ai-val-fb@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let payload = serde_json::json!({
        "provider": "openai",
        "model": "gpt-4",
        "fallbacks": [
            {"provider": "anthropic", "model": "a"},
            {"provider": "anthropic", "model": "b"},
            {"provider": "anthropic", "model": "c"},
            {"provider": "anthropic", "model": "d"},
        ],
    });
    let response = send(
        &plain_state(pool.clone()),
        json_put("/api/v1/tenant/ai/config", user_id, tenant_id, payload),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn delete_tenant_config_reverts_to_platform_default() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "ai-del@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    // Platform default exists
    seed_ai_config(
        &pool,
        None,
        "anthropic",
        "claude-sonnet-4-20250514",
        serde_json::json!([]),
    )
    .await;

    // PUT tenant override
    let put_payload = serde_json::json!({"provider": "openai", "model": "gpt-4"});
    let put_resp = send(
        &plain_state(pool.clone()),
        json_put("/api/v1/tenant/ai/config", user_id, tenant_id, put_payload),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::OK);

    // DELETE override
    let del_resp = send(
        &plain_state(pool.clone()),
        json_delete("/api/v1/tenant/ai/config", user_id, tenant_id),
    )
    .await;
    assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);

    // GET falls back to platform default
    let get_resp = send(
        &plain_state(pool.clone()),
        auth_get("/api/v1/tenant/ai/config", user_id, tenant_id),
    )
    .await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let json = body_json(get_resp).await;
    assert_eq!(json["scope"], "platform_default");
    assert_eq!(json["provider"], "anthropic");
}

#[tokio::test]
async fn config_only_switch_changes_provider() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let anthropic_mock = MockServer::start().await;

    // Use wiremock_state so AiService has the mock URIs
    let custom_state = wiremock_state(pool.clone(), &openai_mock.uri(), &anthropic_mock.uri(), "");

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "ai-switch@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    // Seed both credentials upfront so switching config doesn't need DB changes
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-oa", &master).await;
    seed_ai_credential(&pool, Some(tenant_id), "anthropic", "sk-an", &master).await;

    // PUT config → openai
    let put1 = serde_json::json!({"provider": "openai", "model": "gpt-4"});
    let resp1 = send(
        &custom_state,
        json_put("/api/v1/tenant/ai/config", user_id, tenant_id, put1),
    )
    .await;
    assert_eq!(resp1.status(), StatusCode::OK);

    mock_openai(&openai_mock, "sk-oa", openai_response("openai-says")).await;
    mock_anthropic(
        &anthropic_mock,
        "sk-an",
        anthropic_response("anthropic-says"),
    )
    .await;

    let result1 = custom_state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: None,
            },
            ai_input(),
        )
        .await
        .unwrap();
    assert_eq!(result1.content, "openai-says");
    assert_eq!(result1.provider, "openai");

    // PUT config → anthropic
    let put2 = serde_json::json!({"provider": "anthropic", "model": "claude-sonnet-4-20250514"});
    let resp2 = send(
        &custom_state,
        json_put("/api/v1/tenant/ai/config", user_id, tenant_id, put2),
    )
    .await;
    assert_eq!(resp2.status(), StatusCode::OK);

    let result2 = custom_state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: None,
            },
            ai_input(),
        )
        .await
        .unwrap();
    assert_eq!(result2.content, "anthropic-says");
    assert_eq!(result2.provider, "anthropic");
}

#[tokio::test]
async fn rbac_denies_put_without_ai_agent_manage() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    // "agent" role has conversations.manage but NOT ai_agent.manage
    let user_id = seed_user(&pool, "ai-rbac-deny@test.com", "agent").await;
    seed_membership(&pool, tenant_id, user_id, "agent").await;

    let payload = serde_json::json!({"provider": "openai", "model": "gpt-4"});
    let response = send(
        &plain_state(pool.clone()),
        json_put("/api/v1/tenant/ai/config", user_id, tenant_id, payload),
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn cross_tenant_isolation() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let user_a = seed_user(&pool, "ai-cross-a@test.com", "admin").await;
    let user_b = seed_user(&pool, "ai-cross-b@test.com", "admin").await;
    seed_membership(&pool, tenant_a, user_a, "admin").await;
    seed_membership(&pool, tenant_b, user_b, "admin").await;

    // Configure tenant A
    let payload_a = serde_json::json!({"provider": "openai", "model": "gpt-4"});
    let resp_a = send(
        &plain_state(pool.clone()),
        json_put("/api/v1/tenant/ai/config", user_a, tenant_a, payload_a),
    )
    .await;
    assert_eq!(resp_a.status(), StatusCode::OK);

    // Tenant B's GET should not see tenant A's config
    let resp_b = send(
        &plain_state(pool.clone()),
        auth_get("/api/v1/tenant/ai/config", user_b, tenant_b),
    )
    .await;
    assert_eq!(resp_b.status(), StatusCode::NOT_FOUND);
}

// ── Additional Helpers ─────────────────────────────────────────────────────

#[allow(dead_code)]
fn app_state(pool: sqlx::PgPool, config: config::AppConfig) -> AppState {
    let cfg = Arc::new(config);
    AppState {
        config: Arc::clone(&cfg),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(1)),
        ai: ai::AiService::from_config(pool, &cfg).unwrap(),
    }
}

async fn seed_ai_config_capture(
    pool: &sqlx::PgPool,
    tenant_id: Option<Uuid>,
    provider: &str,
    model: &str,
    fallbacks: serde_json::Value,
    capture_content: bool,
) {
    sqlx::query(
        "INSERT INTO ai_configurations (tenant_id, provider, model, fallbacks, capture_content) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(tenant_id)
    .bind(provider)
    .bind(model)
    .bind(fallbacks)
    .bind(capture_content)
    .execute(pool)
    .await
    .unwrap();
}

async fn mock_openai_stream(mock: &MockServer, api_key: &str, sse_body: Vec<u8>) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", format!("Bearer {}", api_key)))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Type", "text/event-stream")
                .set_body_raw(sse_body, "text/event-stream"),
        )
        .mount(mock)
        .await;
}

fn json_post(uri: &str, user_id: Uuid, tenant_id: Uuid) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(Method::POST)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .body(Body::empty())
        .unwrap()
}

// ── T033: US3 Credential CRUD + Connectivity ──────────────────────────────

#[tokio::test]
async fn put_tenant_credential_returns_key_hint() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "cred-put@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    let payload = serde_json::json!({"api_key": "sk-test-key-12345"});
    let resp = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/credentials/openai",
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["key_hint"], "2345");
    assert_eq!(json["source"], "tenant");
    assert_eq!(json["provider"], "openai");

    let row: Option<(Vec<u8>, Vec<u8>, String)> = sqlx::query_as(
        "SELECT ciphertext, nonce, key_hint FROM ai_credentials \
         WHERE tenant_id = $1 AND provider = 'openai' AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (ciphertext, nonce, hint) = row.expect("credential row should exist");
    assert!(!ciphertext.is_empty(), "ciphertext should not be empty");
    assert!(!nonce.is_empty(), "nonce should not be empty");
    assert_eq!(hint, "2345");
    assert_ne!(
        String::from_utf8_lossy(&ciphertext),
        "sk-test-key-12345",
        "ciphertext must not equal plaintext"
    );
}

#[tokio::test]
async fn masking_on_read_paths() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "cred-mask@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    // Seed platform config so GET returns 200
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;

    let state = plain_state(pool.clone());

    // PUT credential
    let payload = serde_json::json!({"api_key": "sk-secret-9999"});
    let put_resp = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/credentials/openai",
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::OK);

    // GET config should show only the hint, not the full key
    let get_resp = send(
        &state,
        auth_get("/api/v1/tenant/ai/config", user_id, tenant_id),
    )
    .await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let config_json = body_json(get_resp).await;
    let cred = config_json["credential"]
        .as_object()
        .expect("credential object present");
    assert_eq!(cred["key_hint"], "9999");
    assert_eq!(cred["source"], "tenant");
    assert_eq!(cred["provider"], "openai");
    assert!(
        !cred.contains_key("api_key"),
        "full key must not appear in config view"
    );

    // Audit row contains hint but not full key
    let audit: Option<(serde_json::Value,)> = sqlx::query_as(
        "SELECT details FROM audit_logs \
         WHERE action = 'ai_credential.set' AND tenant_id = $1 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let audit_details = audit.expect("audit row exists").0;
    assert_eq!(audit_details["key_hint"], "9999");
    assert_eq!(audit_details["provider"], "openai");
    assert!(
        !audit_details.to_string().contains("sk-secret-9999"),
        "audit must not contain full key"
    );
}

#[tokio::test]
async fn byok_precedence() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "byok@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    // Platform config + platform credential
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-platform-key", &master).await;
    // Tenant credential (BYOK)
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-tenant-key", &master).await;

    // Two mocks: one for tenant key, one for platform key
    mock_openai(&openai_mock, "sk-tenant-key", openai_response("tenant-key")).await;
    mock_openai(
        &openai_mock,
        "sk-platform-key",
        openai_response("platform-key"),
    )
    .await;

    // Tenant key should take precedence
    let result = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: None,
            },
            ai_input(),
        )
        .await
        .unwrap();
    assert_eq!(result.content, "tenant-key");
    assert_eq!(result.provider, "openai");

    // Delete tenant credential — next call should use platform key
    let del_resp = send(
        &state,
        json_delete("/api/v1/tenant/ai/credentials/openai", user_id, tenant_id),
    )
    .await;
    assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);

    let result2 = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: None,
            },
            ai_input(),
        )
        .await
        .unwrap();
    assert_eq!(result2.content, "platform-key");
}

#[tokio::test]
async fn rotation_uses_new_key() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "rotation@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    // Config + first credential
    seed_ai_config(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([]),
    )
    .await;

    // PUT first key via HTTP
    let put1 = serde_json::json!({"api_key": "sk-first-key-v1"});
    let resp1 = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/credentials/openai",
            user_id,
            tenant_id,
            put1,
        ),
    )
    .await;
    assert_eq!(resp1.status(), StatusCode::OK);

    // Audit: rotated = false (first insert)
    let audit1: Option<(serde_json::Value,)> = sqlx::query_as(
        "SELECT details FROM audit_logs \
         WHERE action = 'ai_credential.set' AND tenant_id = $1 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let details1 = audit1.expect("audit row 1").0;
    assert_eq!(details1["rotated"], false);

    // PUT second key — rotation
    let put2 = serde_json::json!({"api_key": "sk-second-key-v2"});
    let resp2 = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/credentials/openai",
            user_id,
            tenant_id,
            put2,
        ),
    )
    .await;
    assert_eq!(resp2.status(), StatusCode::OK);

    // Audit: rotated = true
    let audit2: Option<(serde_json::Value,)> = sqlx::query_as(
        "SELECT details FROM audit_logs \
         WHERE action = 'ai_credential.set' AND tenant_id = $1 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let details2 = audit2.expect("audit row 2").0;
    assert_eq!(details2["rotated"], true);

    // Call complete() — should use the new key
    mock_openai(
        &openai_mock,
        "sk-second-key-v2",
        openai_response("new-key-used"),
    )
    .await;
    let result = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: None,
            },
            ai_input(),
        )
        .await
        .unwrap();
    assert_eq!(result.content, "new-key-used");
}

#[tokio::test]
async fn delete_last_key_returns_not_configured() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let state = plain_state(pool.clone());
    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "del-last@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    // Config + credential
    seed_ai_config(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([]),
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-only-key", &master).await;

    // complete() works before delete
    let openai_mock = MockServer::start().await;
    let state_wm = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");
    mock_openai(&openai_mock, "sk-only-key", openai_response("works")).await;
    let pre = state_wm
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: None,
            },
            ai_input(),
        )
        .await
        .unwrap();
    assert_eq!(pre.content, "works");

    // Delete credential
    let del_resp = send(
        &state,
        json_delete("/api/v1/tenant/ai/credentials/openai", user_id, tenant_id),
    )
    .await;
    assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);

    // Now complete should fail with NotConfigured
    let err = state_wm
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: None,
            },
            ai_input(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ai::AiCallError::NotConfigured));
}

#[tokio::test]
async fn connectivity_test_success() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "conn-ok@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    seed_ai_config(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([]),
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-conn-test", &master).await;
    mock_openai(&openai_mock, "sk-conn-test", openai_response("pong")).await;

    // POST /.../config/test
    let resp = send(
        &state,
        json_post("/api/v1/tenant/ai/config/test", user_id, tenant_id),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["ok"], true);
    assert_eq!(json["provider"], "openai");
    assert_eq!(json["model"], "gpt-4");
    assert!(json["latency_ms"].as_u64().is_some());

    // Zero usage rows written
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ai_usage_records WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "connectivity test must not write usage rows");
}

#[tokio::test]
async fn connectivity_test_auth_failure() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "conn-auth@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    seed_ai_config(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([]),
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-bad-key", &master).await;
    mock_openai_error(&openai_mock, 401).await;

    let resp = send(
        &state,
        json_post("/api/v1/tenant/ai/config/test", user_id, tenant_id),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let json = body_json(resp).await;
    assert_eq!(json["ok"], false);
    assert_eq!(json["error_category"], "authentication");
    let detail = json["detail"].as_str().unwrap_or("");
    assert!(
        !detail.contains("sk-bad-key"),
        "error detail must not leak the api key: {}",
        detail
    );
}

#[tokio::test]
async fn cross_tenant_credential_isolation() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let user_a = seed_user(&pool, "iso-a@test.com", "admin").await;
    let user_b = seed_user(&pool, "iso-b@test.com", "admin").await;
    seed_membership(&pool, tenant_a, user_a, "admin").await;
    seed_membership(&pool, tenant_b, user_b, "admin").await;

    // Platform config so both tenants can GET config
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;

    let state = plain_state(pool.clone());

    // Tenant A puts credential
    let payload_a = serde_json::json!({"api_key": "sk-tenant-a-only"});
    let put_a = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/credentials/openai",
            user_a,
            tenant_a,
            payload_a,
        ),
    )
    .await;
    assert_eq!(put_a.status(), StatusCode::OK);

    // Tenant B's GET config should have null credential
    let get_b = send(
        &state,
        auth_get("/api/v1/tenant/ai/config", user_b, tenant_b),
    )
    .await;
    assert_eq!(get_b.status(), StatusCode::OK);
    let json_b = body_json(get_b).await;
    assert!(
        json_b.get("credential").is_none() || json_b["credential"].is_null(),
        "tenant B must NOT see tenant A's credential"
    );
}

// ── T038: US4 Usage Recording + Capture ───────────────────────────────────

#[tokio::test]
#[allow(clippy::type_complexity)]
async fn usage_recording_success() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;
    let master = master_key();
    seed_ai_config(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([]),
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-usage-ok", &master).await;
    mock_openai(&openai_mock, "sk-usage-ok", openai_response("usage-test")).await;

    let request_id = "usage-test-rid";
    let result = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: Some(request_id.into()),
            },
            ai_input(),
        )
        .await
        .unwrap();
    assert_eq!(result.content, "usage-test");

    // Query usage row
    let rows: Vec<(
        String,
        String,
        Option<i32>,
        Option<i32>,
        String,
        bool,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT provider, model, input_tokens, output_tokens, status, streamed, request_id \
             FROM ai_usage_records WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1, "exactly one usage row");
    let (provider, model, inp, out, status, streamed, rid) = &rows[0];
    assert_eq!(provider, "openai");
    assert_eq!(model, "gpt-4");
    assert_eq!(*inp, Some(10));
    assert_eq!(*out, Some(5));
    assert_eq!(status, "success");
    assert!(!streamed);
    assert_eq!(rid.as_deref(), Some(request_id));
}

#[tokio::test]
#[allow(clippy::type_complexity)]
async fn usage_recording_failure() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let anthropic_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), &anthropic_mock.uri(), "");

    let tenant_id = seed_tenant(&pool).await;
    let master = master_key();
    seed_ai_config(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([{"provider": "anthropic", "model": "claude-sonnet-4-20250514"}]),
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-oa-fail", &master).await;
    seed_ai_credential(&pool, Some(tenant_id), "anthropic", "sk-an-fail", &master).await;

    // All providers down
    mock_openai_error(&openai_mock, 503).await;
    mock_anthropic_error(&anthropic_mock, 503).await;

    let _err = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: None,
            },
            ai_input(),
        )
        .await
        .unwrap_err();

    // One usage row with status=failure, last provider
    let rows: Vec<(String, Option<i32>, Option<i32>, String, Option<String>)> = sqlx::query_as(
        "SELECT provider, input_tokens, output_tokens, status, error_category \
         FROM ai_usage_records WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1, "exactly one usage row on failure");
    let (provider, inp, out, status, err_cat) = &rows[0];
    assert_eq!(
        provider, "anthropic",
        "should record last attempted provider"
    );
    assert_eq!(status, "failure");
    assert_eq!(err_cat.as_deref(), Some("unavailable"));
    assert!(inp.is_none(), "tokens must be NULL on failure");
    assert!(out.is_none(), "tokens must be NULL on failure");
}

#[tokio::test]
async fn usage_recording_no_config() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;

    // No config at all
    let err = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: None,
            },
            ai_input(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ai::AiCallError::NotConfigured));

    // Zero usage rows
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ai_usage_records WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "NotConfigured must not write usage rows");
}

#[tokio::test]
async fn usage_failover_attribution() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let anthropic_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), &anthropic_mock.uri(), "");

    let tenant_id = seed_tenant(&pool).await;
    let master = master_key();
    seed_ai_config(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([{"provider": "anthropic", "model": "claude-sonnet-4-20250514"}]),
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-oa", &master).await;
    seed_ai_credential(&pool, Some(tenant_id), "anthropic", "sk-an", &master).await;

    // Primary down, fallback serves
    mock_openai_error(&openai_mock, 503).await;
    mock_anthropic(
        &anthropic_mock,
        "sk-an",
        anthropic_response("fallback-handled"),
    )
    .await;

    let result = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: Some("failover-attribution".into()),
            },
            ai_input(),
        )
        .await
        .unwrap();
    assert_eq!(result.content, "fallback-handled");
    assert_eq!(result.provider, "anthropic");

    // Usage row should name the fallback provider/model
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT provider, model, status, request_id FROM ai_usage_records \
         WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (provider, model, status, rid) = row.expect("usage row exists");
    assert_eq!(provider, "anthropic");
    assert_eq!(model, "claude-sonnet-4-20250514");
    assert_eq!(status, "success");
    assert_eq!(rid, "failover-attribution");
}

#[tokio::test]
async fn usage_vendor_omits_tokens() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "no-tokens@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();
    seed_ai_config(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([]),
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-no-tokens", &master).await;

    // Response without usage
    let no_usage = serde_json::json!({
        "choices": [{"message": {"content": "no tokens"}, "finish_reason": "stop"}],
        "model": "gpt-4",
    });
    mock_openai(&openai_mock, "sk-no-tokens", no_usage).await;

    state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: Some("no-tokens".into()),
            },
            ai_input(),
        )
        .await
        .unwrap();

    // Tokens should be NULL
    let row: Option<(Option<i32>, Option<i32>)> = sqlx::query_as(
        "SELECT input_tokens, output_tokens FROM ai_usage_records \
         WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (inp, out) = row.expect("usage row exists");
    assert!(inp.is_none(), "input_tokens must be NULL when vendor omits");
    assert!(
        out.is_none(),
        "output_tokens must be NULL when vendor omits"
    );

    // Summary should show unreported_calls
    let summary_resp = send(
        &state,
        auth_get("/api/v1/tenant/ai/usage/summary", user_id, tenant_id),
    )
    .await;
    assert_eq!(summary_resp.status(), StatusCode::OK);
    let summary = body_json(summary_resp).await;
    assert!(
        summary["unreported_calls"].as_i64().unwrap_or(0) >= 1,
        "summary must count unreported calls when tokens are missing"
    );
}

#[tokio::test]
async fn usage_list_pagination() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let user_a = seed_user(&pool, "pag-a@test.com", "admin").await;
    let user_b = seed_user(&pool, "pag-b@test.com", "admin").await;
    seed_membership(&pool, tenant_a, user_a, "admin").await;
    seed_membership(&pool, tenant_b, user_b, "admin").await;

    // Create 3 usage rows for tenant_a, 2 for tenant_b
    for i in 0..3 {
        sqlx::query(
            "INSERT INTO ai_usage_records \
             (tenant_id, provider, model, input_tokens, output_tokens, status, streamed, latency_ms, created_at) \
             VALUES ($1, 'openai', 'gpt-4', $2, $3, 'success', false, 100, now() - interval '1 second' * $4)",
        )
        .bind(tenant_a)
        .bind(10 + i)
        .bind(5 + i)
        .bind(i)
        .execute(&pool)
        .await
        .unwrap();
    }
    for i in 0..2 {
        sqlx::query(
            "INSERT INTO ai_usage_records \
             (tenant_id, provider, model, input_tokens, output_tokens, status, streamed, latency_ms, created_at) \
             VALUES ($1, 'anthropic', 'claude', 15, 8, 'success', false, 200, now() - interval '1 second' * $2)",
        )
        .bind(tenant_b)
        .bind(i)
        .execute(&pool)
        .await
        .unwrap();
    }

    let state = plain_state(pool.clone());

    // Tenant A sees 3 rows
    let resp_a = send(
        &state,
        auth_get("/api/v1/tenant/ai/usage", user_a, tenant_a),
    )
    .await;
    assert_eq!(resp_a.status(), StatusCode::OK);
    let json_a = body_json(resp_a).await;
    assert_eq!(json_a["data"].as_array().unwrap().len(), 3);
    assert_eq!(json_a["pagination"]["has_more"], false);

    // Tenant B sees 2 rows (isolation)
    let resp_b = send(
        &state,
        auth_get("/api/v1/tenant/ai/usage", user_b, tenant_b),
    )
    .await;
    assert_eq!(resp_b.status(), StatusCode::OK);
    let json_b = body_json(resp_b).await;
    assert_eq!(json_b["data"].as_array().unwrap().len(), 2);

    // Pagination: limit=2 on tenant_a
    let resp_page = send(
        &state,
        auth_get("/api/v1/tenant/ai/usage?limit=2", user_a, tenant_a),
    )
    .await;
    assert_eq!(resp_page.status(), StatusCode::OK);
    let page1 = body_json(resp_page).await;
    assert_eq!(page1["data"].as_array().unwrap().len(), 2);
    assert_eq!(page1["pagination"]["has_more"], true);
    let cursor = page1["pagination"]["next_cursor"]
        .as_str()
        .unwrap()
        .to_string();

    // Second page
    let resp_page2 = send(
        &state,
        auth_get(
            &format!("/api/v1/tenant/ai/usage?limit=2&cursor={}", cursor),
            user_a,
            tenant_a,
        ),
    )
    .await;
    assert_eq!(resp_page2.status(), StatusCode::OK);
    let page2 = body_json(resp_page2).await;
    assert_eq!(page2["data"].as_array().unwrap().len(), 1);
    assert_eq!(page2["pagination"]["has_more"], false);
}

#[tokio::test]
async fn usage_summary_totals() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "summary@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    // Insert 3 manual rows
    sqlx::query(
        "INSERT INTO ai_usage_records \
         (tenant_id, provider, model, input_tokens, output_tokens, status, streamed, latency_ms) \
         VALUES ($1, 'openai', 'gpt-4', 10, 5, 'success', false, 100)",
    )
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_usage_records \
         (tenant_id, provider, model, input_tokens, output_tokens, status, streamed, latency_ms) \
         VALUES ($1, 'anthropic', 'claude', 20, 15, 'success', false, 200)",
    )
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ai_usage_records \
         (tenant_id, provider, model, input_tokens, output_tokens, status, streamed, latency_ms) \
         VALUES ($1, 'openai', 'gpt-4', NULL, NULL, 'failure', false, 50)",
    )
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    let state = plain_state(pool.clone());
    let resp = send(
        &state,
        auth_get("/api/v1/tenant/ai/usage/summary", user_id, tenant_id),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["calls"], 3);
    assert_eq!(json["input_tokens"], 30, "10 + 20 = 30");
    assert_eq!(json["output_tokens"], 20, "5 + 15 = 20");
    assert_eq!(
        json["unreported_calls"], 1,
        "the failure row has NULL tokens"
    );
}

#[tokio::test]
async fn usage_detail_rbac() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    // viewer role has ai_agent.view but NOT ai_agent.manage
    let viewer_id = seed_user(&pool, "detail-view@test.com", "viewer").await;
    seed_membership(&pool, tenant_id, viewer_id, "viewer").await;
    // admin role has ai_agent.manage
    let admin_id = seed_user(&pool, "detail-admin@test.com", "admin").await;
    seed_membership(&pool, tenant_id, admin_id, "admin").await;

    // Create a usage row with content fields
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_usage_records \
         (tenant_id, provider, model, input_tokens, output_tokens, status, streamed, latency_ms, \
          request_content, response_content) \
         VALUES ($1, 'openai', 'gpt-4', 10, 5, 'success', false, 100, \
          '{\"msg\":\"hello\"}'::jsonb, 'Hi there') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let state = plain_state(pool.clone());

    // viewer can list
    let list_resp = send(
        &state,
        auth_get("/api/v1/tenant/ai/usage", viewer_id, tenant_id),
    )
    .await;
    assert_eq!(list_resp.status(), StatusCode::OK);

    // viewer cannot access detail
    let detail_url = format!("/api/v1/tenant/ai/usage/{}", id);
    let detail_resp_viewer = send(&state, auth_get(&detail_url, viewer_id, tenant_id)).await;
    assert_eq!(detail_resp_viewer.status(), StatusCode::FORBIDDEN);

    // admin can access detail
    let detail_resp_admin = send(&state, auth_get(&detail_url, admin_id, tenant_id)).await;
    assert_eq!(detail_resp_admin.status(), StatusCode::OK);
    let detail = body_json(detail_resp_admin).await;
    assert_eq!(detail["data"]["provider"], "openai");
    assert_eq!(detail["data"]["input_tokens"], 10);
    assert_eq!(detail["data"]["output_tokens"], 5);
    assert_eq!(detail["data"]["status"], "success");
    assert_eq!(detail["data"]["request_content"]["msg"], "hello");
    assert_eq!(detail["data"]["response_content"], "Hi there");
}

#[tokio::test]
async fn usage_capture_off_by_default() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "cap-off@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    // seed with capture_content = false (default)
    seed_ai_config_capture(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([]),
        false,
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-cap-off", &master).await;
    mock_openai(&openai_mock, "sk-cap-off", openai_response("capture-off")).await;

    state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: Some("cap-off".into()),
            },
            ai_input(),
        )
        .await
        .unwrap();

    // Content columns should be NULL in DB
    let row: Option<(Option<serde_json::Value>, Option<String>)> = sqlx::query_as(
        "SELECT request_content, response_content FROM ai_usage_records \
         WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (req_c, resp_c) = row.expect("usage row exists");
    assert!(
        req_c.is_none(),
        "request_content must be NULL when capture is off"
    );
    assert!(
        resp_c.is_none(),
        "response_content must be NULL when capture is off"
    );

    // Detail endpoint should return nulls
    let usage_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM ai_usage_records WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let detail_resp = send(
        &state,
        auth_get(
            &format!("/api/v1/tenant/ai/usage/{}", usage_id),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(detail_resp.status(), StatusCode::OK);
    let detail = body_json(detail_resp).await;
    assert!(detail["data"]["request_content"].is_null());
    assert!(detail["data"]["response_content"].is_null());
}

#[tokio::test]
async fn usage_capture_opt_in() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "cap-on@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    // Put tenant config with capture_content: true via HTTP
    let config_payload = serde_json::json!({
        "provider": "openai",
        "model": "gpt-4",
        "capture_content": true,
    });
    let config_resp = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/config",
            user_id,
            tenant_id,
            config_payload,
        ),
    )
    .await;
    assert_eq!(config_resp.status(), StatusCode::OK);

    // Verify audit row for capture_content_changed
    let audit_cap: Option<String> = sqlx::query_scalar(
        "SELECT action FROM audit_logs \
         WHERE action = 'ai_config.capture_content_changed' AND tenant_id = $1 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(
        audit_cap.as_deref(),
        Some("ai_config.capture_content_changed")
    );

    // Credential
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-cap-on", &master).await;
    mock_openai(&openai_mock, "sk-cap-on", openai_response("capture-on")).await;

    state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: Some("cap-on".into()),
            },
            ai_input(),
        )
        .await
        .unwrap();

    // Content columns should be populated
    let row: Option<(serde_json::Value, String)> = sqlx::query_as(
        "SELECT request_content, response_content FROM ai_usage_records \
         WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (req_c, resp_c) = row.expect("usage row exists");
    assert!(req_c.is_object(), "request_content must be a JSON object");
    assert!(!resp_c.is_empty(), "response_content must be non-empty");

    // List endpoint has NO content fields
    let list_resp = send(
        &state,
        auth_get("/api/v1/tenant/ai/usage", user_id, tenant_id),
    )
    .await;
    assert_eq!(list_resp.status(), StatusCode::OK);
    let list_json = body_json(list_resp).await;
    let first = &list_json["data"][0];
    assert!(
        first.get("request_content").is_none(),
        "list must not include request_content"
    );
    assert!(
        first.get("response_content").is_none(),
        "list must not include response_content"
    );

    // Detail endpoint returns content
    let usage_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM ai_usage_records WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let detail_resp = send(
        &state,
        auth_get(
            &format!("/api/v1/tenant/ai/usage/{}", usage_id),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(detail_resp.status(), StatusCode::OK);
    let detail = body_json(detail_resp).await;
    assert!(detail["data"]["request_content"].is_object());
    assert!(!detail["data"]["response_content"]
        .as_str()
        .unwrap_or("")
        .is_empty());
}

#[tokio::test]
async fn usage_capture_does_not_retroactively_fill() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "cap-retro@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    // First call with capture off
    seed_ai_config_capture(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([]),
        false,
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-retro", &master).await;
    mock_openai(&openai_mock, "sk-retro", openai_response("first-call")).await;

    state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: Some("pre-toggle".into()),
            },
            ai_input(),
        )
        .await
        .unwrap();

    // Toggle capture on via HTTP PUT
    let toggle = serde_json::json!({
        "provider": "openai",
        "model": "gpt-4",
        "capture_content": true,
    });
    let toggle_resp = send(
        &state,
        json_put("/api/v1/tenant/ai/config", user_id, tenant_id, toggle),
    )
    .await;
    assert_eq!(toggle_resp.status(), StatusCode::OK);

    // Second call with capture on
    mock_openai(&openai_mock, "sk-retro", openai_response("second-call")).await;
    state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: Some("post-toggle".into()),
            },
            ai_input(),
        )
        .await
        .unwrap();

    // Pre-toggle row should still have NULL content
    let rows: Vec<(String, Option<serde_json::Value>, Option<String>)> = sqlx::query_as(
        "SELECT request_id, request_content, response_content FROM ai_usage_records \
         WHERE tenant_id = $1 ORDER BY created_at ASC",
    )
    .bind(tenant_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    let (rid1, req1, resp1) = &rows[0];
    assert_eq!(rid1, "pre-toggle");
    assert!(
        req1.is_none(),
        "pre-toggle row must have NULL request_content"
    );
    assert!(
        resp1.is_none(),
        "pre-toggle row must have NULL response_content"
    );

    let (rid2, req2, resp2) = &rows[1];
    assert_eq!(rid2, "post-toggle");
    assert!(req2.is_some(), "post-toggle row must have request_content");
    assert!(
        resp2.is_some(),
        "post-toggle row must have response_content"
    );
}

// ── T044: US5 SSE Streaming ───────────────────────────────────────────────

fn build_sse_success() -> Vec<u8> {
    let frames = [
        r#"{"choices":[{"delta":{"content":"Hello"},"finish_reason":null}],"model":"gpt-4"}"#,
        r#"{"choices":[{"delta":{"content":" world"},"finish_reason":null}],"model":"gpt-4"}"#,
        r#"{"choices":[{"delta":{},"finish_reason":"stop"}],"model":"gpt-4"}"#,
        r#"{"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
        "[DONE]",
    ];
    frames
        .iter()
        .map(|f| format!("data: {}\n\n", f))
        .collect::<String>()
        .into_bytes()
}

fn build_sse_interrupted() -> Vec<u8> {
    let good =
        r#"{"choices":[{"delta":{"content":"Hello"},"finish_reason":null}],"model":"gpt-4"}"#;
    format!("data: {}\n\ndata: NOT_JSON\n\n", good).into_bytes()
}

#[tokio::test]
async fn streaming_deltas_arrive_before_done() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;
    let master = master_key();
    seed_ai_config(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([]),
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-stream-delta", &master).await;
    mock_openai_stream(&openai_mock, "sk-stream-delta", build_sse_success()).await;

    let mut stream = state
        .ai
        .stream(
            ai::AiCallContext {
                tenant_id,
                request_id: Some("sse-deltas".into()),
            },
            ai_input(),
        )
        .await
        .unwrap();

    let mut deltas = Vec::new();
    let mut got_done = false;
    while let Some(event) = stream.next().await {
        match event {
            ai::AiStreamEvent::Delta(t) => deltas.push(t),
            ai::AiStreamEvent::Done(_) => {
                got_done = true;
                break;
            }
            ai::AiStreamEvent::Error { .. } => break,
        }
    }

    assert!(
        deltas.len() >= 2,
        "expected at least 2 delta events, got {}",
        deltas.len()
    );
    assert!(got_done, "expected Done event");
}

#[tokio::test]
#[allow(clippy::type_complexity)]
async fn streaming_success_writes_usage_row() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;
    let master = master_key();
    seed_ai_config(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([]),
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-stream-usage", &master).await;
    mock_openai_stream(&openai_mock, "sk-stream-usage", build_sse_success()).await;

    let mut stream = state
        .ai
        .stream(
            ai::AiCallContext {
                tenant_id,
                request_id: Some("sse-usage".into()),
            },
            ai_input(),
        )
        .await
        .unwrap();

    while let Some(event) = stream.next().await {
        if matches!(event, ai::AiStreamEvent::Done(_)) {
            break;
        }
    }

    let row: Option<(
        String,
        String,
        Option<i32>,
        Option<i32>,
        String,
        bool,
        String,
    )> = sqlx::query_as(
        "SELECT provider, model, input_tokens, output_tokens, status, streamed, request_id \
         FROM ai_usage_records WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (provider, model, inp, out, status, streamed, rid) = row.expect("usage row exists");
    assert_eq!(provider, "openai");
    assert_eq!(model, "gpt-4");
    assert_eq!(status, "success");
    assert!(streamed, "streamed must be true");
    assert_eq!(rid, "sse-usage");
    assert_eq!(inp, Some(10));
    assert_eq!(out, Some(5));
}

#[tokio::test]
async fn streaming_interrupted_mid_frame() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;
    let master = master_key();
    // Capture on so we get partial response_content
    seed_ai_config_capture(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([]),
        true,
    )
    .await;
    seed_ai_credential(&pool, Some(tenant_id), "openai", "sk-stream-int", &master).await;
    mock_openai_stream(&openai_mock, "sk-stream-int", build_sse_interrupted()).await;

    let mut stream = state
        .ai
        .stream(
            ai::AiCallContext {
                tenant_id,
                request_id: Some("sse-int".into()),
            },
            ai_input(),
        )
        .await
        .unwrap();

    let mut got_delta = false;
    let mut got_error = false;
    while let Some(event) = stream.next().await {
        match event {
            ai::AiStreamEvent::Delta(_) => got_delta = true,
            ai::AiStreamEvent::Error { .. } => got_error = true,
            ai::AiStreamEvent::Done(_) => {}
        }
    }

    assert!(got_delta, "expected at least one delta before interruption");
    assert!(got_error, "expected error after interrupted stream");

    // Usage row should show failure with partial content
    let row: Option<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT status, response_content, request_id FROM ai_usage_records \
         WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (status, resp_c, rid) = row.expect("usage row exists");
    assert_eq!(status, "failure");
    assert_eq!(rid, "sse-int");
    assert!(
        resp_c.as_deref().unwrap_or("").contains("Hello"),
        "partial response_content must contain 'Hello', got: {:?}",
        resp_c
    );
}

#[tokio::test]
async fn streaming_capture_off() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri(), "", "");

    let tenant_id = seed_tenant(&pool).await;
    let master = master_key();
    seed_ai_config_capture(
        &pool,
        Some(tenant_id),
        "openai",
        "gpt-4",
        serde_json::json!([]),
        false,
    )
    .await;
    seed_ai_credential(
        &pool,
        Some(tenant_id),
        "openai",
        "sk-stream-capoff",
        &master,
    )
    .await;
    mock_openai_stream(&openai_mock, "sk-stream-capoff", build_sse_success()).await;

    let mut stream = state
        .ai
        .stream(
            ai::AiCallContext {
                tenant_id,
                request_id: Some("sse-capoff".into()),
            },
            ai_input(),
        )
        .await
        .unwrap();

    while let Some(event) = stream.next().await {
        if matches!(event, ai::AiStreamEvent::Done(_)) {
            break;
        }
    }

    // Content columns should be NULL
    let row: Option<(Option<serde_json::Value>, Option<String>)> = sqlx::query_as(
        "SELECT request_content, response_content FROM ai_usage_records \
         WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (req_c, resp_c) = row.expect("usage row exists");
    assert!(
        req_c.is_none(),
        "request_content must be NULL when capture off"
    );
    assert!(
        resp_c.is_none(),
        "response_content must be NULL when capture off"
    );
}

// ── T046: SC-001 Live Vendor Smoke Test ────────────────────────────────────

#[tokio::test]
#[ignore = "live vendor test; needs LIVE_AI_OPENAI_KEY"]
async fn live_vendor() {
    let api_key = std::env::var("LIVE_AI_OPENAI_KEY").unwrap_or_default();
    if api_key.is_empty() {
        eprintln!("SKIP: LIVE_AI_OPENAI_KEY not set");
        return;
    }
    let model = std::env::var("LIVE_AI_OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());

    let pool = get_pool()
        .await
        .expect("live vendor test needs a database (DATABASE_URL)");
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let state = plain_state(pool.clone());

    // Seed platform config
    seed_ai_config(&pool, None, "openai", &model, serde_json::json!([])).await;
    // Seed platform credential with the live key via raw SQL
    let master_key =
        ai::crypto::MasterKey::from_base64("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=").unwrap();
    let (ciphertext, nonce) =
        ai::crypto::seal(&master_key, &ai::crypto::aad(None, "openai"), &api_key).unwrap();
    let hint = ai::crypto::hint(&api_key);
    sqlx::query(
        "INSERT INTO ai_credentials (tenant_id, provider, ciphertext, nonce, key_hint) VALUES (NULL, 'openai', $1, $2, $3)"
    )
    .bind(&ciphertext)
    .bind(&nonce)
    .bind(&hint)
    .execute(&pool)
    .await
    .unwrap();

    let result = state
        .ai
        .complete(
            ai::AiCallContext {
                tenant_id,
                request_id: Some("live-vendor-test".into()),
            },
            ai::AiInput {
                system: None,
                messages: vec![ai::Message {
                    role: ai::Role::User,
                    content: "Say hello in one word".into(),
                }],
            },
        )
        .await
        .unwrap();

    assert!(
        !result.content.is_empty(),
        "live vendor returned empty content"
    );
    assert_eq!(result.provider, "openai");
    assert!(
        result.usage.input.is_some() || result.usage.output.is_some(),
        "expected at least one token count from live vendor"
    );
}
