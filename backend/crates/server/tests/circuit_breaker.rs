use std::sync::Arc;
use std::time::Duration;

use ai::agent_responder::process_agent_responder_once;
use ai::crypto::{self, MasterKey};
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use serial_test::serial;
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
        integration_secrets_key: None,
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
        rate_limit_enabled: true,
        rate_limit_login_max: 10,
        rate_limit_login_window_secs: 60,
        rate_limit_max_override: 10000,
        rate_limit_fail_mode: "auth_closed".into(),
        security_headers_enabled: true,
        csp_policy:
            "default-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'".into(),
        csp_report_only: true,
        hsts_max_age: 31536000,
        otlp_endpoint: None,
        trace_sample_ratio: 1.0,
        sentry_dsn: None,
        release_sha: None,
        trusted_proxy: false,
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
        budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
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
            eprintln!("skipping circuit_breaker tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping circuit_breaker tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(_pool: &sqlx::PgPool) {
    db::testing::migrate().await;
    sqlx::query(
        "TRUNCATE TABLE ai_generations, ai_usage_records, ai_credentials, ai_configurations, \
         agent_configurations, agent_avatar_uploads, \
         escalations, agent_availability, agent_skills, skills, \
         messages, customer_channel_identifiers, customers, conversations, \
         outbox_events, audit_logs, tenant_invitations, tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(&db::testing::owner_pool())
    .await
    .expect("failed to reset test tables");
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    let slug = format!("circuit-tenant-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Circuit Breaker Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Circuit Breaker Test User")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_membership(_pool: &sqlx::PgPool, tenant_id: Uuid, user_id: Uuid, role: &str) {
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(tenant_id)
        .bind(user_id)
        .bind(role)
        .execute(&db::testing::seed_pool())
        .await
        .unwrap();
}

async fn seed_ai_config(
    _pool: &sqlx::PgPool,
    tenant_id: Option<Uuid>,
    provider: &str,
    model: &str,
    fallbacks: serde_json::Value,
) {
    sqlx::query(
        "INSERT INTO ai_configurations (tenant_id, provider, model, fallbacks) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(tenant_id)
    .bind(provider)
    .bind(model)
    .bind(fallbacks)
    .execute(&db::testing::seed_pool())
    .await
    .unwrap();
}

async fn seed_ai_credential(
    _pool: &sqlx::PgPool,
    tenant_id: Option<Uuid>,
    provider: &str,
    api_key: &str,
    master: &MasterKey,
) {
    let aad = crypto::aad(tenant_id, provider);
    let (ciphertext, nonce) = crypto::seal(master, &aad, api_key).unwrap();
    let hint = crypto::hint(api_key);
    sqlx::query(
        "INSERT INTO ai_credentials (tenant_id, provider, ciphertext, nonce, key_hint) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(tenant_id)
    .bind(provider)
    .bind(ciphertext)
    .bind(nonce)
    .bind(hint)
    .execute(&db::testing::seed_pool())
    .await
    .unwrap();
}

fn master_key() -> MasterKey {
    MasterKey::from_base64("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=").unwrap()
}

// ── HTTP helpers ─────────────────────────────────────────────────────────────

async fn send(state: &AppState, request: Request<Body>) -> axum::response::Response {
    router::app_with_test_routes(state.clone())
        .oneshot(request)
        .await
        .expect("request should complete")
}

fn json_put(uri: &str, user_id: Uuid, tenant_id: Uuid, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(Method::PUT)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

async fn platform_health(state: &AppState, platform_user: Uuid) -> serde_json::Value {
    let response = send(
        state,
        Request::builder()
            .uri("/api/v1/platform/health")
            .method(Method::GET)
            .header("X-Dev-User-Id", platform_user.to_string())
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

// ── Scenario helpers ─────────────────────────────────────────────────────────

/// A web-chat conversation with one customer message and the outbox event that wakes the agent.
async fn customer_asks_for_help(tenant_id: Uuid, name: &str) -> Uuid {
    let seed = db::testing::seed_pool();
    let customer_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind(name)
    .fetch_one(&seed)
    .await
    .unwrap();
    let conversation_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'web_chat', 'open') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&seed)
    .await
    .unwrap();
    let message_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'I need help with my order') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&seed)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, 'conversation', $2, $3, 'conversation.customer_message', $4, now())",
    )
    .bind(Uuid::new_v4())
    .bind(conversation_id)
    .bind(tenant_id)
    .bind(serde_json::json!({
        "conversation_id": conversation_id,
        "message_id": message_id,
        "channel": "web_chat",
    }))
    .execute(&seed)
    .await
    .unwrap();
    conversation_id
}

/// Run the agent responder until the conversation has a generation record, or fail.
async fn run_until_generation_recorded(
    state: &AppState,
    pool: &sqlx::PgPool,
    conversation_id: Uuid,
) {
    for _ in 0..200 {
        let recorded: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM ai_generations WHERE conversation_id = $1)",
        )
        .bind(conversation_id)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap();
        if recorded {
            return;
        }
        if !process_agent_responder_once(pool, &state.ai, &state.escalations)
            .await
            .expect("agent responder")
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    panic!("no generation was recorded for conversation {conversation_id}");
}

async fn openai_calls(mock: &MockServer) -> usize {
    mock.received_requests().await.unwrap().len()
}

async fn generation(conversation_id: Uuid) -> (String, Option<String>) {
    sqlx::query_as("SELECT outcome, error_category FROM ai_generations WHERE conversation_id = $1")
        .bind(conversation_id)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap()
}

// ═══════════════════════════════════════════════════════════════════════════════
// MOH-40: an AI provider outage opens the circuit; customers are handed to a human
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[serial(circuit_breaker)]
async fn provider_outage_opens_the_circuit_and_turns_escalate_to_a_human() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    observability::metrics::init();

    let openai_mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
        .mount(&openai_mock)
        .await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());

    let tenant_id = seed_tenant(&pool).await;
    let admin_id = seed_user(&pool, "circuit-admin@test.com").await;
    seed_membership(&pool, tenant_id, admin_id, "admin").await;
    let platform_user: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name, platform_role) \
         VALUES ('circuit-platform@test.com', 'Platform', 'super_admin') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    seed_ai_config(&pool, None, "openai", "gpt-4", serde_json::json!([])).await;
    seed_ai_credential(&pool, None, "openai", "sk-circuit-key", &master_key()).await;
    let put = send(
        &state,
        json_put(
            "/api/v1/tenant/ai/agent",
            admin_id,
            tenant_id,
            serde_json::json!({
                "name": "Circuit Agent",
                "avatar": { "kind": "preset", "preset": "spark" },
                "tone": "professional",
                "businessRules": [],
                "escalationRules": [],
                "enabledChannels": ["web_chat"],
                "providerSelection": { "provider": "openai", "model": "gpt-4" },
            }),
        ),
    )
    .await;
    assert_eq!(put.status(), StatusCode::CREATED);

    // The health view lists the circuit, closed, before anything goes wrong.
    let before = platform_health(&state, platform_user).await;
    let circuit = before["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "ai:openai circuit")
        .expect("the openai circuit is listed from startup");
    assert_eq!(circuit["verdict"], "healthy");

    // Two customers hit the outage. Each turn is retried (3 calls), and the fifth failed call in a
    // row opens the circuit; the sixth try of the second turn is skipped.
    let first = customer_asks_for_help(tenant_id, "First").await;
    run_until_generation_recorded(&state, &pool, first).await;
    let second = customer_asks_for_help(tenant_id, "Second").await;
    run_until_generation_recorded(&state, &pool, second).await;
    assert_eq!(
        openai_calls(&openai_mock).await,
        5,
        "the circuit opened on the fifth failure"
    );
    let snap = state
        .ai
        .resilience()
        .snapshots()
        .into_iter()
        .find(|s| s.name == "ai:openai")
        .unwrap();
    assert_eq!(snap.state, resilience::State::Open);
    assert_eq!(snap.trips_total, 1);

    // A third customer: no call reaches the provider, and the customer still gets the honest
    // message and a human is brought in. Nothing fails silently.
    let third = customer_asks_for_help(tenant_id, "Third").await;
    run_until_generation_recorded(&state, &pool, third).await;
    assert_eq!(
        openai_calls(&openai_mock).await,
        5,
        "the open circuit sent nothing to the provider"
    );

    let (outcome, category) = generation(third).await;
    assert_eq!(outcome, "fallback");
    assert_eq!(category.as_deref(), Some("unavailable"));

    let system_messages: Vec<String> = sqlx::query_scalar(
        "SELECT body FROM messages WHERE tenant_id = $1 AND conversation_id = $2 AND kind = 'system'",
    )
    .bind(tenant_id)
    .bind(third)
    .fetch_all(&db::testing::seed_pool())
    .await
    .unwrap();
    assert_eq!(system_messages.len(), 1);
    assert!(
        system_messages[0].contains("team member will follow up"),
        "{}",
        system_messages[0]
    );
    assert!(
        escalations::routing::has_open_escalation(&pool, tenant_id, third)
            .await
            .unwrap(),
        "a human is brought in"
    );
    let ai_replies: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE conversation_id = $1 AND kind = 'ai'",
    )
    .bind(third)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();
    assert_eq!(ai_replies, 0);

    // The platform health view shows the open circuit, and the metrics carry the state and trips.
    let after = platform_health(&state, platform_user).await;
    let circuit = after["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "ai:openai circuit")
        .unwrap();
    assert_eq!(circuit["verdict"], "unhealthy");
    assert!(circuit["reason"].as_str().unwrap().contains("circuit open"));
    assert_eq!(after["overall"], "unhealthy");

    let metrics = observability::metrics::prometheus_handle().render();
    assert!(
        metrics.contains("circuit_breaker_state{breaker=\"ai:openai\"} 2"),
        "state gauge missing:\n{metrics}"
    );
    assert!(
        metrics.contains("circuit_breaker_trips_total{breaker=\"ai:openai\"} 1"),
        "trips counter missing:\n{metrics}"
    );
    assert!(
        metrics.contains(
            "circuit_breaker_rejections_total{breaker=\"ai:openai\",reason=\"circuit_open\"}"
        ),
        "rejections counter missing:\n{metrics}"
    );
}
