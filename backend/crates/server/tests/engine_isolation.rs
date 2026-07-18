use std::sync::Arc;
use std::time::Duration;

use ai::agent_responder::process_agent_responder_once;
use ai::crypto::{self, MasterKey};
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::json;
use server::router;
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::matchers::{method, path};
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
            eprintln!("skipping engine_isolation tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping engine_isolation tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query(
        "TRUNCATE TABLE ai_generations, ai_usage_records, ai_credentials, ai_configurations, \
         message_citations, messages, customer_channel_identifiers, customers, conversations, \
         knowledge_chunks, knowledge_item_tags, knowledge_items, knowledge_categories, \
         agent_configurations, agent_avatar_uploads, \
         escalations, agent_availability, agent_skills, skills, \
         outbox_events, audit_logs, tenant_invitations, tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("failed to reset test tables");
}

async fn seed_tenant(pool: &sqlx::PgPool, label: &str) -> Uuid {
    let slug = format!("engine-iso-{}-{}", label, Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(format!("Engine Isolation Tenant {}", label))
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Engine Isolation Test User")
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

async fn seed_knowledge_article(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    title: &str,
    body: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO knowledge_items (tenant_id, item_type, title, body, status, source, \
         created_by_display) \
         VALUES ($1, 'article', $2, $3, 'published', 'authored', 'Test User') RETURNING id",
    )
    .bind(tenant_id)
    .bind(title)
    .bind(body)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Wiremock response matching the OpenAI /v1/chat/completions endpoint.
async fn mock_openai_completion(mock: &MockServer, content: &str) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-test",
            "object": "chat.completion",
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": content },
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 50, "completion_tokens": 10, "total_tokens": 60 }
        })))
        .mount(mock)
        .await;
}

/// HTTP helpers (minimal — only what this test needs).
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
// FR-014 / SC-004: Cross-tenant isolation for the conversation engine
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn engine_cross_tenant_isolation() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let master = master_key();

    // Seed platform AI config + credential (shared, used by both tenants)
    seed_ai_config(&pool, "openai", "gpt-4").await;
    seed_ai_credential(&pool, "openai", "sk-engine-isolation-key", &master).await;

    // ── Tenant A ─────────────────────────────────────────────────────────────
    let tenant_a = seed_tenant(&pool, "A").await;
    let user_a = seed_user(&pool, "engine-iso-a@test.com").await;
    seed_membership(&pool, tenant_a, user_a, "admin").await;

    // Agent config for tenant A — distinctive name for assertion
    let agent_payload_a = json!({
        "name": "Tenant-A-Assistant",
        "avatar": { "kind": "preset", "preset": "spark" },
        "tone": "professional",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": { "provider": "openai", "model": "gpt-4" },
    });
    let put_a = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_a, tenant_a, agent_payload_a),
    )
    .await;
    assert_eq!(put_a.status(), StatusCode::CREATED);

    // Tenant A knowledge article — distinctive data that must not leak
    let _article_a = seed_knowledge_article(
        &pool,
        tenant_a,
        "Tenant A Secret Policy",
        "This is confidential information for tenant A only.",
    )
    .await;

    // Tenant A conversation + customer message + outbox event
    let customer_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_a)
    .bind("Customer A")
    .fetch_one(&pool)
    .await
    .unwrap();

    let conv_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_a)
    .bind(customer_a)
    .fetch_one(&pool)
    .await
    .unwrap();

    let msg_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'Message from tenant A') RETURNING id",
    )
    .bind(tenant_a)
    .bind(conv_a)
    .fetch_one(&pool)
    .await
    .unwrap();

    let outbox_payload_a = json!({
        "conversation_id": conv_a,
        "message_id": msg_a,
        "channel": "web_chat",
    });
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.customer_message', $4)",
    )
    .bind(Uuid::new_v4())
    .bind(conv_a)
    .bind(tenant_a)
    .bind(outbox_payload_a)
    .execute(&pool)
    .await
    .unwrap();

    // ── Tenant B ─────────────────────────────────────────────────────────────
    let tenant_b = seed_tenant(&pool, "B").await;
    let user_b = seed_user(&pool, "engine-iso-b@test.com").await;
    seed_membership(&pool, tenant_b, user_b, "admin").await;

    // Agent config for tenant B — distinctive name for assertion
    let agent_payload_b = json!({
        "name": "Tenant-B-Assistant",
        "avatar": { "kind": "preset", "preset": "nova" },
        "tone": "friendly",
        "business_rules": [],
        "escalation_rules": [],
        "enabled_channels": ["web_chat"],
        "provider_selection": { "provider": "openai", "model": "gpt-4" },
    });
    let put_b = send(
        &state,
        json_put("/api/v1/tenant/ai/agent", user_b, tenant_b, agent_payload_b),
    )
    .await;
    assert_eq!(put_b.status(), StatusCode::CREATED);

    // Tenant B knowledge article — distinctive data
    let _article_b = seed_knowledge_article(
        &pool,
        tenant_b,
        "Tenant B Public Guide",
        "This is public information available to tenant B customers.",
    )
    .await;

    // Tenant B conversation
    let customer_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_b)
    .bind("Customer B")
    .fetch_one(&pool)
    .await
    .unwrap();

    let conv_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_b)
    .bind(customer_b)
    .fetch_one(&pool)
    .await
    .unwrap();

    let msg_b = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'Message from tenant B') RETURNING id",
    )
    .bind(tenant_b)
    .bind(conv_b)
    .fetch_one(&pool)
    .await
    .unwrap();

    let outbox_payload_b = json!({
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

    // Wiremock: return a tenant-B-specific reply
    mock_openai_completion(&openai_mock, "I am tenant B's assistant.").await;

    // ── Drive the responder for tenant B's events ────────────────────────────
    // Process up to 5 iterations to handle tenant B's outbox event (tenant A's
    // event may also be processed, but we only assert about B).
    for _ in 0..5 {
        let processed = process_agent_responder_once(&pool, &state.ai, &state.escalations)
            .await
            .expect("agent responder should not panic");
        if !processed {
            break;
        }
    }

    // ── Assertions ───────────────────────────────────────────────────────────

    // 1. Tenant B's conversation received an AI reply
    let b_ai_messages: Vec<(String,)> = sqlx::query_as(
        "SELECT body FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_b)
    .bind(conv_b)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        b_ai_messages.len(),
        1,
        "tenant B should have exactly one AI reply"
    );
    let b_reply_body = &b_ai_messages[0].0;
    assert_eq!(b_reply_body, "I am tenant B's assistant.");

    // 2. B's reply does not contain tenant A's data
    assert!(
        !b_reply_body.contains("Tenant A"),
        "tenant B's reply must not reference tenant A's data"
    );

    // 3. No ai_generations row for tenant A referencing B's conversation
    let a_cross_tenant_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ai_generations WHERE tenant_id = $1 AND conversation_id = $2",
    )
    .bind(tenant_a)
    .bind(conv_b)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        a_cross_tenant_count, 0,
        "no ai_generations row should link tenant A with B's conversation"
    );

    // 4. Exactly one ai_generations row for tenant B with outcome = 'success'
    let b_gen_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ai_generations WHERE tenant_id = $1 AND conversation_id = $2",
    )
    .bind(tenant_b)
    .bind(conv_b)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        b_gen_count, 1,
        "tenant B should have exactly one ai_generations row for its conversation"
    );

    let b_gen_outcome: String = sqlx::query_scalar(
        "SELECT outcome FROM ai_generations WHERE tenant_id = $1 AND conversation_id = $2",
    )
    .bind(tenant_b)
    .bind(conv_b)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        b_gen_outcome, "success",
        "B's generation outcome must be success"
    );
}
