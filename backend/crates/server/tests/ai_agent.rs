use std::sync::Arc;
use std::time::Duration;

use ai::agent_responder::process_agent_responder_once;
use ai::agent_rules::BASELINE_ESCALATION_REASON;
use ai::crypto::{self, MasterKey};
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
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
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
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

fn wiremock_state(pool: sqlx::PgPool, openai_uri: &str) -> AppState {
    let mut cfg = test_config();
    cfg.ai_openai_base_url = Some(openai_uri.to_string());
    AppState {
        config: Arc::new(cfg.clone()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(1)),
        ai: ai::AiService::from_config(pool, &cfg).unwrap(),
    }
}

fn wiremock_anthropic_state(pool: sqlx::PgPool, anthropic_uri: &str) -> AppState {
    let mut cfg = test_config();
    cfg.ai_anthropic_base_url = Some(anthropic_uri.to_string());
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
            eprintln!("skipping ai agent tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping ai agent tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query(
        "TRUNCATE TABLE ai_usage_records, ai_credentials, ai_configurations, \
         agent_configurations, agent_avatar_uploads, \
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
    let slug = format!("agent-tenant-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("AI Agent Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str, _role: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("AI Agent Test User")
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

async fn mock_openai(mock: &MockServer, api_key: &str, body: serde_json::Value) {
    Mock::given(wm_method("POST"))
        .and(wm_path("/v1/chat/completions"))
        .and(wm_header("Authorization", format!("Bearer {}", api_key)))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(mock)
        .await;
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

fn auth_get(uri: &str, user_id: Uuid, tenant_id: Uuid) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(Method::GET)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .body(Body::empty())
        .unwrap()
}

fn anthropic_response(content: &str) -> serde_json::Value {
    serde_json::json!({
        "content": [{"text": content}],
        "model": "claude-sonnet-5",
        "usage": {"input_tokens": 10, "output_tokens": 5},
        "stop_reason": "end_turn"
    })
}

async fn mock_anthropic(mock: &MockServer, api_key: &str, body: serde_json::Value) {
    Mock::given(wm_method("POST"))
        .and(wm_path("/v1/messages"))
        .and(wm_header("x-api-key", api_key))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(mock)
        .await;
}

fn raw_put(
    uri: &str,
    user_id: Uuid,
    tenant_id: Uuid,
    content_type: &str,
    body: Vec<u8>,
) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(Method::PUT)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(body))
        .unwrap()
}

async fn seed_skill(pool: &sqlx::PgPool, tenant_id: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO skills (tenant_id, name) VALUES ($1, $2) RETURNING id")
        .bind(tenant_id)
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

// A minimal 1x1 white PNG
fn small_png() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x60,
        0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0x98, 0xA2, 0x3F, 0x1D, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

fn oversized_body() -> Vec<u8> {
    vec![0u8; 262_145]
}

fn json_post(uri: &str, user_id: Uuid, tenant_id: Uuid, body: serde_json::Value) -> Request<Body> {
    let bytes = serde_json::to_vec(&body).unwrap();
    Request::builder()
        .uri(uri)
        .method(Method::POST)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("content-type", "application/json")
        .body(Body::from(bytes))
        .unwrap()
}

// ═══════════════════════════════════════════════════════════════════════════════
// T024 — GET returns editable defaults when unconfigured
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn get_returns_editable_defaults_when_unconfigured() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t024@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());
    let response = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent", user_id, tenant_id),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["configured"], false);

    let agent = &json["agent"];
    assert_eq!(agent["name"], "AI Assistant");
    assert_eq!(agent["tone"], "professional");
    assert_eq!(agent["enabled_channels"], serde_json::json!(["web_chat"]));
    assert_eq!(agent["is_default"], true);
    assert!(agent["version"].is_null());
    assert!(agent["updated_at"].is_null());
    assert_eq!(agent["avatar"]["kind"], "preset");
    assert_eq!(agent["avatar"]["preset"], "spark");
    assert!(agent["upload_url"].is_null() || agent.get("upload_url").is_none());

    let provider = &agent["provider_selection"];
    assert!(provider["provider"].is_null());
    assert!(provider["model"].is_null());
    assert_eq!(provider["stale"], false);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T025 — First save creates and activates agent
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn first_save_creates_and_activates_agent() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t025@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    // First save — no version, expect 201 with version 1
    let payload = serde_json::json!({
        "name": "My Agent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": null,
    });

    let resp = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent",
            user_id,
            tenant_id,
            payload.clone(),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let json = body_json(resp).await;
    assert_eq!(json["configured"], true);
    assert_eq!(json["agent"]["version"], 1);
    assert_eq!(json["agent"]["name"], "My Agent");

    // Second save — no version on existing row should be 409
    let resp2 = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, payload),
    )
    .await;
    assert_eq!(resp2.status(), StatusCode::CONFLICT);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T025b — Stale version conflicts without overwriting
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn stale_version_conflicts_without_overwriting() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t025b@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    // 1. First save
    let v1_payload = serde_json::json!({
        "name": "Agent V1",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": null,
    });

    let resp1 = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, v1_payload),
    )
    .await;
    assert_eq!(resp1.status(), StatusCode::CREATED);
    let json1 = body_json(resp1).await;
    assert_eq!(json1["agent"]["version"], 1);

    // 2. Update with correct version 1
    let v2_payload = serde_json::json!({
        "name": "Agent V2",
        "avatar": { "kind": "preset", "preset": "nova" },
        "tone": "friendly",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": null,
        "version": 1,
    });

    let resp2 = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, v2_payload),
    )
    .await;
    assert_eq!(resp2.status(), StatusCode::OK);
    let json2 = body_json(resp2).await;
    assert_eq!(json2["agent"]["version"], 2);
    assert_eq!(json2["agent"]["name"], "Agent V2");

    // 3. Stale version 1 should conflict
    let stale_payload = serde_json::json!({
        "name": "Agent Stale",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": null,
        "version": 1,
    });

    let resp3 = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, stale_payload),
    )
    .await;
    assert_eq!(resp3.status(), StatusCode::CONFLICT);

    // 4. GET should still show version 2's data
    let get_resp = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent", user_id, tenant_id),
    )
    .await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let get_json = body_json(get_resp).await;
    assert_eq!(get_json["agent"]["version"], 2);
    assert_eq!(get_json["agent"]["name"], "Agent V2");
}

// ═══════════════════════════════════════════════════════════════════════════════
// T026 — Save round-trips on reload
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn save_round_trips_on_reload() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t026@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    let payload = serde_json::json!({
        "name": "Roundtrip Agent",
        "avatar": { "kind": "preset", "preset": "nova" },
        "tone": "friendly",
        "business_rules": ["Be concise", "Use emoji sparingly"],
        "escalation_rules": [
            {
                "name": "Refund Request",
                "trigger": "topic_keywords",
                "keywords": ["refund", "money back"],
                "required_skill_ids": []
            }
        ],
        "enabled_channels": ["web_chat", "email"],
        "provider_selection": null,
    });

    let put_resp = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent",
            user_id,
            tenant_id,
            payload.clone(),
        ),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::CREATED);

    let get_resp = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent", user_id, tenant_id),
    )
    .await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let json = body_json(get_resp).await;

    assert_eq!(json["configured"], true);
    assert_eq!(json["agent"]["name"], "Roundtrip Agent");
    assert_eq!(json["agent"]["tone"], "friendly");
    assert_eq!(
        json["agent"]["business_rules"],
        serde_json::json!(["Be concise", "Use emoji sparingly"])
    );
    assert_eq!(
        json["agent"]["enabled_channels"],
        serde_json::json!(["web_chat", "email"])
    );
    assert_eq!(json["agent"]["avatar"]["kind"], "preset");
    assert_eq!(json["agent"]["avatar"]["preset"], "nova");
    assert_eq!(json["agent"]["version"], 1);
    assert!(!json["agent"]["updated_at"].is_null());

    let rules = &json["agent"]["escalation_rules"];
    assert_eq!(rules.as_array().unwrap().len(), 1);
    assert_eq!(rules[0]["name"], "Refund Request");
    assert_eq!(rules[0]["trigger"], "topic_keywords");
    assert_eq!(
        rules[0]["keywords"],
        serde_json::json!(["refund", "money back"])
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// T027 — Save rejects invalid payload atomically
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn save_rejects_invalid_payload_atomically() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t027@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    // PUT with empty name
    let invalid_payload = serde_json::json!({
        "name": "   ",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": null,
    });

    let put_resp = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent",
            user_id,
            tenant_id,
            invalid_payload,
        ),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let json = body_json(put_resp).await;
    assert_eq!(json["error"]["code"], "validation_failed");
    let details = json["error"]["details"].as_array().unwrap();
    assert!(details.iter().any(|d| d["field"] == "name"));

    // GET should show unconfigured — nothing was persisted
    let get_resp = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent", user_id, tenant_id),
    )
    .await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let get_json = body_json(get_resp).await;
    assert_eq!(get_json["configured"], false);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T028 — Cross-tenant isolation
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cross_tenant_isolation() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let user_a = seed_user(&pool, "t028-a@test.com", "admin").await;
    let user_b = seed_user(&pool, "t028-b@test.com", "admin").await;
    seed_membership(&pool, tenant_a, user_a, "admin").await;
    seed_membership(&pool, tenant_b, user_b, "admin").await;

    let state = plain_state(pool.clone());

    // Tenant A saves an agent
    let payload_a = serde_json::json!({
        "name": "Tenant-A Agent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": null,
    });

    let put_a = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_a, tenant_a, payload_a),
    )
    .await;
    assert_eq!(put_a.status(), StatusCode::CREATED);

    // Tenant B's GET shows configured: false
    let get_b = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent", user_b, tenant_b),
    )
    .await;
    assert_eq!(get_b.status(), StatusCode::OK);
    let json_b = body_json(get_b).await;
    assert_eq!(json_b["configured"], false);
    assert_eq!(json_b["agent"]["name"], "AI Assistant");
    assert_ne!(json_b["agent"]["name"], "Tenant-A Agent");
}

// ═══════════════════════════════════════════════════════════════════════════════
// T029 — Configured agent drives AI reply
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn configured_agent_drives_ai_reply() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t029@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    // Seed platform AI config + credential so the AI layer resolves
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-agent-test-key", &master).await;

    // PUT agent config with distinctive name, tone, prompt, and resolvable provider
    let agent_payload = serde_json::json!({
        "name": "AgentX",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "friendly",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": { "provider": "openai", "model": "gpt-4" },
    });

    let put_resp = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, agent_payload),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::CREATED);

    // Mock OpenAI to return a reply
    mock_openai(
        &openai_mock,
        "sk-agent-test-key",
        openai_response("AI reply here"),
    )
    .await;

    // Create a customer for the conversation
    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Test Customer")
    .fetch_one(&pool)
    .await
    .unwrap();

    // Create a conversation
    let conversation_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Insert a customer message
    let message_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'Hello, I need help') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Insert an outbox event
    let outbox_payload = serde_json::json!({
        "conversation_id": conversation_id,
        "message_id": message_id,
        "channel": "web_chat",
    });
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.customer_message', $4)",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(tenant_id)
    .bind(outbox_payload)
    .execute(&pool)
    .await
    .unwrap();

    // Process the outbox event
    let processed = process_agent_responder_once(&pool, &state.ai, &state.escalations)
        .await
        .expect("agent responder should succeed");
    assert!(processed, "agent responder should have processed an event");

    // Assert an ai-kind message was inserted
    let ai_messages: Vec<String> = sqlx::query_scalar(
        "SELECT body FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(ai_messages.len(), 1, "expected exactly one AI reply");
    assert_eq!(ai_messages[0], "AI reply here");

    // Assert the outbound request contained the configured name and tone directive
    let requests = openai_mock.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1, "expected one request to OpenAI");

    let body: serde_json::Value =
        serde_json::from_slice(&requests[0].body).expect("valid JSON body");
    let system_msg = body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "system")
        .map(|m| m["content"].as_str().unwrap_or(""))
        .unwrap_or("");

    assert!(
        system_msg.contains("AgentX"),
        "system message should contain the agent name 'AgentX', got: {}",
        system_msg
    );
    assert!(
        system_msg.contains("warm"),
        "system message should contain 'friendly' tone directive 'warm', got: {}",
        system_msg
    );
    assert!(
        system_msg.contains("Custom prompt text"),
        "system message should contain the custom prompt, got: {}",
        system_msg
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// T030 — Avatar preset select and upload
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn avatar_preset_select_and_upload() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t030@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    // 1. PUT agent config with avatar preset "spark" → succeeds
    let initial_payload = serde_json::json!({
        "name": "Avatar Agent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": null,
    });

    let resp1 = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent",
            user_id,
            tenant_id,
            initial_payload,
        ),
    )
    .await;
    assert_eq!(resp1.status(), StatusCode::CREATED);
    let json1 = body_json(resp1).await;
    assert_eq!(json1["agent"]["avatar"]["kind"], "preset");
    assert_eq!(json1["agent"]["avatar"]["preset"], "spark");
    assert_eq!(json1["agent"]["version"], 1);

    // 2. PUT avatar upload with valid PNG → succeeds, bumps version
    let png_bytes = small_png();
    let avatar_resp = send(
        &state,
        raw_put(
            "/api/v1/tenant/ai/agent/avatar",
            user_id,
            tenant_id,
            "image/png",
            png_bytes,
        ),
    )
    .await;
    assert_eq!(avatar_resp.status(), StatusCode::OK);
    let avatar_json = body_json(avatar_resp).await;
    assert_eq!(avatar_json["avatar"]["kind"], "upload");
    assert!(avatar_json["avatar"]["preset"].is_null());
    assert_eq!(avatar_json["version"], 2);

    // 3. GET avatar → serves with right Content-Type
    let get_avatar_resp = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent/avatar", user_id, tenant_id),
    )
    .await;
    assert_eq!(get_avatar_resp.status(), StatusCode::OK);
    assert_eq!(
        get_avatar_resp
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap(),
        "image/png"
    );
    let body_bytes = get_avatar_resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    assert!(!body_bytes.is_empty());

    // 4. PUT agent config with avatar preset "orbit" → succeeds
    let switch_payload = serde_json::json!({
        "name": "Avatar Agent",
        "avatar": { "kind": "preset", "preset": "orbit" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": null,
        "version": 2,
    });

    let resp_switch = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent",
            user_id,
            tenant_id,
            switch_payload,
        ),
    )
    .await;
    assert_eq!(resp_switch.status(), StatusCode::OK);
    let switch_json = body_json(resp_switch).await;
    assert_eq!(switch_json["agent"]["avatar"]["kind"], "preset");
    assert_eq!(switch_json["agent"]["avatar"]["preset"], "orbit");
    assert_eq!(switch_json["agent"]["version"], 3);

    // 5. GET avatar → 404 (upload soft-deleted when switching to preset)
    let get_after_switch = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent/avatar", user_id, tenant_id),
    )
    .await;
    assert_eq!(get_after_switch.status(), StatusCode::NOT_FOUND);

    // 6. Oversized upload → error, previous avatar unchanged
    let oversized_resp = send(
        &state,
        raw_put(
            "/api/v1/tenant/ai/agent/avatar",
            user_id,
            tenant_id,
            "image/png",
            oversized_body(),
        ),
    )
    .await;
    assert_eq!(oversized_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // Wrong content type → error
    let wrong_ct_resp = send(
        &state,
        raw_put(
            "/api/v1/tenant/ai/agent/avatar",
            user_id,
            tenant_id,
            "image/gif",
            small_png(),
        ),
    )
    .await;
    assert_eq!(wrong_ct_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // After failed upload attempts, agent config still has preset "orbit", version 3
    let get_final = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent", user_id, tenant_id),
    )
    .await;
    assert_eq!(get_final.status(), StatusCode::OK);
    let final_json = body_json(get_final).await;
    assert_eq!(final_json["agent"]["version"], 3);
    assert_eq!(final_json["agent"]["avatar"]["kind"], "preset");
    assert_eq!(final_json["agent"]["avatar"]["preset"], "orbit");
}

// ═══════════════════════════════════════════════════════════════════════════════
// T038 — Unconfigured tenant sends single auto-ack
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn unconfigured_tenant_sends_single_auto_ack() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t038@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    // Create a customer
    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("T038 Customer")
    .fetch_one(&pool)
    .await
    .unwrap();

    // Create a conversation via the API
    let create_payload = serde_json::json!({
        "customer_id": customer_id,
        "channel": "web_chat",
        "message": { "body": "Hello" },
    });
    let create_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/conversations",
            user_id,
            tenant_id,
            create_payload,
        ),
    )
    .await;
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let create_resp_body = body_json(create_resp).await;
    let conversation_id: Uuid = create_resp_body["data"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    // Post a customer message (emits outbox event)
    let msg_payload = serde_json::json!({ "kind": "customer", "body": "I need help" });
    let msg_resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/conversations/{conversation_id}/messages"),
            user_id,
            tenant_id,
            msg_payload,
        ),
    )
    .await;
    assert_eq!(msg_resp.status(), StatusCode::OK);

    // Run the responder
    let processed = process_agent_responder_once(&pool, &state.ai, &state.escalations)
        .await
        .expect("responder should succeed");
    assert!(processed, "responder should have processed an event");

    // Assert exactly one system message
    let system_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages \
         WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'system'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(system_count, 1, "expected exactly one system message");

    // Assert conversation detail shows awaiting_ai_decision: true
    let detail_resp = send(
        &state,
        auth_get(
            &format!("/api/v1/tenant/conversations/{conversation_id}"),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(detail_resp.status(), StatusCode::OK);
    let detail_json = body_json(detail_resp).await;
    assert_eq!(
        detail_json["data"]["awaiting_ai_decision"], true,
        "expected awaiting_ai_decision to be true after auto-ack"
    );

    // Post a second customer message
    let msg2_payload = serde_json::json!({ "kind": "customer", "body": "Still need help" });
    let msg2_resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/conversations/{conversation_id}/messages"),
            user_id,
            tenant_id,
            msg2_payload,
        ),
    )
    .await;
    assert_eq!(msg2_resp.status(), StatusCode::OK);

    // Run responder again
    let processed2 = process_agent_responder_once(&pool, &state.ai, &state.escalations)
        .await
        .expect("responder should succeed");
    assert!(processed2, "responder should have processed second event");

    // Assert still exactly one system message (no duplicate ack)
    let system_count2: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages \
         WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'system'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        system_count2, 1,
        "expected still exactly one system message (no duplicate ack)"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// T039 — ai_handling platform_ai requires resolvable layer
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn ai_handling_platform_ai_requires_resolvable_layer() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t039@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("T039 Customer")
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

    // Without resolvable AI config → 422
    let resp_no_config = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/conversations/{conversation_id}/ai-handling"),
            user_id,
            tenant_id,
            serde_json::json!({ "mode": "platform_ai" }),
        ),
    )
    .await;
    assert_eq!(resp_no_config.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // With resolvable AI config → 200
    let master = master_key();
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-t039-test-key", &master).await;

    let resp_with_config = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/conversations/{conversation_id}/ai-handling"),
            user_id,
            tenant_id,
            serde_json::json!({ "mode": "platform_ai" }),
        ),
    )
    .await;
    assert_eq!(resp_with_config.status(), StatusCode::OK);
    let json = body_json(resp_with_config).await;
    assert_eq!(json["data"]["ai_handling"], "platform_ai");
}

// ═══════════════════════════════════════════════════════════════════════════════
// T040 — platform_ai then AI reply
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn ai_handling_platform_ai_then_ai_reply() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t040@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    // Seed platform AI config + credential
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-t040-test-key", &master).await;

    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("T040 Customer")
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

    // Set platform_ai
    let ai_handling_resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/conversations/{conversation_id}/ai-handling"),
            user_id,
            tenant_id,
            serde_json::json!({ "mode": "platform_ai" }),
        ),
    )
    .await;
    assert_eq!(ai_handling_resp.status(), StatusCode::OK);

    // Mock OpenAI response
    mock_openai(
        &openai_mock,
        "sk-t040-test-key",
        openai_response("Platform AI reply"),
    )
    .await;

    // Post a customer message
    let msg_payload = serde_json::json!({ "kind": "customer", "body": "Hello platform AI" });
    let msg_resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/conversations/{conversation_id}/messages"),
            user_id,
            tenant_id,
            msg_payload,
        ),
    )
    .await;
    assert_eq!(msg_resp.status(), StatusCode::OK);

    // Run responder
    let processed = process_agent_responder_once(&pool, &state.ai, &state.escalations)
        .await
        .expect("responder should succeed");
    assert!(processed, "responder should have processed an event");

    // Assert an ai-kind reply was inserted
    let ai_messages: Vec<String> = sqlx::query_scalar(
        "SELECT body FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(ai_messages.len(), 1, "expected exactly one AI reply");
    assert_eq!(ai_messages[0], "Platform AI reply");

    // Assert audit log has conversation.ai_handling_set
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs \
         WHERE tenant_id = $1 AND action = 'conversation.ai_handling_set'",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 1, "expected one ai_handling_set audit log");
}

// ═══════════════════════════════════════════════════════════════════════════════
// T041 — human escalates immediately
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn ai_handling_human_escalates_immediately() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t041@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("T041 Customer")
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

    // POST ai-handling {"mode": "human"} → 200
    let resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/conversations/{conversation_id}/ai-handling"),
            user_id,
            tenant_id,
            serde_json::json!({ "mode": "human" }),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert!(
        json["data"]["escalation"].is_object(),
        "response should include an escalation reference"
    );

    // Assert escalation exists with reason "no AI agent configured"
    let esc_reason: Option<String> = sqlx::query_scalar(
        "SELECT reason FROM escalations \
         WHERE tenant_id = $1 AND conversation_id = $2",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(esc_reason.is_some(), "expected an escalation");
    assert_eq!(
        esc_reason.unwrap(),
        "no AI agent configured",
        "escalation reason should match UNCONFIGURED_ESCALATION_REASON"
    );

    // Post a customer message and run responder — should not create new message/escalation
    let msg_payload = serde_json::json!({ "kind": "customer", "body": "Human support needed" });
    let msg_resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/conversations/{conversation_id}/messages"),
            user_id,
            tenant_id,
            msg_payload,
        ),
    )
    .await;
    assert_eq!(msg_resp.status(), StatusCode::OK);

    let processed = process_agent_responder_once(&pool, &state.ai, &state.escalations)
        .await
        .expect("responder should succeed");
    assert!(processed, "responder should have processed the event");

    // Assert no new message (beyond the customer one we just posted) and no new escalation
    let system_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages \
         WHERE tenant_id = $1 AND conversation_id = $2 AND kind IN ('ai', 'system')",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        system_count, 0,
        "expected no ai or system messages after human handling"
    );

    let esc_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM escalations \
         WHERE tenant_id = $1 AND conversation_id = $2",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        esc_count, 1,
        "expected still exactly one escalation (no duplicate)"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// T042 — ai_handling rejects once agent configured
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn ai_handling_rejects_once_agent_configured() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t042@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("T042 Customer")
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

    // Save an agent config (PUT)
    let agent_payload = serde_json::json!({
        "name": "T042 Agent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": null,
    });

    let put_resp = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, agent_payload),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::CREATED);

    // POST ai-handling on any conversation → 409
    let ai_handling_resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/conversations/{conversation_id}/ai-handling"),
            user_id,
            tenant_id,
            serde_json::json!({ "mode": "platform_ai" }),
        ),
    )
    .await;
    assert_eq!(ai_handling_resp.status(), StatusCode::CONFLICT);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T043 — human to platform_ai blocked once escalated
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn ai_handling_human_to_platform_ai_blocked_once_escalated() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t043@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("T043 Customer")
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

    // Set mode "human" (creates escalation)
    let human_resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/conversations/{conversation_id}/ai-handling"),
            user_id,
            tenant_id,
            serde_json::json!({ "mode": "human" }),
        ),
    )
    .await;
    assert_eq!(human_resp.status(), StatusCode::OK);

    // Attempt to set "platform_ai" on same conversation → 409
    let platform_resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/conversations/{conversation_id}/ai-handling"),
            user_id,
            tenant_id,
            serde_json::json!({ "mode": "platform_ai" }),
        ),
    )
    .await;
    assert_eq!(platform_resp.status(), StatusCode::CONFLICT);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T043b — unresolvable platform_ai returns to awaiting decision
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn unresolvable_platform_ai_returns_to_awaiting_decision() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t043b@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    // Seed platform AI config + credential so AI layer resolves
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-t043b-test-key", &master).await;

    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("T043b Customer")
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

    // Post an initial customer message to trigger the auto-ack first
    let msg_payload = serde_json::json!({ "kind": "customer", "body": "Initial help request" });
    let msg_resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/conversations/{conversation_id}/messages"),
            user_id,
            tenant_id,
            msg_payload,
        ),
    )
    .await;
    assert_eq!(msg_resp.status(), StatusCode::OK);

    // Run responder to get the auto-ack
    let processed = process_agent_responder_once(&pool, &state.ai, &state.escalations)
        .await
        .expect("responder should succeed");
    assert!(
        processed,
        "responder should have processed the auto-ack event"
    );

    let detail_resp = send(
        &state,
        auth_get(
            &format!("/api/v1/tenant/conversations/{conversation_id}"),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(detail_resp.status(), StatusCode::OK);
    let detail_json = body_json(detail_resp).await;
    assert_eq!(
        detail_json["data"]["awaiting_ai_decision"], true,
        "awaiting_ai_decision should be true before setting platform_ai"
    );

    // Set platform_ai while AI layer resolves
    let set_resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/conversations/{conversation_id}/ai-handling"),
            user_id,
            tenant_id,
            serde_json::json!({ "mode": "platform_ai" }),
        ),
    )
    .await;
    assert_eq!(set_resp.status(), StatusCode::OK);

    // Remove the tenant's AI-layer credential/config (fixture teardown)
    sqlx::query("DELETE FROM ai_credentials WHERE tenant_id IS NULL AND provider = 'openai'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM ai_configurations WHERE tenant_id IS NULL AND provider = 'openai'")
        .execute(&pool)
        .await
        .unwrap();

    // GET conversation detail → awaiting_ai_decision: true
    // (even though ai_handling is still "platform_ai")
    let detail_resp2 = send(
        &state,
        auth_get(
            &format!("/api/v1/tenant/conversations/{conversation_id}"),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(detail_resp2.status(), StatusCode::OK);
    let detail_json2 = body_json(detail_resp2).await;
    assert_eq!(
        detail_json2["data"]["ai_handling"], "platform_ai",
        "ai_handling should still be platform_ai"
    );
    assert_eq!(
        detail_json2["data"]["awaiting_ai_decision"], true,
        "awaiting_ai_decision should be true since platform_ai is now unresolvable"
    );

    // POST ai-handling {mode: "human"} from that state → still succeeds
    let human_resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/conversations/{conversation_id}/ai-handling"),
            user_id,
            tenant_id,
            serde_json::json!({ "mode": "human" }),
        ),
    )
    .await;
    assert_eq!(human_resp.status(), StatusCode::OK);
    let human_json = body_json(human_resp).await;
    assert!(
        human_json["data"]["escalation"].is_object(),
        "human escalation should succeed after unresolvable platform_ai"
    );

    // Post a new customer message and run responder
    let msg2_payload =
        serde_json::json!({ "kind": "customer", "body": "Follow up after human escalation" });
    let msg2_resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/conversations/{conversation_id}/messages"),
            user_id,
            tenant_id,
            msg2_payload,
        ),
    )
    .await;
    assert_eq!(msg2_resp.status(), StatusCode::OK);

    let processed2 = process_agent_responder_once(&pool, &state.ai, &state.escalations)
        .await
        .expect("responder should succeed");
    assert!(
        processed2,
        "responder should have processed the follow-up event"
    );

    // Assert no second auto-ack and no AI reply
    let system_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages \
         WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'system'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        system_count, 1,
        "expected still exactly one system message (no second auto-ack)"
    );

    let ai_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages \
         WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ai_count, 0, "expected no AI reply after human handling");
}

// ═══════════════════════════════════════════════════════════════════════════════
// T058 — Business rule appears in composed prompt
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn business_rule_appears_in_composed_prompt() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t058@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-t058-key", &master).await;

    let agent_payload = serde_json::json!({
        "name": "RuleAgent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": ["never promise refunds"],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": { "provider": "openai", "model": "gpt-4" },
    });

    let put_resp = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, agent_payload),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::CREATED);

    mock_openai(&openai_mock, "sk-t058-key", openai_response("OK")).await;

    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("T058 Customer")
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

    let message_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'Hello') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let outbox_payload = serde_json::json!({
        "conversation_id": conversation_id,
        "message_id": message_id,
        "channel": "web_chat",
    });
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.customer_message', $4)",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(tenant_id)
    .bind(outbox_payload)
    .execute(&pool)
    .await
    .unwrap();

    let processed = process_agent_responder_once(&pool, &state.ai, &state.escalations)
        .await
        .expect("agent responder should succeed");
    assert!(processed, "agent responder should have processed an event");

    let requests = openai_mock.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1, "expected one request to OpenAI");

    let body: serde_json::Value =
        serde_json::from_slice(&requests[0].body).expect("valid JSON body");
    let system_msg = body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "system")
        .map(|m| m["content"].as_str().unwrap_or(""))
        .unwrap_or("");

    assert!(
        system_msg.contains("1. never promise refunds"),
        "system message should contain the numbered business rule, got: {}",
        system_msg
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// T059 — Keyword rule escalates with rule name as reason
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn keyword_rule_escalates_with_rule_name_reason() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t059@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-t059-key", &master).await;

    // Seed a real skill so the rule reference is valid
    let skill_id = seed_skill(&pool, tenant_id, "refund-support").await;

    let rule_id = Uuid::new_v4();
    let agent_payload = serde_json::json!({
        "name": "EscalateAgent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [
            {
                "id": rule_id,
                "name": "Refund requests",
                "trigger": "topic_keywords",
                "keywords": ["refund"],
                "required_skill_ids": [skill_id]
            }
        ],
        "enabled_channels": ["web_chat"],
        "provider_selection": { "provider": "openai", "model": "gpt-4" },
    });

    let put_resp = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, agent_payload),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::CREATED);

    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("T059 Customer")
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

    // Message containing the trigger keyword
    let message_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'I need a refund please') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let outbox_payload = serde_json::json!({
        "conversation_id": conversation_id,
        "message_id": message_id,
        "channel": "web_chat",
    });
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.customer_message', $4)",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(tenant_id)
    .bind(outbox_payload)
    .execute(&pool)
    .await
    .unwrap();

    let processed = process_agent_responder_once(&pool, &state.ai, &state.escalations)
        .await
        .expect("agent responder should succeed");
    assert!(processed, "agent responder should have processed an event");

    // No AI reply should have been inserted
    let ai_messages: Vec<String> = sqlx::query_scalar(
        "SELECT body FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        ai_messages.is_empty(),
        "no AI reply should exist after escalation"
    );

    // Escalation should exist with the rule name as reason and correct skill routed
    let escalation: Option<(String, Vec<Uuid>)> = sqlx::query_as(
        "SELECT reason, required_skill_ids FROM escalations \
         WHERE tenant_id = $1 AND conversation_id = $2",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (reason, skill_ids) = escalation.expect("escalation should have been created");
    assert_eq!(
        reason, "Refund requests",
        "escalation reason should be the rule name"
    );
    assert!(
        skill_ids.contains(&skill_id),
        "escalation should reference the seeded skill"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// T060 — Baseline escalation survives zero tenant rules
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn baseline_escalation_survives_zero_tenant_rules() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t060@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-t060-key", &master).await;

    // Agent with empty escalation rules
    let agent_payload = serde_json::json!({
        "name": "BaselineAgent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": { "provider": "openai", "model": "gpt-4" },
    });

    let put_resp = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, agent_payload),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::CREATED);

    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("T060 Customer")
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

    // Message triggering baseline human request
    let message_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'I want to talk to a human') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let outbox_payload = serde_json::json!({
        "conversation_id": conversation_id,
        "message_id": message_id,
        "channel": "web_chat",
    });
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.customer_message', $4)",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(tenant_id)
    .bind(outbox_payload)
    .execute(&pool)
    .await
    .unwrap();

    let processed = process_agent_responder_once(&pool, &state.ai, &state.escalations)
        .await
        .expect("agent responder should succeed");
    assert!(processed, "agent responder should have processed an event");

    // Escalation should exist with BASELINE_ESCALATION_REASON
    let reason: Option<String> = sqlx::query_scalar(
        "SELECT reason FROM escalations \
         WHERE tenant_id = $1 AND conversation_id = $2",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(
        reason.as_deref(),
        Some(BASELINE_ESCALATION_REASON),
        "escalation reason should be the baseline escalation reason"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// T061 — Broken skill ref surfaced on read
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn broken_skill_ref_surfaced_on_read() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let state = plain_state(pool.clone());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t061@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    // Seed a skill, then soft-delete it
    let skill_id = seed_skill(&pool, tenant_id, "obsolete-skill").await;
    sqlx::query("UPDATE skills SET deleted_at = now() WHERE id = $1")
        .bind(skill_id)
        .execute(&pool)
        .await
        .unwrap();

    let rule_id = Uuid::new_v4();
    let agent_payload = serde_json::json!({
        "name": "BrokenRefAgent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [
            {
                "id": rule_id,
                "name": "DependsOnObsolete",
                "trigger": "topic_keywords",
                "keywords": ["obsolete"],
                "required_skill_ids": [skill_id]
            }
        ],
        "enabled_channels": ["web_chat"],
        "provider_selection": null,
    });

    let put_resp = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, agent_payload),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::CREATED);

    // GET should surface the broken skill ref
    let get_resp = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent", user_id, tenant_id),
    )
    .await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let json = body_json(get_resp).await;

    let rules = json["agent"]["escalation_rules"].as_array().unwrap();
    assert_eq!(rules.len(), 1);
    let broken = rules[0]["broken_skill_refs"].as_array().unwrap();
    assert_eq!(broken.len(), 1, "expected one broken skill ref");
    let broken_id = broken[0].as_str().unwrap();
    assert_eq!(
        Uuid::parse_str(broken_id).unwrap(),
        skill_id,
        "broken_skill_refs should contain the deleted skill id"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// T062 — Save rejects unknown skill reference
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn save_rejects_unknown_skill_reference() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let state = plain_state(pool.clone());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t062@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let fake_skill_id = Uuid::parse_str("00000000-0000-0000-0000-000000000999").unwrap();
    let agent_payload = serde_json::json!({
        "name": "BadRefAgent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [
            {
                "id": Uuid::new_v4(),
                "name": "BadRule",
                "trigger": "topic_keywords",
                "keywords": ["test"],
                "required_skill_ids": [fake_skill_id]
            }
        ],
        "enabled_channels": ["web_chat"],
        "provider_selection": null,
    });

    let put_resp = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, agent_payload),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let json = body_json(put_resp).await;
    let error_msg = json["error"]["message"].as_str().unwrap_or("");
    assert!(
        error_msg.contains("BadRule"),
        "error should name the rule, got: {}",
        error_msg
    );
    assert!(
        error_msg.contains("non-existent skill"),
        "error should mention non-existent skills, got: {}",
        error_msg
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// T063 — Save rejects malformed rules
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn save_rejects_malformed_rules() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let state = plain_state(pool.clone());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t063@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    // topic_keywords with empty keywords → 422
    let empty_keywords_payload = serde_json::json!({
        "name": "EmptyKwAgent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [
            {
                "id": Uuid::new_v4(),
                "name": "EmptyKwRule",
                "trigger": "topic_keywords",
                "keywords": [],
                "required_skill_ids": []
            }
        ],
        "enabled_channels": ["web_chat"],
        "provider_selection": null,
    });

    let resp1 = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent",
            user_id,
            tenant_id,
            empty_keywords_payload,
        ),
    )
    .await;
    assert_eq!(resp1.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let json1 = body_json(resp1).await;
    assert_eq!(json1["error"]["code"], "validation_failed");

    // human_request with non-empty keywords → 422
    let nonempty_keywords_payload = serde_json::json!({
        "name": "NonemptyKwAgent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [
            {
                "id": Uuid::new_v4(),
                "name": "HumanWithKwRule",
                "trigger": "human_request",
                "keywords": ["should", "not", "be", "here"],
                "required_skill_ids": []
            }
        ],
        "enabled_channels": ["web_chat"],
        "provider_selection": null,
    });

    let resp2 = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent",
            user_id,
            tenant_id,
            nonempty_keywords_payload,
        ),
    )
    .await;
    assert_eq!(resp2.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let json2 = body_json(resp2).await;
    assert_eq!(json2["error"]["code"], "validation_failed");
}

// ═══════════════════════════════════════════════════════════════════════════════
// T066 — Disabled channel blocks AI reply
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn disabled_channel_blocks_ai_reply() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t066@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    // Seed platform AI config + credential so AI layer would resolve if reached
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-t066-key", &master).await;
    mock_openai(
        &openai_mock,
        "sk-t066-key",
        openai_response("should not be called"),
    )
    .await;

    // Save agent with empty enabled_channels — all channels disabled
    let payload = serde_json::json!({
        "name": "Disabled Channel Agent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": [],
        "provider_selection": null,
    });

    let resp = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, payload),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Create a customer, conversation, and post a web_chat message
    let customer_id: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("T066 Customer")
    .fetch_one(&pool)
    .await
    .unwrap();

    let conversation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let message_id: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'Hello from T066') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let outbox_payload = serde_json::json!({
        "conversation_id": conversation_id,
        "message_id": message_id,
        "channel": "web_chat",
    });
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.customer_message', $4)",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(tenant_id)
    .bind(outbox_payload)
    .execute(&pool)
    .await
    .unwrap();

    // Run the responder
    let processed = process_agent_responder_once(&pool, &state.ai, &state.escalations)
        .await
        .expect("agent responder should succeed");
    assert!(
        processed,
        "responder should have processed (blocked) the event"
    );

    // Assert no AI or system message was inserted
    let messages: Vec<String> = sqlx::query_scalar(
        "SELECT kind FROM messages WHERE tenant_id = $1 AND conversation_id = $2 \
         AND kind IN ('ai', 'system')",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        messages.is_empty(),
        "expected no ai/system messages, got: {:?}",
        messages
    );

    // Assert no escalation was created
    let escalation_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM escalations WHERE tenant_id = $1 AND conversation_id = $2",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(escalation_count, 0, "no escalation should be created");

    // Assert outbox event was consumed
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events WHERE tenant_id = $1 AND event_type = 'conversation.customer_message'",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(outbox_count, 0, "outbox event should be consumed");
}

// ═══════════════════════════════════════════════════════════════════════════════
// T067 — Re-enabled channel resumes replies
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn re_enabled_channel_resumes_replies() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t067@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    // Seed platform AI config + credential
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-t067-key", &master).await;

    // Save agent with web_chat enabled
    let payload = serde_json::json!({
        "name": "ReEnabled Agent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": null,
    });

    let resp = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, payload),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Create a customer and conversation
    let customer_id: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("T067 Customer")
    .fetch_one(&pool)
    .await
    .unwrap();

    let conversation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Mock OpenAI to return a reply
    mock_openai(
        &openai_mock,
        "sk-t067-key",
        openai_response("Re-enabled AI reply"),
    )
    .await;

    // Post a customer message
    let message_id: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'Hello after re-enable') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let outbox_payload = serde_json::json!({
        "conversation_id": conversation_id,
        "message_id": message_id,
        "channel": "web_chat",
    });
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.customer_message', $4)",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(tenant_id)
    .bind(outbox_payload)
    .execute(&pool)
    .await
    .unwrap();

    // Run the responder
    let processed = process_agent_responder_once(&pool, &state.ai, &state.escalations)
        .await
        .expect("agent responder should succeed");
    assert!(processed, "responder should have processed the event");

    // Assert an AI reply was inserted
    let ai_messages: Vec<String> = sqlx::query_scalar(
        "SELECT body FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(ai_messages.len(), 1, "expected exactly one AI reply");
    assert_eq!(ai_messages[0], "Re-enabled AI reply");
}

// ═══════════════════════════════════════════════════════════════════════════════
// T068 — All channels disabled save succeeds
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn all_channels_disabled_save_succeeds() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t068@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    // PUT with empty enabled_channels
    let payload = serde_json::json!({
        "name": "All Disabled Agent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": [],
        "provider_selection": null,
    });

    let put_resp = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, payload),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::CREATED);

    // GET should reflect empty enabled_channels
    let get_resp = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent", user_id, tenant_id),
    )
    .await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let json = body_json(get_resp).await;
    assert_eq!(json["configured"], true);
    assert_eq!(json["agent"]["enabled_channels"], serde_json::json!([]));
}

// ═══════════════════════════════════════════════════════════════════════════════
// T069 — Verify validation allows empty enabled_channels
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn empty_enabled_channels_passes_validation() {
    use ai::agent_config::validate_payload;
    use ai::agent_config::AgentConfigPayload;
    use ai::agent_config::AvatarPayload;

    let payload = AgentConfigPayload {
        name: "Test Agent".into(),
        avatar: AvatarPayload {
            kind: "preset".into(),
            preset: Some("spark".into()),
        },
        tone: "professional".into(),
        business_rules: vec![],
        escalation_rules: vec![],
        enabled_channels: vec![],
        provider_selection: None,
        version: None,
    };

    assert!(validate_payload(&payload).is_ok());
}

#[test]
fn invalid_channel_fails_validation() {
    use ai::agent_config::validate_payload;
    use ai::agent_config::AgentConfigPayload;
    use ai::agent_config::AvatarPayload;
    use ai::agent_config::CATALOG_CHANNELS;

    let payload = AgentConfigPayload {
        name: "Test Agent".into(),
        avatar: AvatarPayload {
            kind: "preset".into(),
            preset: Some("spark".into()),
        },
        tone: "professional".into(),
        business_rules: vec![],
        escalation_rules: vec![],
        enabled_channels: vec!["invalid_channel".into()],
        provider_selection: None,
        version: None,
    };

    let result = validate_payload(&payload);
    assert!(result.is_err());
    let issues = result.unwrap_err();
    assert!(issues.iter().any(|i| i.field == "enabled_channels[0]"));

    // Verify CATALOG_CHANNELS defines the valid set
    assert!(CATALOG_CHANNELS.contains(&"web_chat"));
    assert!(CATALOG_CHANNELS.contains(&"email"));
    assert!(CATALOG_CHANNELS.contains(&"phone"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// T070 — Every write is audited
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn every_write_is_audited() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t070@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    // 1. Create agent config
    let create_payload = serde_json::json!({
        "name": "Audited Agent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": null,
    });

    let create_resp = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent",
            user_id,
            tenant_id,
            create_payload.clone(),
        ),
    )
    .await;
    assert_eq!(create_resp.status(), StatusCode::CREATED);

    // Audit row for create
    let create_audit: Option<(String, Option<Uuid>, serde_json::Value)> = sqlx::query_as(
        "SELECT action, actor_user_id, details FROM audit_logs \
         WHERE tenant_id = $1 AND action = 'agent_config.created' \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (action, actor, details) = create_audit.expect("agent_config.created audit row");
    assert_eq!(action, "agent_config.created");
    assert_eq!(actor, Some(user_id));
    assert_eq!(details["name"], "Audited Agent");

    // 2. Update agent config
    let update_payload = serde_json::json!({
        "name": "Audited Agent Updated",
        "avatar": { "kind": "preset", "preset": "nova" },
        "tone": "friendly",
        "business_rules": ["Be nice"],
        "escalation_rules": [],
        "enabled_channels": ["web_chat", "email"],
        "provider_selection": null,
        "version": 1,
    });

    let update_resp = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent",
            user_id,
            tenant_id,
            update_payload,
        ),
    )
    .await;
    assert_eq!(update_resp.status(), StatusCode::OK);

    // Audit row for update
    let update_audit: Option<(String, Option<Uuid>, serde_json::Value)> = sqlx::query_as(
        "SELECT action, actor_user_id, details FROM audit_logs \
         WHERE tenant_id = $1 AND action = 'agent_config.updated' \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (action2, actor2, details2) = update_audit.expect("agent_config.updated audit row");
    assert_eq!(action2, "agent_config.updated");
    assert_eq!(actor2, Some(user_id));
    let changed_fields: Vec<String> =
        serde_json::from_value(details2["changed_fields"].clone()).unwrap();
    assert!(changed_fields.contains(&"name".to_string()));
    assert!(changed_fields.contains(&"avatar".to_string()));
    assert!(changed_fields.contains(&"tone".to_string()));
    assert!(changed_fields.contains(&"business_rules".to_string()));
    assert!(changed_fields.contains(&"enabled_channels".to_string()));

    // 3. Upload avatar
    let png_bytes = small_png();
    let avatar_resp = send(
        &state,
        raw_put(
            "/api/v1/tenant/ai/agent/avatar",
            user_id,
            tenant_id,
            "image/png",
            png_bytes,
        ),
    )
    .await;
    assert_eq!(avatar_resp.status(), StatusCode::OK);

    // Audit row for avatar update
    let avatar_audit: Option<(String, Option<Uuid>, serde_json::Value)> = sqlx::query_as(
        "SELECT action, actor_user_id, details FROM audit_logs \
         WHERE tenant_id = $1 AND action = 'agent_config.avatar_updated' \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (action3, actor3, details3) = avatar_audit.expect("agent_config.avatar_updated audit row");
    assert_eq!(action3, "agent_config.avatar_updated");
    assert_eq!(actor3, Some(user_id));
    assert_eq!(details3["kind"], "upload");
}

// ═══════════════════════════════════════════════════════════════════════════════
// T071 — Unauthorized roles get 403
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn unauthorized_roles_get_403() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let state = plain_state(pool.clone());

    // Roles without ai_agent.view or ai_agent.manage → 403
    for role in ["manager", "agent", "viewer"] {
        let tenant_id = seed_tenant(&pool).await;
        let user_id = seed_user(&pool, &format!("t071-{role}@test.com"), role).await;
        seed_membership(&pool, tenant_id, user_id, role).await;

        // GET → 403
        let get_resp = send(
            &state,
            auth_get("/api/v1/tenant/ai/agent", user_id, tenant_id),
        )
        .await;
        assert_eq!(
            get_resp.status(),
            StatusCode::FORBIDDEN,
            "GET should be 403 for {role}"
        );

        // PUT → 403
        let put_payload = serde_json::json!({
            "name": "Role Test",
            "avatar": { "kind": "preset", "preset": "spark" },
            "tone": "professional",
            "business_rules": [],
            "escalation_rules": [],
            "enabled_channels": ["web_chat"],
            "provider_selection": null,
        });
        let put_resp = send(
            &state,
            json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, put_payload),
        )
        .await;
        assert_eq!(
            put_resp.status(),
            StatusCode::FORBIDDEN,
            "PUT should be 403 for {role}"
        );
    }

    // Owner → succeeds
    let owner_tenant = seed_tenant(&pool).await;
    let owner_id = seed_user(&pool, "t071-owner@test.com", "owner").await;
    seed_membership(&pool, owner_tenant, owner_id, "owner").await;

    let get_owner = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent", owner_id, owner_tenant),
    )
    .await;
    assert_eq!(get_owner.status(), StatusCode::OK);

    let put_owner = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent",
            owner_id,
            owner_tenant,
            serde_json::json!({
                "name": "Owner Agent",
                "avatar": { "kind": "preset", "preset": "spark" },
                "tone": "professional",
                "business_rules": [],
                "escalation_rules": [],
                "enabled_channels": ["web_chat"],
                "provider_selection": null,
            }),
        ),
    )
    .await;
    assert_eq!(put_owner.status(), StatusCode::CREATED);

    // Admin → succeeds
    let admin_tenant = seed_tenant(&pool).await;
    let admin_id = seed_user(&pool, "t071-admin@test.com", "admin").await;
    seed_membership(&pool, admin_tenant, admin_id, "admin").await;

    let get_admin = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent", admin_id, admin_tenant),
    )
    .await;
    assert_eq!(get_admin.status(), StatusCode::OK);

    let put_admin = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent",
            admin_id,
            admin_tenant,
            serde_json::json!({
                "name": "Admin Agent",
                "avatar": { "kind": "preset", "preset": "spark" },
                "tone": "professional",
                "business_rules": [],
                "escalation_rules": [],
                "enabled_channels": ["web_chat"],
                "provider_selection": null,
            }),
        ),
    )
    .await;
    assert_eq!(put_admin.status(), StatusCode::CREATED);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T072 — Platform actor attribution via tenant switch
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn platform_actor_attribution_via_tenant_switch() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let state = plain_state(pool.clone());

    // Create a platform user (super_admin) with membership in the tenant
    let platform_user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name, platform_role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(format!("t072-platform@{}", Uuid::new_v4().simple()))
    .bind("Platform Actor")
    .bind("super_admin")
    .fetch_one(&pool)
    .await
    .unwrap();

    seed_membership(&pool, tenant_id, platform_user_id, "admin").await;

    // Platform user modifies agent config in tenant context
    let put_payload = serde_json::json!({
        "name": "Platform-Modified Agent",
        "avatar": { "kind": "preset", "preset": "orbit" },
        "tone": "friendly",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": null,
    });

    let put_resp = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent",
            platform_user_id,
            tenant_id,
            put_payload,
        ),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::CREATED);

    // Assert audit row's actor_user_id identifies the platform user
    let audit: Option<(Option<Uuid>,)> = sqlx::query_as(
        "SELECT actor_user_id FROM audit_logs \
         WHERE tenant_id = $1 AND action = 'agent_config.created' \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let (actor,) = audit.expect("expected audit row for agent_config.created");
    assert_eq!(actor, Some(platform_user_id));
}

// ═══════════════════════════════════════════════════════════════════════════════
// T050 — Options lists only credential-backed providers
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn options_lists_only_credential_backed_providers() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t050@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    // Seed platform AI config + only anthropic credential
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "anthropic", "sk-ant-test-key", &master).await;

    let state = plain_state(pool.clone());

    let response = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent/options", user_id, tenant_id),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;

    // Assert tones, channels, avatar_presets, prompt_max_length, limits present
    assert!(json["tones"].is_array(), "tones must be present");
    assert!(json["channels"].is_array(), "channels must be present");
    assert!(
        json["avatar_presets"].is_array(),
        "avatar_presets must be present"
    );
    assert_eq!(json["prompt_max_length"], 8000);
    assert_eq!(json["limits"]["business_rules_max"], 20);
    assert_eq!(json["limits"]["escalation_rules_max"], 20);

    // Assert provider credential availability
    let providers = json["providers"].as_array().unwrap();
    let anthropic = providers
        .iter()
        .find(|p| p["provider"] == "anthropic")
        .unwrap();
    assert_eq!(anthropic["credential_available"], true);
    let openai = providers
        .iter()
        .find(|p| p["provider"] == "openai")
        .unwrap();
    assert_eq!(openai["credential_available"], false);
    let gemini = providers
        .iter()
        .find(|p| p["provider"] == "gemini")
        .unwrap();
    assert_eq!(gemini["credential_available"], false);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T051 — Provider override serves AI reply
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn provider_override_serves_ai_reply() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let anthropic_mock = MockServer::start().await;
    let state = wiremock_anthropic_state(pool.clone(), &anthropic_mock.uri());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t051@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    // Seed platform AI config + anthropic credential
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "anthropic", "sk-ant-t051-key", &master).await;

    // PUT agent with provider_selection for anthropic/claude-sonnet-5
    let agent_payload = serde_json::json!({
        "name": "OverrideAgent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": { "provider": "anthropic", "model": "claude-sonnet-5" },
    });

    let put_resp = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, agent_payload),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::CREATED);

    // Mock Anthropic to return a reply
    mock_anthropic(
        &anthropic_mock,
        "sk-ant-t051-key",
        anthropic_response("Anthropic reply here"),
    )
    .await;

    // Create customer + conversation + message + outbox event
    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("T051 Customer")
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

    let message_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'Hello from T051') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let outbox_payload = serde_json::json!({
        "conversation_id": conversation_id,
        "message_id": message_id,
        "channel": "web_chat",
    });
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.customer_message', $4)",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(tenant_id)
    .bind(outbox_payload)
    .execute(&pool)
    .await
    .unwrap();

    let processed = process_agent_responder_once(&pool, &state.ai, &state.escalations)
        .await
        .expect("agent responder should succeed");
    assert!(processed, "agent responder should have processed an event");

    // Assert AI message was inserted
    let ai_messages: Vec<String> = sqlx::query_scalar(
        "SELECT body FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(ai_messages.len(), 1, "expected exactly one AI reply");
    assert_eq!(ai_messages[0], "Anthropic reply here");

    // Assert the call went to Anthropic
    let requests = anthropic_mock.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1, "expected one request to Anthropic");

    let body: serde_json::Value =
        serde_json::from_slice(&requests[0].body).expect("valid JSON body");
    assert_eq!(body["model"], "claude-sonnet-5");
    assert!(body["messages"].is_array());
}

// ═══════════════════════════════════════════════════════════════════════════════
// T052 — Stale override falls back
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn stale_override_falls_back() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t052@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    // Seed platform AI config + credential for openai (default path)
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-t052-openai-key", &master).await;
    // Also seed a provider override credential (anthropic) that we'll delete
    seed_ai_credential(&pool, None, "anthropic", "sk-t052-ant-key", &master).await;

    // PUT agent with anthropic override (credential resolvable)
    let agent_payload = serde_json::json!({
        "name": "StaleAgent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": { "provider": "anthropic", "model": "claude-sonnet-5" },
    });

    let put_resp = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, agent_payload),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::CREATED);
    let put_json = body_json(put_resp).await;
    assert_eq!(put_json["agent"]["provider_selection"]["stale"], false);
    let _version = put_json["agent"]["version"].as_i64().unwrap();

    // Delete the anthropic credential
    sqlx::query(
        "UPDATE ai_credentials SET deleted_at = now() \
         WHERE tenant_id IS NULL AND provider = 'anthropic' AND deleted_at IS NULL",
    )
    .execute(&pool)
    .await
    .unwrap();

    // GET and assert provider_selection.stale = true
    let get_resp = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent", user_id, tenant_id),
    )
    .await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let get_json = body_json(get_resp).await;
    assert_eq!(get_json["agent"]["provider_selection"]["stale"], true);

    // Mock OpenAI (default path)
    mock_openai(
        &openai_mock,
        "sk-t052-openai-key",
        openai_response("Fallback reply"),
    )
    .await;

    // Create customer + conversation + message + outbox event
    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("T052 Customer")
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

    let message_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'T052 test message') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let outbox_payload = serde_json::json!({
        "conversation_id": conversation_id,
        "message_id": message_id,
        "channel": "web_chat",
    });
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.customer_message', $4)",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(tenant_id)
    .bind(outbox_payload)
    .execute(&pool)
    .await
    .unwrap();

    let processed = process_agent_responder_once(&pool, &state.ai, &state.escalations)
        .await
        .expect("agent responder should succeed");
    assert!(processed, "agent responder should have processed an event");

    // Assert the AI reply came from OpenAI (default path)
    let ai_messages: Vec<String> = sqlx::query_scalar(
        "SELECT body FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(ai_messages.len(), 1, "expected exactly one AI reply");
    assert_eq!(ai_messages[0], "Fallback reply");

    // Assert the request went to OpenAI, not Anthropic
    let requests = openai_mock.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        1,
        "expected one request to OpenAI (fallback)"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// T053 — Save rejects unresolvable provider
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn save_rejects_unresolvable_provider() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t053@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    // PUT agent with provider_selection for a provider that has no credential
    let agent_payload = serde_json::json!({
        "name": "NoCredAgent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": { "provider": "anthropic", "model": "claude-sonnet-5" },
    });

    let put_resp = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_id, tenant_id, agent_payload),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let json = body_json(put_resp).await;
    let error_msg = json["error"]["message"].as_str().unwrap_or("");
    assert!(
        error_msg.contains("anthropic"),
        "error should mention the provider name, got: {}",
        error_msg
    );
    assert!(
        error_msg.contains("credential"),
        "error should mention credential, got: {}",
        error_msg
    );
}
