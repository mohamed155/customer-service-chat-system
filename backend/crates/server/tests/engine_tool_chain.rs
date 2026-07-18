use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ai::agent_responder::process_agent_responder_once;
use ai::crypto::{self, MasterKey};
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

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
            eprintln!("skipping engine_tool_chain tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping engine_tool_chain tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query(
        "TRUNCATE TABLE ai_generations, ai_usage_records, ai_credentials, ai_configurations, \
         agent_configurations, agent_avatar_uploads, \
         escalations, agent_availability, agent_skills, skills, \
         messages, customer_channel_identifiers, customers, conversations, \
         outbox_events, audit_logs, tenant_invitations, tenant_memberships, tenants, users, \
         tool_requests, tenant_tool_policies \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("failed to reset test tables");
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    let slug = format!("tool-chain-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Tool Chain Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Tool Chain Test User")
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

async fn seed_ai_config(pool: &sqlx::PgPool, tenant_id: Option<Uuid>, provider: &str, model: &str) {
    sqlx::query(
        "INSERT INTO ai_configurations (tenant_id, provider, model, fallbacks) \
         VALUES ($1, $2, $3, '[]'::jsonb)",
    )
    .bind(tenant_id)
    .bind(provider)
    .bind(model)
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

fn tool_call_response(tool_name: &str, arguments: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "chatcmpl-test-tool",
        "object": "chat.completion",
        "model": "gpt-4",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_lookup",
                    "type": "function",
                    "function": {
                        "name": tool_name,
                        "arguments": arguments
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": { "prompt_tokens": 50, "completion_tokens": 10, "total_tokens": 60 }
    })
}

fn text_response(content: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "chatcmpl-test-text",
        "object": "chat.completion",
        "model": "gpt-4",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": content },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 50, "completion_tokens": 10, "total_tokens": 60 }
    })
}

struct ToolThenTextResponder {
    called: AtomicBool,
    tool_call: ResponseTemplate,
    text: ResponseTemplate,
}

impl Respond for ToolThenTextResponder {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        if self.called.fetch_or(true, Ordering::SeqCst) {
            self.text.clone()
        } else {
            self.tool_call.clone()
        }
    }
}

// ── T025: Auto tool executes (lookup_customer) ─────────────────────────────────

#[tokio::test]
async fn engine_tool_chain_executes_lookup_customer() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t025@test.com").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    seed_ai_config(&pool, None, "openai", "gpt-4").await;
    seed_ai_credential(&pool, None, "openai", "sk-t025-test-key", &master).await;

    // Enable lookup_customer via policy
    sqlx::query(
        "INSERT INTO tenant_tool_policies (tenant_id, tool_name, enabled, require_approval, updated_by_membership_id) \
         VALUES ($1, 'lookup_customer', true, false, $2)",
    )
    .bind(tenant_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    // PUT agent config
    let agent_payload = serde_json::json!({
        "name": "T025 Agent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": { "provider": "openai", "model": "gpt-4" },
    });
    let app_state_for_put = wiremock_state(pool.clone(), &openai_mock.uri());
    let bytes = serde_json::to_vec(&agent_payload).unwrap();
    let put_resp = axum::body::Body::from(bytes);
    let req = axum::http::Request::builder()
        .uri("/api/v1/tenant/ai/agent")
        .method(axum::http::Method::PUT)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("content-type", "application/json")
        .body(put_resp)
        .unwrap();
    let put_resp = server::router::app_with_test_routes(app_state_for_put)
        .oneshot(req)
        .await
        .expect("agent config put");
    assert_eq!(put_resp.status(), 201);

    // Create customer with distinctive profile data
    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name, email, phone) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Maya Chen")
    .bind("maya@example.com")
    .bind("+1-555-0100")
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

    // Insert a customer message that would prompt tool use
    let message_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'What do you know about me?') RETURNING id",
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

    // Mock OpenAI: first call returns tool_call, second returns text with profile data
    let tool_response = tool_call_response("lookup_customer", "{}");
    let text_resp = text_response("I found your profile, Maya Chen! Your email is maya@example.com and your phone is +1-555-0100.");

    let responder = ToolThenTextResponder {
        called: AtomicBool::new(false),
        tool_call: ResponseTemplate::new(200).set_body_json(tool_response),
        text: ResponseTemplate::new(200).set_body_json(text_resp),
    };

    Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/chat/completions"))
        .respond_with(responder)
        .mount(&openai_mock)
        .await;

    // Drive the agent responder
    let mut processed = false;
    for i in 0..20 {
        match process_agent_responder_once(&pool, &state.ai, &state.escalations).await {
            Ok(true) => {
                processed = true;
            }
            Ok(false) => break,
            Err(e) => panic!("agent responder error at iteration {i}: {e}"),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(processed, "agent responder should have processed the event");

    // Assert AI reply reflects the profile data
    let ai_messages: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, body FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(!ai_messages.is_empty(), "expected at least one AI reply");
    let (_, ai_body) = &ai_messages[0];
    assert!(
        ai_body.contains("Maya Chen"),
        "AI reply should contain customer name"
    );
    assert!(
        ai_body.contains("maya@example.com"),
        "AI reply should contain customer email"
    );

    // Assert exactly one tool_requests row with status='succeeded', builtin, chain_index=0
    let tool_rows: Vec<(
        String,
        String,
        i16,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    )> = sqlx::query_as(
        "SELECT status, tool_source, chain_index, started_at, finished_at \
             FROM tool_requests WHERE tenant_id = $1 AND conversation_id = $2",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert!(
        !tool_rows.is_empty(),
        "expected at least one tool_requests row"
    );
    let succeeded_rows: Vec<_> = tool_rows
        .iter()
        .filter(|(s, _, _, _, _)| s == "succeeded")
        .collect();
    assert_eq!(
        succeeded_rows.len(),
        1,
        "expected exactly one succeeded row"
    );

    let (status, tool_source, chain_index, started_at, finished_at) = &succeeded_rows[0];
    assert_eq!(*status, "succeeded");
    assert_eq!(*tool_source, "builtin");
    assert_eq!(*chain_index, 0);
    assert!(
        started_at.is_some(),
        "started_at should be non-null for succeeded tool"
    );
    assert!(
        finished_at.is_some(),
        "finished_at should be non-null for succeeded tool"
    );
}
