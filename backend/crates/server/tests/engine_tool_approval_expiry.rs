use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ai::agent_responder::process_agent_responder_once;
use ai::crypto::{self, MasterKey};
use server::state::AppState;
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
    let slug = format!("app-expiry-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Approval Expiry Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Approval Expiry Test User")
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

fn master_key() -> MasterKey {
    MasterKey::from_base64("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=").unwrap()
}

// ═══════════════════════════════════════════════════════════════════════════════
// T043 — Expiry: create a pending request past expires_at, run sweep_expired,
// assert status=expired + ai.tool_decision event + follow-up reply.
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn engine_tool_approval_expiry() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let openai_mock = MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t043@test.com").await;
    let _membership_id = seed_membership(&pool, tenant_id, user_id, "admin").await;
    let master = master_key();

    // Create a customer and conversation so we can seed a tool request
    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Expiry Customer")
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

    let gen_id = Uuid::new_v4();

    // Insert a tool_requests row with expires_at in the past
    let tool_request_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tool_requests \
         (tenant_id, conversation_id, generation_id, tool_name, tool_source, \
          arguments, status, approval_required, chain_index, expires_at) \
         VALUES ($1, $2, $3, 'update_customer_contact', 'builtin', \
          '{}'::jsonb, 'awaiting_approval', true, 0, now() - interval '1 minute') \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(gen_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Also create an ai_generations row so the orphan check doesn't block
    sqlx::query(
        "INSERT INTO ai_generations (id, tenant_id, conversation_id, trigger_message_id, \
         outcome, attempts, latency_ms) \
         VALUES ($1, $2, $3, $4, 'awaiting_tool_approval', 1, 0)",
    )
    .bind(gen_id)
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap();

    // Run sweep_expired
    let swept = tools::approval::sweep_expired(&pool)
        .await
        .expect("sweep_expired should succeed");
    assert_eq!(swept, 1, "expected exactly 1 expired row to be swept");

    // Assert the tool request status is now 'expired'
    let status: String = sqlx::query_scalar("SELECT status FROM tool_requests WHERE id = $1")
        .bind(tool_request_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        status, "expired",
        "tool request should be expired after sweep"
    );

    // Assert an ai.tool_decision outbox event was created
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events \
         WHERE event_type = 'ai.tool_decision' AND tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        event_count, 1,
        "expected exactly one ai.tool_decision event after expiry"
    );

    // Verify the payload references the correct tool request
    let (payload_json,): (serde_json::Value,) = sqlx::query_as(
        "SELECT payload FROM outbox_events \
         WHERE event_type = 'ai.tool_decision' AND tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        payload_json["outcome"], "expired",
        "decision event outcome should be 'expired'"
    );

    // Drive the responder to run the follow-up generation
    let mut follow_up = false;
    for i in 0..20 {
        match process_agent_responder_once(&pool, &state.ai, &state.escalations).await {
            Ok(true) => {
                follow_up = true;
            }
            Ok(false) => break,
            Err(e) => panic!("responder error at iteration {i}: {e}"),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(follow_up, "follow-up generation should have been processed");

    // Assert a follow-up AI reply exists
    let ai_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        ai_count >= 1,
        "expected at least one AI reply after expiry follow-up, got {ai_count}"
    );
}
