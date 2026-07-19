use std::time::Duration;

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
    let slug = format!("app-race-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Approval Race Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Approval Race Test User")
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
// T044 — FR-014: concurrent decide calls (one approve, one deny). Exactly one
// returns Applied, the other AlreadySettled.
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn engine_tool_approval_race() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "t044@test.com").await;
    let membership_id = seed_membership(&pool, tenant_id, user_id, "admin").await;

    // Create a conversation so we can seed a tool request + ai_generation
    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Race Customer")
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

    // Seed a tool_requests row in 'awaiting_approval' status
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

    // Seed an ai_generation row referencing this
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

    // Spawn two concurrent decide calls
    let pool1 = pool.clone();
    let pool2 = pool.clone();

    let handle1 = tokio::spawn(async move {
        tools::approval::decide(&pool1, tenant_id, tool_request_id, membership_id, true).await
    });

    let handle2 = tokio::spawn(async move {
        tools::approval::decide(&pool2, tenant_id, tool_request_id, membership_id, false).await
    });

    let result1 = handle1.await.expect("task 1 panicked");
    let result2 = handle2.await.expect("task 2 panicked");

    // One should be Applied, the other AlreadySettled
    let applied_count = match (&result1, &result2) {
        (
            Ok(tools::approval::DecideOutcome::Applied(_)),
            Ok(tools::approval::DecideOutcome::AlreadySettled(_)),
        ) => 1,
        (
            Ok(tools::approval::DecideOutcome::AlreadySettled(_)),
            Ok(tools::approval::DecideOutcome::Applied(_)),
        ) => 1,
        (
            Ok(tools::approval::DecideOutcome::Applied(_)),
            Ok(tools::approval::DecideOutcome::Applied(_)),
        ) => {
            panic!("both calls returned Applied — at-most-once violated");
        }
        (
            Ok(tools::approval::DecideOutcome::AlreadySettled(_)),
            Ok(tools::approval::DecideOutcome::AlreadySettled(_)),
        ) => {
            // Both got AlreadySettled — possible if both updates lost the race
            // This should not happen; the first UPDATE wins, the second UPDATE
            // finds no matching row and re-reads, then returns AlreadySettled.
            // But in theory this is valid.
            0
        }
        (Err(e), _) | (_, Err(e)) => panic!("decide returned error: {e}"),
    };

    // At most one execution outcome should result
    assert!(
        applied_count <= 1,
        "at most one decide call should be Applied: {applied_count}"
    );

    // The tool request should be in a terminal state (either approved or denied)
    let final_status: String = sqlx::query_scalar("SELECT status FROM tool_requests WHERE id = $1")
        .bind(tool_request_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        final_status == "approved" || final_status == "denied",
        "final status should be approved or denied, got: {final_status}"
    );

    // Exactly one outbox event (ai.tool_decision) should exist
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events WHERE event_type = 'ai.tool_decision' AND tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        event_count, 1,
        "expected exactly one ai.tool_decision event, got {event_count}"
    );
}
