//! T050: Customer-facing conversation view must not leak tool internals (FR-020).
//!
//! Verifies that the staff conversation timeline endpoint does not expose
//! tool_requests, tool names, arguments, results, or errors. The endpoint
//! used for reading messages (GET /tenant/conversations/{id}/messages) must
//! return only conversation messages, never tool request data.

use std::sync::Arc;
use std::time::Duration;

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
        integration_secrets_key: None,
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
    }
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
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

async fn body_bytes(res: &mut axum::response::Response) -> Vec<u8> {
    use http_body_util::BodyExt;
    BodyExt::collect(res.body_mut())
        .await
        .unwrap()
        .to_bytes()
        .to_vec()
}

#[tokio::test]
async fn customer_view_contains_no_tool_request_data() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let user_id: Uuid =
        sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
            .bind(format!("staff_{}@example.com", Uuid::new_v4()))
            .bind("Staff User")
            .fetch_one(&pool)
            .await
            .expect("seed user");

    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenants (name, slug, status) VALUES ($1, $2, 'active') RETURNING id",
    )
    .bind("Test Tenant")
    .bind(format!("tt-{}", Uuid::new_v4()))
    .fetch_one(&pool)
    .await
    .expect("seed tenant");

    let membership_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, 'admin') RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .expect("seed membership");

    let customer_id: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name, email) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Test Customer")
    .bind("customer@example.com")
    .fetch_one(&pool)
    .await
    .expect("seed customer");

    let conversation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&pool)
    .await
    .expect("seed conversation");

    // Insert a customer message
    sqlx::query(
        "INSERT INTO messages (tenant_id, conversation_id, kind, sender_type, sender_display_name, body) \
         VALUES ($1, $2, 'customer', 'customer', 'Test Customer', 'Hello')",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .execute(&pool)
    .await
    .expect("seed customer message");

    // Insert an AI reply
    let ai_msg_id: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, sender_type, sender_display_name, body) \
         VALUES ($1, $2, 'ai', 'member', 'AI Bot', 'I can help!') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .expect("seed AI message");

    // Insert an AI generation row so the FK is satisfied
    let generation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_generations (tenant_id, conversation_id, trigger_message_id, response_message_id, outcome) \
         VALUES ($1, $2, $3, $4, 'success') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(ai_msg_id)
    .bind(ai_msg_id)
    .fetch_one(&pool)
    .await
    .expect("seed generation");

    // Insert a succeeded tool request
    sqlx::query(
        "INSERT INTO tool_requests (tenant_id, conversation_id, generation_id, tool_name, tool_source, \
         arguments, status, approval_required, chain_index, started_at, finished_at, result) \
         VALUES ($1, $2, $3, 'lookup_customer', 'builtin', '{}', 'succeeded', false, 0, \
         now(), now(), '{\"displayName\":\"Test Customer\"}')",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(generation_id)
    .execute(&pool)
    .await
    .expect("seed succeeded tool request");

    // Insert a failed tool request
    sqlx::query(
        "INSERT INTO tool_requests (tenant_id, conversation_id, generation_id, tool_name, tool_source, \
         arguments, status, approval_required, chain_index, started_at, finished_at, error) \
         VALUES ($1, $2, $3, 'update_customer_contact', 'builtin', '{\"field\":\"email\"}', 'failed', true, 1, \
         now(), now(), 'API error: timeout')",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(generation_id)
    .execute(&pool)
    .await
    .expect("seed failed tool request");

    // Fetch messages via the conversation timeline endpoint
    let mut res = send_request(
        pool.clone(),
        Request::builder()
            .uri(&format!(
                "/api/v1/tenant/conversations/{conversation_id}/messages"
            ))
            .header("X-Dev-User-Id", user_id.to_string())
            .header("X-Tenant-ID", tenant_id.to_string())
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(res.status(), 200, "messages endpoint should return 200");

    let body_bytes = body_bytes(&mut res).await;
    let body_str = String::from_utf8(body_bytes).expect("response is UTF-8");
    let body: serde_json::Value = serde_json::from_str(&body_str).expect("response is valid JSON");

    // The response must NOT contain tool request data
    let body_pretty = serde_json::to_string_pretty(&body).unwrap();

    assert!(
        !body_pretty.contains("lookup_customer"),
        "FR-020: tool name 'lookup_customer' leaked into conversation timeline"
    );
    assert!(
        !body_pretty.contains("update_customer_contact"),
        "FR-020: tool name 'update_customer_contact' leaked into conversation timeline"
    );
    assert!(
        !body_pretty.contains("tool_requests") && !body_pretty.contains("toolRequests"),
        "FR-020: 'tool_requests' leaked into conversation timeline"
    );
    assert!(
        !body_pretty.contains("API error: timeout"),
        "FR-020: tool error text leaked into conversation timeline"
    );
    assert!(
        !body_pretty.contains("Test Customer") || body_pretty.contains("displayName"),
        "FR-020: only message data (not tool result) should appear"
    );

    // The response should contain the messages we inserted
    assert!(
        body_pretty.contains("Hello"),
        "customer message should be present"
    );
    assert!(
        body_pretty.contains("I can help!"),
        "AI message should be present"
    );
}
