use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
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

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("skipping tool_decide_endpoint tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping tool_decide_endpoint tests: DATABASE_URL unreachable");
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
    .unwrap();
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(email)
    .bind("Decide Endpoint Test User")
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    let slug = format!("decide-endpoint-{}", Uuid::new_v4().simple());
    sqlx::query_scalar::<_, Uuid>("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Decide Endpoint Test Tenant")
        .bind(&slug)
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

async fn body_bytes(res: &mut axum::response::Response) -> Vec<u8> {
    let body = std::mem::take(res.body_mut());
    BodyExt::collect(body).await.unwrap().to_bytes().to_vec()
}

// ═══════════════════════════════════════════════════════════════════════════════
// T046 — POST /tenant/tool-requests/{id}/decide: 200 on first, 409 on second,
// Viewer-role gets 403.
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tool_decide_endpoint() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_admin = seed_user(&pool, "t046-admin@test.com").await;
    let membership_admin = seed_membership(&pool, tenant_id, user_admin, "admin").await;

    let user_viewer = seed_user(&pool, "t046-viewer@test.com").await;
    seed_membership(&pool, tenant_id, user_viewer, "viewer").await;

    // Create a conversation + tool request in awaiting_approval status
    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Decide Customer")
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

    // Build the app
    let state = test_app_state(pool.clone());
    let app = router::app(state);

    // ── First decide (approve) — expect 200 ──────────────────────────────────
    let body = serde_json::to_vec(&json!({"decision": "approve"})).unwrap();
    let mut resp1 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/tenant/tool-requests/{tool_request_id}/decide"
                ))
                .method("POST")
                .header("X-Dev-User-Id", user_admin.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp1.status(),
        StatusCode::OK,
        "first decide should return 200"
    );
    let body1: serde_json::Value = serde_json::from_slice(&body_bytes(&mut resp1).await).unwrap();
    assert_eq!(
        body1["status"], "approved",
        "tool request should be approved"
    );

    // ── Second decide (approve again) — expect 409 ───────────────────────────
    let body2 = serde_json::to_vec(&json!({"decision": "approve"})).unwrap();
    let mut resp2 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/tenant/tool-requests/{tool_request_id}/decide"
                ))
                .method("POST")
                .header("X-Dev-User-Id", user_admin.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .header("content-type", "application/json")
                .body(Body::from(body2))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp2.status(),
        StatusCode::CONFLICT,
        "second decide should return 409"
    );

    // ── Create a second tool request and try decide with viewer role — 403 ──
    let tr2_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tool_requests \
         (tenant_id, conversation_id, generation_id, tool_name, tool_source, \
          arguments, status, approval_required, chain_index) \
         VALUES ($1, $2, $3, 'update_customer_contact', 'builtin', \
          '{}'::jsonb, 'awaiting_approval', true, 1) \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(gen_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let body3 = serde_json::to_vec(&json!({"decision": "deny"})).unwrap();
    let mut resp3 = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/tenant/tool-requests/{tr2_id}/decide"))
                .method("POST")
                .header("X-Dev-User-Id", user_viewer.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .header("content-type", "application/json")
                .body(Body::from(body3))
                .unwrap(),
        )
        .await
        .unwrap();
    // Viewer role → 403 Forbidden
    assert_eq!(
        resp3.status(),
        StatusCode::FORBIDDEN,
        "viewer role should get 403 for decide"
    );
}
