#[path = "support/openai_mock.rs"]
mod openai_mock;
use std::sync::Arc;
use std::time::Duration;

use ai::agent_responder::process_agent_responder_once;
use ai::crypto::{self, MasterKey};
use axum::body::Body;
use axum::http::{Method, Request};
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
        secrets_root_key: None,
        secrets_root_key_previous: None,
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
        rate_limit_enabled: true,
        rate_limit_login_max: 10,
        rate_limit_login_window_secs: 60,
        rate_limit_max_override: 10000,
        rate_limit_fail_mode: "auth_closed".into(),
        security_headers_enabled: true,
        csp_policy:
            "default-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'".into(),
        csp_report_only: true,
        hsts_max_age: 31536000,
        otlp_endpoint: None,
        trace_sample_ratio: 1.0,
        sentry_dsn: None,
        release_sha: None,
        trusted_proxy: false,
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
        budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
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
            eprintln!("skipping billing_responder_gate tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping billing_responder_gate tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::testing::migrate().await;
    sqlx::query(
        "TRUNCATE TABLE ai_generations, ai_usage_records, ai_credentials, ai_configurations, \
         agent_configurations, agent_avatar_uploads, \
         escalations, agent_availability, agent_skills, skills, \
         messages, customer_channel_identifiers, customers, conversations, \
         outbox_events, audit_logs, tenant_invitations, tenant_memberships, \
         tenants, users, tenant_subscriptions \
         RESTART IDENTITY CASCADE",
    )
    .execute(&db::testing::owner_pool())
    .await
    .expect("failed to reset test tables");
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    let slug = format!("bill-gate-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Billing Gate Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Billing Gate Test User")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_membership(pool: &sqlx::PgPool, tenant_id: Uuid, user_id: Uuid, role: &str) {
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(tenant_id)
        .bind(user_id)
        .bind(role)
        .execute(&db::testing::seed_pool())
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
    .execute(&db::testing::seed_pool())
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
    .execute(&db::testing::seed_pool())
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
        .respond_with(openai_mock::OpenAiReply::new(body))
        .mount(mock)
        .await;
}

async fn send(state: &AppState, request: Request<Body>) -> axum::response::Response {
    router::app_with_test_routes(state.clone())
        .oneshot(request)
        .await
        .expect("request should complete")
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

async fn create_plan(
    pool: &sqlx::PgPool,
    token_allowance: Option<i64>,
    conversation_allowance: Option<i32>,
) -> Uuid {
    let name = format!("Gate Plan {}", Uuid::new_v4().simple());
    sqlx::query_scalar(
        "INSERT INTO billing_plans \
         (name, monthly_price_cents, token_allowance, conversation_allowance, is_default, status) \
         VALUES ($1, 0, $2, $3, false, 'active') RETURNING id",
    )
    .bind(&name)
    .bind(token_allowance)
    .bind(conversation_allowance)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn subscribe_tenant(pool: &sqlx::PgPool, tenant_id: Uuid, plan_id: Uuid) {
    sqlx::query(
        "INSERT INTO tenant_subscriptions (tenant_id, plan_id, status) VALUES ($1, $2, 'active')",
    )
    .bind(tenant_id)
    .bind(plan_id)
    .execute(&db::testing::seed_pool())
    .await
    .unwrap();
}

async fn insert_tokens(pool: &sqlx::PgPool, tenant_id: Uuid, input: i32, output: i32) {
    sqlx::query(
        "INSERT INTO ai_usage_records \
         (tenant_id, provider, model, input_tokens, output_tokens, status, streamed, latency_ms) \
         VALUES ($1, 'openai', 'gpt-4', $2, $3, 'success', false, 100)",
    )
    .bind(tenant_id)
    .bind(input)
    .bind(output)
    .execute(&db::testing::seed_pool())
    .await
    .unwrap();
}

async fn insert_agent_config(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    enabled_channels: &[&str],
    escalation_rules: serde_json::Value,
) {
    let channels_json = serde_json::to_value(enabled_channels).unwrap();
    sqlx::query(
        "INSERT INTO agent_configurations \
         (tenant_id, name, is_default, avatar_kind, tone, \
          business_rules, escalation_rules, enabled_channels, provider, model) \
         VALUES ($1, $2, true, 'preset', 'professional', \
                 '[]', $3, $4, 'openai', 'gpt-4')",
    )
    .bind(tenant_id)
    .bind(format!("Gate Agent {}", Uuid::new_v4().simple()))
    .bind(escalation_rules)
    .bind(channels_json)
    .execute(&db::testing::seed_pool())
    .await
    .unwrap();
}

// ═══════════════════════════════════════════════════════════════════════════════
// T035 — Billing gate
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[serial_test::serial(billing_gate)]
async fn exhausted_allowance_gate_deletes_outbox_and_blocks_ai() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());
    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t035-exhausted@test.com").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-t035-exhausted", &master).await;

    // Agent config
    insert_agent_config(&pool, tenant_id, &["web_chat"], serde_json::json!([])).await;

    // Billing: plan with 0 token allowance + exhausted usage
    let plan_id = create_plan(&pool, Some(0), Some(100)).await;
    subscribe_tenant(&pool, tenant_id, plan_id).await;
    insert_tokens(&pool, tenant_id, 100, 0).await;

    // Customer + conversation + message + outbox
    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Exhausted Gate Customer")
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();

    let conversation_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();

    let message_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'I need help with billing') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();

    let outbox_id = Uuid::new_v4();
    let outbox_payload = serde_json::json!({
        "conversation_id": conversation_id,
        "message_id": message_id,
        "channel": "web_chat",
    });
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.customer_message', $4, now())",
    )
    .bind(outbox_id)
    .bind(conversation_id.to_string())
    .bind(tenant_id.to_string())
    .bind(outbox_payload)
    .execute(&db::testing::seed_pool())
    .await
    .unwrap();

    // Run responder once
    let mut processed = false;
    for i in 0..20 {
        match process_agent_responder_once(&pool, &state.ai, &state.escalations).await {
            Ok(true) => {
                processed = true;
            }
            Ok(false) => break,
            Err(e) => panic!("responder error iteration {i}: {e}"),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(processed, "responder should have processed the event");

    // Assert outbox event deleted
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events WHERE id = $1")
        .bind(outbox_id)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap();
    assert_eq!(remaining, 0, "outbox event should have been deleted");

    // Assert no AI message was inserted
    let ai_msgs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();
    assert_eq!(ai_msgs, 0, "no AI message should have been inserted");
}

#[tokio::test]
#[serial_test::serial(billing_gate)]
async fn conversation_and_customer_message_survive_gate() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());
    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t035-survive@test.com").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-t035-survive", &master).await;
    insert_agent_config(&pool, tenant_id, &["web_chat"], serde_json::json!([])).await;

    let plan_id = create_plan(&pool, Some(0), Some(0)).await;
    subscribe_tenant(&pool, tenant_id, plan_id).await;
    insert_tokens(&pool, tenant_id, 10, 0).await;

    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Survive Gate Customer")
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();

    let conversation_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();

    let message_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'Will I survive the billing gate?') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();

    let outbox_id = Uuid::new_v4();
    let outbox_payload = serde_json::json!({
        "conversation_id": conversation_id,
        "message_id": message_id,
        "channel": "web_chat",
    });
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.customer_message', $4, now())",
    )
    .bind(outbox_id)
    .bind(conversation_id.to_string())
    .bind(tenant_id.to_string())
    .bind(outbox_payload)
    .execute(&db::testing::seed_pool())
    .await
    .unwrap();

    let mut processed = false;
    for i in 0..20 {
        match process_agent_responder_once(&pool, &state.ai, &state.escalations).await {
            Ok(true) => {
                processed = true;
            }
            Ok(false) => break,
            Err(e) => panic!("responder error iteration {i}: {e}"),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(processed);

    // Conversation still exists
    let conv_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(conversation_id)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();
    assert!(conv_exists, "conversation should still exist");

    // Customer message still exists
    let msg_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM messages WHERE id = $1)")
            .bind(message_id)
            .fetch_one(&db::testing::seed_pool())
            .await
            .unwrap();
    assert!(msg_exists, "customer message should still exist");
}

#[tokio::test]
#[serial_test::serial(billing_gate)]
async fn escalation_rule_still_matches_while_over_limit() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());
    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t035-escalation@test.com").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-t035-escalation", &master).await;

    // Agent config with a keyword escalation rule that matches "urgent"
    let escalation_rule = serde_json::json!([{
        "id": Uuid::new_v4(),
        "name": "Urgent Keyword",
        "trigger": "TopicKeywords",
        "keywords": ["urgent"],
        "requiredSkillIds": []
    }]);
    insert_agent_config(&pool, tenant_id, &["web_chat"], escalation_rule).await;

    // Billing exhausted
    let plan_id = create_plan(&pool, Some(0), Some(0)).await;
    subscribe_tenant(&pool, tenant_id, plan_id).await;
    insert_tokens(&pool, tenant_id, 10, 0).await;

    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Escalation Gate Customer")
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();

    let conversation_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();

    let message_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'This is urgent help please') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();

    let outbox_id = Uuid::new_v4();
    let outbox_payload = serde_json::json!({
        "conversation_id": conversation_id,
        "message_id": message_id,
        "channel": "web_chat",
    });
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.customer_message', $4, now())",
    )
    .bind(outbox_id)
    .bind(conversation_id.to_string())
    .bind(tenant_id.to_string())
    .bind(outbox_payload)
    .execute(&db::testing::seed_pool())
    .await
    .unwrap();

    let mut processed = false;
    for i in 0..20 {
        match process_agent_responder_once(&pool, &state.ai, &state.escalations).await {
            Ok(true) => {
                processed = true;
            }
            Ok(false) => break,
            Err(e) => panic!("responder error iteration {i}: {e}"),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(processed);

    // Outbox should be deleted (escalation path consumed it)
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events WHERE id = $1")
        .bind(outbox_id)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap();
    assert_eq!(remaining, 0, "outbox should be consumed by escalation");

    // No AI message should exist — the response was an escalation, not an AI reply
    let ai_msgs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();
    assert_eq!(ai_msgs, 0);
}

#[tokio::test]
#[serial_test::serial(billing_gate)]
async fn human_agent_reply_succeeds_while_ai_paused() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());
    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t035-human@test.com").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-t035-human", &master).await;
    insert_agent_config(&pool, tenant_id, &["web_chat"], serde_json::json!([])).await;

    // Billing exhausted
    let plan_id = create_plan(&pool, Some(0), Some(0)).await;
    subscribe_tenant(&pool, tenant_id, plan_id).await;
    insert_tokens(&pool, tenant_id, 10, 0).await;

    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Human Reply Customer")
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();

    let conversation_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();

    // POST a human reply via the API while billing is paused
    let reply_payload = serde_json::json!({
        "kind": "reply",
        "body": "I'll help you with that right now"
    });
    let uri = format!("/api/v1/tenant/conversations/{conversation_id}/messages");
    let resp = send(&state, json_post(&uri, user_id, tenant_id, reply_payload)).await;
    assert!(
        resp.status().is_success(),
        "human reply should succeed while billing is paused, got {}",
        resp.status()
    );
}

#[tokio::test]
#[serial_test::serial(billing_gate)]
async fn allowance_available_responder_proceeds_normally() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());
    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t035-allowed@test.com").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-t035-allowed", &master).await;

    // Agent config without escalation rules
    insert_agent_config(&pool, tenant_id, &["web_chat"], serde_json::json!([])).await;

    // Billing: generous allowance, not exhausted
    let plan_id = create_plan(&pool, Some(1_000_000), Some(10_000)).await;
    subscribe_tenant(&pool, tenant_id, plan_id).await;

    mock_openai(
        &openai_mock,
        "sk-t035-allowed",
        openai_response("Hello! I'm your AI assistant. How can I help you today?"),
    )
    .await;

    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Allowed Gate Customer")
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();

    let conversation_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();

    let message_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'I need assistance') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();

    let outbox_id = Uuid::new_v4();
    let outbox_payload = serde_json::json!({
        "conversation_id": conversation_id,
        "message_id": message_id,
        "channel": "web_chat",
    });
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.customer_message', $4, now())",
    )
    .bind(outbox_id)
    .bind(conversation_id.to_string())
    .bind(tenant_id.to_string())
    .bind(outbox_payload)
    .execute(&db::testing::seed_pool())
    .await
    .unwrap();

    let mut processed = false;
    for i in 0..20 {
        match process_agent_responder_once(&pool, &state.ai, &state.escalations).await {
            Ok(true) => {
                processed = true;
            }
            Ok(false) => break,
            Err(e) => panic!("responder error iteration {i}: {e}"),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(processed);

    // Assert AI reply was generated
    let ai_messages: Vec<String> = sqlx::query_scalar(
        "SELECT body FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&db::testing::seed_pool())
    .await
    .unwrap();
    assert!(!ai_messages.is_empty(), "expected at least one AI reply");
    assert_eq!(
        ai_messages[0],
        "Hello! I'm your AI assistant. How can I help you today?"
    );
}
