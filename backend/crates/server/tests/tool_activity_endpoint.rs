use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
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
        integration_secrets_key: None,
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
            eprintln!("skipping tool_activity_endpoint tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping tool_activity_endpoint tests: DATABASE_URL unreachable");
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
    .bind("Tool Activity Test User")
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    let slug = format!("tool-activity-{}", Uuid::new_v4().simple());
    sqlx::query_scalar::<_, Uuid>("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Tool Activity Test Tenant")
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
    use http_body_util::BodyExt;
    let body = std::mem::take(res.body_mut());
    BodyExt::collect(body).await.unwrap().to_bytes().to_vec()
}

// ═══════════════════════════════════════════════════════════════════════════════
// T028 — GET /tenant/conversations/{id}/tool-activity returns the tool entry
// with correct toolName; cross-tenant isolation returns empty.
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tool_activity_endpoint_returns_succeeded_tool() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_a = seed_tenant(&pool).await;
    let user_a = seed_user(&pool, "t028a@test.com").await;
    let membership_a = seed_membership(&pool, tenant_a, user_a, "admin").await;

    let tenant_b = seed_tenant(&pool).await;
    let user_b = seed_user(&pool, "t028b@test.com").await;
    let _membership_b = seed_membership(&pool, tenant_b, user_b, "viewer").await;

    // Create a conversation for tenant A
    let customer_a = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_a)
    .bind("Activity Customer")
    .fetch_one(&pool)
    .await
    .unwrap();

    let gen_id = Uuid::new_v4();
    let conversation_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_a)
    .bind(customer_a)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Insert a succeeded tool_requests row for tenant A
    let tool_request_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tool_requests \
         (tenant_id, conversation_id, generation_id, tool_name, tool_source, \
          arguments, status, approval_required, chain_index, \
          started_at, finished_at) \
         VALUES ($1, $2, $3, 'lookup_customer', 'builtin', '{}'::jsonb, \
          'succeeded', false, 0, now(), now()) RETURNING id",
    )
    .bind(tenant_a)
    .bind(conversation_id)
    .bind(gen_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Test: tenant A requests tool-activity for its own conversation
    let state = test_app_state(pool.clone());
    let app = router::app(state.clone());

    let mut resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/tenant/conversations/{conversation_id}/tool-activity"
                ))
                .header("X-Dev-User-Id", user_a.to_string())
                .header("X-Tenant-ID", tenant_a.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = serde_json::from_slice(&body_bytes(&mut resp).await).unwrap();
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "expected exactly one tool-activity item");

    let item = &items[0];
    assert_eq!(item["toolName"], "lookup_customer");
    assert_eq!(item["status"], "succeeded");
    assert_eq!(item["toolSource"], "builtin");
    assert_eq!(item["id"], tool_request_id.to_string());

    // Test: tenant B requests tool-activity for tenant A's conversation
    let mut resp_b = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/tenant/conversations/{conversation_id}/tool-activity"
                ))
                .header("X-Dev-User-Id", user_b.to_string())
                .header("X-Tenant-ID", tenant_b.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // Tenant B gets an empty list (no matching rows due to tenant isolation)
    assert_eq!(resp_b.status(), StatusCode::OK);
    let body_b: serde_json::Value = serde_json::from_slice(&body_bytes(&mut resp_b).await).unwrap();
    let items_b = body_b["items"].as_array().unwrap();
    assert!(
        items_b.is_empty(),
        "tenant B should get empty tool-activity for tenant A's conversation"
    );
}
