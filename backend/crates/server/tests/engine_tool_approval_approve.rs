use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
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
            eprintln!("skipping: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping: DATABASE_URL unreachable");
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
    let slug = format!("app-approve-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Approval Approve Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Approval Approve Test User")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_membership(pool: &sqlx::PgPool, tenant_id: Uuid, user_id: Uuid, role: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .fetch_one(pool)
    .await
    .unwrap()
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
                    "id": "call_update",
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

// Custom Responder that returns tool_call exactly once, then text the second time,
// then something else for any further calls (so the tool chain doesn't go infinite).
struct ApprovalFlowResponder {
    call_count: AtomicU8,
    tool_call: ResponseTemplate,
    text: ResponseTemplate,
}

impl Respond for ApprovalFlowResponder {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let count = self.call_count.fetch_add(1, Ordering::SeqCst);
        match count {
            0 => self.tool_call.clone(),
            1 => self.text.clone(),
            _ => {
                // Further calls return a brief text so the engine doesn't stall
                self.text.clone()
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// T041 — Approval via decide(approve=true): the tool executes, customer contact
// changes, and a second AI reply is generated.
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn engine_tool_approval_approve() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t041@test.com").await;
    let membership_id = seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    seed_ai_config(&pool, None, "openai", "gpt-4").await;
    seed_ai_credential(&pool, None, "openai", "sk-t041-test-key", &master).await;

    // Enable update_customer_contact policy (built-in approval tool)
    sqlx::query(
        "INSERT INTO tenant_tool_policies (tenant_id, tool_name, enabled, require_approval, updated_by_membership_id) \
         VALUES ($1, 'update_customer_contact', true, true, $2)",
    )
    .bind(tenant_id)
    .bind(membership_id)
    .execute(&pool)
    .await
    .unwrap();

    // PUT agent config
    let agent_payload = serde_json::json!({
        "name": "T041 Agent",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": { "provider": "openai", "model": "gpt-4" },
    });
    let bytes = serde_json::to_vec(&agent_payload).unwrap();
    let put_resp =
        server::router::app_with_test_routes(wiremock_state(pool.clone(), &openai_mock.uri()))
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/tenant/ai/agent")
                    .method(axum::http::Method::PUT)
                    .header("X-Dev-User-Id", user_id.to_string())
                    .header("X-Tenant-ID", tenant_id.to_string())
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(bytes))
                    .unwrap(),
            )
            .await
            .unwrap();
    assert_eq!(put_resp.status(), 201);

    // Create customer with initial contact data
    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name, email, phone) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Anna Approve")
    .bind("anna.old@example.com")
    .bind("+1-555-0303")
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

    // Insert a customer message that would prompt the tool
    let message_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'Please update my email to anna.new@example.com') RETURNING id",
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

    // Mock OpenAI: first call returns tool_call for update_customer_contact
    // (which triggers the awaiting_approval flow, ending the generation).
    let tool_resp = tool_call_response(
        "update_customer_contact",
        r#"{"field": "email", "value": "anna.new@example.com"}"#,
    );
    let text_resp = text_response("I've updated your email to anna.new@example.com.");

    let responder = ApprovalFlowResponder {
        call_count: AtomicU8::new(0),
        tool_call: ResponseTemplate::new(200).set_body_json(tool_resp),
        text: ResponseTemplate::new(200).set_body_json(text_resp),
    };

    Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/chat/completions"))
        .respond_with(responder)
        .mount(&openai_mock)
        .await;

    // Drive the responder for the customer message event (creates the
    // awaiting_approval request + interim message).
    let mut processed = false;
    for i in 0..10 {
        match process_agent_responder_once(&pool, &state.ai, &state.escalations).await {
            Ok(true) => {
                processed = true;
            }
            Ok(false) => break,
            Err(e) => panic!("agent responder error at iteration {i}: {e}"),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(processed, "agent responder should have processed");

    // ── Assert interim state ─────────────────────────────────────────────────
    // An interim AI message should exist (the holding message)
    let ai_messages: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, body FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        !ai_messages.is_empty(),
        "expected at least one AI message (interim)"
    );

    // A tool_requests row with status='awaiting_approval' should exist
    let pending_rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, status FROM tool_requests \
         WHERE tenant_id = $1 AND conversation_id = $2 AND status = 'awaiting_approval'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        pending_rows.len(),
        1,
        "expected exactly one awaiting_approval row"
    );
    let tool_request_id = pending_rows[0].0;

    // The ai_generations row should have outcome=awaiting_tool_approval
    let gen_outcome: String = sqlx::query_scalar(
        "SELECT outcome FROM ai_generations WHERE tenant_id = $1 AND conversation_id = $2",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        gen_outcome, "awaiting_tool_approval",
        "generation outcome should be awaiting_tool_approval"
    );

    // ── Approve the tool request ─────────────────────────────────────────────
    let outcome = tools::approval::decide(&pool, tenant_id, tool_request_id, membership_id, true)
        .await
        .expect("decide should succeed");

    match &outcome {
        tools::approval::DecideOutcome::Applied(row) => {
            assert_eq!(
                row.status, "approved",
                "row should be approved after approve(true)"
            );
        }
        _ => panic!("expected Applied outcome, got {:?}", outcome),
    }

    // Drive the responder again to consume the ai.tool_decision event
    let mut follow_up_processed = false;
    for i in 0..20 {
        match process_agent_responder_once(&pool, &state.ai, &state.escalations).await {
            Ok(true) => {
                follow_up_processed = true;
            }
            Ok(false) => break,
            Err(e) => panic!("responder error at iteration {i}: {e}"),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        follow_up_processed,
        "follow-up generation should have been processed"
    );

    // ── Assert final state after follow-up ───────────────────────────────────
    // The tool should have executed (status changed from awaiting_approval)
    let updated_row: (
        String,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as("SELECT status, started_at, finished_at FROM tool_requests WHERE id = $1")
        .bind(tool_request_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        updated_row.0, "succeeded",
        "tool should have status 'succeeded' after approve+execute"
    );
    assert!(
        updated_row.1.is_some(),
        "started_at should be set after execution"
    );
    assert!(
        updated_row.2.is_some(),
        "finished_at should be set after execution"
    );

    // The customer's email should have been updated by the tool
    let customer_email: Option<String> =
        sqlx::query_scalar("SELECT email FROM customers WHERE id = $1 AND tenant_id = $2")
            .bind(customer_id)
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        customer_email.as_deref(),
        Some("anna.new@example.com"),
        "customer email should be updated by the tool"
    );

    // A second AI reply should exist
    let ai_messages_after: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, body FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        ai_messages_after.len() >= 2,
        "expected at least 2 AI messages (interim + follow-up), got {}",
        ai_messages_after.len()
    );
}
