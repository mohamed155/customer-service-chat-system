use std::sync::Arc;
use std::time::Duration;

use ai::crypto::{self, MasterKey};
use axum::body::Body;
use axum::http::Request;
use server::router;
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;

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
    let slug = format!("app-cancel-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Approval Cancel Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Approval Cancel Test User")
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

// ═══════════════════════════════════════════════════════════════════════════════
// T045 — Cancel via human claim: pending request + claim conversation →
// request settles to 'cancelled', no follow-up generation.
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn engine_tool_approval_cancel() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t045@test.com").await;
    let _membership_id = seed_membership(&pool, tenant_id, user_id, "agent").await;

    // Create a customer + conversation
    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Cancel Customer")
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

    // Seed an awaiting_approval tool_requests row
    let tool_request_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tool_requests \
         (tenant_id, conversation_id, generation_id, tool_name, tool_source, \
          arguments, status, approval_required, chain_index) \
         VALUES ($1, $2, $3, 'update_customer_contact', 'builtin', \
          '{}'::jsonb, 'awaiting_approval', true, 0) \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(gen_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Seed an ai_generations row
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

    // Simulate human claiming the conversation (set ai_handling='human')
    // This triggers cancellation of pending tool requests
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE conversations SET ai_handling = 'human' WHERE id = $1 AND tenant_id = $2")
        .bind(conversation_id)
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .unwrap();

    // Call cancel_pending_for_conversation within the same transaction
    let cancelled =
        tools::approval::cancel_pending_for_conversation(&mut tx, tenant_id, conversation_id)
            .await
            .unwrap();
    assert_eq!(
        cancelled.len(),
        1,
        "expected exactly 1 pending tool request to be cancelled"
    );

    tx.commit().await.unwrap();

    // Assert the tool request is now 'cancelled' with started_at IS NULL
    let (status, started_at): (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT status, started_at FROM tool_requests WHERE id = $1")
            .bind(tool_request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "cancelled", "tool request should be cancelled");
    assert!(
        started_at.is_none(),
        "started_at should be NULL for cancelled tool"
    );

    // Assert NO ai.tool_decision outbox event was created (FR-015 — no
    // follow-up generation for cancellation)
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events WHERE event_type = 'ai.tool_decision'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        event_count, 0,
        "no ai.tool_decision events should exist for cancellation"
    );

    // Assert NO AI message was stored (no follow-up generation)
    let ai_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        ai_count, 0,
        "no AI reply should be generated after cancellation"
    );
}

fn test_app_state(pool: sqlx::PgPool) -> AppState {
    AppState {
        config: Arc::new(test_config()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool, &test_config()).unwrap(),
    }
}

async fn send_request(pool: sqlx::PgPool, req: Request<Body>) -> axum::response::Response {
    let state = test_app_state(pool);
    let app = router::app(state);
    app.oneshot(req).await.expect("request should succeed")
}

// ═══════════════════════════════════════════════════════════════════════════════
// T076 — Cancel via real HTTP claim endpoint: pending tool request → POST
// /tenant/escalations/{id}/claim → request becomes 'cancelled', no follow-up.
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn engine_tool_approval_cancel_via_claim_endpoint() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t076@test.com").await;
    let _membership_id = seed_membership(&pool, tenant_id, user_id, "admin").await;

    // Create a customer + conversation
    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Claim Cancel Customer")
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

    // Seed an awaiting_approval tool_requests row
    let tool_request_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tool_requests \
         (tenant_id, conversation_id, generation_id, tool_name, tool_source, \
          arguments, status, approval_required, chain_index) \
         VALUES ($1, $2, $3, 'update_customer_contact', 'builtin', \
          '{}'::jsonb, 'awaiting_approval', true, 0) \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(gen_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Seed an ai_generations row
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

    // Seed an active queued escalation row
    let escalation_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO escalations (tenant_id, conversation_id, reason, status) \
         VALUES ($1, $2, 'test escalation', 'queued') \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Drive the real HTTP claim endpoint
    let resp = send_request(
        pool.clone(),
        Request::builder()
            .uri(format!("/api/v1/tenant/escalations/{escalation_id}/claim"))
            .method("POST")
            .header("X-Dev-User-Id", user_id.to_string())
            .header("X-Tenant-ID", tenant_id.to_string())
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(resp.status(), 200, "claim endpoint should return 200");

    // Assert the tool request is now 'cancelled' with started_at IS NULL
    let (status, started_at): (String, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT status, started_at FROM tool_requests WHERE id = $1")
            .bind(tool_request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "cancelled", "tool request should be cancelled");
    assert!(
        started_at.is_none(),
        "started_at should be NULL for cancelled tool"
    );

    // Assert NO ai.tool_decision outbox event was created
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events WHERE event_type = 'ai.tool_decision'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        event_count, 0,
        "no ai.tool_decision events should exist for cancellation"
    );

    // Assert NO AI message was stored
    let ai_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'ai'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        ai_count, 0,
        "no AI reply should be generated after cancellation"
    );
}
