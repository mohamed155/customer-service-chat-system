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
            eprintln!("skipping feedback API tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping feedback API tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
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
    .expect("failed to reset feedback test tables");
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

async fn seed_tenant(pool: &sqlx::PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(name)
        .bind(format!("fbk-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

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

fn authed_json_post(path: &str, token: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .uri(path)
        .method(Method::POST)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

fn authed_get(path: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .method(Method::GET)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

/// Create a widget conversation, set status to ended, and return (conv_id, token).
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
        .bind("Feedback Test User")
        .fetch_one(pool)
        .await
        .unwrap()
}

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

// ── Tests: T008 / T050 / T021 / T035 / T038 ────────────────────────────────

/// T050 — channel='widget' regression test: assert that POST /widget/v1/conversations
/// persists a conversation row with channel = 'widget'. This must fail before
/// migration 0051 Part 1 is applied and pass after.
#[tokio::test]
async fn t050_channel_widget_regression() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T050").await;
    let pub_id = "wgt_t050";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let token = mint_session(&pool, pub_id).await;

    let response = send(
        pool.clone(),
        authed_json_post(
            "/api/v1/widget/v1/conversations",
            &token,
            serde_json::json!({}),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let json = body_json(response).await;
    let conv_id: Uuid = json["data"]["conversation"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let row: (String,) = sqlx::query_as("SELECT channel FROM conversations WHERE id = $1")
        .bind(conv_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.0, "widget");
}

/// T008a — submit on an ended (resolved) conversation returns 201 and persists one row.
#[tokio::test]
async fn t008a_submit_on_ended_conversation_returns_201() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T008a").await;
    let pub_id = "wgt_t008a";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let (conv_id, token) = setup_ended_conversation(&pool, tenant_id, pub_id, "resolved").await;

    let response = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/feedback"),
            &token,
            serde_json::json!({ "rating": 4 }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let json = body_json(response).await;
    assert_eq!(json["data"]["feedback"]["rating"], 4);

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversation_feedback WHERE conversation_id = $1")
            .bind(conv_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

/// T008b — submitting the same conversation twice returns 200 and exactly one row.
#[tokio::test]
async fn t008b_duplicate_submission_returns_200() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T008b").await;
    let pub_id = "wgt_t008b";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let (conv_id, token) = setup_ended_conversation(&pool, tenant_id, pub_id, "closed").await;

    let payload = serde_json::json!({ "rating": 3 });
    let first = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/feedback"),
            &token,
            payload.clone(),
        ),
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);

    let second = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/feedback"),
            &token,
            payload,
        ),
    )
    .await;
    assert_eq!(second.status(), StatusCode::OK);

    let json = body_json(second).await;
    assert_eq!(json["data"]["feedback"]["rating"], 3);

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversation_feedback WHERE conversation_id = $1")
            .bind(conv_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

/// T008c — rating 0 and rating 6 return 422.
#[tokio::test]
async fn t008c_invalid_ratings_return_422() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T008c").await;
    let pub_id = "wgt_t008c";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let (conv_id, token) = setup_ended_conversation(&pool, tenant_id, pub_id, "resolved").await;

    let zero = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/feedback"),
            &token,
            serde_json::json!({ "rating": 0 }),
        ),
    )
    .await;
    assert_eq!(zero.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let six = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/feedback"),
            &token,
            serde_json::json!({ "rating": 6 }),
        ),
    )
    .await;
    assert_eq!(six.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

/// T008d — submitting for a conversation owned by a different session returns 404.
#[tokio::test]
async fn t008d_wrong_session_ownership_returns_404() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T008d").await;
    let pub_id = "wgt_t008d";
    seed_widget_instance(&pool, tenant_id, pub_id).await;

    // Create conversation with session A
    let (conv_id, _token_a) = setup_ended_conversation(&pool, tenant_id, pub_id, "resolved").await;

    // Submit feedback with session B
    let token_b = mint_session(&pool, pub_id).await;

    let response = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/feedback"),
            &token_b,
            serde_json::json!({ "rating": 5 }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// T008e — submitting for an open conversation returns 422.
#[tokio::test]
async fn t008e_open_conversation_returns_422() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T008e").await;
    let pub_id = "wgt_t008e";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let (conv_id, token) = setup_ended_conversation(&pool, tenant_id, pub_id, "open").await;

    let response = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/feedback"),
            &token,
            serde_json::json!({ "rating": 3 }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

/// T008f — GET /widget/v1/feedback/pending returns the ended unrated conversation,
/// then returns data: null after feedback is submitted.
#[tokio::test]
async fn t008f_pending_feedback_flow() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T008f").await;
    let pub_id = "wgt_t008f";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let (conv_id, token) = setup_ended_conversation(&pool, tenant_id, pub_id, "resolved").await;

    // Before submit: pending feedback should exist
    let pending_resp = send(
        pool.clone(),
        authed_get("/api/v1/widget/v1/feedback/pending", &token),
    )
    .await;
    assert_eq!(pending_resp.status(), StatusCode::OK);
    let pending_body = body_json(pending_resp).await;
    assert_eq!(
        pending_body["data"]["conversationId"].as_str().unwrap(),
        conv_id.to_string()
    );

    // Submit feedback
    let submit = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/feedback"),
            &token,
            serde_json::json!({ "rating": 2 }),
        ),
    )
    .await;
    assert_eq!(submit.status(), StatusCode::CREATED);

    // After submit: pending feedback should be null
    let after_resp = send(
        pool.clone(),
        authed_get("/api/v1/widget/v1/feedback/pending", &token),
    )
    .await;
    assert_eq!(after_resp.status(), StatusCode::OK);
    let after_body = body_json(after_resp).await;
    assert!(after_body["data"].is_null());
}

/// T021a — 2000 character comment accepted.
#[tokio::test]
async fn t021a_2000_char_comment_accepted() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T021a").await;
    let pub_id = "wgt_t021a";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let (conv_id, token) = setup_ended_conversation(&pool, tenant_id, pub_id, "closed").await;

    let comment = "x".repeat(2000);
    let response = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/feedback"),
            &token,
            serde_json::json!({ "rating": 5, "comment": comment }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let json = body_json(response).await;
    assert_eq!(
        json["data"]["feedback"]["comment"].as_str().unwrap().len(),
        2000
    );
}

/// T021b — 2001 character comment returns 422 and no row is persisted.
#[tokio::test]
async fn t021b_2001_char_comment_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T021b").await;
    let pub_id = "wgt_t021b";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let (conv_id, token) = setup_ended_conversation(&pool, tenant_id, pub_id, "resolved").await;

    let comment = "x".repeat(2001);
    let response = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/feedback"),
            &token,
            serde_json::json!({ "rating": 4, "comment": comment }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM conversation_feedback WHERE conversation_id = $1")
            .bind(conv_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

/// T021c — whitespace-only comment stored as SQL NULL.
#[tokio::test]
async fn t021c_whitespace_only_comment_stored_as_null() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T021c").await;
    let pub_id = "wgt_t021c";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let (conv_id, token) = setup_ended_conversation(&pool, tenant_id, pub_id, "resolved").await;

    let response = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/feedback"),
            &token,
            serde_json::json!({ "rating": 3, "comment": "   " }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let comment: Option<String> =
        sqlx::query_scalar("SELECT comment FROM conversation_feedback WHERE conversation_id = $1")
            .bind(conv_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        comment.is_none(),
        "whitespace-only comment should be stored as NULL"
    );
}

/// T021d — comment round-trips unchanged.
#[tokio::test]
async fn t021d_comment_round_trip() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T021d").await;
    let pub_id = "wgt_t021d";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let (conv_id, token) = setup_ended_conversation(&pool, tenant_id, pub_id, "closed").await;

    let expected = "Great service, very helpful!";
    let response = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/feedback"),
            &token,
            serde_json::json!({ "rating": 5, "comment": expected }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let json = body_json(response).await;
    assert_eq!(
        json["data"]["feedback"]["comment"].as_str().unwrap(),
        expected
    );

    // Also verify via GET to the feedback endpoint (the duplicate path)
    let second = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/feedback"),
            &token,
            serde_json::json!({ "rating": 5, "comment": expected }),
        ),
    )
    .await;
    assert_eq!(second.status(), StatusCode::OK);
    let json2 = body_json(second).await;
    assert_eq!(
        json2["data"]["feedback"]["comment"].as_str().unwrap(),
        expected
    );
}

/// T035a — AI-only conversation: channel set, agent_configuration_id set, assigned_membership_id NULL.
#[tokio::test]
async fn t035a_ai_only_conversation_attribution() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T035a").await;
    let pub_id = "wgt_t035a";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let (conv_id, token) = setup_ended_conversation(&pool, tenant_id, pub_id, "resolved").await;

    // Create an agent configuration
    // Note: No status column in migration 0041; the ac.status field referenced
    // by resolve_ai_agent_configuration is not present in the schema.
    let agent_id: Uuid = sqlx::query_scalar(
        "INSERT INTO agent_configurations \
         (tenant_id, name, is_default, provider, model) \
         VALUES ($1, $2, true, 'openai', 'gpt-4') RETURNING id",
    )
    .bind(tenant_id)
    .bind("Test Agent")
    .fetch_one(&pool)
    .await
    .unwrap();

    // Create a message in the conversation so ai_generations can reference it
    let msg_id: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'Hello') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conv_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Create an ai_generation row to trigger attribution
    sqlx::query(
        "INSERT INTO ai_generations \
         (tenant_id, conversation_id, trigger_message_id, outcome, attempts, latency_ms) \
         VALUES ($1, $2, $3, 'success', 1, 100)",
    )
    .bind(tenant_id)
    .bind(conv_id)
    .bind(msg_id)
    .execute(&pool)
    .await
    .unwrap();

    let response = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/feedback"),
            &token,
            serde_json::json!({ "rating": 4 }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let row: (String, Option<Uuid>, Option<Uuid>) = sqlx::query_as(
        "SELECT channel, agent_configuration_id, assigned_membership_id \
         FROM conversation_feedback WHERE conversation_id = $1",
    )
    .bind(conv_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "widget");
    assert_eq!(
        row.1,
        Some(agent_id),
        "agent_configuration_id should be set for AI-only conversation"
    );
    assert_eq!(
        row.2, None,
        "assigned_membership_id should be NULL for AI-only conversation"
    );
}

/// T035b — Human-assigned conversation: both attribution columns set.
#[tokio::test]
async fn t035b_human_assigned_conversation_attribution() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T035b").await;
    let pub_id = "wgt_t035b";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let (_user_id, membership_id) = seed_admin(&pool, tenant_id, "admin@t035b.test").await;

    // Create conversation with assigned membership and with AI generations
    let token = mint_session(&pool, pub_id).await;
    let conv_resp = send(
        pool.clone(),
        authed_json_post(
            "/api/v1/widget/v1/conversations",
            &token,
            serde_json::json!({}),
        ),
    )
    .await;
    let conv_id: Uuid = body_json(conv_resp).await["data"]["conversation"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    sqlx::query(
        "UPDATE conversations SET status = 'resolved', assigned_membership_id = $1 WHERE id = $2",
    )
    .bind(membership_id)
    .bind(conv_id)
    .execute(&pool)
    .await
    .unwrap();

    // Create agent configuration and ai_generation (to set agent_configuration_id)
    let agent_id: Uuid = sqlx::query_scalar(
        "INSERT INTO agent_configurations \
         (tenant_id, name, is_default, provider, model) \
         VALUES ($1, $2, true, 'openai', 'gpt-4') RETURNING id",
    )
    .bind(tenant_id)
    .bind("Test Agent T035b")
    .fetch_one(&pool)
    .await
    .unwrap();

    let msg_id: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'Help') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conv_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO ai_generations \
         (tenant_id, conversation_id, trigger_message_id, outcome, attempts, latency_ms) \
         VALUES ($1, $2, $3, 'success', 1, 100)",
    )
    .bind(tenant_id)
    .bind(conv_id)
    .bind(msg_id)
    .execute(&pool)
    .await
    .unwrap();

    let response = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/feedback"),
            &token,
            serde_json::json!({ "rating": 5 }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let row: (String, Option<Uuid>, Option<Uuid>) = sqlx::query_as(
        "SELECT channel, agent_configuration_id, assigned_membership_id \
         FROM conversation_feedback WHERE conversation_id = $1",
    )
    .bind(conv_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "widget");
    assert_eq!(
        row.1,
        Some(agent_id),
        "agent_configuration_id should be set"
    );
    assert_eq!(
        row.2,
        Some(membership_id),
        "assigned_membership_id should be set for human-assigned conversation"
    );
}

/// T035c — Neither AI nor human: both attribution columns NULL, channel always set.
#[tokio::test]
async fn t035c_neither_ai_nor_human_attribution() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T035c").await;
    let pub_id = "wgt_t035c";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let (conv_id, token) = setup_ended_conversation(&pool, tenant_id, pub_id, "resolved").await;

    let response = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/feedback"),
            &token,
            serde_json::json!({ "rating": 3 }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let row: (String, Option<Uuid>, Option<Uuid>) = sqlx::query_as(
        "SELECT channel, agent_configuration_id, assigned_membership_id \
         FROM conversation_feedback WHERE conversation_id = $1",
    )
    .bind(conv_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, "widget");
    assert_eq!(
        row.1, None,
        "agent_configuration_id should be NULL when no AI generations exist"
    );
    assert_eq!(
        row.2, None,
        "assigned_membership_id should be NULL when unassigned"
    );
}

/// T038a — GET /tenant/feedback/summary returns correct average_rating and feedback_count.
#[tokio::test]
async fn t038a_feedback_summary_with_data() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T038a").await;
    let user_id = seed_user(&pool, "admin@t038a.test").await;
    let _membership_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, 'admin', 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Seed a customer and multiple conversations with feedback
    let customer_id: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Test Customer")
    .fetch_one(&pool)
    .await
    .unwrap();

    for rating in [3i16, 4, 5] {
        let conv_id: Uuid = sqlx::query_scalar(
            "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
             VALUES ($1, $2, 'web_chat', 'closed') RETURNING id",
        )
        .bind(tenant_id)
        .bind(customer_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO conversation_feedback \
             (tenant_id, conversation_id, channel, rating) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(tenant_id)
        .bind(conv_id)
        .bind("web_chat")
        .bind(rating)
        .execute(&pool)
        .await
        .unwrap();
    }

    let response = send(
        pool.clone(),
        authenticated_request("/api/v1/tenant/feedback/summary", user_id, tenant_id),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    // average of 3, 4, 5 = 4.0
    assert_eq!(json["average_rating"].as_f64().unwrap(), 4.0);
    assert_eq!(json["feedback_count"].as_i64().unwrap(), 3);
}

/// T038b — Summary with no feedback returns average_rating: null, feedback_count: 0.
#[tokio::test]
async fn t038b_feedback_summary_empty() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T038b").await;
    let user_id = seed_user(&pool, "admin@t038b.test").await;
    let _membership_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, 'admin', 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let response = send(
        pool.clone(),
        authenticated_request("/api/v1/tenant/feedback/summary", user_id, tenant_id),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert!(json["average_rating"].is_null());
    assert_eq!(json["feedback_count"].as_i64().unwrap(), 0);
}

/// T038c — Cross-tenant isolation: feedback in tenant A does not affect tenant B's summary.
#[tokio::test]
async fn t038c_cross_tenant_isolation() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_a = seed_tenant(&pool, "T038cA").await;
    let tenant_b = seed_tenant(&pool, "T038cB").await;

    // Seed customer in tenant A
    let customer_a: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_a)
    .bind("Cust A")
    .fetch_one(&pool)
    .await
    .unwrap();

    // Add feedback in tenant A only
    let conv_a: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'closed') RETURNING id",
    )
    .bind(tenant_a)
    .bind(customer_a)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO conversation_feedback (tenant_id, conversation_id, channel, rating) \
         VALUES ($1, $2, $3, 5)",
    )
    .bind(tenant_a)
    .bind(conv_a)
    .bind("web_chat")
    .execute(&pool)
    .await
    .unwrap();

    let user_a = seed_user(&pool, "admin@t038cA.test").await;
    let user_b = seed_user(&pool, "admin@t038cB.test").await;
    for (uid, tid) in [(user_a, tenant_a), (user_b, tenant_b)] {
        sqlx::query(
            "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
             VALUES ($1, $2, 'admin', 'active')",
        )
        .bind(tid)
        .bind(uid)
        .execute(&pool)
        .await
        .unwrap();
    }

    // Tenant A sees the feedback
    let resp_a = send(
        pool.clone(),
        authenticated_request("/api/v1/tenant/feedback/summary", user_a, tenant_a),
    )
    .await;
    let json_a = body_json(resp_a).await;
    assert_eq!(json_a["average_rating"].as_f64().unwrap(), 5.0);
    assert_eq!(json_a["feedback_count"].as_i64().unwrap(), 1);

    // Tenant B sees empty summary
    let resp_b = send(
        pool.clone(),
        authenticated_request("/api/v1/tenant/feedback/summary", user_b, tenant_b),
    )
    .await;
    let json_b = body_json(resp_b).await;
    assert!(json_b["average_rating"].is_null());
    assert_eq!(json_b["feedback_count"].as_i64().unwrap(), 0);
}

// ── Tests: T025 Tenant-Read (US3) ────────────────────────────────────────────

/// T025a — GET /tenant/conversations/{id} returns feedback object for rated conversation.
#[tokio::test]
async fn t025a_detail_returns_feedback_for_rated() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T025a").await;
    let pub_id = "wgt_t025a";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let (user_id, _membership_id) = seed_admin(&pool, tenant_id, "admin@t025a.test").await;
    let (conv_id, token) = setup_ended_conversation(&pool, tenant_id, pub_id, "resolved").await;

    let submit = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/feedback"),
            &token,
            serde_json::json!({ "rating": 4, "comment": "Great support!" }),
        ),
    )
    .await;
    assert_eq!(submit.status(), StatusCode::CREATED);

    let response = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/conversations/{conv_id}"),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let feedback = &json["data"]["feedback"];
    assert!(feedback.is_object());
    assert_eq!(feedback["rating"], 4);
    assert_eq!(feedback["comment"], "Great support!");
    assert!(feedback["submitted_at"].is_string());
}

/// T025b — GET /tenant/conversations/{id} returns feedback: null for unrated conversation.
#[tokio::test]
async fn t025b_detail_returns_null_for_unrated() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T025b").await;
    let pub_id = "wgt_t025b";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let (user_id, _membership_id) = seed_admin(&pool, tenant_id, "admin@t025b.test").await;
    let (conv_id, _token) = setup_ended_conversation(&pool, tenant_id, pub_id, "closed").await;

    let response = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/conversations/{conv_id}"),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert!(
        json["data"]["feedback"].is_null(),
        "feedback should be null for unrated conversation"
    );
}

/// T025c — GET /tenant/conversations includes rating on rated rows.
#[tokio::test]
async fn t025c_list_includes_rating_for_rated() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T025c").await;
    let pub_id = "wgt_t025c";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let (user_id, _membership_id) = seed_admin(&pool, tenant_id, "admin@t025c.test").await;
    let (conv_id, token) = setup_ended_conversation(&pool, tenant_id, pub_id, "resolved").await;

    let submit = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_id}/feedback"),
            &token,
            serde_json::json!({ "rating": 5 }),
        ),
    )
    .await;
    assert_eq!(submit.status(), StatusCode::CREATED);

    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/conversations?status=all",
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let convs = json["data"].as_array().unwrap();
    let found = convs.iter().find(|c| c["id"] == conv_id.to_string());
    assert!(found.is_some(), "rated conversation should appear in list");
    assert_eq!(found.unwrap()["rating"], 5);
}

/// T025d — GET /tenant/conversations has rating: null on unrated rows.
#[tokio::test]
async fn t025d_list_rating_null_for_unrated() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "T025d").await;
    let pub_id = "wgt_t025d";
    seed_widget_instance(&pool, tenant_id, pub_id).await;
    let (user_id, _membership_id) = seed_admin(&pool, tenant_id, "admin@t025d.test").await;
    let (conv_id, _token) = setup_ended_conversation(&pool, tenant_id, pub_id, "resolved").await;

    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/conversations?status=all",
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let convs = json["data"].as_array().unwrap();
    let found = convs.iter().find(|c| c["id"] == conv_id.to_string());
    assert!(
        found.is_some(),
        "unrated conversation should appear in list"
    );
    assert!(
        found.unwrap()["rating"].is_null(),
        "rating should be null for unrated conversation"
    );
}

/// T025e — Cross-tenant isolation: feedback from tenant A not visible to tenant B.
#[tokio::test]
async fn t025e_cross_tenant_isolation() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_a = seed_tenant(&pool, "T025eA").await;
    let tenant_b = seed_tenant(&pool, "T025eB").await;

    let pub_a = "wgt_t025eA";
    let pub_b = "wgt_t025eB";
    seed_widget_instance(&pool, tenant_a, pub_a).await;
    seed_widget_instance(&pool, tenant_b, pub_b).await;

    let (_user_a, _mid_a) = seed_admin(&pool, tenant_a, "admin@t025eA.test").await;
    let (user_b, _mid_b) = seed_admin(&pool, tenant_b, "admin@t025eB.test").await;

    let (conv_a, token_a) = setup_ended_conversation(&pool, tenant_a, pub_a, "resolved").await;
    let submit = send(
        pool.clone(),
        authed_json_post(
            &format!("/api/v1/widget/v1/conversations/{conv_a}/feedback"),
            &token_a,
            serde_json::json!({ "rating": 4 }),
        ),
    )
    .await;
    assert_eq!(submit.status(), StatusCode::CREATED);

    let response = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/conversations/{conv_a}"),
            user_b,
            tenant_b,
        ),
    )
    .await;
    if response.status() == StatusCode::OK {
        let json = body_json(response).await;
        assert!(
            json["data"]["feedback"].is_null(),
            "tenant B should not see tenant A's feedback"
        );
    } else {
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
