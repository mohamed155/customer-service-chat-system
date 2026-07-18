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
            eprintln!("skipping engine_supersede tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping engine_supersede tests: DATABASE_URL is unreachable");
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
         outbox_events, audit_logs, tenant_invitations, tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("failed to reset test tables");
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    let slug = format!("engine-supersede-tenant-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Engine Supersede Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Engine Supersede Test User")
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

// ── HTTP Helpers ───────────────────────────────────────────────────────────────

async fn send(state: &AppState, request: Request<Body>) -> axum::response::Response {
    router::app_with_test_routes(state.clone())
        .oneshot(request)
        .await
        .expect("request should complete")
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

// ═══════════════════════════════════════════════════════════════════════════════
// T021 — US3: Scenario 2 — Supersede on rapid second customer message
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn engine_supersede_rapid_second_message() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t021-sc2@test.com").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    // Seed platform AI config + credential
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-t021-sc2-key", &master).await;

    // PUT agent config
    let agent_payload = serde_json::json!({
        "name": "T021 Scenario 2 Agent",
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

    // Wiremock: delayed response so the engine has time to process a second message
    let mock_response = serde_json::json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "model": "gpt-4",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "AI response after supersede." },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 50, "completion_tokens": 10, "total_tokens": 60 }
    });
    Mock::given(wm_method("POST"))
        .and(wm_path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(2))
                .set_body_json(mock_response),
        )
        .mount(&openai_mock)
        .await;

    // Create a customer for the conversation
    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("T021 SC2 Customer")
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

    // ── First customer message + outbox ────────────────────────────────────
    let message_id_1 = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'First message') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let outbox_payload_1 = serde_json::json!({
        "conversation_id": conversation_id,
        "message_id": message_id_1,
        "channel": "web_chat",
    });
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.customer_message', $4)",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(tenant_id)
    .bind(outbox_payload_1)
    .execute(&pool)
    .await
    .unwrap();

    // ── Second customer message + outbox (immediately after the first) ────
    let message_id_2 = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'Second message — should supersede the first') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let outbox_payload_2 = serde_json::json!({
        "conversation_id": conversation_id,
        "message_id": message_id_2,
        "channel": "web_chat",
    });
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.customer_message', $4)",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(tenant_id)
    .bind(outbox_payload_2)
    .execute(&pool)
    .await
    .unwrap();

    // ── Drive the responder until idle (up to 30 iterations) ──────────────
    let mut processed = false;
    for i in 0..30 {
        match process_agent_responder_once(&pool, &state.ai, &state.escalations).await {
            Ok(true) => {
                processed = true;
            }
            Ok(false) => break,
            Err(e) => panic!("agent responder error at iteration {i}: {e}"),
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        processed,
        "agent responder should have processed at least one event"
    );

    // ════════════════════════ Assertions ════════════════════════

    // Exactly one ai-kind message (the second message's reply)
    let ai_messages: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, body FROM messages \
         WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        ai_messages.len(),
        1,
        "expected exactly one AI reply (second message should produce a response, \
         first should be superseded)"
    );
    assert_eq!(ai_messages[0].1, "AI response after supersede.");

    // ai_generations has exactly 2 rows: one superseded, one success
    let gen_rows: Vec<(Uuid, String, Option<Uuid>)> = sqlx::query_as(
        "SELECT id, outcome, response_message_id FROM ai_generations \
         WHERE tenant_id = $1 AND conversation_id = $2 \
         ORDER BY created_at ASC",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        gen_rows.len(),
        2,
        "expected exactly two ai_generations rows"
    );

    let (_gen_id_1, outcome_1, resp_msg_id_1) = &gen_rows[0];
    let (_gen_id_2, outcome_2, resp_msg_id_2) = &gen_rows[1];

    // First generation (triggered by the first customer message) should be superseded
    // Second generation (triggered by the second customer message) should succeed
    let outcomes: Vec<&str> = gen_rows.iter().map(|(_, o, _)| o.as_str()).collect();
    assert!(
        outcomes.contains(&"superseded"),
        "expected one superseded generation, got: {outcomes:?}"
    );
    assert!(
        outcomes.contains(&"success"),
        "expected one success generation, got: {outcomes:?}"
    );

    // The success row has a non-null response_message_id pointing to the stored AI reply
    let success_row = gen_rows.iter().find(|(_, o, _)| o == "success").unwrap();
    assert!(
        success_row.2.is_some(),
        "success generation should have a non-null response_message_id"
    );
    assert_eq!(
        success_row.2.unwrap(),
        ai_messages[0].0,
        "response_message_id should match the stored ai message id"
    );

    // The superseded row has a null response_message_id
    let superseded_row = gen_rows.iter().find(|(_, o, _)| o == "superseded").unwrap();
    assert!(
        superseded_row.2.is_none(),
        "superseded generation should have null response_message_id"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// T021 — US3: Scenario 3 — Escalation cancels in-flight generation
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn engine_supersede_escalation_cancels_generation() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t021-sc3@test.com").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    // Seed platform AI config + credential
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-t021-sc3-key", &master).await;

    // PUT agent config
    let agent_payload = serde_json::json!({
        "name": "T021 Scenario 3 Agent",
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

    // Wiremock: respond normally (no delay)
    let mock_response = serde_json::json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "model": "gpt-4",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "This should not be stored." },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 50, "completion_tokens": 10, "total_tokens": 60 }
    });
    Mock::given(wm_method("POST"))
        .and(wm_path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_response))
        .mount(&openai_mock)
        .await;

    // Create a customer for the conversation
    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("T021 SC3 Customer")
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

    // Insert a customer message + outbox event
    let message_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'Help me please') RETURNING id",
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

    // Insert an open escalation before running the responder
    // (status = 'queued' is the non-closed, non-assigned status that
    //  escalations::routing::has_open_escalation checks for)
    sqlx::query(
        "INSERT INTO escalations (tenant_id, conversation_id, status, reason) \
         VALUES ($1, $2, 'queued', 'manual takeover')",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .execute(&pool)
    .await
    .unwrap();

    // ── Drive the responder once ──────────────────────────────────────────
    let processed = process_agent_responder_once(&pool, &state.ai, &state.escalations)
        .await
        .expect("agent responder should not panic");
    assert!(
        processed,
        "responder should have processed the outbox event"
    );

    // ════════════════════════ Assertions ════════════════════════

    // No ai-kind message exists (escalation cancelled the AI response)
    let ai_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages \
         WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        ai_count, 0,
        "expected no ai-kind messages when escalation cancels generation"
    );

    // Exactly one ai_generations row with outcome = 'cancelled_escalation'
    let gen_rows: Vec<(Uuid, String, Option<Uuid>)> = sqlx::query_as(
        "SELECT id, outcome, response_message_id FROM ai_generations \
         WHERE tenant_id = $1 AND conversation_id = $2",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(gen_rows.len(), 1, "expected exactly one ai_generations row");
    let (_gen_id, outcome, resp_msg_id) = &gen_rows[0];
    assert_eq!(
        outcome.as_str(),
        "cancelled_escalation",
        "expected outcome 'cancelled_escalation', got '{outcome}'"
    );
    assert!(
        resp_msg_id.is_none(),
        "cancelled_escalation generation should have null response_message_id"
    );
}
