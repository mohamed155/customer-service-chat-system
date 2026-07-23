// Analytics API integration tests (spec 025).

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
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

fn app_state(pool: sqlx::PgPool) -> AppState {
    AppState {
        config: Arc::new(test_config()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool, &test_config()).unwrap(),
    }
}

#[allow(dead_code)]
fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

#[allow(dead_code)]
async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            eprintln!("skipping analytics API tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping analytics API tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

#[allow(dead_code)]
async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query("DELETE FROM escalations")
        .execute(pool)
        .await
        .expect("failed to delete escalations");
    sqlx::query(
        "TRUNCATE TABLE conversation_feedback, ai_generations, \
         ai_usage_records, agent_configurations, messages, \
         customer_channel_identifiers, customers, conversations, \
         widget_sessions, widget_instances, outbox_events, audit_logs, \
         tenant_invitations, tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("failed to reset analytics test tables");
}

async fn send(pool: sqlx::PgPool, request: Request<Body>) -> Response {
    router::app_with_test_routes(app_state(pool))
        .oneshot(request)
        .await
        .expect("request should complete")
}

async fn body_json(response: Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

// ── Seed helpers ────────────────────────────────────────────────────────────

#[allow(dead_code)]
async fn seed_tenant(pool: &sqlx::PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(name)
        .bind(format!("an-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[allow(dead_code)]
async fn seed_widget_instance(pool: &sqlx::PgPool, tenant_id: Uuid, public_id: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO widget_instances \
         (tenant_id, public_id, name, display_name, enabled, allowed_domains) \
         VALUES ($1, $2, $3, $4, true, '{}') RETURNING id",
    )
    .bind(tenant_id)
    .bind(public_id)
    .bind("Test Widget")
    .bind("Test Widget")
    .fetch_one(pool)
    .await
    .unwrap()
}

#[allow(dead_code)]
async fn mint_session(pool: &sqlx::PgPool, public_id: &str) -> String {
    let body = serde_json::json!({ "widgetId": public_id });
    let response = send(
        pool.clone(),
        Request::builder()
            .uri("/api/v1/widget/v1/sessions")
            .method(Method::POST)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let json = body_json(response).await;
    json["sessionToken"].as_str().unwrap().to_owned()
}

#[allow(dead_code)]
fn authed_json_post(path: &str, token: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .uri(path)
        .method(Method::POST)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

#[allow(dead_code)]
fn authed_get(path: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .method(Method::GET)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

#[allow(dead_code)]
async fn setup_ended_conversation(
    pool: &sqlx::PgPool,
    _tenant_id: Uuid,
    public_id: &str,
    status: &str,
) -> (Uuid, String) {
    let token = mint_session(pool, public_id).await;

    let conv_resp = send(
        pool.clone(),
        authed_json_post(
            "/api/v1/widget/v1/conversations",
            &token,
            serde_json::json!({}),
        ),
    )
    .await;
    assert_eq!(conv_resp.status(), StatusCode::CREATED);
    let conv_id: Uuid = body_json(conv_resp).await["data"]["conversation"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    sqlx::query("UPDATE conversations SET status = $1 WHERE id = $2")
        .bind(status)
        .bind(conv_id)
        .execute(pool)
        .await
        .unwrap();

    (conv_id, token)
}

#[allow(dead_code)]
fn authenticated_request(uri: &str, user_id: Uuid, tenant_id: Uuid) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(Method::GET)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .body(Body::empty())
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Analytics Test User")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[allow(dead_code)]
async fn seed_admin(pool: &sqlx::PgPool, tenant_id: Uuid, email: &str) -> (Uuid, Uuid) {
    let user_id = seed_user(pool, email).await;
    let membership_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, 'admin', 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap();
    (user_id, membership_id)
}

/// Seed a user with an active tenant membership in the given role.
/// `role` must be one of: owner, admin, manager, agent, viewer.
#[allow(dead_code)]
async fn seed_member(pool: &sqlx::PgPool, tenant_id: Uuid, email: &str, role: &str) -> Uuid {
    let user_id = seed_user(pool, email).await;
    sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, $3, 'active')",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .execute(pool)
    .await
    .unwrap();
    user_id
}

/// Seed the canonical analytics dataset: two tenants with conversations,
/// messages, feedback, usage records, and generations.
///
/// Returns `(tenant_a_id, tenant_b_id)`.
#[allow(dead_code)]
async fn seed_canonical_dataset(pool: &sqlx::PgPool) -> (Uuid, Uuid) {
    // ── Tenants ──────────────────────────────────────────────────────────────
    let tenant_a: Uuid = sqlx::query_scalar(
        "INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id",
    )
    .bind("Analytics Tenant A")
    .bind(format!("an-a-{}", Uuid::new_v4().simple()))
    .fetch_one(pool)
    .await
    .unwrap();

    let tenant_b: Uuid = sqlx::query_scalar(
        "INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id",
    )
    .bind("Analytics Tenant B")
    .bind(format!("an-b-{}", Uuid::new_v4().simple()))
    .fetch_one(pool)
    .await
    .unwrap();

    // ── Customers ────────────────────────────────────────────────────────────
    let customer_a: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_a)
    .bind("Analytics Customer A")
    .fetch_one(pool)
    .await
    .unwrap();

    let customer_b: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_b)
    .bind("Analytics Customer B")
    .fetch_one(pool)
    .await
    .unwrap();

    // ── Tenant A: conversations C1–C5 ────────────────────────────────────────
    let c1: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, \
         created_at, last_activity_at, updated_at) \
         VALUES ($1, $2, 'widget', 'closed', \
         '2026-03-10T09:00:00Z'::timestamptz, '2026-03-10T09:00:20Z'::timestamptz, \
         '2026-03-10T09:00:20Z'::timestamptz) RETURNING id",
    )
    .bind(tenant_a)
    .bind(customer_a)
    .fetch_one(pool)
    .await
    .unwrap();

    let c2: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, \
         created_at, last_activity_at, updated_at) \
         VALUES ($1, $2, 'widget', 'resolved', \
         '2026-03-10T10:00:00Z'::timestamptz, '2026-03-10T10:02:30Z'::timestamptz, \
         '2026-03-10T10:02:30Z'::timestamptz) RETURNING id",
    )
    .bind(tenant_a)
    .bind(customer_a)
    .fetch_one(pool)
    .await
    .unwrap();

    let c3: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, \
         created_at, last_activity_at, updated_at) \
         VALUES ($1, $2, 'widget', 'closed', \
         '2026-03-11T09:00:00Z'::timestamptz, '2026-03-11T09:00:40Z'::timestamptz, \
         '2026-03-11T09:00:40Z'::timestamptz) RETURNING id",
    )
    .bind(tenant_a)
    .bind(customer_a)
    .fetch_one(pool)
    .await
    .unwrap();

    let c4: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, \
         created_at, last_activity_at, updated_at) \
         VALUES ($1, $2, 'email', 'open', \
         '2026-03-11T10:00:00Z'::timestamptz, '2026-03-11T10:00:00Z'::timestamptz, \
         '2026-03-11T10:00:00Z'::timestamptz) RETURNING id",
    )
    .bind(tenant_a)
    .bind(customer_a)
    .fetch_one(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, \
         created_at, last_activity_at, updated_at, deleted_at) \
         VALUES ($1, $2, 'widget', 'closed', \
         '2026-03-12T09:00:00Z'::timestamptz, '2026-03-12T09:00:00Z'::timestamptz, \
         '2026-03-12T09:00:00Z'::timestamptz, '2026-03-12T12:00:00Z'::timestamptz)",
    )
    .bind(tenant_a)
    .bind(customer_a)
    .execute(pool)
    .await
    .unwrap();

    // ── Tenant A: escalations (C3 only) ──────────────────────────────────────
    sqlx::query(
        "INSERT INTO escalations (tenant_id, conversation_id, reason, status, closed_at) \
         VALUES ($1, $2, 'test', 'closed', now())",
    )
    .bind(tenant_a)
    .bind(c3)
    .execute(pool)
    .await
    .unwrap();

    // ── Tenant A: messages ───────────────────────────────────────────────────
    // C1: customer (trigger for G1), then ai
    let msg_c1_customer: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body, created_at) \
         VALUES ($1, $2, 'customer', 'x', '2026-03-10T09:00:00Z'::timestamptz) RETURNING id",
    )
    .bind(tenant_a)
    .bind(c1)
    .fetch_one(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body, created_at) \
         VALUES ($1, $2, 'ai', 'x', '2026-03-10T09:00:20Z'::timestamptz)",
    )
    .bind(tenant_a)
    .bind(c1)
    .execute(pool)
    .await
    .unwrap();

    // C2: customer, ai, customer, ai
    sqlx::query(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body, created_at) \
         VALUES ($1, $2, 'customer', 'x', '2026-03-10T10:00:00Z'::timestamptz)",
    )
    .bind(tenant_a)
    .bind(c2)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body, created_at) \
         VALUES ($1, $2, 'ai', 'x', '2026-03-10T10:01:00Z'::timestamptz)",
    )
    .bind(tenant_a)
    .bind(c2)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body, created_at) \
         VALUES ($1, $2, 'customer', 'x', '2026-03-10T10:02:00Z'::timestamptz)",
    )
    .bind(tenant_a)
    .bind(c2)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body, created_at) \
         VALUES ($1, $2, 'ai', 'x', '2026-03-10T10:02:30Z'::timestamptz)",
    )
    .bind(tenant_a)
    .bind(c2)
    .execute(pool)
    .await
    .unwrap();

    // C3: customer, ai
    sqlx::query(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body, created_at) \
         VALUES ($1, $2, 'customer', 'x', '2026-03-11T09:00:00Z'::timestamptz)",
    )
    .bind(tenant_a)
    .bind(c3)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body, created_at) \
         VALUES ($1, $2, 'ai', 'x', '2026-03-11T09:00:40Z'::timestamptz)",
    )
    .bind(tenant_a)
    .bind(c3)
    .execute(pool)
    .await
    .unwrap();

    // C4: customer (trigger for G2)
    let msg_c4_customer: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body, created_at) \
         VALUES ($1, $2, 'customer', 'x', '2026-03-11T10:00:00Z'::timestamptz) RETURNING id",
    )
    .bind(tenant_a)
    .bind(c4)
    .fetch_one(pool)
    .await
    .unwrap();

    // ── Tenant A: usage records U1–U3 ───────────────────────────────────────
    let u1: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_usage_records \
         (tenant_id, provider, model, input_tokens, output_tokens, status, streamed, latency_ms, created_at) \
         VALUES ($1, 'openai', 'gpt-4o', 100, 50, 'success', false, 10, \
         '2026-03-10T09:00:10Z'::timestamptz) RETURNING id",
    )
    .bind(tenant_a)
    .fetch_one(pool)
    .await
    .unwrap();

    let u2: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_usage_records \
         (tenant_id, provider, model, input_tokens, output_tokens, status, streamed, latency_ms, created_at) \
         VALUES ($1, 'openai', 'gpt-4o', 200, NULL, 'success', false, 10, \
         '2026-03-11T10:00:10Z'::timestamptz) RETURNING id",
    )
    .bind(tenant_a)
    .fetch_one(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO ai_usage_records \
         (tenant_id, provider, model, input_tokens, output_tokens, status, streamed, latency_ms, created_at) \
         VALUES ($1, 'openai', 'gpt-4o', 10, 5, 'success', false, 10, \
         '2026-03-11T11:00:00Z'::timestamptz)",
    )
    .bind(tenant_a)
    .execute(pool)
    .await
    .unwrap();

    // ── Tenant A: generations G1–G2 ──────────────────────────────────────────
    // G1 → C1, usage U1
    sqlx::query(
        "INSERT INTO ai_generations \
         (tenant_id, conversation_id, trigger_message_id, usage_record_id, outcome, latency_ms) \
         VALUES ($1, $2, $3, $4, 'success', 10)",
    )
    .bind(tenant_a)
    .bind(c1)
    .bind(msg_c1_customer)
    .bind(u1)
    .execute(pool)
    .await
    .unwrap();

    // G2 → C4, usage U2
    sqlx::query(
        "INSERT INTO ai_generations \
         (tenant_id, conversation_id, trigger_message_id, usage_record_id, outcome, latency_ms) \
         VALUES ($1, $2, $3, $4, 'success', 10)",
    )
    .bind(tenant_a)
    .bind(c4)
    .bind(msg_c4_customer)
    .bind(u2)
    .execute(pool)
    .await
    .unwrap();

    // U3 has no generation row (unattributed-token case).

    // ── Tenant A: feedback ───────────────────────────────────────────────────
    sqlx::query(
        "INSERT INTO conversation_feedback \
         (tenant_id, conversation_id, channel, rating, submitted_at) \
         VALUES ($1, $2, 'widget', 5, '2026-03-10T12:00:00Z'::timestamptz)",
    )
    .bind(tenant_a)
    .bind(c1)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO conversation_feedback \
         (tenant_id, conversation_id, channel, rating, submitted_at) \
         VALUES ($1, $2, 'widget', 3, '2026-03-11T12:00:00Z'::timestamptz)",
    )
    .bind(tenant_a)
    .bind(c2)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO conversation_feedback \
         (tenant_id, conversation_id, channel, rating, submitted_at) \
         VALUES ($1, $2, 'widget', 4, '2026-03-16T12:00:00Z'::timestamptz)",
    )
    .bind(tenant_a)
    .bind(c3)
    .execute(pool)
    .await
    .unwrap();

    // ── Tenant B: isolation control ─────────────────────────────────────────
    let conv_b: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, \
         created_at, last_activity_at, updated_at) \
         VALUES ($1, $2, 'widget', 'closed', \
         '2026-03-10T09:00:00Z'::timestamptz, '2026-03-10T09:00:00Z'::timestamptz, \
         '2026-03-10T09:00:00Z'::timestamptz) RETURNING id",
    )
    .bind(tenant_b)
    .bind(customer_b)
    .fetch_one(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO conversation_feedback \
         (tenant_id, conversation_id, channel, rating, submitted_at) \
         VALUES ($1, $2, 'widget', 1, '2026-03-10T12:00:00Z'::timestamptz)",
    )
    .bind(tenant_b)
    .bind(conv_b)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO ai_usage_records \
         (tenant_id, provider, model, input_tokens, output_tokens, status, streamed, latency_ms, created_at) \
         VALUES ($1, 'openai', 'gpt-4o', 999, 0, 'success', false, 10, \
         '2026-03-10T09:00:00Z'::timestamptz)",
    )
    .bind(tenant_b)
    .execute(pool)
    .await
    .unwrap();

    (tenant_a, tenant_b)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn summary_returns_expected_metrics_for_seeded_tenant() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let (tenant_a, _tenant_b) = seed_canonical_dataset(&pool).await;
    let user = seed_member(&pool, tenant_a, "admin@analytics-t014.test", "admin").await;
    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/analytics/summary?from=2026-03-10&to=2026-03-12",
            user,
            tenant_a,
        ),
    )
    .await;
    let body = body_json(response).await;
    assert_eq!(body["conversation_volume"], 4);
    assert_eq!(body["concluded_count"], 3);
    let ai_rate = body["ai_resolution_rate"].as_f64().unwrap();
    assert!((ai_rate - 2.0 / 3.0).abs() < 0.001);
    assert!((body["handoff_rate"].as_f64().unwrap() - 0.25).abs() < 0.001);
    assert!((body["avg_first_response_seconds"].as_f64().unwrap() - 40.0).abs() < 0.001);
    assert!((body["avg_response_seconds"].as_f64().unwrap() - 37.5).abs() < 0.001);
    assert!((body["satisfaction_avg"].as_f64().unwrap() - 4.0).abs() < 0.001);
    assert_eq!(body["satisfaction_count"], 2);
    assert_eq!(body["total_tokens"], 365);
    assert_eq!(body["unattributed_tokens"], 15);
}

#[tokio::test]
async fn summary_is_tenant_isolated() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let (tenant_a, tenant_b) = seed_canonical_dataset(&pool).await;
    let user_b = seed_member(&pool, tenant_b, "admin@analytics-t015.test", "admin").await;
    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/analytics/summary?from=2026-03-10&to=2026-03-12",
            user_b,
            tenant_b,
        ),
    )
    .await;
    let body = body_json(response).await;
    assert_eq!(body["conversation_volume"], 1);
    assert_eq!(body["satisfaction_count"], 1);
    assert!((body["satisfaction_avg"].as_f64().unwrap() - 1.0).abs() < 0.001);
    assert_eq!(body["total_tokens"], 999);
    let user_a = seed_member(&pool, tenant_a, "admin@analytics-t015-a.test", "admin").await;
    let response2 = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/analytics/summary?from=2026-03-10&to=2026-03-12",
            user_a,
            tenant_a,
        ),
    )
    .await;
    let body2 = body_json(response2).await;
    assert_eq!(body2["conversation_volume"], 4);
}

#[tokio::test]
async fn summary_enforces_rbac() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let (tenant_a, _tenant_b) = seed_canonical_dataset(&pool).await;
    let uri = "/api/v1/tenant/analytics/summary?from=2026-03-10&to=2026-03-12";
    let roles: [&str; 5] = ["owner", "admin", "manager", "agent", "viewer"];
    let expected: [StatusCode; 5] = [
        StatusCode::OK,
        StatusCode::OK,
        StatusCode::OK,
        StatusCode::FORBIDDEN,
        StatusCode::FORBIDDEN,
    ];
    for (role, expected_status) in roles.iter().zip(expected.iter()) {
        let user =
            seed_member(&pool, tenant_a, &format!("{role}@analytics-t016.test"), role).await;
        let response = send(
            pool.clone(),
            authenticated_request(uri, user, tenant_a),
        )
        .await;
        assert_eq!(
            response.status(),
            *expected_status,
            "role {role} expected {expected_status} but got {}",
            response.status()
        );
    }
    let unauth = Request::builder()
        .uri(uri)
        .method(Method::GET)
        .header("X-Tenant-ID", tenant_a.to_string())
        .body(Body::empty())
        .unwrap();
    let response = send(pool.clone(), unauth).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn summary_empty_range_returns_zeroes_not_error() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let (tenant_a, _tenant_b) = seed_canonical_dataset(&pool).await;
    let user = seed_member(&pool, tenant_a, "admin@analytics-t017.test", "admin").await;
    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/analytics/summary?from=2026-01-01&to=2026-01-07",
            user,
            tenant_a,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["conversation_volume"], 0);
    assert_eq!(body["concluded_count"], 0);
    assert!(body["ai_resolution_rate"].is_null());
    assert!(body["handoff_rate"].is_null());
    assert!(body["avg_first_response_seconds"].is_null());
    assert!(body["satisfaction_avg"].is_null());
}

#[tokio::test]
async fn summary_respects_date_range() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let (tenant_a, _) = seed_canonical_dataset(&pool).await;
    let user = seed_member(&pool, tenant_a, "admin@t032.test", "admin").await;

    // Day 1: 2026-03-10 — only C1 and C2
    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/analytics/summary?from=2026-03-10&to=2026-03-10",
            user,
            tenant_a,
        ),
    )
    .await;
    let body = body_json(response).await;
    assert_eq!(body["conversation_volume"], 2);
    assert_eq!(body["satisfaction_count"], 1);
    assert!((body["satisfaction_avg"].as_f64().unwrap() - 5.0).abs() < 0.001);
    assert_eq!(body["total_tokens"], 150);

    // Day 2: 2026-03-11 — C3 and C4
    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/analytics/summary?from=2026-03-11&to=2026-03-11",
            user,
            tenant_a,
        ),
    )
    .await;
    let body = body_json(response).await;
    assert_eq!(body["conversation_volume"], 2);
    assert_eq!(body["total_tokens"], 215);

    // Full three-day range — C1, C2, C3, C4
    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/analytics/summary?from=2026-03-10&to=2026-03-12",
            user,
            tenant_a,
        ),
    )
    .await;
    let body = body_json(response).await;
    assert_eq!(body["conversation_volume"], 4);
}

#[tokio::test]
async fn summary_rejects_invalid_ranges() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let (tenant_a, _) = seed_canonical_dataset(&pool).await;
    let user = seed_member(&pool, tenant_a, "admin@t033.test", "admin").await;

    let cases: &[(&str, &str, &str)] = &[
        ("from=2026-03-12&to=2026-03-10", "from must be on or before to", "/api/v1/tenant/analytics/summary?from=2026-03-12&to=2026-03-10"),
        ("", "Date range must not exceed 366 days", "/api/v1/tenant/analytics/summary?from=2025-01-01&to=2026-12-31"),
        ("", "Invalid date format, expected YYYY-MM-DD", "/api/v1/tenant/analytics/summary?from=notadate&to=2026-03-10"),
        ("", "Unknown channel", "/api/v1/tenant/analytics/summary?channel=carrier-pigeon"),
    ];

    for (_label, expected_msg, uri) in cases {
        let response = send(pool.clone(), authenticated_request(uri, user, tenant_a)).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "expected 422 for {uri}"
        );
        let body = body_json(response).await;
        assert_eq!(
            body["error"]["message"],
            *expected_msg,
            "unexpected error message for {uri}"
        );
    }

    // Omitting from and to should succeed (defaults to 30-day range).
    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/analytics/summary",
            user,
            tenant_a,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn summary_returns_channel_breakdown() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let (tenant_a, tenant_b) = seed_canonical_dataset(&pool).await;
    let user_a = seed_member(&pool, tenant_a, "admin@t049-a.test", "admin").await;
    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/analytics/summary?from=2026-03-10&to=2026-03-12",
            user_a,
            tenant_a,
        ),
    )
    .await;
    let body = body_json(response).await;
    let channels = body["channels"].as_array().unwrap();
    assert_eq!(channels.len(), 2);
    assert_eq!(channels[0]["channel"], "widget");
    assert_eq!(channels[0]["conversation_count"], 3);
    assert!((channels[0]["share"].as_f64().unwrap() - 0.75).abs() < 0.001);
    assert_eq!(channels[1]["channel"], "email");
    assert_eq!(channels[1]["conversation_count"], 1);
    assert!((channels[1]["share"].as_f64().unwrap() - 0.25).abs() < 0.001);

    let user_b = seed_member(&pool, tenant_b, "admin@t049-b.test", "admin").await;
    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/analytics/summary?from=2026-03-10&to=2026-03-12",
            user_b,
            tenant_b,
        ),
    )
    .await;
    let body = body_json(response).await;
    let channels = body["channels"].as_array().unwrap();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0]["channel"], "widget");
    assert_eq!(channels[0]["conversation_count"], 1);
    assert!((channels[0]["share"].as_f64().unwrap() - 1.0).abs() < 0.001);
}

#[tokio::test]
async fn summary_respects_channel_filter() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let (tenant_a, _tenant_b) = seed_canonical_dataset(&pool).await;
    let user = seed_member(&pool, tenant_a, "admin@t050.test", "admin").await;

    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/analytics/summary?from=2026-03-10&to=2026-03-12&channel=widget",
            user,
            tenant_a,
        ),
    )
    .await;
    let body = body_json(response).await;
    assert_eq!(body["conversation_volume"], 3);
    assert!((body["handoff_rate"].as_f64().unwrap() - 1.0 / 3.0).abs() < 0.001);
    assert_eq!(body["total_tokens"], 150);
    assert_eq!(body["unattributed_tokens"], 0);
    let channels = body["channels"].as_array().unwrap();
    assert_eq!(channels.len(), 2);

    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/analytics/summary?from=2026-03-10&to=2026-03-12&channel=email",
            user,
            tenant_a,
        ),
    )
    .await;
    let body = body_json(response).await;
    assert_eq!(body["conversation_volume"], 1);
    assert_eq!(body["total_tokens"], 200);
    let channels = body["channels"].as_array().unwrap();
    assert_eq!(channels.len(), 2);
}

#[tokio::test]
async fn timeseries_returns_one_zero_filled_entry_per_day() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let (tenant_a, _tenant_b) = seed_canonical_dataset(&pool).await;
    let user = seed_member(&pool, tenant_a, "admin@t038.test", "admin").await;
    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/analytics/timeseries?from=2026-03-10&to=2026-03-12",
            user,
            tenant_a,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let days = body["days"].as_array().unwrap();
    assert_eq!(days.len(), 3);

    assert_eq!(days[0]["date"], "2026-03-10");
    assert_eq!(days[0]["conversation_volume"], 2);
    assert_eq!(days[0]["ai_resolved"], 2);
    assert_eq!(days[0]["handed_off"], 0);
    assert!((days[0]["satisfaction_avg"].as_f64().unwrap() - 5.0).abs() < 0.001);
    assert_eq!(days[0]["satisfaction_count"], 1);
    assert_eq!(days[0]["total_tokens"], 150);

    assert_eq!(days[1]["date"], "2026-03-11");
    assert_eq!(days[1]["conversation_volume"], 2);
    assert_eq!(days[1]["ai_resolved"], 0);
    assert_eq!(days[1]["handed_off"], 1);
    assert!((days[1]["satisfaction_avg"].as_f64().unwrap() - 3.0).abs() < 0.001);
    assert_eq!(days[1]["satisfaction_count"], 1);
    assert_eq!(days[1]["total_tokens"], 215);

    assert_eq!(days[2]["date"], "2026-03-12");
    assert_eq!(days[2]["conversation_volume"], 0);
    assert_eq!(days[2]["ai_resolved"], 0);
    assert_eq!(days[2]["handed_off"], 0);
    assert!(days[2]["satisfaction_avg"].is_null());
    assert_eq!(days[2]["satisfaction_count"], 0);
    assert_eq!(days[2]["total_tokens"], 0);
}

#[tokio::test]
async fn timeseries_day_count_matches_range_length() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let (tenant_a, _tenant_b) = seed_canonical_dataset(&pool).await;
    let user = seed_member(&pool, tenant_a, "admin@t039.test", "admin").await;

    // 1-day range
    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/analytics/timeseries?from=2026-03-10&to=2026-03-10",
            user,
            tenant_a,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let days = body["days"].as_array().unwrap();
    assert_eq!(days.len(), 1);
    assert_eq!(days[0]["date"], "2026-03-10");

    // 7-day range
    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/analytics/timeseries?from=2026-03-10&to=2026-03-16",
            user,
            tenant_a,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let days = body["days"].as_array().unwrap();
    assert_eq!(days.len(), 7);
    assert_eq!(days[0]["date"], "2026-03-10");
    assert_eq!(days[6]["date"], "2026-03-16");

    // 31-day range
    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/analytics/timeseries?from=2026-01-01&to=2026-01-31",
            user,
            tenant_a,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let days = body["days"].as_array().unwrap();
    assert_eq!(days.len(), 31);
    assert_eq!(days[0]["date"], "2026-01-01");
    assert_eq!(days[30]["date"], "2026-01-31");
}

#[tokio::test]
async fn past_period_metrics_do_not_drift_when_new_activity_arrives() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let (tenant_a, _) = seed_canonical_dataset(&pool).await;
    let user = seed_member(&pool, tenant_a, "admin@t062.test", "admin").await;

    let url = "/api/v1/tenant/analytics/summary?from=2026-03-10&to=2026-03-12";
    let first = send(
        pool.clone(),
        authenticated_request(url, user, tenant_a),
    )
    .await;
    let body_first = body_json(first).await;

    let customer_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM customers WHERE tenant_id = $1 LIMIT 1",
    )
    .bind(tenant_a)
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, \
         created_at, last_activity_at, updated_at) \
         VALUES ($1, $2, 'widget', 'closed', \
         '2026-04-01T09:00:00Z'::timestamptz, '2026-04-01T09:00:00Z'::timestamptz, \
         '2026-04-01T09:00:00Z'::timestamptz)",
    )
    .bind(tenant_a)
    .bind(customer_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO ai_usage_records \
         (tenant_id, provider, model, input_tokens, output_tokens, status, streamed, latency_ms, created_at) \
         VALUES ($1, 'openai', 'gpt-4o', 500, 0, 'success', false, 10, \
         '2026-04-01T09:00:00Z'::timestamptz)",
    )
    .bind(tenant_a)
    .execute(&pool)
    .await
    .unwrap();

    let second = send(
        pool.clone(),
        authenticated_request(url, user, tenant_a),
    )
    .await;
    let body_second = body_json(second).await;

    assert_eq!(body_first, body_second);
}

/// Opt-in performance check: seeds 100k conversations and asserts both endpoints
/// respond in <3s. Run with `cargo test --test analytics_api -- --ignored`.
#[ignore]
#[tokio::test]
async fn summary_and_timeseries_are_fast_on_a_large_tenant() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;

    let tenant_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id",
    )
    .bind("Performance Tenant")
    .bind(format!("perf-{}", Uuid::new_v4().simple()))
    .fetch_one(&pool)
    .await
    .unwrap();

    let customer_id: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Perf Customer")
    .fetch_one(&pool)
    .await
    .unwrap();

    let user = seed_member(&pool, tenant_id, "admin@t063.test", "admin").await;

    sqlx::query(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, \
         created_at, last_activity_at, updated_at) \
         SELECT $1, $2, 'widget', 'closed', \
                '2026-01-01T00:00:00Z'::timestamptz + (n * interval '1 minute'), \
                '2026-01-01T00:00:00Z'::timestamptz + (n * interval '1 minute'), \
                '2026-01-01T00:00:00Z'::timestamptz + (n * interval '1 minute') \
         FROM generate_series(1, 100000) AS n",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .execute(&pool)
    .await
    .unwrap();

    let start = std::time::Instant::now();
    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/analytics/summary?from=2026-01-01&to=2026-04-01",
            user,
            tenant_id,
        ),
    )
    .await;
    let summary_elapsed = start.elapsed();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        summary_elapsed.as_secs() < 3,
        "summary took {:?} (expected <3s)",
        summary_elapsed
    );

    let start = std::time::Instant::now();
    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/analytics/timeseries?from=2026-01-01&to=2026-04-01",
            user,
            tenant_id,
        ),
    )
    .await;
    let timeseries_elapsed = start.elapsed();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        timeseries_elapsed.as_secs() < 3,
        "timeseries took {:?} (expected <3s)",
        timeseries_elapsed
    );
}
