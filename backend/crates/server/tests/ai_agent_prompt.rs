use std::sync::Arc;
use std::time::Duration;

use ai::agent_responder::process_agent_responder_once;
use ai::crypto::{self, MasterKey};
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
            eprintln!("skipping ai agent prompt tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping ai agent prompt tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query(
        "TRUNCATE TABLE ai_usage_records, ai_credentials, ai_configurations, \
         agent_configurations, agent_prompts, agent_prompt_versions, agent_avatar_uploads, \
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
    let slug = format!("apt-tenant-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("AI Agent Prompt Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str, _role: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("AI Agent Prompt Test User")
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

fn auth_get(uri: &str, user_id: Uuid, tenant_id: Uuid) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(Method::GET)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .body(Body::empty())
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
// T019a — First save on a tenant with no prompt row
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn first_save_creates_version_1() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t019a@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    // GET bootstrap — should show exists: false, activeVersion: 0
    let get_resp = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent/prompt", user_id, tenant_id),
    )
    .await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let get_json = body_json(get_resp).await;
    assert_eq!(get_json["prompt"]["exists"], false);
    assert_eq!(get_json["prompt"]["activeVersion"], 0);
    assert!(!get_json["prompt"]["content"]
        .as_str()
        .unwrap_or("")
        .is_empty());
    assert!(get_json["prompt"]["updatedAt"].is_null());
    assert!(get_json["prompt"]["updatedBy"].is_null());
    assert!(!get_json["variables"].as_array().unwrap().is_empty());
    assert!(get_json["limits"]["maxContentLength"].as_u64().unwrap() > 0);

    // First save with baseVersion: 0
    let content =
        "You are {{agent_name}} for {{tenant_name}}. Help {{customer_name}} via {{channel}}.";
    let save_payload = serde_json::json!({
        "content": content,
        "changeNote": "Initial prompt",
        "baseVersion": 0,
    });
    let put_resp = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            save_payload,
        ),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::OK);
    let put_json = body_json(put_resp).await;
    assert_eq!(put_json["version"], 1);
    assert_eq!(put_json["created"], true);
    assert!(put_json["updatedBy"]
        .as_str()
        .unwrap_or("")
        .contains("AI Agent Prompt Test User"));

    // GET bootstrap now shows exists: true, activeVersion: 1
    let get2 = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent/prompt", user_id, tenant_id),
    )
    .await;
    assert_eq!(get2.status(), StatusCode::OK);
    let get2_json = body_json(get2).await;
    assert_eq!(get2_json["prompt"]["exists"], true);
    assert_eq!(get2_json["prompt"]["activeVersion"], 1);
    assert_eq!(get2_json["prompt"]["content"], content);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T019b — Second save with correct baseVersion
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn second_save_creates_version_2() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t019b@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    // First save
    let content1 = "Original prompt {{agent_name}}.";
    let put1 = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": content1,
                "changeNote": "v1",
                "baseVersion": 0,
            }),
        ),
    )
    .await;
    assert_eq!(put1.status(), StatusCode::OK);

    // Second save with baseVersion: 1
    let content2 = "Updated prompt {{agent_name}} v2.";
    let put2 = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": content2,
                "changeNote": "v2",
                "baseVersion": 1,
            }),
        ),
    )
    .await;
    assert_eq!(put2.status(), StatusCode::OK);
    let put2_json = body_json(put2).await;
    assert_eq!(put2_json["version"], 2);
    assert_eq!(put2_json["created"], true);

    // GET shows activeVersion: 2 with updated content
    let get_resp = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent/prompt", user_id, tenant_id),
    )
    .await;
    let get_json = body_json(get_resp).await;
    assert_eq!(get_json["prompt"]["activeVersion"], 2);
    assert_eq!(get_json["prompt"]["content"], content2);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T019c — Stale baseVersion returns 409
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn stale_base_version_conflicts() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t019c@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    // First save
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": "Version 1 content {{agent_name}}.",
                "changeNote": "v1",
                "baseVersion": 0,
            }),
        ),
    )
    .await;

    // Second save (bumps to v2)
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": "Version 2 content {{agent_name}}.",
                "changeNote": "v2",
                "baseVersion": 1,
            }),
        ),
    )
    .await;

    // Third save with stale baseVersion: 1 (should be 2) → 409
    let put3 = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": "Stale save {{agent_name}}.",
                "changeNote": "stale",
                "baseVersion": 1,
            }),
        ),
    )
    .await;
    assert_eq!(put3.status(), StatusCode::CONFLICT);
    let err_json = body_json(put3).await;
    assert_eq!(err_json["error"]["code"], "conflict");
    assert_eq!(err_json["error"]["details"][0]["activeVersion"], 2);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T019d — Save with byte-identical content returns created: false (no-op)
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn same_content_is_no_op() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t019d@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    let content = "No-op test prompt {{agent_name}}.";

    // First save
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": content,
                "changeNote": "v1",
                "baseVersion": 0,
            }),
        ),
    )
    .await;

    // Second save with same content → no-op
    let put2 = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": content,
                "changeNote": "same content",
                "baseVersion": 1,
            }),
        ),
    )
    .await;
    assert_eq!(put2.status(), StatusCode::OK);
    let put2_json = body_json(put2).await;
    assert_eq!(put2_json["created"], false);
    assert_eq!(put2_json["version"], 1);

    // Active version should still be 1
    let get_resp = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent/prompt", user_id, tenant_id),
    )
    .await;
    let get_json = body_json(get_resp).await;
    assert_eq!(get_json["prompt"]["activeVersion"], 1);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T019e — Responder end-to-end with variable substitution
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn responder_substitutes_prompt_variables() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t019e@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    // Seed platform AI config + credential
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-prompt-test-key", &master).await;

    // PUT agent config
    let agent_payload = serde_json::json!({
        "name": "PromptBot",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "friendly",
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

    // Save a prompt with all four variables
    let prompt_content = "You are {{agent_name}} helping {{tenant_name}}. \
                          Customer {{customer_name}} is chatting via {{channel}}.";
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": prompt_content,
                "changeNote": "variable test",
                "baseVersion": 0,
            }),
        ),
    )
    .await;

    // Mock OpenAI
    mock_openai(
        &openai_mock,
        "sk-prompt-test-key",
        openai_response("AI reply with vars"),
    )
    .await;

    // Set tenant name (it was created with "AI Agent Prompt Test Tenant")
    // The actual tenant name is set by seed_tenant
    let tenant_name = "AI Agent Prompt Test Tenant";

    // Create customer with a real display name
    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Jamie Test")
    .fetch_one(&pool)
    .await
    .unwrap();

    // Create conversation
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
         VALUES ($1, $2, 'customer', 'I need help with my order') RETURNING id",
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

    // Assert AI reply was inserted
    let ai_messages: Vec<String> = sqlx::query_scalar(
        "SELECT body FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(ai_messages.len(), 1, "expected exactly one AI reply");

    // Assert the outbound request contained the rendered prompt
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
        system_msg.contains("PromptBot"),
        "system message should contain agent name 'PromptBot', got: {system_msg}"
    );
    assert!(
        system_msg.contains(tenant_name),
        "system message should contain tenant name '{tenant_name}', got: {system_msg}"
    );
    assert!(
        system_msg.contains("Jamie Test"),
        "system message should contain customer name 'Jamie Test', got: {system_msg}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// T019f — Second save rebinding: newer version is used by responder
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn second_save_rebinding_reflects_new_prompt() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t019f@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-prompt-rebind", &master).await;

    // PUT agent config
    let agent_payload = serde_json::json!({
        "name": "RebindBot",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
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

    // Save v1 prompt
    let v1_content = "V1 prompt: {{agent_name}} says hello.";
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": v1_content,
                "changeNote": "v1",
                "baseVersion": 0,
            }),
        ),
    )
    .await;

    // Save v2 prompt
    let v2_content = "V2 prompt: {{agent_name}} says goodbye.";
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": v2_content,
                "changeNote": "v2",
                "baseVersion": 1,
            }),
        ),
    )
    .await;

    // Verify active version is 2
    let get_check = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent/prompt", user_id, tenant_id),
    )
    .await;
    let check_json = body_json(get_check).await;
    assert_eq!(check_json["prompt"]["activeVersion"], 2);
    assert_eq!(check_json["prompt"]["content"], v2_content);

    // Mock OpenAI
    mock_openai(
        &openai_mock,
        "sk-prompt-rebind",
        openai_response("Rebound reply"),
    )
    .await;

    // Create customer + conversation + message
    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Rebind Customer")
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
         VALUES ($1, $2, 'customer', 'Help me!') RETURNING id",
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

    // Process
    let processed = process_agent_responder_once(&pool, &state.ai, &state.escalations)
        .await
        .expect("responder should succeed");
    assert!(processed, "responder should have processed an event");

    // Assert the outbound request contains V2 content (not V1)
    let requests = openai_mock.received_requests().await.unwrap();
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
        system_msg.contains("says goodbye"),
        "system message should contain V2 prompt content (says goodbye), got: {system_msg}"
    );
    assert!(
        !system_msg.contains("says hello"),
        "system message should NOT contain V1 prompt content (says hello), got: {system_msg}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// T020 — RBAC: unauthorized roles get 403
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn unauthorized_roles_get_403_for_prompt_endpoints() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let state = plain_state(pool.clone());

    for role in ["manager", "agent", "viewer"] {
        let tenant_id = seed_tenant(&pool).await;
        let user_id = seed_user(&pool, &format!("t020-{role}@test.com"), role).await;
        seed_membership(&pool, tenant_id, user_id, role).await;

        // GET → 403
        let get_resp = send(
            &state,
            auth_get("/api/v1/tenant/ai/agent/prompt", user_id, tenant_id),
        )
        .await;
        assert_eq!(
            get_resp.status(),
            StatusCode::FORBIDDEN,
            "GET should be 403 for {role}"
        );

        // PUT → 403
        let put_payload = serde_json::json!({
            "content": "test {{agent_name}}",
            "changeNote": null,
            "baseVersion": 0,
        });
        let put_resp = send(
            &state,
            json_put(
                "/api/v1/tenant/ai/agent/prompt",
                user_id,
                tenant_id,
                put_payload,
            ),
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
    let owner_id = seed_user(&pool, "t020-owner@test.com", "owner").await;
    seed_membership(&pool, owner_tenant, owner_id, "owner").await;

    let get_owner = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent/prompt", owner_id, owner_tenant),
    )
    .await;
    assert_eq!(get_owner.status(), StatusCode::OK);

    let put_owner = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            owner_id,
            owner_tenant,
            serde_json::json!({
                "content": "Owner prompt {{agent_name}}.",
                "changeNote": "owner",
                "baseVersion": 0,
            }),
        ),
    )
    .await;
    assert_eq!(put_owner.status(), StatusCode::OK);

    // Admin → succeeds
    let admin_tenant = seed_tenant(&pool).await;
    let admin_id = seed_user(&pool, "t020-admin@test.com", "admin").await;
    seed_membership(&pool, admin_tenant, admin_id, "admin").await;

    let get_admin = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent/prompt", admin_id, admin_tenant),
    )
    .await;
    assert_eq!(get_admin.status(), StatusCode::OK);

    let put_admin = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            admin_id,
            admin_tenant,
            serde_json::json!({
                "content": "Admin prompt {{agent_name}}.",
                "changeNote": "admin",
                "baseVersion": 0,
            }),
        ),
    )
    .await;
    assert_eq!(put_admin.status(), StatusCode::OK);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T043a — History pagination: save >25 versions, page through, assert hasMore flips
//          and every version appears once
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn prompt_version_history_pagination() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t043a@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    let count = 30i32;
    let mut all_versions = Vec::new();
    for i in 1..=count {
        let content = format!("Version {} content {{agent_name}}.", i);
        let base_version = if i == 1 { 0 } else { i - 1 };
        let resp = send(
            &state,
            json_put(
                "/api/v1/tenant/ai/agent/prompt",
                user_id,
                tenant_id,
                serde_json::json!({
                    "content": content,
                    "changeNote": format!("v{}", i),
                    "baseVersion": base_version,
                }),
            ),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK, "save v{i} failed");
        all_versions.push(i);
    }

    // First page: limit=25, no before → expect 25 items, hasMore=true
    let page1 = send(
        &state,
        auth_get(
            "/api/v1/tenant/ai/agent/prompt/versions?limit=25",
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(page1.status(), StatusCode::OK);
    let page1_json = body_json(page1).await;
    let items1 = page1_json["items"].as_array().unwrap();
    assert_eq!(items1.len(), 25, "first page should have 25 items");
    assert_eq!(page1_json["hasMore"], true, "first page should have more");

    let page1_nums: Vec<i32> = items1
        .iter()
        .map(|v| v["versionNumber"].as_i64().unwrap() as i32)
        .collect();
    let expected_page1: Vec<i32> = (6..=30).rev().collect(); // 30, 29, ..., 6
    assert_eq!(page1_nums, expected_page1, "first page version numbers");

    // Second page: limit=25, before=lowest from first page (6)
    let page2 = send(
        &state,
        auth_get(
            "/api/v1/tenant/ai/agent/prompt/versions?limit=25&before=6",
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(page2.status(), StatusCode::OK);
    let page2_json = body_json(page2).await;
    let items2 = page2_json["items"].as_array().unwrap();
    assert_eq!(items2.len(), 5, "second page should have 5 items");
    assert_eq!(
        page2_json["hasMore"], false,
        "second page should have no more"
    );

    let page2_nums: Vec<i32> = items2
        .iter()
        .map(|v| v["versionNumber"].as_i64().unwrap() as i32)
        .collect();
    let expected_page2: Vec<i32> = (1..=5).rev().collect(); // 5, 4, 3, 2, 1
    assert_eq!(page2_nums, expected_page2, "second page version numbers");

    // Every version appears exactly once
    let all_seen: Vec<i32> = page1_nums.into_iter().chain(page2_nums).collect();
    assert_eq!(all_seen.len() as i32, count, "total count");
    for v in 1..=count {
        assert!(all_seen.contains(&v), "version {v} should appear once");
    }

    // Latest version is marked as active
    assert_eq!(items1[0]["isActive"], true, "v30 should be active");
    assert_eq!(items1[0]["versionNumber"], 30);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T043b — Version detail 200 + 404 for unknown number + 404 cross-tenant
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn prompt_version_detail_scenarios() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t043b@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let other_tenant = seed_tenant(&pool).await;
    let other_user = seed_user(&pool, "t043b-other@test.com", "admin").await;
    seed_membership(&pool, other_tenant, other_user, "admin").await;

    let state = plain_state(pool.clone());

    // Save v1
    let v1_content = "Detail test prompt {{agent_name}}.";
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": v1_content,
                "changeNote": "v1",
                "baseVersion": 0,
            }),
        ),
    )
    .await;

    // Get v1 detail → 200
    let detail = send(
        &state,
        auth_get(
            "/api/v1/tenant/ai/agent/prompt/versions/1",
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(detail.status(), StatusCode::OK);
    let detail_json = body_json(detail).await;
    assert_eq!(detail_json["versionNumber"], 1);
    assert_eq!(detail_json["content"], v1_content);
    assert_eq!(detail_json["isActive"], true);
    assert!(detail_json["createdBy"]
        .as_str()
        .unwrap_or("")
        .contains("AI Agent Prompt Test User"));

    // Get non-existent version → 404
    let missing = send(
        &state,
        auth_get(
            "/api/v1/tenant/ai/agent/prompt/versions/999",
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    // Cross-tenant → 404
    let cross = send(
        &state,
        auth_get(
            "/api/v1/tenant/ai/agent/prompt/versions/1",
            other_user,
            other_tenant,
        ),
    )
    .await;
    assert_eq!(cross.status(), StatusCode::NOT_FOUND);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T043c — Restore happy path: restore older version, assert new version
//         content = source, restoredFrom set, becomes active
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn restore_older_version_creates_new_version() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t043c@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    // Save v1
    let v1_content = "Original v1 content {{agent_name}}.";
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": v1_content,
                "changeNote": "v1",
                "baseVersion": 0,
            }),
        ),
    )
    .await;

    // Save v2 (different content)
    let v2_content = "Updated v2 content {{agent_name}}.";
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": v2_content,
                "changeNote": "v2",
                "baseVersion": 1,
            }),
        ),
    )
    .await;

    // Restore v1
    let restore = send(
        &state,
        json_post(
            "/api/v1/tenant/ai/agent/prompt/versions/1/restore",
            user_id,
            tenant_id,
            serde_json::json!({"baseVersion": 2}),
        ),
    )
    .await;
    assert_eq!(restore.status(), StatusCode::OK);
    let restore_json = body_json(restore).await;
    assert_eq!(restore_json["version"], 3);
    assert_eq!(restore_json["created"], true);
    assert_eq!(restore_json["restoredFrom"], 1);
    assert!(restore_json["updatedBy"]
        .as_str()
        .unwrap_or("")
        .contains("AI Agent Prompt Test User"));

    // GET bootstrap — active content should match v1
    let get_resp = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent/prompt", user_id, tenant_id),
    )
    .await;
    let get_json = body_json(get_resp).await;
    assert_eq!(get_json["prompt"]["activeVersion"], 3);
    assert_eq!(get_json["prompt"]["content"], v1_content);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T043d — Restore no-op: content identical to active → created: false
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn restore_identical_content_is_no_op() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t043d@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    let content = "No-op restore content {{agent_name}}.";

    // Save v1
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": content,
                "baseVersion": 0,
            }),
        ),
    )
    .await;

    // Restore v1 (same as active) → no-op
    let restore = send(
        &state,
        json_post(
            "/api/v1/tenant/ai/agent/prompt/versions/1/restore",
            user_id,
            tenant_id,
            serde_json::json!({"baseVersion": 1}),
        ),
    )
    .await;
    assert_eq!(restore.status(), StatusCode::OK);
    let restore_json = body_json(restore).await;
    assert_eq!(restore_json["created"], false);
    assert_eq!(restore_json["version"], 1);

    // Active version unchanged
    let get_resp = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent/prompt", user_id, tenant_id),
    )
    .await;
    let get_json = body_json(get_resp).await;
    assert_eq!(get_json["prompt"]["activeVersion"], 1);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T043e — Restore conflict: stale baseVersion → 409
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn restore_with_stale_base_version_conflicts() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t043e@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    // Save v1, v2, v3 (active = 3)
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({"content": "v1 {{agent_name}}.", "baseVersion": 0}),
        ),
    )
    .await;
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({"content": "v2 {{agent_name}}.", "baseVersion": 1}),
        ),
    )
    .await;
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({"content": "v3 {{agent_name}}.", "baseVersion": 2}),
        ),
    )
    .await;

    // Restore v1 with baseVersion 1 (stale, active is 3) → 409
    let restore = send(
        &state,
        json_post(
            "/api/v1/tenant/ai/agent/prompt/versions/1/restore",
            user_id,
            tenant_id,
            serde_json::json!({"baseVersion": 1}),
        ),
    )
    .await;
    assert_eq!(restore.status(), StatusCode::CONFLICT);
    let err_json = body_json(restore).await;
    assert_eq!(err_json["error"]["code"], "conflict");
    assert_eq!(err_json["error"]["details"][0]["activeVersion"], 3);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T043f — Restore blocked by validation: version with unknown variable → 422
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn restore_invalid_content_returns_422() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t043f@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    // Save v1 (valid)
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": "Valid prompt {{agent_name}}.",
                "baseVersion": 0,
            }),
        ),
    )
    .await;

    // Get prompt_id
    let prompt_id: Uuid = sqlx::query_scalar("SELECT id FROM agent_prompts WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    // Insert v2 with invalid content via raw SQL (bypassing validate_prompt)
    sqlx::query(
        "INSERT INTO agent_prompt_versions \
         (tenant_id, prompt_id, version_number, content, created_by_display) \
         VALUES ($1, $2, 2, 'Invalid {{business_hours}} content.', 'hacker')",
    )
    .bind(tenant_id)
    .bind(prompt_id)
    .execute(&pool)
    .await
    .unwrap();

    // Try to restore v2 → 422
    let restore = send(
        &state,
        json_post(
            "/api/v1/tenant/ai/agent/prompt/versions/2/restore",
            user_id,
            tenant_id,
            serde_json::json!({"baseVersion": 1}),
        ),
    )
    .await;
    assert_eq!(restore.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let err_json = body_json(restore).await;
    assert_eq!(err_json["error"]["code"], "validation_failed");
}

// ═══════════════════════════════════════════════════════════════════════════════
// T043g — Audit assertion for version_restored
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn restore_records_audit_event() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t043g@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    // Save v1, v2
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({"content": "v1 {{agent_name}}.", "baseVersion": 0}),
        ),
    )
    .await;
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({"content": "v2 {{agent_name}}.", "baseVersion": 1}),
        ),
    )
    .await;

    // Restore v1
    send(
        &state,
        json_post(
            "/api/v1/tenant/ai/agent/prompt/versions/1/restore",
            user_id,
            tenant_id,
            serde_json::json!({"baseVersion": 2}),
        ),
    )
    .await;

    // Assert audit log entry
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs \
         WHERE tenant_id = $1 AND action = 'agent_prompt.version_restored'",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        audit_count, 1,
        "expected exactly one version_restored audit event"
    );

    // Verify the audit details
    let audit_details: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT details FROM audit_logs \
         WHERE tenant_id = $1 AND action = 'agent_prompt.version_restored'",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    let details = audit_details.expect("audit details should exist");
    assert_eq!(details["version"], 3);
    assert_eq!(details["restored_from"], 1);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T063 — Placeholders stored raw (not sample-substituted) in version detail
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn placeholders_stored_raw_in_version_detail() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t063@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    let content = "Hi {{agent_name}} from {{tenant_name}}";

    // Save
    let put_resp = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": content,
                "changeNote": "placeholder storage test",
                "baseVersion": 0,
            }),
        ),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::OK);

    // GET version detail and assert content is byte-identical
    let detail = send(
        &state,
        auth_get(
            "/api/v1/tenant/ai/agent/prompt/versions/1",
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(detail.status(), StatusCode::OK);
    let detail_json = body_json(detail).await;
    assert_eq!(detail_json["content"], content);
    assert_eq!(
        detail_json["content"].as_str().unwrap().len(),
        content.len(),
        "byte length must match — placeholders stored raw, not sample-substituted"
    );
}

#[tokio::test]
async fn restore_cross_tenant_returns_404() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let user_a = seed_user(&pool, "t043h-a@test.com", "admin").await;
    let user_b = seed_user(&pool, "t043h-b@test.com", "admin").await;
    seed_membership(&pool, tenant_a, user_a, "admin").await;
    seed_membership(&pool, tenant_b, user_b, "admin").await;
    let state = plain_state(pool.clone());

    // Save v1 in tenant_a
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_a,
            tenant_a,
            serde_json::json!({
                "content": "Tenant A prompt {{agent_name}}.",
                "baseVersion": 0,
            }),
        ),
    )
    .await;

    // Restore from tenant_b → 404 (tenant isolation)
    let restore = send(
        &state,
        json_post(
            "/api/v1/tenant/ai/agent/prompt/versions/1/restore",
            user_b,
            tenant_b,
            serde_json::json!({"baseVersion": 0}),
        ),
    )
    .await;
    assert_eq!(restore.status(), StatusCode::NOT_FOUND);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T044 — RBAC: unauthorized roles get 403 for US2 endpoints
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn unauthorized_roles_get_403_for_prompt_version_endpoints() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let state = plain_state(pool.clone());

    for role in ["manager", "agent", "viewer"] {
        let tenant_id = seed_tenant(&pool).await;
        let user_id = seed_user(&pool, &format!("t044-{role}@test.com"), role).await;
        seed_membership(&pool, tenant_id, user_id, role).await;

        // list_prompt_versions GET → 403
        let list_resp = send(
            &state,
            auth_get(
                "/api/v1/tenant/ai/agent/prompt/versions",
                user_id,
                tenant_id,
            ),
        )
        .await;
        assert_eq!(
            list_resp.status(),
            StatusCode::FORBIDDEN,
            "list versions should be 403 for {role}"
        );

        // get_prompt_version GET → 403
        let get_resp = send(
            &state,
            auth_get(
                "/api/v1/tenant/ai/agent/prompt/versions/1",
                user_id,
                tenant_id,
            ),
        )
        .await;
        assert_eq!(
            get_resp.status(),
            StatusCode::FORBIDDEN,
            "get version should be 403 for {role}"
        );

        // restore_prompt_version POST → 403
        let restore_resp = send(
            &state,
            json_post(
                "/api/v1/tenant/ai/agent/prompt/versions/1/restore",
                user_id,
                tenant_id,
                serde_json::json!({"baseVersion": 0}),
            ),
        )
        .await;
        assert_eq!(
            restore_resp.status(),
            StatusCode::FORBIDDEN,
            "restore should be 403 for {role}"
        );
    }

    // Owner → succeeds
    let owner_tenant = seed_tenant(&pool).await;
    let owner_id = seed_user(&pool, "t044-owner@test.com", "owner").await;
    seed_membership(&pool, owner_tenant, owner_id, "owner").await;

    // Save a prompt so versions exist
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            owner_id,
            owner_tenant,
            serde_json::json!({
                "content": "Owner prompt {{agent_name}}.",
                "baseVersion": 0,
            }),
        ),
    )
    .await;

    let list_owner = send(
        &state,
        auth_get(
            "/api/v1/tenant/ai/agent/prompt/versions",
            owner_id,
            owner_tenant,
        ),
    )
    .await;
    assert_eq!(list_owner.status(), StatusCode::OK);

    let get_owner = send(
        &state,
        auth_get(
            "/api/v1/tenant/ai/agent/prompt/versions/1",
            owner_id,
            owner_tenant,
        ),
    )
    .await;
    assert_eq!(get_owner.status(), StatusCode::OK);

    let restore_owner = send(
        &state,
        json_post(
            "/api/v1/tenant/ai/agent/prompt/versions/1/restore",
            owner_id,
            owner_tenant,
            serde_json::json!({"baseVersion": 1}),
        ),
    )
    .await;
    assert_eq!(restore_owner.status(), StatusCode::OK);

    // Admin → succeeds
    let admin_tenant = seed_tenant(&pool).await;
    let admin_id = seed_user(&pool, "t044-admin@test.com", "admin").await;
    seed_membership(&pool, admin_tenant, admin_id, "admin").await;

    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            admin_id,
            admin_tenant,
            serde_json::json!({
                "content": "Admin prompt {{agent_name}}.",
                "baseVersion": 0,
            }),
        ),
    )
    .await;

    let list_admin = send(
        &state,
        auth_get(
            "/api/v1/tenant/ai/agent/prompt/versions",
            admin_id,
            admin_tenant,
        ),
    )
    .await;
    assert_eq!(list_admin.status(), StatusCode::OK);

    let get_admin = send(
        &state,
        auth_get(
            "/api/v1/tenant/ai/agent/prompt/versions/1",
            admin_id,
            admin_tenant,
        ),
    )
    .await;
    assert_eq!(get_admin.status(), StatusCode::OK);

    let restore_admin = send(
        &state,
        json_post(
            "/api/v1/tenant/ai/agent/prompt/versions/1/restore",
            admin_id,
            admin_tenant,
            serde_json::json!({"baseVersion": 1}),
        ),
    )
    .await;
    assert_eq!(restore_admin.status(), StatusCode::OK);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T066 — Validation rejection tests for PUT (US4)
// ═══════════════════════════════════════════════════════════════════════════════

async fn seed_user_with_display(pool: &sqlx::PgPool, email: &str, display_name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind(display_name)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn put_and_expect_422(
    state: &AppState,
    user_id: Uuid,
    tenant_id: Uuid,
    content: &str,
) -> serde_json::Value {
    let resp = send(
        state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({"content": content, "baseVersion": 1}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let j = body_json(resp).await;
    assert_eq!(j["error"]["code"], "validation_failed");
    j
}

async fn assert_bootstrap_unchanged(
    state: &AppState,
    user_id: Uuid,
    tenant_id: Uuid,
    expected_version: i32,
    expected_content: &str,
) {
    let g = send(
        state,
        auth_get("/api/v1/tenant/ai/agent/prompt", user_id, tenant_id),
    )
    .await;
    let gj = body_json(g).await;
    assert_eq!(gj["prompt"]["activeVersion"], expected_version);
    assert_eq!(gj["prompt"]["content"], expected_content);
}

#[tokio::test]
async fn put_prompt_unknown_variable_returns_422() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t066-unknown-var@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    let valid = "Valid prompt {{agent_name}}.";
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({"content": valid, "baseVersion": 0}),
        ),
    )
    .await;

    let j = put_and_expect_422(&state, user_id, tenant_id, "{{business_hours}}").await;
    assert_eq!(j["error"]["details"][0]["code"], "unknown_variable");
    let msg = j["error"]["details"][0]["message"].as_str().unwrap();
    assert!(msg.contains("business_hours"));
    assert!(msg.contains("offset"));

    assert_bootstrap_unchanged(&state, user_id, tenant_id, 1, valid).await;
}

#[tokio::test]
async fn put_prompt_unclosed_placeholder_returns_422() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t066-unclosed@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    let valid = "Valid prompt {{agent_name}}.";
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({"content": valid, "baseVersion": 0}),
        ),
    )
    .await;

    let j = put_and_expect_422(&state, user_id, tenant_id, "{{agent_name").await;
    assert_eq!(j["error"]["details"][0]["code"], "malformed_placeholder");

    assert_bootstrap_unchanged(&state, user_id, tenant_id, 1, valid).await;
}

#[tokio::test]
async fn put_prompt_stray_closing_braces_returns_422() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t066-stray@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    let valid = "Valid prompt {{agent_name}}.";
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({"content": valid, "baseVersion": 0}),
        ),
    )
    .await;

    let j = put_and_expect_422(&state, user_id, tenant_id, "stray }} braces").await;
    assert_eq!(j["error"]["details"][0]["code"], "malformed_placeholder");

    assert_bootstrap_unchanged(&state, user_id, tenant_id, 1, valid).await;
}

#[tokio::test]
async fn put_prompt_empty_content_returns_422() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t066-empty@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    let valid = "Valid prompt {{agent_name}}.";
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({"content": valid, "baseVersion": 0}),
        ),
    )
    .await;

    let j = put_and_expect_422(&state, user_id, tenant_id, "").await;
    assert_eq!(j["error"]["details"][0]["code"], "required");

    assert_bootstrap_unchanged(&state, user_id, tenant_id, 1, valid).await;
}

#[tokio::test]
async fn put_prompt_content_too_long_returns_422() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t066-too-long@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    let valid = "Valid prompt {{agent_name}}.";
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({"content": valid, "baseVersion": 0}),
        ),
    )
    .await;

    let long = "z".repeat(8001);
    let j = put_and_expect_422(&state, user_id, tenant_id, &long).await;
    assert_eq!(j["error"]["details"][0]["code"], "too_long");

    assert_bootstrap_unchanged(&state, user_id, tenant_id, 1, valid).await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// T069 — US5 dedicated audit tests
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn save_creates_version_created_audit_event() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t069-vc@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    let content = "Audit test prompt {{agent_name}}.";
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": content,
                "changeNote": "audit test",
                "baseVersion": 0,
            }),
        ),
    )
    .await;

    let (
        audit_action,
        audit_actor,
        audit_tenant,
        audit_resource_type,
        _audit_resource_id,
        audit_details,
    ): (String, Uuid, Uuid, String, String, serde_json::Value) = sqlx::query_as(
        "SELECT action, actor_user_id, tenant_id, resource_type, resource_id, details \
         FROM audit_logs WHERE action = 'agent_prompt.version_created' AND tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(audit_action, "agent_prompt.version_created");
    assert_eq!(audit_actor, user_id);
    assert_eq!(audit_tenant, tenant_id);
    assert_eq!(audit_resource_type, "agent_prompt");
    assert_eq!(audit_details["version"], 1);
    assert!(audit_details["content_length"].as_i64().unwrap() > 0);
    assert_eq!(audit_details["has_change_note"], true);
    assert!(audit_details.get("content").is_none());
}

#[tokio::test]
async fn restore_creates_version_restored_audit_event() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t069-vr@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({"content": "v1 {{agent_name}}.", "baseVersion": 0}),
        ),
    )
    .await;
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({"content": "v2 {{agent_name}}.", "baseVersion": 1}),
        ),
    )
    .await;

    send(
        &state,
        json_post(
            "/api/v1/tenant/ai/agent/prompt/versions/1/restore",
            user_id,
            tenant_id,
            serde_json::json!({"baseVersion": 2}),
        ),
    )
    .await;

    let (audit_action, audit_details): (String, serde_json::Value) = sqlx::query_as(
        "SELECT action, details FROM audit_logs \
         WHERE action = 'agent_prompt.version_restored' AND tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(audit_action, "agent_prompt.version_restored");
    assert_eq!(audit_details["version"], 3);
    assert_eq!(audit_details["restored_from"], 1);
}

#[tokio::test]
async fn prompt_history_created_by_shows_each_users_display_name() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let user_a = seed_user_with_display(&pool, "t069-user-a@test.com", "Alice Admin").await;
    let user_b = seed_user_with_display(&pool, "t069-user-b@test.com", "Bob Builder").await;
    seed_membership(&pool, tenant_id, user_a, "admin").await;
    seed_membership(&pool, tenant_id, user_b, "admin").await;
    let state = plain_state(pool.clone());

    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_a,
            tenant_id,
            serde_json::json!({"content": "Alice version {{agent_name}}.", "baseVersion": 0}),
        ),
    )
    .await;

    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_b,
            tenant_id,
            serde_json::json!({"content": "Bob version {{agent_name}}.", "baseVersion": 1}),
        ),
    )
    .await;

    let g = send(
        &state,
        auth_get("/api/v1/tenant/ai/agent/prompt/versions", user_a, tenant_id),
    )
    .await;
    let gj = body_json(g).await;
    let items = gj["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["createdBy"], "Bob Builder");
    assert_eq!(items[0]["versionNumber"], 2);
    assert_eq!(items[1]["createdBy"], "Alice Admin");
    assert_eq!(items[1]["versionNumber"], 1);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T070 — Snapshot attribution test
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn created_by_is_snapshot_not_live_lookup() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user_with_display(&pool, "t070@test.com", "Original Name").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({"content": "Snapshot test {{agent_name}}.", "baseVersion": 0}),
        ),
    )
    .await;

    sqlx::query("UPDATE users SET display_name = 'Changed Name' WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

    let detail = send(
        &state,
        auth_get(
            "/api/v1/tenant/ai/agent/prompt/versions/1",
            user_id,
            tenant_id,
        ),
    )
    .await;
    let dj = body_json(detail).await;
    assert_eq!(dj["createdBy"], "Original Name");
    assert_ne!(dj["createdBy"], "Changed Name");

    let history = send(
        &state,
        auth_get(
            "/api/v1/tenant/ai/agent/prompt/versions",
            user_id,
            tenant_id,
        ),
    )
    .await;
    let hj = body_json(history).await;
    assert_eq!(hj["items"][0]["createdBy"], "Original Name");
}

// ═══════════════════════════════════════════════════════════════════════════════
// T085 — changeNote length validation
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn change_note_too_long_returns_422() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t085@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    let valid_content = "Valid prompt {{agent_name}}.";
    // Create an initial version
    send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": valid_content,
                "changeNote": "initial",
                "baseVersion": 0,
            }),
        ),
    )
    .await;

    // PUT with a 501-char changeNote → 422
    let long_note = "x".repeat(501);
    let resp = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent/prompt",
            user_id,
            tenant_id,
            serde_json::json!({
                "content": "Updated content {{agent_name}}.",
                "changeNote": long_note,
                "baseVersion": 1,
            }),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let j = body_json(resp).await;
    assert_eq!(j["error"]["code"], "validation_failed");
    assert_eq!(j["error"]["details"][0]["field"], "changeNote");
    assert_eq!(j["error"]["details"][0]["code"], "invalid_length");

    // Bootstrap unchanged — no version was created
    assert_bootstrap_unchanged(&state, user_id, tenant_id, 1, valid_content).await;
}
