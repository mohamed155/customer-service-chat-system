//! T051: Each terminal tool-request status has a complete, inspectable
//! `tool_requests` row with correct status and consistent timestamps (SC-001/SC-005).
//!
//! Drives one request into each terminal status:
//! - `succeeded`, `failed`, `timed_out` (via direct DB + executor simulation)
//! - `refused` (via direct insert — refused is set atomically by the engine)
//! - `denied` (via approval::decide)
//! - `expired` (via approval::sweep_expired)
//! - `cancelled` (via approval::cancel_pending_for_conversation)

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
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

async fn get_pool() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

/// Seed minimal data and return (tenant_id, conversation_id, generation_id).
async fn seed_minimal(pool: &PgPool) -> (Uuid, Uuid, Uuid) {
    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenants (name, slug, status) VALUES ($1, $2, 'active') RETURNING id",
    )
    .bind("Audit Tenant")
    .bind(format!("audit-{}", Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("seed tenant");

    let customer_id: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name, email) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Audit Customer")
    .bind("audit@example.com")
    .fetch_one(pool)
    .await
    .expect("seed customer");

    let conversation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(pool)
    .await
    .expect("seed conversation");

    let generation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_generations (tenant_id, conversation_id, trigger_message_id, outcome) \
         VALUES ($1, $2, '00000000-0000-0000-0000-000000000000', 'success') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(pool)
    .await
    .expect("seed generation");

    (tenant_id, conversation_id, generation_id)
}

/// Insert a raw tool_requests row, return its id.
async fn insert_request(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    generation_id: Uuid,
    status: &str,
    started: bool,
    finished: bool,
) -> Uuid {
    let started_at = if started {
        Some(chrono::Utc::now())
    } else {
        None
    };
    let finished_at = if finished {
        Some(chrono::Utc::now())
    } else {
        None
    };
    let result = if status == "succeeded" {
        Some(serde_json::json!({"ok": true}))
    } else {
        None::<serde_json::Value>
    };
    let error = if status == "failed" || status == "timed_out" {
        Some("simulated error".into())
    } else {
        None::<String>
    };

    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tool_requests \
         (tenant_id, conversation_id, generation_id, tool_name, tool_source, \
          arguments, status, approval_required, chain_index, \
          started_at, finished_at, result, error) \
         VALUES ($1, $2, $3, $4, 'builtin', '{}', $5, false, 0, $6, $7, $8, $9) \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(generation_id)
    .bind(format!("audit_tool_{status}"))
    .bind(status)
    .bind(started_at)
    .bind(finished_at)
    .bind(result)
    .bind(error)
    .fetch_one(pool)
    .await
    .expect("insert tool_request")
}

/// Fetch a tool_request row and return its JSON representation.
async fn fetch_json(pool: &PgPool, id: Uuid) -> serde_json::Value {
    let row: serde_json::Value = sqlx::query_scalar(
        "SELECT row_to_json(t) FROM ( \
         SELECT id, status, started_at, finished_at, result, error, \
                decided_by_membership_id, decided_at \
         FROM tool_requests WHERE id = $1) t",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("fetch tool_request");
    row
}

#[tokio::test]
async fn succeeded_has_complete_row_with_started_and_finished() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let (tid, cid, gid) = seed_minimal(&pool).await;

    let id = insert_request(&pool, tid, cid, gid, "succeeded", true, true).await;
    let row = fetch_json(&pool, id).await;

    assert_eq!(row["status"], "succeeded");
    assert!(
        row["started_at"].is_string(),
        "succeeded must have started_at"
    );
    assert!(
        row["finished_at"].is_string(),
        "succeeded must have finished_at"
    );
    assert!(row["result"].is_object(), "succeeded should have a result");
}

#[tokio::test]
async fn failed_has_complete_row_with_error() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let (tid, cid, gid) = seed_minimal(&pool).await;

    let id = insert_request(&pool, tid, cid, gid, "failed", true, true).await;
    let row = fetch_json(&pool, id).await;

    assert_eq!(row["status"], "failed");
    assert!(row["started_at"].is_string(), "failed must have started_at");
    assert!(
        row["finished_at"].is_string(),
        "failed must have finished_at"
    );
    assert_eq!(row["error"], "simulated error");
}

#[tokio::test]
async fn refused_has_started_at_null() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let (tid, cid, gid) = seed_minimal(&pool).await;

    let id = insert_request(&pool, tid, cid, gid, "refused", false, false).await;
    let row = fetch_json(&pool, id).await;

    assert_eq!(row["status"], "refused");
    assert!(
        row["started_at"].is_null(),
        "refused must have started_at IS NULL"
    );
    assert!(
        row["finished_at"].is_null(),
        "refused must have finished_at IS NULL"
    );
}

#[tokio::test]
async fn denied_has_started_at_null_and_decision_fields() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let (tid, cid, gid) = seed_minimal(&pool).await;

    // Create an awaiting_approval row
    let req_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tool_requests \
         (tenant_id, conversation_id, generation_id, tool_name, tool_source, \
          arguments, status, approval_required, chain_index, expires_at) \
         VALUES ($1, $2, $3, 'deny_test', 'builtin', '{}', 'awaiting_approval', true, 0, \
         now() + interval '5 minutes') RETURNING id",
    )
    .bind(tid)
    .bind(cid)
    .bind(gid)
    .fetch_one(&pool)
    .await
    .expect("seed awaiting_approval row");

    // Create a decider membership
    let decider_user_id: Uuid =
        sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
            .bind(format!("decider_{}@example.com", Uuid::new_v4()))
            .bind("Decider User")
            .fetch_one(&pool)
            .await
            .expect("seed decider user");

    let decider_membership_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) \
         VALUES ($1, $2, 'admin') RETURNING id",
    )
    .bind(tid)
    .bind(decider_user_id)
    .fetch_one(&pool)
    .await
    .expect("seed decider membership");

    // Deny via approval::decide
    let outcome = tools::approval::decide(&pool, tid, req_id, decider_membership_id, false)
        .await
        .expect("decide should succeed");
    assert!(
        matches!(outcome, tools::approval::DecideOutcome::Applied(_)),
        "first decide must be Applied"
    );

    let row = fetch_json(&pool, req_id).await;
    assert_eq!(row["status"], "denied");
    assert!(
        row["started_at"].is_null(),
        "denied must have started_at IS NULL"
    );
    assert!(
        row["decided_by_membership_id"].is_string(),
        "denied must have decided_by_membership_id"
    );
    assert!(row["decided_at"].is_string(), "denied must have decided_at");
}

#[tokio::test]
async fn expired_has_started_at_null_and_decision_fields() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let (tid, cid, gid) = seed_minimal(&pool).await;

    // Create an awaiting_approval row that has already expired
    let req_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tool_requests \
         (tenant_id, conversation_id, generation_id, tool_name, tool_source, \
          arguments, status, approval_required, chain_index, expires_at) \
         VALUES ($1, $2, $3, 'expire_test', 'builtin', '{}', 'awaiting_approval', true, 0, \
         now() - interval '1 minute') RETURNING id",
    )
    .bind(tid)
    .bind(cid)
    .bind(gid)
    .fetch_one(&pool)
    .await
    .expect("seed awaiting_approval row");

    // Sweep expired
    let swept = tools::approval::sweep_expired(&pool)
        .await
        .expect("sweep_expired should succeed");
    assert_eq!(swept, 1, "exactly one row should be swept");

    let row = fetch_json(&pool, req_id).await;
    assert_eq!(row["status"], "expired");
    assert!(
        row["started_at"].is_null(),
        "expired must have started_at IS NULL"
    );
    assert!(
        row["decided_at"].is_string(),
        "expired must have decided_at"
    );
}

#[tokio::test]
async fn cancelled_has_started_at_null_and_decision_fields() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let (tid, cid, gid) = seed_minimal(&pool).await;

    // Create an awaiting_approval row
    let req_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tool_requests \
         (tenant_id, conversation_id, generation_id, tool_name, tool_source, \
          arguments, status, approval_required, chain_index, expires_at) \
         VALUES ($1, $2, $3, 'cancel_test', 'builtin', '{}', 'awaiting_approval', true, 0, \
         now() + interval '5 minutes') RETURNING id",
    )
    .bind(tid)
    .bind(cid)
    .bind(gid)
    .fetch_one(&pool)
    .await
    .expect("seed awaiting_approval row");

    // Cancel via approval::cancel_pending_for_conversation
    let mut tx = pool.begin().await.expect("begin tx");
    let cancelled = tools::approval::cancel_pending_for_conversation(&mut tx, tid, cid)
        .await
        .expect("cancel should succeed");
    tx.commit().await.expect("commit tx");
    assert_eq!(cancelled.len(), 1, "exactly one row should be cancelled");

    let row = fetch_json(&pool, req_id).await;
    assert_eq!(row["status"], "cancelled");
    assert!(
        row["started_at"].is_null(),
        "cancelled must have started_at IS NULL"
    );
    assert!(
        row["decided_at"].is_string(),
        "cancelled must have decided_at"
    );
}

#[tokio::test]
async fn timed_out_has_started_at_finished_at_and_error() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let (tid, cid, gid) = seed_minimal(&pool).await;

    let id = insert_request(&pool, tid, cid, gid, "timed_out", true, true).await;
    let row = fetch_json(&pool, id).await;

    assert_eq!(row["status"], "timed_out");
    assert!(
        row["started_at"].is_string(),
        "timed_out must have started_at"
    );
    assert!(
        row["finished_at"].is_string(),
        "timed_out must have finished_at"
    );
    assert_eq!(row["error"], "simulated error");
}
