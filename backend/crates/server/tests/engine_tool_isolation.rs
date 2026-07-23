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
        integration_secrets_key: None,
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
            eprintln!("skipping engine_tool_isolation tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping engine_tool_isolation tests: DATABASE_URL is unreachable");
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

async fn seed_tenant(pool: &sqlx::PgPool, label: &str) -> Uuid {
    let slug = format!("tool-iso-{}-{}", label, Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(format!("Tool Isolation Tenant {}", label))
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Tool Isolation Test User")
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

async fn seed_ai_config(pool: &sqlx::PgPool, provider: &str, model: &str) {
    sqlx::query(
        "INSERT INTO ai_configurations (tenant_id, provider, model, fallbacks) \
         VALUES (NULL, $1, $2, '[]'::jsonb)",
    )
    .bind(provider)
    .bind(model)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_ai_credential(
    pool: &sqlx::PgPool,
    provider: &str,
    api_key: &str,
    master: &MasterKey,
) {
    let aad = crypto::aad(None, provider);
    let (ciphertext, nonce) = crypto::seal(master, &aad, api_key).unwrap();
    let hint = crypto::hint(api_key);
    sqlx::query(
        "INSERT INTO ai_credentials (tenant_id, provider, ciphertext, nonce, key_hint) \
         VALUES (NULL, $1, $2, $3, $4)",
    )
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

// ═══════════════════════════════════════════════════════════════════════════════
// T026 — Tool isolation (SC-003): two tenants A and B, drive B, assert B's data
// is never mixed with A's.
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn engine_tool_isolation() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let master = master_key();

    // ── Tenant A ─────────────────────────────────────────────────────────────
    let tenant_a = seed_tenant(&pool, "A").await;
    let user_a = seed_user(&pool, "tool-iso-a@test.com").await;
    seed_membership(&pool, tenant_a, user_a, "admin").await;

    // Agent config for tenant A
    let agent_payload_a = serde_json::json!({
        "name": "Tenant-A-Assistant",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": { "provider": "openai", "model": "gpt-4" },
    });
    let bytes_a = serde_json::to_vec(&agent_payload_a).unwrap();
    let put_a =
        server::router::app_with_test_routes(wiremock_state(pool.clone(), &openai_mock.uri()))
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/tenant/ai/agent")
                    .method(axum::http::Method::PUT)
                    .header("X-Dev-User-Id", user_a.to_string())
                    .header("X-Tenant-ID", tenant_a.to_string())
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(bytes_a))
                    .unwrap(),
            )
            .await
            .unwrap();
    assert_eq!(put_a.status(), 201);

    // Enable lookup_customer for tenant A
    sqlx::query(
        "INSERT INTO tenant_tool_policies (tenant_id, tool_name, enabled, require_approval, updated_by_membership_id) \
         VALUES ($1, 'lookup_customer', true, false, $2)",
    )
    .bind(tenant_a)
    .bind(user_a)
    .execute(&pool)
    .await
    .unwrap();

    // Tenant A customer with distinctive data
    let customer_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name, email, phone) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(tenant_a)
    .bind("Alice TenantA")
    .bind("alice@tenanta.com")
    .bind("+1-555-0101")
    .fetch_one(&pool)
    .await
    .unwrap();

    // Tenant A conversation
    let conv_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_a)
    .bind(customer_a)
    .fetch_one(&pool)
    .await
    .unwrap();

    // ── Tenant B ─────────────────────────────────────────────────────────────
    let tenant_b = seed_tenant(&pool, "B").await;
    let user_b = seed_user(&pool, "tool-iso-b@test.com").await;
    seed_membership(&pool, tenant_b, user_b, "admin").await;

    let agent_payload_b = serde_json::json!({
        "name": "Tenant-B-Assistant",
        "avatar": { "kind": "preset", "preset": "nova" },
        "tone": "friendly",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": { "provider": "openai", "model": "gpt-4" },
    });
    let bytes_b = serde_json::to_vec(&agent_payload_b).unwrap();
    let put_b =
        server::router::app_with_test_routes(wiremock_state(pool.clone(), &openai_mock.uri()))
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/tenant/ai/agent")
                    .method(axum::http::Method::PUT)
                    .header("X-Dev-User-Id", user_b.to_string())
                    .header("X-Tenant-ID", tenant_b.to_string())
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(bytes_b))
                    .unwrap(),
            )
            .await
            .unwrap();
    assert_eq!(put_b.status(), 201);

    // Enable lookup_customer for tenant B
    sqlx::query(
        "INSERT INTO tenant_tool_policies (tenant_id, tool_name, enabled, require_approval, updated_by_membership_id) \
         VALUES ($1, 'lookup_customer', true, false, $2)",
    )
    .bind(tenant_b)
    .bind(user_b)
    .execute(&pool)
    .await
    .unwrap();

    // Tenant B customer with distinctive data (different from A)
    let customer_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name, email, phone) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(tenant_b)
    .bind("Bob TenantB")
    .bind("bob@tenantb.com")
    .bind("+1-555-0202")
    .fetch_one(&pool)
    .await
    .unwrap();

    // Tenant B conversation
    let conv_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_b)
    .bind(customer_b)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Tenant B customer message
    let msg_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'What do you know about me?') RETURNING id",
    )
    .bind(tenant_b)
    .bind(conv_b)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Tenant B outbox event
    let outbox_payload_b = serde_json::json!({
        "conversation_id": conv_b,
        "message_id": msg_b,
        "channel": "web_chat",
    });
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.customer_message', $4)",
    )
    .bind(Uuid::new_v4())
    .bind(conv_b)
    .bind(tenant_b)
    .bind(outbox_payload_b)
    .execute(&pool)
    .await
    .unwrap();

    // Seed platform AI config + credential (shared)
    seed_ai_config(&pool, "openai", "gpt-4").await;
    seed_ai_credential(&pool, "openai", "sk-tool-iso-key", &master).await;

    // Mock OpenAI: first call tool_call for lookup_customer, second call text with B's data
    let tool_resp = tool_call_response("lookup_customer", "{}");
    let text_resp = text_response("I found Bob TenantB, email bob@tenantb.com, phone +1-555-0202");
    let responder = ToolThenTextResponder {
        called: AtomicBool::new(false),
        tool_call: ResponseTemplate::new(200).set_body_json(tool_resp),
        text: ResponseTemplate::new(200).set_body_json(text_resp),
    };
    Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/chat/completions"))
        .respond_with(responder)
        .mount(&openai_mock)
        .await;

    // Drive the responder for tenant B's event
    for _ in 0..10 {
        let processed = process_agent_responder_once(&pool, &state.ai, &state.escalations)
            .await
            .expect("agent responder should not panic");
        if !processed {
            break;
        }
    }

    // ── Assertions ───────────────────────────────────────────────────────────

    // 1. Tenant B's tool_requests row references B's conversation and has B's tenant_id
    let b_tool_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tool_requests WHERE tenant_id = $1 AND conversation_id = $2",
    )
    .bind(tenant_b)
    .bind(conv_b)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        b_tool_count > 0,
        "tenant B should have tool_requests for its conversation"
    );

    // 2. Tenant A-scoped query for B's conversation returns zero rows
    let a_cross_tool_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tool_requests WHERE tenant_id = $1 AND conversation_id = $2",
    )
    .bind(tenant_a)
    .bind(conv_b)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        a_cross_tool_count, 0,
        "tenant A should see zero tool_requests for B's conversation"
    );

    // 3. Tenant B's AI reply references B's customer data, not A's
    let b_ai_messages: Vec<(String,)> = sqlx::query_as(
        "SELECT body FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_b)
    .bind(conv_b)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        !b_ai_messages.is_empty(),
        "tenant B should have an AI reply"
    );
    let b_reply = &b_ai_messages[0].0;
    assert!(
        b_reply.contains("Bob TenantB"),
        "B's reply should reference B's customer, got: {b_reply}"
    );
    assert!(
        !b_reply.contains("Alice"),
        "B's reply must NOT reference tenant A's customer data"
    );

    // 4. Tenant B's succeeded tool_requests row has correct data
    let b_succeeded: Vec<(String, i16)> = sqlx::query_as(
        "SELECT tool_source, chain_index FROM tool_requests \
         WHERE tenant_id = $1 AND conversation_id = $2 AND status = 'succeeded'",
    )
    .bind(tenant_b)
    .bind(conv_b)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        !b_succeeded.is_empty(),
        "B should have at least one succeeded tool request"
    );
    assert_eq!(b_succeeded[0].0, "builtin");
    assert_eq!(b_succeeded[0].1, 0);
}
