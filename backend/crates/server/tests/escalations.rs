use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode};
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
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
    }
}

fn app_state(pool: sqlx::PgPool) -> AppState {
    let escalations = escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(1));
    AppState {
        config: Arc::new(test_config()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations,
        ai: ai::AiService::from_config(pool.clone(), &test_config()).unwrap(),
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
            eprintln!("skipping escalations live tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping escalations live tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query(
        "TRUNCATE TABLE escalations, agent_availability, agent_skills, skills, \
         messages, customer_channel_identifiers, customers, conversations, \
         outbox_events, audit_logs, tenant_invitations, tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("failed to reset test tables");
}

async fn send(state: &AppState, request: Request<Body>) -> Response {
    router::app_with_test_routes(state.clone())
        .oneshot(request)
        .await
        .expect("request should complete")
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

async fn body_json(response: Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[allow(dead_code)]
async fn assert_error_has_request_id(headers: &HeaderMap, body: &serde_json::Value) {
    let request_id_header = headers
        .get("X-Request-Id")
        .and_then(|v| v.to_str().ok())
        .expect("X-Request-Id header must be present");
    let request_id_body = body["error"]["request_id"]
        .as_str()
        .expect("error body must contain request_id");
    assert!(!request_id_header.is_empty());
    assert!(!request_id_body.is_empty());
    assert_eq!(request_id_header, request_id_body);
}

async fn send_get(state: &AppState, user_id: Uuid, tenant_id: Uuid, uri: &str) -> Response {
    send(state, authenticated_request(uri, user_id, tenant_id)).await
}

fn json_post(uri: &str, user_id: Uuid, tenant_id: Uuid, body: serde_json::Value) -> Request<Body> {
    let bytes = serde_json::to_vec(&body).unwrap();
    Request::builder()
        .uri(uri)
        .method(Method::POST)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("content-type", "application/json")
        .body(Body::from(bytes))
        .unwrap()
}

fn json_put(uri: &str, user_id: Uuid, tenant_id: Uuid, body: serde_json::Value) -> Request<Body> {
    let bytes = serde_json::to_vec(&body).unwrap();
    Request::builder()
        .uri(uri)
        .method(Method::PUT)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("content-type", "application/json")
        .body(Body::from(bytes))
        .unwrap()
}

fn json_patch(uri: &str, user_id: Uuid, tenant_id: Uuid, body: serde_json::Value) -> Request<Body> {
    let bytes = serde_json::to_vec(&body).unwrap();
    Request::builder()
        .uri(uri)
        .method(Method::PATCH)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("content-type", "application/json")
        .body(Body::from(bytes))
        .unwrap()
}

fn json_delete(uri: &str, user_id: Uuid, tenant_id: Uuid) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(Method::DELETE)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .body(Body::empty())
        .unwrap()
}

async fn send_post(
    state: &AppState,
    user_id: Uuid,
    tenant_id: Uuid,
    uri: &str,
    body: &serde_json::Value,
) -> Response {
    send(state, json_post(uri, user_id, tenant_id, body.clone())).await
}

async fn send_put(
    state: &AppState,
    user_id: Uuid,
    tenant_id: Uuid,
    uri: &str,
    body: &serde_json::Value,
) -> Response {
    send(state, json_put(uri, user_id, tenant_id, body.clone())).await
}

async fn send_delete(state: &AppState, user_id: Uuid, tenant_id: Uuid, uri: &str) -> Response {
    send(state, json_delete(uri, user_id, tenant_id)).await
}

// ---------------------------------------------------------------------------
// Presence helper
// ---------------------------------------------------------------------------

struct PresenceGuard {
    cancel: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Drop for PresenceGuard {
    fn drop(&mut self) {
        if let Some(tx) = self.cancel.take() {
            let _ = tx.send(());
        }
    }
}

async fn connect_presence(state: &AppState, user_id: Uuid, tenant_id: Uuid) -> PresenceGuard {
    let request = authenticated_request("/api/v1/tenant/events", user_id, tenant_id);
    let response = send(state, request).await;
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        let _body = response.into_body();
        let _ = rx.await;
    });
    PresenceGuard { cancel: Some(tx) }
}

// ---------------------------------------------------------------------------
// Seed helpers
// ---------------------------------------------------------------------------

struct SeededMembership {
    user_id: Uuid,
    membership_id: Uuid,
}

async fn seed_tenant(pool: &sqlx::PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(name)
        .bind(format!("esc-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Escalations Test User")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_admin(pool: &sqlx::PgPool, tenant_id: Uuid, email: &str) -> SeededMembership {
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
    SeededMembership {
        user_id,
        membership_id,
    }
}

async fn seed_member(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    email: &str,
    role: &str,
) -> SeededMembership {
    let user_id = seed_user(pool, email).await;
    let membership_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, $3, 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .fetch_one(pool)
    .await
    .unwrap();
    SeededMembership {
        user_id,
        membership_id,
    }
}

async fn seed_customer(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    name: &str,
    email: Option<&str>,
    phone: Option<&str>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name, email, phone) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(tenant_id)
    .bind(name)
    .bind(email)
    .bind(phone)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_conversation(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    customer_id: Uuid,
    channel: &str,
    status: &str,
    assigned_membership_id: Option<Uuid>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, \
         assigned_membership_id, last_activity_at) \
         VALUES ($1, $2, $3, $4, $5, now()) RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(channel)
    .bind(status)
    .bind(assigned_membership_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_skill(pool: &sqlx::PgPool, tenant_id: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO skills (tenant_id, name) VALUES ($1, $2) RETURNING id")
        .bind(tenant_id)
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_agent_skill(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    membership_id: Uuid,
    skill_id: Uuid,
) {
    sqlx::query(
        "INSERT INTO agent_skills (tenant_id, membership_id, skill_id) \
         VALUES ($1, $2, $3)",
    )
    .bind(tenant_id)
    .bind(membership_id)
    .bind(skill_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_availability(pool: &sqlx::PgPool, tenant_id: Uuid, membership_id: Uuid, state: &str) {
    sqlx::query(
        "INSERT INTO agent_availability (tenant_id, membership_id, state) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (tenant_id, membership_id) \
         DO UPDATE SET state = $3, state_changed_at = now()",
    )
    .bind(tenant_id)
    .bind(membership_id)
    .bind(state)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_inactive_member(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    email: &str,
    role: &str,
) -> SeededMembership {
    let user_id = seed_user(pool, email).await;
    let membership_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, $3, 'deactivated') RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .fetch_one(pool)
    .await
    .unwrap();
    SeededMembership {
        user_id,
        membership_id,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn encode_query(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                format!("{}", byte as char).into_bytes()
            }
            _ => format!("%{byte:02X}").into_bytes(),
        })
        .map(char::from)
        .collect()
}

#[allow(dead_code)]
fn encode_cursor(cursor: &str) -> String {
    encode_query(cursor)
}

fn escalation_ids(body: &serde_json::Value) -> Vec<Uuid> {
    body["data"]
        .as_array()
        .expect("queue data array")
        .iter()
        .map(|item| {
            Uuid::parse_str(item["escalation"]["id"].as_str().expect("escalation id")).unwrap()
        })
        .collect()
}

async fn get_queue_list(state: &AppState, user_id: Uuid, tenant_id: Uuid, query: &str) -> Response {
    send_get(
        state,
        user_id,
        tenant_id,
        &format!("/api/v1/tenant/escalations/queue?{query}"),
    )
    .await
}

fn make_epoch_cursor() -> String {
    let epoch: chrono::DateTime<chrono::Utc> = chrono::DateTime::UNIX_EPOCH;
    escalations::queries::encode_queue_cursor(&epoch, &Uuid::nil())
}

async fn collect_queue_pages(state: &AppState, user_id: Uuid, tenant_id: Uuid) -> Vec<Uuid> {
    let mut cursor: Option<String> = Some(make_epoch_cursor());
    let mut ids = Vec::new();
    loop {
        let mut params = vec!["limit=2".to_string()];
        if let Some(c) = cursor.take() {
            params.push(format!("cursor={}", c));
        }
        let response = get_queue_list(state, user_id, tenant_id, &params.join("&")).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        ids.extend(escalation_ids(&body));
        if !body["pagination"]["hasMore"].as_bool().unwrap() {
            break;
        }
        cursor = Some(
            body["pagination"]["nextCursor"]
                .as_str()
                .expect("next cursor when hasMore")
                .to_owned(),
        );
    }
    ids
}

async fn get_conversation(
    state: &AppState,
    user_id: Uuid,
    tenant_id: Uuid,
    id: Uuid,
) -> serde_json::Value {
    let resp = send_get(
        state,
        user_id,
        tenant_id,
        &format!("/api/v1/tenant/conversations/{id}"),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    body_json(resp).await
}

async fn process_outbox(state: &AppState) {
    let pool = &state.db;
    let runtime = &state.escalations;
    for _ in 0..20 {
        if !escalations::events::process_escalation_outbox_once(pool, runtime)
            .await
            .expect("outbox processing failed")
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn fetch_audit_count(pool: &sqlx::PgPool, action: &str, resource_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE action = $1 AND resource_id = $2")
        .bind(action)
        .bind(resource_id.to_string())
        .fetch_one(pool)
        .await
        .unwrap()
}

// ===========================================================================
// T016: Escalation Routing (US1)
// ===========================================================================

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn skill_match_routes_to_correct_agent() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "skill-match").await;
    let agent = seed_member(&pool, tenant, "agent@skill-match.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    let billing = seed_skill(&pool, tenant, "billing").await;
    seed_agent_skill(&pool, tenant, agent.membership_id, billing).await;
    seed_availability(&pool, tenant, agent.membership_id, "available").await;
    let _guard = connect_presence(&state, agent.user_id, tenant).await;

    let resp = send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "billing issue", "requiredSkillIds": [billing]}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    assert_eq!(body["status"], "assigned");
    assert_eq!(body["routing"]["reason"], "skill_match");
    assert_eq!(body["routing"]["matchedSkills"][0], "billing");
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn lower_load_wins() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "lower-load").await;
    let agent_a = seed_member(&pool, tenant, "agent-a@load.test", "agent").await;
    let agent_b = seed_member(&pool, tenant, "agent-b@load.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    // Agent A has one conversation (higher load)
    let _assigned_conv = seed_conversation(
        &pool,
        tenant,
        customer,
        "web_chat",
        "open",
        Some(agent_a.membership_id),
    )
    .await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    let billing = seed_skill(&pool, tenant, "billing").await;
    seed_agent_skill(&pool, tenant, agent_a.membership_id, billing).await;
    seed_agent_skill(&pool, tenant, agent_b.membership_id, billing).await;
    seed_availability(&pool, tenant, agent_a.membership_id, "available").await;
    seed_availability(&pool, tenant, agent_b.membership_id, "available").await;
    let _guard_a = connect_presence(&state, agent_a.user_id, tenant).await;
    let _guard_b = connect_presence(&state, agent_b.user_id, tenant).await;

    let resp = send_post(
        &state,
        agent_a.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "issue", "requiredSkillIds": [billing]}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    assert_eq!(body["status"], "assigned");
    assert_eq!(
        body["routing"]["assignedMembershipId"],
        agent_b.membership_id.to_string()
    );
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn two_skill_match_prefers_agent_with_both_skills() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "two-skill").await;
    let agent_a = seed_member(&pool, tenant, "agent-a@skill.test", "agent").await;
    let agent_b = seed_member(&pool, tenant, "agent-b@skill.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    let skill1 = seed_skill(&pool, tenant, "support").await;
    let skill2 = seed_skill(&pool, tenant, "billing").await;
    seed_agent_skill(&pool, tenant, agent_a.membership_id, skill1).await;
    seed_agent_skill(&pool, tenant, agent_b.membership_id, skill1).await;
    seed_agent_skill(&pool, tenant, agent_b.membership_id, skill2).await;
    seed_availability(&pool, tenant, agent_a.membership_id, "available").await;
    seed_availability(&pool, tenant, agent_b.membership_id, "available").await;
    let _guard_a = connect_presence(&state, agent_a.user_id, tenant).await;
    let _guard_b = connect_presence(&state, agent_b.user_id, tenant).await;

    let resp = send_post(
        &state,
        agent_a.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "complex", "requiredSkillIds": [skill1, skill2]}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    assert_eq!(body["status"], "assigned");
    assert_eq!(
        body["routing"]["assignedMembershipId"],
        agent_b.membership_id.to_string()
    );
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn no_matching_agent_fallback_to_load_fallback() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "no-match").await;
    let agent = seed_member(&pool, tenant, "agent@no-match.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    // Agent has no skills; escalation requires "billing"
    let billing = seed_skill(&pool, tenant, "billing").await;
    seed_availability(&pool, tenant, agent.membership_id, "available").await;
    let _guard = connect_presence(&state, agent.user_id, tenant).await;

    let resp = send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "issue", "requiredSkillIds": [billing]}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    assert_eq!(body["status"], "assigned");
    assert_eq!(body["routing"]["reason"], "load_fallback");
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn no_required_skills_fallback_to_load_fallback() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "no-req").await;
    let agent = seed_member(&pool, tenant, "agent@no-req.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    seed_availability(&pool, tenant, agent.membership_id, "available").await;
    let _guard = connect_presence(&state, agent.user_id, tenant).await;

    let resp = send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "general help"}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    assert_eq!(body["status"], "assigned");
    assert_eq!(body["routing"]["reason"], "load_fallback");
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn no_agent_present_or_available_queues_escalation() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "no-agent").await;
    let _agent = seed_member(&pool, tenant, "agent@no-agent.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;

    let resp = send_post(
        &state,
        // Use the admin as the escalating user
        seed_admin(&pool, tenant, "admin@no-agent.test")
            .await
            .user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "help"}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    assert_eq!(body["status"], "queued");
    assert!(body["routing"].is_null());
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn duplicate_escalation_returns_409() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "dup-esc").await;
    let agent = seed_member(&pool, tenant, "agent@dup.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    seed_availability(&pool, tenant, agent.membership_id, "available").await;
    let _guard = connect_presence(&state, agent.user_id, tenant).await;

    let uri = format!("/api/v1/tenant/conversations/{conv}/escalate");
    let payload = serde_json::json!({"reason": "help"});
    let r1 = send_post(&state, agent.user_id, tenant, &uri, &payload).await;
    assert_eq!(r1.status(), StatusCode::CREATED);

    let r2 = send_post(&state, agent.user_id, tenant, &uri, &payload).await;
    assert_eq!(r2.status(), StatusCode::CONFLICT);
    let body = body_json(r2).await;
    assert_eq!(body["error"]["code"], "conflict");
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn closed_conversation_escalation_returns_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "closed-esc").await;
    let admin = seed_admin(&pool, tenant, "admin@closed.test").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "closed", None).await;

    let resp = send_post(
        &state,
        admin.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "help"}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn unknown_tenant_skill_id_returns_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "unknown-skill").await;
    let admin = seed_admin(&pool, tenant, "admin@unknown-skill.test").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    let fake_id = Uuid::new_v4();

    let resp = send_post(
        &state,
        admin.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "help", "requiredSkillIds": [fake_id]}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn cross_tenant_conversation_returns_404() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant_a = seed_tenant(&pool, "tenant-a-404").await;
    let tenant_b = seed_tenant(&pool, "tenant-b-404").await;
    let admin_a = seed_admin(&pool, tenant_a, "admin@a.test").await;
    let customer_a = seed_customer(&pool, tenant_a, "Cust", None, None).await;
    let conv_a = seed_conversation(&pool, tenant_a, customer_a, "web_chat", "open", None).await;

    // admin_a tries to escalate conversation from tenant_a using tenant_b's tenant_id
    let resp = send_post(
        &state,
        admin_a.user_id,
        tenant_b,
        &format!("/api/v1/tenant/conversations/{conv_a}/escalate"),
        &serde_json::json!({"reason": "help"}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn audit_escalation_created_and_assigned() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "audit-esc").await;
    let agent = seed_member(&pool, tenant, "agent@audit.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    seed_availability(&pool, tenant, agent.membership_id, "available").await;
    let _guard = connect_presence(&state, agent.user_id, tenant).await;

    let resp = send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "audit test"}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    let escalation_id: Uuid = body["id"].as_str().unwrap().parse().unwrap();

    let created_count = fetch_audit_count(&pool, "escalation.created", escalation_id).await;
    let assigned_count = fetch_audit_count(&pool, "escalation.assigned", escalation_id).await;
    assert_eq!(created_count, 1, "escalation.created must be recorded");
    assert_eq!(assigned_count, 1, "escalation.assigned must be recorded");
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn tenant_isolation_routes_to_correct_tenant() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant_a = seed_tenant(&pool, "iso-a").await;
    let tenant_b = seed_tenant(&pool, "iso-b").await;
    let agent_a = seed_member(&pool, tenant_a, "agent@iso-a.test", "agent").await;
    let agent_b = seed_member(&pool, tenant_b, "agent@iso-b.test", "agent").await;
    let customer_a = seed_customer(&pool, tenant_a, "Cust A", None, None).await;
    let conv_a = seed_conversation(&pool, tenant_a, customer_a, "web_chat", "open", None).await;
    let skill_a = seed_skill(&pool, tenant_a, "support").await;
    let skill_b = seed_skill(&pool, tenant_b, "support").await;
    seed_agent_skill(&pool, tenant_a, agent_a.membership_id, skill_a).await;
    seed_agent_skill(&pool, tenant_b, agent_b.membership_id, skill_b).await;
    seed_availability(&pool, tenant_a, agent_a.membership_id, "available").await;
    seed_availability(&pool, tenant_b, agent_b.membership_id, "available").await;
    let _guard_a = connect_presence(&state, agent_a.user_id, tenant_a).await;
    let _guard_b = connect_presence(&state, agent_b.user_id, tenant_b).await;

    let resp = send_post(
        &state,
        agent_a.user_id,
        tenant_a,
        &format!("/api/v1/tenant/conversations/{conv_a}/escalate"),
        &serde_json::json!({"reason": "help", "requiredSkillIds": [skill_a]}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    assert_eq!(body["status"], "assigned");
    assert_eq!(
        body["routing"]["assignedMembershipId"],
        agent_a.membership_id.to_string()
    );
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn escalated_at_set_on_conversation() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "esc-at").await;
    let agent = seed_member(&pool, tenant, "agent@esc-at.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    seed_availability(&pool, tenant, agent.membership_id, "available").await;
    let _guard = connect_presence(&state, agent.user_id, tenant).await;

    let resp = send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "set flag"}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let escalated_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT escalated_at FROM conversations WHERE id = $1")
            .bind(conv)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(escalated_at.is_some(), "escalated_at must be set");
}

// ===========================================================================
// T017: Inbox Filter (FR-001a)
// ===========================================================================

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn escalated_conversations_filter() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "inbox-esc").await;
    let agent = seed_member(&pool, tenant, "agent@inbox.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv_esc = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    let conv_normal = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    seed_availability(&pool, tenant, agent.membership_id, "available").await;
    let _guard = connect_presence(&state, agent.user_id, tenant).await;

    // Escalate conv_esc
    let resp = send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv_esc}/escalate"),
        &serde_json::json!({"reason": "filter test"}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = send_get(
        &state,
        agent.user_id,
        tenant,
        "/api/v1/tenant/conversations?escalated=true",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let ids: Vec<Uuid> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().parse().unwrap())
        .collect();
    assert!(ids.contains(&conv_esc));
    assert!(!ids.contains(&conv_normal));
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn escalated_filter_combinable_with_status() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "inbox-combo").await;
    let agent = seed_member(&pool, tenant, "agent@combo.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv_open = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    let conv_pending =
        seed_conversation(&pool, tenant, customer, "web_chat", "pending", None).await;
    seed_availability(&pool, tenant, agent.membership_id, "available").await;
    let _guard = connect_presence(&state, agent.user_id, tenant).await;

    send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv_open}/escalate"),
        &serde_json::json!({"reason": "combo"}),
    )
    .await;
    send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv_pending}/escalate"),
        &serde_json::json!({"reason": "combo"}),
    )
    .await;

    let resp = send_get(
        &state,
        agent.user_id,
        tenant,
        "/api/v1/tenant/conversations?escalated=true&status=open",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let ids: Vec<Uuid> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().parse().unwrap())
        .collect();
    assert!(ids.contains(&conv_open));
    assert!(!ids.contains(&conv_pending));
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn no_escalated_conversations_returns_empty() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "inbox-empty").await;
    let agent = seed_member(&pool, tenant, "agent@empty.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;

    let resp = send_get(
        &state,
        agent.user_id,
        tenant,
        "/api/v1/tenant/conversations?escalated=true",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert!(body["data"].as_array().unwrap().is_empty());
}

// ===========================================================================
// T022: Queue + Claim (US2)
// ===========================================================================

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn queue_list_oldest_first_keyset_paginated() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "queue-pages").await;
    let agent = seed_member(&pool, tenant, "agent@queue.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    // No agent present/available → escalations go to queued
    let conv1 = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    let conv2 = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    let conv3 = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;

    send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv1}/escalate"),
        &serde_json::json!({"reason": "q1"}),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv2}/escalate"),
        &serde_json::json!({"reason": "q2"}),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv3}/escalate"),
        &serde_json::json!({"reason": "q3"}),
    )
    .await;

    let ids = collect_queue_pages(&state, agent.user_id, tenant).await;
    assert_eq!(ids.len(), 3);
    // Oldest first
    let detail1 = get_conversation(&state, agent.user_id, tenant, conv1).await;
    let detail2 = get_conversation(&state, agent.user_id, tenant, conv2).await;
    let detail3 = get_conversation(&state, agent.user_id, tenant, conv3).await;
    let t1 = detail1["escalation"]["escalatedAt"]
        .as_str()
        .unwrap()
        .to_string();
    let t2 = detail2["escalation"]["escalatedAt"]
        .as_str()
        .unwrap()
        .to_string();
    let t3 = detail3["escalation"]["escalatedAt"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        t1 < t2 && t2 < t3,
        "escalations must be ordered by escalated_at ASC"
    );
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn claim_assigns_and_audits() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "claim-test").await;
    let agent = seed_member(&pool, tenant, "agent@claim.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;

    // Create queued escalation (no available agent)
    let esc_resp = send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "claim me"}),
    )
    .await;
    assert_eq!(esc_resp.status(), StatusCode::CREATED);
    let esc_id: Uuid = body_json(esc_resp).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    // Claim it
    let resp = send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/escalations/{esc_id}/claim"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["status"], "assigned");
    assert_eq!(body["routing"]["reason"], "manual_claim");

    let audit_count = fetch_audit_count(&pool, "escalation.assigned", esc_id).await;
    assert_eq!(audit_count, 1);
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn concurrent_claims_one_succeeds_one_409() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "concurrent-claim").await;
    let agent_a = seed_member(&pool, tenant, "agent-a@conclaim.test", "agent").await;
    let agent_b = seed_member(&pool, tenant, "agent-b@conclaim.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;

    let esc_resp = send_post(
        &state,
        agent_a.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "concurrent"}),
    )
    .await;
    let esc_id: Uuid = body_json(esc_resp).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let state1 = state.clone();
    let state2 = state.clone();
    let jh1 = tokio::spawn(async move {
        send_post(
            &state1,
            agent_a.user_id,
            tenant,
            &format!("/api/v1/tenant/escalations/{esc_id}/claim"),
            &serde_json::json!({}),
        )
        .await
    });
    let jh2 = tokio::spawn(async move {
        send_post(
            &state2,
            agent_b.user_id,
            tenant,
            &format!("/api/v1/tenant/escalations/{esc_id}/claim"),
            &serde_json::json!({}),
        )
        .await
    });

    let (r1, r2) = tokio::join!(jh1, jh2);
    let statuses = [r1.unwrap().status(), r2.unwrap().status()];
    assert!(statuses.contains(&StatusCode::OK));
    assert!(statuses.contains(&StatusCode::CONFLICT));
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn away_agent_can_claim() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "away-claim").await;
    let agent = seed_member(&pool, tenant, "agent@away-claim.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;

    let esc_resp = send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "away claim"}),
    )
    .await;
    let esc_id: Uuid = body_json(esc_resp).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    // Agent is away by default, not seeded as available
    let resp = send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/escalations/{esc_id}/claim"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn skill_aware_drain_picks_correct_entry() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "skill-drain").await;
    let agent = seed_member(&pool, tenant, "agent@drain.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let arabic = seed_skill(&pool, tenant, "arabic").await;
    let billing = seed_skill(&pool, tenant, "billing").await;
    seed_agent_skill(&pool, tenant, agent.membership_id, billing).await;

    // Older queued escalation requires "arabic" (agent doesn't have it)
    let conv_old = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv_old}/escalate"),
        &serde_json::json!({"reason": "arabic help", "requiredSkillIds": [arabic]}),
    )
    .await;

    tokio::time::sleep(Duration::from_millis(10)).await;

    // Newer queued escalation requires "billing" (agent has it)
    let conv_new = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv_new}/escalate"),
        &serde_json::json!({"reason": "billing help", "requiredSkillIds": [billing]}),
    )
    .await;

    // Agent becomes available+present → drain should pick the newer billing entry
    seed_availability(&pool, tenant, agent.membership_id, "available").await;
    let _guard = connect_presence(&state, agent.user_id, tenant).await;
    // Trigger drain by toggling availability
    let put_resp = send_put(
        &state,
        agent.user_id,
        tenant,
        "/api/v1/tenant/availability/me",
        &serde_json::json!({"state": "available"}),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::OK);

    let detail = get_conversation(&state, agent.user_id, tenant, conv_new).await;
    assert_eq!(detail["escalation"]["status"], "assigned");
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn agent_matching_neither_gets_oldest_entry() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "no-match-drain").await;
    let agent = seed_member(&pool, tenant, "agent@nodrain.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let arabic = seed_skill(&pool, tenant, "arabic").await;

    let conv_old = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv_old}/escalate"),
        &serde_json::json!({"reason": "old"}),
    )
    .await;

    tokio::time::sleep(Duration::from_millis(10)).await;

    let conv_new = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv_new}/escalate"),
        &serde_json::json!({"reason": "new", "requiredSkillIds": [arabic]}),
    )
    .await;

    seed_availability(&pool, tenant, agent.membership_id, "available").await;
    let _guard = connect_presence(&state, agent.user_id, tenant).await;
    let put_resp = send_put(
        &state,
        agent.user_id,
        tenant,
        "/api/v1/tenant/availability/me",
        &serde_json::json!({"state": "available"}),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::OK);

    let detail = get_conversation(&state, agent.user_id, tenant, conv_old).await;
    assert_eq!(detail["escalation"]["status"], "assigned");
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn three_queued_one_agent_assigns_exactly_one() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "three-queue").await;
    let agent = seed_member(&pool, tenant, "agent@three.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;

    let convs = [
        seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await,
        seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await,
        seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await,
    ];
    for (i, conv) in convs.iter().enumerate() {
        send_post(
            &state,
            agent.user_id,
            tenant,
            &format!("/api/v1/tenant/conversations/{conv}/escalate"),
            &serde_json::json!({"reason": format!("q{i}")}),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    seed_availability(&pool, tenant, agent.membership_id, "available").await;
    let _guard = connect_presence(&state, agent.user_id, tenant).await;

    let remaining = collect_queue_pages(&state, agent.user_id, tenant).await;
    assert_eq!(
        remaining.len(),
        2,
        "one should be assigned, two remain queued"
    );
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn resolve_queued_removes_from_queue() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "resolve-queue").await;
    let agent = seed_member(&pool, tenant, "agent@resolve.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;

    send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "resolve me"}),
    )
    .await;

    // Resolve the conversation
    let patch_resp = send(
        &state,
        json_patch(
            &format!("/api/v1/tenant/conversations/{conv}"),
            agent.user_id,
            tenant,
            serde_json::json!({"status": "resolved"}),
        ),
    )
    .await;
    assert_eq!(patch_resp.status(), StatusCode::OK);

    // Process outbox to close the escalation
    process_outbox(&state).await;

    let remaining = collect_queue_pages(&state, agent.user_id, tenant).await;
    assert!(remaining.is_empty(), "queued escalation should be closed");

    let escalation_status: String = sqlx::query_scalar(
        "SELECT status FROM escalations \
         WHERE tenant_id = $1 AND conversation_id = $2 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant)
    .bind(conv)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(escalation_status, "closed");
}

// ===========================================================================
// T043: Availability (US3)
// ===========================================================================

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn get_me_availability_defaults_to_away() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "avail-default").await;
    let agent = seed_member(&pool, tenant, "agent@avail.test", "agent").await;

    let resp = send_get(
        &state,
        agent.user_id,
        tenant,
        "/api/v1/tenant/availability/me",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["state"], "away");
    assert!(body["stateChangedAt"].is_null());
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn put_available_get_reflects() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "avail-toggle").await;
    let agent = seed_member(&pool, tenant, "agent@toggle.test", "agent").await;

    let put_resp = send_put(
        &state,
        agent.user_id,
        tenant,
        "/api/v1/tenant/availability/me",
        &serde_json::json!({"state": "available"}),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::OK);
    let put_body = body_json(put_resp).await;
    assert_eq!(put_body["state"], "available");
    assert!(put_body["stateChangedAt"].is_string());

    let get_resp = send_get(
        &state,
        agent.user_id,
        tenant,
        "/api/v1/tenant/availability/me",
    )
    .await;
    let get_body = body_json(get_resp).await;
    assert_eq!(get_body["state"], "available");
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn toggle_available_drains_queue() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "drain-on").await;
    let agent = seed_member(&pool, tenant, "agent@drain-on.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;

    send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "drain on"}),
    )
    .await;

    seed_availability(&pool, tenant, agent.membership_id, "available").await;
    let _guard = connect_presence(&state, agent.user_id, tenant).await;

    let put_resp = send_put(
        &state,
        agent.user_id,
        tenant,
        "/api/v1/tenant/availability/me",
        &serde_json::json!({"state": "available"}),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::OK);

    let detail = get_conversation(&state, agent.user_id, tenant, conv).await;
    assert_eq!(detail["escalation"]["status"], "assigned");
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn toggle_away_does_not_unassign() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "away-no-unassign").await;
    let agent = seed_member(&pool, tenant, "agent@away-nu.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    seed_availability(&pool, tenant, agent.membership_id, "available").await;
    let _guard = connect_presence(&state, agent.user_id, tenant).await;

    // Escalate → assigned
    send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "stay assigned"}),
    )
    .await;

    // Toggle to away
    send_put(
        &state,
        agent.user_id,
        tenant,
        "/api/v1/tenant/availability/me",
        &serde_json::json!({"state": "away"}),
    )
    .await;

    let detail = get_conversation(&state, agent.user_id, tenant, conv).await;
    assert_eq!(detail["escalation"]["status"], "assigned");
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn away_agent_still_claims_after_toggle() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "away-claim2").await;
    let agent = seed_member(&pool, tenant, "agent@away-claim2.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;

    // Create queued escalation
    let esc_resp = send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "away claim2"}),
    )
    .await;
    let esc_id: Uuid = body_json(esc_resp).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    // Set to away explicitly
    send_put(
        &state,
        agent.user_id,
        tenant,
        "/api/v1/tenant/availability/me",
        &serde_json::json!({"state": "away"}),
    )
    .await;

    let resp = send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/escalations/{esc_id}/claim"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn presence_timeout_reverts_to_away() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "pres-timeout").await;
    let agent = seed_member(&pool, tenant, "agent@timeout.test", "agent").await;

    // Connect presence
    let guard = connect_presence(&state, agent.user_id, tenant).await;
    // Set available
    send_put(
        &state,
        agent.user_id,
        tenant,
        "/api/v1/tenant/availability/me",
        &serde_json::json!({"state": "available"}),
    )
    .await;

    // Drop presence guard → timer starts
    drop(guard);

    // Wait for grace period (1s) + buffer
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify DB state is away
    let db_state: Option<String> = sqlx::query_scalar(
        "SELECT state FROM agent_availability \
         WHERE tenant_id = $1 AND membership_id = $2",
    )
    .bind(tenant)
    .bind(agent.membership_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(db_state.as_deref(), Some("away"));

    // Verify audit log
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs \
         WHERE action = 'availability.changed' AND details->>'cause' = 'presence_timeout'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 1, "presence_timeout must be audited");
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn deactivated_member_treated_as_unavailable() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "deactivated").await;
    let deactivated = seed_inactive_member(&pool, tenant, "deactivated@test.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;

    seed_availability(&pool, tenant, deactivated.membership_id, "available").await;
    // Can't connect presence for deactivated (no SSE membership lookup), but that's fine
    // because the member is not even a candidate

    let resp = send_post(
        &state,
        seed_admin(&pool, tenant, "admin@deact.test").await.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "deactivated test"}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    assert_eq!(
        body["status"], "queued",
        "deactivated member not selectable"
    );
}

// ===========================================================================
// T054: Skills (US4)
// ===========================================================================

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn create_rename_delete_skill() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "crud-skill").await;
    let member = seed_member(&pool, tenant, "member@crud.test", "agent").await;

    // Create
    let create_resp = send_post(
        &state,
        member.user_id,
        tenant,
        "/api/v1/tenant/skills",
        &serde_json::json!({"name": "support"}),
    )
    .await;
    assert_eq!(create_resp.status(), StatusCode::OK);
    let skill_id: Uuid = body_json(create_resp).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    // List
    let list_resp = send_get(&state, member.user_id, tenant, "/api/v1/tenant/skills").await;
    assert_eq!(list_resp.status(), StatusCode::OK);
    let names: Vec<String> = body_json(list_resp).await["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"support".to_string()));

    // Rename
    let rename_resp = send(
        &state,
        json_patch(
            &format!("/api/v1/tenant/skills/{skill_id}"),
            member.user_id,
            tenant,
            serde_json::json!({"name": "premium-support"}),
        ),
    )
    .await;
    assert_eq!(rename_resp.status(), StatusCode::OK);

    // Delete
    let del_resp = send_delete(
        &state,
        member.user_id,
        tenant,
        &format!("/api/v1/tenant/skills/{skill_id}"),
    )
    .await;
    assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);

    let list_resp2 = send_get(&state, member.user_id, tenant, "/api/v1/tenant/skills").await;
    let names2: Vec<String> = body_json(list_resp2).await["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_string())
        .collect();
    assert!(!names2.contains(&"premium-support".to_string()));
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn case_insensitive_duplicate_skill_409() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "dup-skill").await;
    let member = seed_member(&pool, tenant, "member@dupskill.test", "agent").await;

    let r1 = send_post(
        &state,
        member.user_id,
        tenant,
        "/api/v1/tenant/skills",
        &serde_json::json!({"name": "Billing"}),
    )
    .await;
    assert_eq!(r1.status(), StatusCode::OK);

    let r2 = send_post(
        &state,
        member.user_id,
        tenant,
        "/api/v1/tenant/skills",
        &serde_json::json!({"name": "billing"}),
    )
    .await;
    assert_eq!(r2.status(), StatusCode::CONFLICT);
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn delete_skill_removes_agent_skills_and_queue_refs() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "del-skill-refs").await;
    let agent = seed_member(&pool, tenant, "agent@delref.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    let skill = seed_skill(&pool, tenant, "obsolete").await;
    seed_agent_skill(&pool, tenant, agent.membership_id, skill).await;

    // Create queued escalation referencing the skill
    send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "ref test", "requiredSkillIds": [skill]}),
    )
    .await;

    // Delete the skill
    let del_resp = send_delete(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/skills/{skill}"),
    )
    .await;
    assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);

    // agent_skills should be cleaned by CASCADE
    let agent_skill_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM agent_skills WHERE skill_id = $1")
            .bind(skill)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(agent_skill_count, 0);

    let escalation_exists: bool = sqlx::query_scalar(
        "SELECT COUNT(*) > 0 FROM escalations \
         WHERE tenant_id = $1 AND conversation_id = $2",
    )
    .bind(tenant)
    .bind(conv)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        escalation_exists,
        "escalation row must still exist after skill delete"
    );
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn put_member_skills_idempotent() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "put-skills").await;
    let agent = seed_member(&pool, tenant, "agent@putskills.test", "agent").await;
    let skill_a = seed_skill(&pool, tenant, "a").await;
    let skill_b = seed_skill(&pool, tenant, "b").await;

    let put_resp = send_put(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/members/{}/skills", agent.membership_id),
        &serde_json::json!({"skillIds": [skill_a, skill_b]}),
    )
    .await;
    assert_eq!(put_resp.status(), StatusCode::OK);

    // Idempotent second call
    let put_resp2 = send_put(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/members/{}/skills", agent.membership_id),
        &serde_json::json!({"skillIds": [skill_a, skill_b]}),
    )
    .await;
    assert_eq!(put_resp2.status(), StatusCode::OK);

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_skills WHERE tenant_id = $1 AND membership_id = $2",
    )
    .bind(tenant)
    .bind(agent.membership_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn unknown_skill_id_on_member_skills_returns_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "unknown-mem-skills").await;
    let agent = seed_member(&pool, tenant, "agent@unknown.test", "agent").await;
    let fake_id = Uuid::new_v4();

    let resp = send_put(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/members/{}/skills", agent.membership_id),
        &serde_json::json!({"skillIds": [fake_id]}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn without_members_manage_returns_403_for_skill_management() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "403-skills").await;
    let viewer = seed_member(&pool, tenant, "viewer@403.test", "viewer").await;
    let skill = seed_skill(&pool, tenant, "test").await;

    // Viewer tries to delete a skill
    let resp = send_delete(
        &state,
        viewer.user_id,
        tenant,
        &format!("/api/v1/tenant/skills/{skill}"),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // Viewer tries to set member skills
    let resp = send_put(
        &state,
        viewer.user_id,
        tenant,
        &format!("/api/v1/tenant/members/{}/skills", viewer.membership_id),
        &serde_json::json!({"skillIds": []}),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn tenant_isolation_skills() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant_a = seed_tenant(&pool, "iso-skills-a").await;
    let tenant_b = seed_tenant(&pool, "iso-skills-b").await;
    let member_a = seed_member(&pool, tenant_a, "member@iso-a.test", "agent").await;
    let _member_b = seed_member(&pool, tenant_b, "member@iso-b.test", "agent").await;
    let _skill_a = seed_skill(&pool, tenant_a, "unique-a").await;
    let _skill_b = seed_skill(&pool, tenant_b, "unique-b").await;

    let list_a =
        body_json(send_get(&state, member_a.user_id, tenant_a, "/api/v1/tenant/skills").await)
            .await;
    let names_a: Vec<String> = list_a["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_string())
        .collect();
    assert!(names_a.contains(&"unique-a".to_string()));
    assert!(!names_a.contains(&"unique-b".to_string()));
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn each_skill_mutation_writes_audit() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "audit-skills").await;
    let member = seed_member(&pool, tenant, "member@audit-skill.test", "agent").await;

    let audit_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs")
        .fetch_one(&pool)
        .await
        .unwrap();

    let create_resp = send_post(
        &state,
        member.user_id,
        tenant,
        "/api/v1/tenant/skills",
        &serde_json::json!({"name": "audited-skill"}),
    )
    .await;
    assert_eq!(create_resp.status(), StatusCode::OK);
    let skill_id: Uuid = body_json(create_resp).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    send(
        &state,
        json_patch(
            &format!("/api/v1/tenant/skills/{skill_id}"),
            member.user_id,
            tenant,
            serde_json::json!({"name": "renamed-skill"}),
        ),
    )
    .await;

    send_delete(
        &state,
        member.user_id,
        tenant,
        &format!("/api/v1/tenant/skills/{skill_id}"),
    )
    .await;

    let audit_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        audit_after >= audit_before + 3,
        "create+rename+delete should write 3+ audit rows"
    );
}

// ===========================================================================
// T067: Escalation Banner (US5)
// ===========================================================================

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn get_conversation_embeds_escalation() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "embed-esc").await;
    let agent = seed_member(&pool, tenant, "agent@embed.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    seed_availability(&pool, tenant, agent.membership_id, "available").await;
    let _guard = connect_presence(&state, agent.user_id, tenant).await;

    // Not escalated → null
    let detail = get_conversation(&state, agent.user_id, tenant, conv).await;
    assert!(
        detail["escalation"].is_null(),
        "never-escalated must be null"
    );

    // Escalate
    send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "embed test"}),
    )
    .await;

    // Now has escalation
    let detail2 = get_conversation(&state, agent.user_id, tenant, conv).await;
    assert!(
        detail2["escalation"].is_object(),
        "escalated must have escalation object"
    );
    assert_eq!(detail2["escalation"]["status"], "assigned");
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn patch_assignee_escalated_conversation_relabels() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "patch-relabel").await;
    let agent_a = seed_member(&pool, tenant, "agent-a@relabel.test", "agent").await;
    let agent_b = seed_member(&pool, tenant, "agent-b@relabel.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    seed_availability(&pool, tenant, agent_a.membership_id, "available").await;
    seed_availability(&pool, tenant, agent_b.membership_id, "available").await;
    let _guard_a = connect_presence(&state, agent_a.user_id, tenant).await;
    let _guard_b = connect_presence(&state, agent_b.user_id, tenant).await;

    // Escalate
    let esc_resp = send_post(
        &state,
        agent_a.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "relabel test"}),
    )
    .await;
    assert_eq!(esc_resp.status(), StatusCode::CREATED);
    let esc_body = body_json(esc_resp).await;
    let original_assignee = esc_body["routing"]["assignedMembershipId"]
        .as_str()
        .unwrap()
        .to_string();

    // PATCH assignee to the other agent
    let target = if original_assignee == agent_a.membership_id.to_string() {
        agent_b.membership_id
    } else {
        agent_a.membership_id
    };
    let patch_resp = send(
        &state,
        json_patch(
            &format!("/api/v1/tenant/conversations/{conv}"),
            agent_a.user_id,
            tenant,
            serde_json::json!({"assignedMembershipId": target}),
        ),
    )
    .await;
    assert_eq!(patch_resp.status(), StatusCode::OK);

    // Process outbox
    process_outbox(&state).await;

    // Check escalation relabeled
    let routing_reason: Option<String> = sqlx::query_scalar(
        "SELECT routing_reason FROM escalations \
         WHERE tenant_id = $1 AND conversation_id = $2 AND status = 'assigned' \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant)
    .bind(conv)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(routing_reason.as_deref(), Some("manual_reassignment"));
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn routing_engine_origin_does_not_relabel() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant = seed_tenant(&pool, "no-relabel").await;
    let agent = seed_member(&pool, tenant, "agent@norelabel.test", "agent").await;
    let customer = seed_customer(&pool, tenant, "Cust", None, None).await;
    let conv = seed_conversation(&pool, tenant, customer, "web_chat", "open", None).await;
    seed_availability(&pool, tenant, agent.membership_id, "available").await;
    let _guard = connect_presence(&state, agent.user_id, tenant).await;

    // Escalate (origin = "escalations")
    send_post(
        &state,
        agent.user_id,
        tenant,
        &format!("/api/v1/tenant/conversations/{conv}/escalate"),
        &serde_json::json!({"reason": "no relabel"}),
    )
    .await;

    // Process outbox
    process_outbox(&state).await;

    // Routing reason should still be skill_match (or load_fallback), NOT manual_reassignment
    let routing_reason: String = sqlx::query_scalar(
        "SELECT routing_reason FROM escalations \
         WHERE tenant_id = $1 AND conversation_id = $2 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant)
    .bind(conv)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_ne!(routing_reason, "manual_reassignment");
}

#[tokio::test]
#[serial_test::serial(escalations_db)]
async fn cross_tenant_conversation_detail_404() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let state = app_state(pool.clone());
    let tenant_a = seed_tenant(&pool, "detail-404-a").await;
    let tenant_b = seed_tenant(&pool, "detail-404-b").await;
    let admin_a = seed_admin(&pool, tenant_a, "admin@detail-a.test").await;
    let customer_a = seed_customer(&pool, tenant_a, "Cust", None, None).await;
    let conv_a = seed_conversation(&pool, tenant_a, customer_a, "web_chat", "open", None).await;

    // admin_a looks up conv_a but with tenant_b's tenant_id
    let resp = send_get(
        &state,
        admin_a.user_id,
        tenant_b,
        &format!("/api/v1/tenant/conversations/{conv_a}"),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
