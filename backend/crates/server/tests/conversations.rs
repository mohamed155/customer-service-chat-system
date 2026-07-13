use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::response::Response;
use chrono::{DateTime, Utc};
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
    }
}

fn app_state(pool: sqlx::PgPool) -> AppState {
    AppState {
        config: Arc::new(test_config()),
        db: pool,
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
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
            eprintln!("skipping conversations live tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping conversations live tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query(
        "TRUNCATE TABLE messages, customer_channel_identifiers, customers, conversations, \
         outbox_events, audit_logs, tenant_invitations, tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("failed to reset conversation test tables");
}

async fn send(pool: sqlx::PgPool, request: Request<Body>) -> Response {
    router::app_with_test_routes(app_state(pool))
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

async fn assert_error_has_request_id(headers: &HeaderMap, body: &serde_json::Value) {
    let request_id_header = headers
        .get("X-Request-Id")
        .and_then(|v| v.to_str().ok())
        .expect("X-Request-Id header must be present");
    let request_id_body = body["error"]["request_id"]
        .as_str()
        .expect("error body must contain request_id");
    assert!(
        !request_id_header.is_empty(),
        "X-Request-Id header must be non-empty"
    );
    assert!(
        !request_id_body.is_empty(),
        "body error.request_id must be non-empty"
    );
    assert_eq!(
        request_id_header, request_id_body,
        "body error.request_id must match X-Request-Id header"
    );
}

async fn send_get(pool: &sqlx::PgPool, user_id: Uuid, tenant_id: Uuid, uri: &str) -> Response {
    send(pool.clone(), authenticated_request(uri, user_id, tenant_id)).await
}

// ---------------------------------------------------------------------------
// Seed helpers
// ---------------------------------------------------------------------------

async fn seed_tenant(pool: &sqlx::PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(name)
        .bind(format!("conv-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Conversations Test User")
        .fetch_one(pool)
        .await
        .unwrap()
}

struct SeededMembership {
    user_id: Uuid,
    membership_id: Uuid,
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
    SeededMembership { user_id, membership_id }
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
    SeededMembership { user_id, membership_id }
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
    last_activity_at: DateTime<Utc>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, \
         assigned_membership_id, last_activity_at) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(channel)
    .bind(status)
    .bind(assigned_membership_id)
    .bind(last_activity_at)
    .fetch_one(pool)
    .await
    .unwrap()
}

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

fn encode_cursor(cursor: &str) -> String {
    encode_query(cursor)
}

fn item_ids(body: &serde_json::Value) -> Vec<Uuid> {
    body["data"]
        .as_array()
        .expect("list data array")
        .iter()
        .map(|item| Uuid::parse_str(item["id"].as_str().expect("conversation id")).unwrap())
        .collect()
}

fn conversation_statuses(body: &serde_json::Value) -> Vec<String> {
    body["data"]
        .as_array()
        .expect("list data array")
        .iter()
        .map(|item| {
            item["status"]
                .as_str()
                .expect("conversation status")
                .to_owned()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Inbox list helpers
// ---------------------------------------------------------------------------

async fn get_inbox_list(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    tenant_id: Uuid,
    query: &str,
) -> Response {
    send_get(
        pool,
        user_id,
        tenant_id,
        &format!("/api/v1/tenant/conversations?{query}"),
    )
    .await
}

async fn collect_pages(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    tenant_id: Uuid,
    extra_params: &[(&str, &str)],
) -> Vec<Uuid> {
    let mut cursor: Option<String> = None;
    let mut ids = Vec::new();
    loop {
        let mut params = vec!["limit=2".to_string()];
        for (key, value) in extra_params {
            params.push(format!("{}={}", encode_query(key), encode_query(value)));
        }
        if let Some(c) = cursor.take() {
            params.push(format!("cursor={}", encode_cursor(&c)));
        }
        let response = get_inbox_list(pool, user_id, tenant_id, &params.join("&")).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "inbox page should return 200"
        );
        let body = body_json(response).await;
        ids.extend(item_ids(&body));
        if !body["pagination"]["has_more"].as_bool().unwrap() {
            assert!(
                body["pagination"]["next_cursor"].is_null(),
                "next_cursor must be null when has_more is false"
            );
            break;
        }
        cursor = Some(
            body["pagination"]["next_cursor"]
                .as_str()
                .expect("next cursor when has_more")
                .to_owned(),
        );
    }
    ids
}

// ---------------------------------------------------------------------------
// User Story 1 — Inbox List
//
// Tests for the `GET /tenant/conversations` endpoint.  These are currently
// marked `#[ignore]` because the inbox list handler and its route
// registration (T013/T014) do not exist yet.  Once the route is registered,
// remove the `#[ignore]` annotations to verify the full contract.
//
// All tests are live-database-gated via `REQUIRE_DB_TESTS=1` (same pattern
// as the rest of the integration test suites) and tagged
// `serial(conversations_db)` so they share a single test binary and a single
// truncate-on-entry reset.
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations route (T013/T014)"]
async fn default_view_shows_only_open_conversations() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Default Open Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "default-open@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Default Open Customer", None, None).await;

    let base = Utc::now() - chrono::Duration::hours(10);
    seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "web_chat",
        "open",
        None,
        base + chrono::Duration::minutes(0),
    )
    .await;
    seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "whatsapp",
        "open",
        None,
        base + chrono::Duration::minutes(10),
    )
    .await;
    seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "email",
        "pending",
        None,
        base + chrono::Duration::minutes(20),
    )
    .await;
    seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "telegram",
        "resolved",
        None,
        base + chrono::Duration::minutes(30),
    )
    .await;
    seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "web_chat",
        "closed",
        None,
        base + chrono::Duration::minutes(40),
    )
    .await;

    let response = get_inbox_list(&pool, admin.user_id, tenant_id, "").await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "inbox list must return 200 (Q2 default view)"
    );
    let body = body_json(response).await;
    let statuses = conversation_statuses(&body);
    assert_eq!(
        statuses,
        vec!["open"; 2],
        "default inbox must show only open conversations, got {statuses:?}"
    );
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations route (T013/T014)"]
async fn status_all_returns_every_status() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Status All Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "status-all@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Status All Customer", None, None).await;

    let base = Utc::now() - chrono::Duration::hours(5);
    for (i, status) in ["open", "pending", "resolved", "closed"].iter().enumerate() {
        seed_conversation(
            &pool,
            tenant_id,
            customer_id,
            "web_chat",
            status,
            None,
            base + chrono::Duration::minutes(i as i64 * 10),
        )
        .await;
    }

    let response = get_inbox_list(&pool, admin.user_id, tenant_id, "status=all").await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "status=all must return 200"
    );
    let body = body_json(response).await;
    let statuses = conversation_statuses(&body);
    assert_eq!(statuses.len(), 4, "status=all must return all 4 statuses");
    for s in ["open", "pending", "resolved", "closed"] {
        assert!(
            statuses.contains(&s.to_owned()),
            "status=all must include {s}, got {statuses:?}"
        );
    }
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations route (T013/T014)"]
async fn status_filter_returns_only_matching_status() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Status Filter Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "status-filter@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Status Filter Customer", None, None).await;

    let base = Utc::now() - chrono::Duration::hours(3);
    seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "web_chat",
        "open",
        None,
        base + chrono::Duration::minutes(0),
    )
    .await;
    seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "email",
        "pending",
        None,
        base + chrono::Duration::minutes(10),
    )
    .await;

    let response = get_inbox_list(&pool, admin.user_id, tenant_id, "status=pending").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let statuses = conversation_statuses(&body);
    assert_eq!(statuses, vec!["pending"], "only pending conversations");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations route (T013/T014)"]
async fn assignee_me_filter_returns_only_assigned_to_current_user() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Assignee Me Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "assignee-me@example.com").await;
    let other = seed_member(&pool, tenant_id, "other-agent@example.com", "agent").await;
    let customer_id = seed_customer(&pool, tenant_id, "Assignee Me Customer", None, None).await;

    let base = Utc::now() - chrono::Duration::hours(2);
    let my_conv = seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "web_chat",
        "open",
        Some(admin.membership_id),
        base + chrono::Duration::minutes(10),
    )
    .await;
    seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "email",
        "open",
        Some(other.membership_id),
        base + chrono::Duration::minutes(0),
    )
    .await;

    let response = get_inbox_list(&pool, admin.user_id, tenant_id, "assignee=me").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let ids = item_ids(&body);
    assert_eq!(
        ids,
        vec![my_conv],
        "assignee=me must return only conversations assigned to the current user"
    );
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations route (T013/T014)"]
async fn assignee_unassigned_returns_only_unassigned_conversations() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Assignee Unassigned Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "assignee-unassigned@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Unassigned Customer", None, None).await;

    let base = Utc::now() - chrono::Duration::hours(2);
    let unassigned = seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "web_chat",
        "open",
        None,
        base + chrono::Duration::minutes(10),
    )
    .await;
    seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "email",
        "open",
        Some(admin.membership_id),
        base + chrono::Duration::minutes(0),
    )
    .await;

    let response = get_inbox_list(&pool, admin.user_id, tenant_id, "assignee=unassigned").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let ids = item_ids(&body);
    assert_eq!(
        ids,
        vec![unassigned],
        "assignee=unassigned must return only conversations with no assignee"
    );
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations route (T013/T014)"]
async fn assignee_uuid_filter_returns_conversations_for_that_membership() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Assignee UUID Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "assignee-uuid@example.com").await;
    let other = seed_member(&pool, tenant_id, "target-agent@example.com", "agent").await;
    let customer_id = seed_customer(&pool, tenant_id, "UUID Customer", None, None).await;

    let base = Utc::now() - chrono::Duration::hours(2);
    seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "web_chat",
        "open",
        Some(admin.membership_id),
        base + chrono::Duration::minutes(10),
    )
    .await;
    let target = seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "email",
        "open",
        Some(other.membership_id),
        base + chrono::Duration::minutes(0),
    )
    .await;

    let response = get_inbox_list(
        &pool,
        admin.user_id,
        tenant_id,
        &format!("assignee={}", other.membership_id),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let ids = item_ids(&body);
    assert_eq!(
        ids,
        vec![target],
        "assignee=<uuid> must return only conversations for that membership"
    );
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations route (T013/T014)"]
async fn channel_filter_returns_only_conversations_on_that_channel() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Channel Filter Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "channel-filter@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Channel Customer", None, None).await;

    let base = Utc::now() - chrono::Duration::hours(2);
    seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "web_chat",
        "open",
        None,
        base + chrono::Duration::minutes(10),
    )
    .await;
    let email_conv = seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "email",
        "open",
        None,
        base + chrono::Duration::minutes(0),
    )
    .await;

    let response = get_inbox_list(&pool, admin.user_id, tenant_id, "channel=email").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let ids = item_ids(&body);
    assert_eq!(
        ids,
        vec![email_conv],
        "channel=email must return only email conversations"
    );
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations route (T013/T014)"]
async fn combined_filters_narrow_results_correctly() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Combined Filter Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "combined-filter@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Combined Customer", None, None).await;

    let base = Utc::now() - chrono::Duration::hours(4);
    // open + web_chat + unassigned
    seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "web_chat",
        "open",
        None,
        base + chrono::Duration::minutes(30),
    )
    .await;
    // pending + email + assigned to admin
    let target = seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "email",
        "pending",
        Some(admin.membership_id),
        base + chrono::Duration::minutes(20),
    )
    .await;
    // pending + web_chat + unassigned
    seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "web_chat",
        "pending",
        None,
        base + chrono::Duration::minutes(10),
    )
    .await;
    // pending + email + unassigned
    seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "email",
        "pending",
        None,
        base + chrono::Duration::minutes(0),
    )
    .await;

    let q = format!(
        "status=pending&channel=email&assignee={}",
        admin.membership_id
    );
    let response = get_inbox_list(&pool, admin.user_id, tenant_id, &q).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let ids = item_ids(&body);
    assert_eq!(
        ids,
        vec![target],
        "combined filters must narrow to one result, got {ids:?}"
    );
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations route (T013/T014)"]
async fn unknown_status_value_returns_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Unknown Status Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "unknown-status@example.com").await;

    let response = get_inbox_list(&pool, admin.user_id, tenant_id, "status=invalid_status").await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "unknown status must yield 422"
    );
    assert_eq!(body["error"]["code"], "validation_failed");
    assert_error_has_request_id(&headers, &body).await;
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations route (T013/T014)"]
async fn unknown_channel_value_returns_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Unknown Channel Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "unknown-channel@example.com").await;

    let response = get_inbox_list(&pool, admin.user_id, tenant_id, "channel=signal").await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "unknown channel must yield 422"
    );
    assert_eq!(body["error"]["code"], "validation_failed");
    assert_error_has_request_id(&headers, &body).await;
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations route (T013/T014)"]
async fn unknown_assignee_value_returns_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Unknown Assignee Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "unknown-assignee@example.com").await;

    let response = get_inbox_list(&pool, admin.user_id, tenant_id, "assignee=unknown").await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "unknown assignee value must yield 422"
    );
    assert_eq!(body["error"]["code"], "validation_failed");
    assert_error_has_request_id(&headers, &body).await;
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations route (T013/T014)"]
async fn keyset_pagination_has_more_and_no_duplicates() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Pagination Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "pagination@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Pagination Customer", None, None).await;

    let base = Utc::now() - chrono::Duration::hours(5);
    let mut seeded = Vec::new();
    for i in 0..5 {
        let id = seed_conversation(
            &pool,
            tenant_id,
            customer_id,
            "web_chat",
            "open",
            None,
            base + chrono::Duration::minutes(i as i64 * 10),
        )
        .await;
        seeded.push(id);
    }
    // Newest last (largest last_activity_at) appears first.
    seeded.reverse();

    let all_ids = collect_pages(&pool, admin.user_id, tenant_id, &[]).await;
    assert_eq!(
        all_ids, seeded,
        "keyset pagination must return all items in order without duplicates"
    );
    let unique: std::collections::HashSet<Uuid> = all_ids.iter().copied().collect();
    assert_eq!(
        unique.len(),
        all_ids.len(),
        "no duplicate ids across pagination pages"
    );
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations route (T013/T014)"]
async fn empty_filter_match_returns_data_array_empty() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Empty Filter Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "empty-filter@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Empty Filter Customer", None, None).await;

    seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "web_chat",
        "open",
        None,
        Utc::now() - chrono::Duration::hours(1),
    )
    .await;

    let response = get_inbox_list(&pool, admin.user_id, tenant_id, "channel=email").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(
        body["data"],
        serde_json::json!([]),
        "no match must return empty data array"
    );
    assert!(
        !body["pagination"]["has_more"]
            .as_bool()
            .expect("has_more present"),
        "has_more must be false when no items match"
    );
    assert!(
        body["pagination"]["next_cursor"].is_null(),
        "next_cursor must be null when no items match"
    );
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations route (T013/T014)"]
async fn per_tenant_isolation_list_and_pagination() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_a = seed_tenant(&pool, "Isolation A").await;
    let tenant_b = seed_tenant(&pool, "Isolation B").await;
    let admin_a = seed_admin(&pool, tenant_a, "isolation-a@example.com").await;
    let admin_b = seed_admin(&pool, tenant_b, "isolation-b@example.com").await;
    let customer_a = seed_customer(&pool, tenant_a, "Customer A", None, None).await;
    let customer_b = seed_customer(&pool, tenant_b, "Customer B", None, None).await;

    let base = Utc::now() - chrono::Duration::hours(3);
    let mut a_ids = Vec::new();
    let mut b_ids = Vec::new();
    for i in 0..3 {
        a_ids.push(
            seed_conversation(
                &pool,
                tenant_a,
                customer_a,
                "web_chat",
                "open",
                None,
                base + chrono::Duration::minutes(i as i64 * 10),
            )
            .await,
        );
        b_ids.push(
            seed_conversation(
                &pool,
                tenant_b,
                customer_b,
                "email",
                "pending",
                None,
                base + chrono::Duration::minutes(i as i64 * 10),
            )
            .await,
        );
    }

    // Tenant A sees only A's conversations in both the list and pagination.
    let a_observed: std::collections::HashSet<Uuid> =
        collect_pages(&pool, admin_a.user_id, tenant_a, &[])
            .await
            .into_iter()
            .collect();
    assert_eq!(
        a_observed,
        a_ids.iter().copied().collect(),
        "tenant A must see all its conversations"
    );
    assert!(
        b_ids.iter().all(|id| !a_observed.contains(id)),
        "tenant A must never see tenant B's conversations"
    );

    // Tenant B sees only B's conversations.
    let b_observed: std::collections::HashSet<Uuid> =
        collect_pages(&pool, admin_b.user_id, tenant_b, &[])
            .await
            .into_iter()
            .collect();
    assert_eq!(
        b_observed,
        b_ids.iter().copied().collect(),
        "tenant B must see all its conversations"
    );
    assert!(
        a_ids.iter().all(|id| !b_observed.contains(id)),
        "tenant B must never see tenant A's conversations"
    );
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations route (T013/T014)"]
async fn per_tenant_isolation_status_count_respects_boundaries() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_a = seed_tenant(&pool, "Isolation Count A").await;
    let tenant_b = seed_tenant(&pool, "Isolation Count B").await;
    let admin_a = seed_admin(&pool, tenant_a, "count-isolation-a@example.com").await;
    let admin_b = seed_admin(&pool, tenant_b, "count-isolation-b@example.com").await;
    let customer_a = seed_customer(&pool, tenant_a, "Count A", None, None).await;

    // Only tenant A has conversations.
    seed_conversation(
        &pool,
        tenant_a,
        customer_a,
        "web_chat",
        "open",
        None,
        Utc::now() - chrono::Duration::hours(1),
    )
    .await;

    // Tenant A sees the conversation.
    let a_body = body_json(
        get_inbox_list(&pool, admin_a.user_id, tenant_a, "").await,
    )
    .await;
    assert_eq!(
        a_body["data"].as_array().unwrap().len(),
        1,
        "tenant A must see its conversation"
    );

    // Tenant B sees nothing.
    let b_body = body_json(
        get_inbox_list(&pool, admin_b.user_id, tenant_b, "status=all").await,
    )
    .await;
    assert_eq!(
        b_body["data"].as_array().unwrap().len(),
        0,
        "tenant B must see zero conversations"
    );
}

// ---------------------------------------------------------------------------
// Inbox response shape validation
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations route (T013/T014)"]
async fn inbox_item_shape_includes_expected_fields() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Shape Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "shape@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Shape Customer", Some("shape@test.com"), None)
        .await;

    seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "web_chat",
        "open",
        Some(admin.membership_id),
        Utc::now() - chrono::Duration::hours(1),
    )
    .await;

    let body = body_json(get_inbox_list(&pool, admin.user_id, tenant_id, "").await).await;
    let item = &body["data"][0];
    assert!(item["id"].is_string(), "id must be a string");
    assert!(item["customer"]["id"].is_string(), "customer.id must be a string");
    assert!(
        item["customer"]["display_name"].is_string(),
        "customer.display_name must be a string"
    );
    assert!(item["channel"].is_string(), "channel must be a string");
    assert!(item["status"].is_string(), "status must be a string");
    assert!(item["assignee"].is_object(), "assignee must be an object");
    assert!(
        item["assignee"]["membership_id"].is_string(),
        "assignee.membership_id must be a string"
    );
    assert!(
        item["assignee"]["display_name"].is_string(),
        "assignee.display_name must be a string"
    );
    assert!(
        item["assignee"]["active"].is_boolean(),
        "assignee.active must be a boolean"
    );
    assert!(
        item["last_activity_at"].is_string(),
        "last_activity_at must be a string"
    );
    assert!(
        item["created_at"].is_string(),
        "created_at must be a string"
    );
    // last_message is optional — may be null when no messages exist.
    assert!(
        item["last_message"].is_null() || item["last_message"].is_object(),
        "last_message must be null or an object"
    );
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations route (T013/T014)"]
async fn inbox_item_assignee_is_null_when_unassigned() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Null Assignee Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "null-assignee@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Null Assignee Customer", None, None).await;

    seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "web_chat",
        "open",
        None,
        Utc::now() - chrono::Duration::hours(1),
    )
    .await;

    let body = body_json(get_inbox_list(&pool, admin.user_id, tenant_id, "").await).await;
    assert!(
        body["data"][0]["assignee"].is_null(),
        "assignee must be null when conversation is unassigned"
    );
}

// ---------------------------------------------------------------------------
// Extended seed / request helpers (US2–US5)
// ---------------------------------------------------------------------------

async fn seed_message(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    kind: &str,
    sender_membership_id: Option<Uuid>,
    logged_by_membership_id: Option<Uuid>,
    body: &str,
    created_at: chrono::DateTime<chrono::Utc>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, \
         sender_membership_id, logged_by_membership_id, body, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(kind)
    .bind(sender_membership_id)
    .bind(logged_by_membership_id)
    .bind(body)
    .bind(created_at)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_viewer(pool: &sqlx::PgPool, tenant_id: Uuid, email: &str) -> Uuid {
    let user_id = seed_user(pool, email).await;
    sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, 'viewer', 'active')",
    )
    .bind(tenant_id)
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
    user_id
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
         VALUES ($1, $2, $3, 'disabled') RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .fetch_one(pool)
    .await
    .unwrap();
    SeededMembership { user_id, membership_id }
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

async fn fetch_audit_count(pool: &sqlx::PgPool, action: &str, resource_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs \
         WHERE action = $1 AND resource_id = $2",
    )
    .bind(action)
    .bind(resource_id.to_string())
    .fetch_one(pool)
    .await
    .unwrap()
}

fn message_ids(body: &serde_json::Value) -> Vec<Uuid> {
    body["data"]
        .as_array()
        .expect("timeline data array")
        .iter()
        .map(|item| Uuid::parse_str(item["id"].as_str().expect("message id")).unwrap())
        .collect()
}

// ---------------------------------------------------------------------------
// User Story 2 — Read a Conversation Timeline
//
// Tests for GET /tenant/conversations/{id} (detail) and
// GET /tenant/conversations/{id}/messages (timeline keyset pagination).
// Marked #[ignore] until the detail/timeline handlers (T026–T029) are
// registered.
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations/{id} (T028/T029)"]
async fn detail_returns_conversation_and_participants() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Detail Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "detail@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Detail Customer", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open",
        Some(admin.membership_id), Utc::now() - chrono::Duration::hours(1),
    ).await;
    seed_message(&pool, tenant_id, conv_id, "reply",
        Some(admin.membership_id), None, "Hello",
        Utc::now() - chrono::Duration::minutes(30),
    ).await;

    let response = send_get(&pool, admin.user_id, tenant_id,
        &format!("/api/v1/tenant/conversations/{conv_id}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let data = &body["data"];

    assert_eq!(data["id"].as_str().unwrap(), conv_id.to_string());
    assert_eq!(data["customer"]["id"].as_str().unwrap(), customer_id.to_string());
    assert_eq!(data["customer"]["display_name"].as_str().unwrap(), "Detail Customer");
    assert_eq!(data["channel"].as_str().unwrap(), "web_chat");
    assert_eq!(data["status"].as_str().unwrap(), "open");
    assert_eq!(data["assignee"]["membership_id"].as_str().unwrap(), admin.membership_id.to_string());

    let participants = data["participants"].as_array().expect("participants array");
    assert!(!participants.is_empty(), "participants must have at least the customer");
    let customer_participant = participants.iter().find(|p| {
        p["id"].as_str() == Some(&customer_id.to_string())
    });
    assert!(customer_participant.is_some(), "participants must include the customer");
    assert!(data["created_at"].is_string());
    assert!(data["last_activity_at"].is_string());
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations/{id}/messages (T028/T029)"]
async fn timeline_returns_messages_in_desc_order_paginated() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Timeline Order Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "timeline-order@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Timeline Customer", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "email", "open", None,
        Utc::now() - chrono::Duration::hours(2),
    ).await;

    let base = Utc::now() - chrono::Duration::hours(1);
    let mut msg_ids = Vec::new();
    for i in 0..5 {
        let id = seed_message(&pool, tenant_id, conv_id, "reply",
            Some(admin.membership_id), None, &format!("Message {i}"),
            base + chrono::Duration::minutes(i as i64 * 10),
        ).await;
        msg_ids.push(id);
    }
    msg_ids.reverse(); // newest first

    // Collect all pages with limit=2
    let mut cursor: Option<String> = None;
    let mut collected = Vec::new();
    loop {
        let mut uri = format!("/api/v1/tenant/conversations/{conv_id}/messages?limit=2");
        if let Some(ref c) = cursor {
            uri.push_str(&format!("&cursor={}", encode_cursor(c)));
        }
        let response = send_get(&pool, admin.user_id, tenant_id, &uri).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        collected.extend(message_ids(&body));
        if !body["pagination"]["has_more"].as_bool().unwrap() {
            assert!(body["pagination"]["next_cursor"].is_null(),
                "next_cursor must be null when has_more is false");
            break;
        }
        cursor = Some(body["pagination"]["next_cursor"]
            .as_str().expect("next cursor").to_owned());
    }

    assert_eq!(collected, msg_ids,
        "timeline must return messages in created_at DESC order without duplicates");
    let unique: std::collections::HashSet<Uuid> = collected.iter().copied().collect();
    assert_eq!(unique.len(), collected.len(), "no duplicate messages across pages");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations/{id}/messages (T028/T029)"]
async fn timeline_tie_broken_by_seq() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Tiebreak Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "tiebreak@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Tiebreak Customer", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open", None,
        Utc::now() - chrono::Duration::hours(1),
    ).await;

    let same_time = Utc::now() - chrono::Duration::minutes(30);
    let mut msg_ids = Vec::new();
    for i in 0..4 {
        let id = seed_message(&pool, tenant_id, conv_id, "reply",
            Some(admin.membership_id), None, &format!("Same time msg {i}"),
            same_time,
        ).await;
        msg_ids.push(id);
    }
    // seq is identity — later inserts have higher seq, so newest seq first
    msg_ids.reverse();

    let response = send_get(&pool, admin.user_id, tenant_id,
        &format!("/api/v1/tenant/conversations/{conv_id}/messages?limit=10")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let ids = message_ids(&body);
    assert_eq!(ids, msg_ids,
        "same-created_at messages must be tie-broken by seq DESC");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations/{id}/messages (T028/T029)"]
async fn load_older_never_reorders_or_duplicates() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Load Older Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "load-older@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Load Older Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open", None,
        Utc::now() - chrono::Duration::hours(3),
    ).await;

    let base = Utc::now() - chrono::Duration::hours(2);
    for i in 0..8 {
        seed_message(&pool, tenant_id, conv_id, "reply",
            Some(admin.membership_id), None, &format!("Msg {i}"),
            base + chrono::Duration::minutes(i as i64 * 10),
        ).await;
    }

    // First page: newest 3
    let response = send_get(&pool, admin.user_id, tenant_id,
        &format!("/api/v1/tenant/conversations/{conv_id}/messages?limit=3")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let page1 = body_json(response).await;
    let page1_ids: std::collections::HashSet<Uuid> = message_ids(&page1).into_iter().collect();
    let cursor = page1["pagination"]["next_cursor"].as_str().map(|c| c.to_owned());

    // Second page: load older
    if let Some(c) = cursor {
        let response2 = send_get(&pool, admin.user_id, tenant_id,
            &format!("/api/v1/tenant/conversations/{conv_id}/messages?limit=3&cursor={}",
                encode_cursor(&c))).await;
        assert_eq!(response2.status(), StatusCode::OK);
        let page2 = body_json(response2).await;
        let page2_ids: std::collections::HashSet<Uuid> = message_ids(&page2).into_iter().collect();

        // No overlap
        assert!(page1_ids.is_disjoint(&page2_ids),
            "load-older must not return messages already seen");
    }
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations/{id}/messages (T028/T029)"]
async fn empty_timeline_shows_empty_list() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Empty Timeline Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "empty-timeline@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Empty Timeline Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open", None,
        Utc::now(),
    ).await;

    let response = send_get(&pool, admin.user_id, tenant_id,
        &format!("/api/v1/tenant/conversations/{conv_id}/messages")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["data"], serde_json::json!([]),
        "conversation with no messages must return empty data array");
    assert!(!body["pagination"]["has_more"].as_bool().unwrap());
    assert!(body["pagination"]["next_cursor"].is_null());
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations/{id} and /messages (T028/T029)"]
async fn cross_tenant_detail_timeline_404() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_a = seed_tenant(&pool, "Cross Detail A").await;
    let tenant_b = seed_tenant(&pool, "Cross Detail B").await;
    let admin_b = seed_admin(&pool, tenant_b, "cross-detail-b@example.com").await;
    let customer_a = seed_customer(&pool, tenant_a, "Cross Detail Cust A", None, None).await;
    let conv_a = seed_conversation(
        &pool, tenant_a, customer_a, "web_chat", "open", None,
        Utc::now(),
    ).await;

    // Detail — tenant B requests tenant A's conversation
    let detail_resp = send_get(&pool, admin_b.user_id, tenant_b,
        &format!("/api/v1/tenant/conversations/{conv_a}")).await;
    assert_eq!(detail_resp.status(), StatusCode::NOT_FOUND,
        "cross-tenant detail must return 404");
    let detail_headers = detail_resp.headers().clone();
    let detail_body = body_json(detail_resp).await;
    assert_eq!(detail_body["error"]["code"], "not_found");
    assert_error_has_request_id(&detail_headers, &detail_body).await;

    // Timeline — tenant B requests tenant A's conversation messages
    let tl_resp = send_get(&pool, admin_b.user_id, tenant_b,
        &format!("/api/v1/tenant/conversations/{conv_a}/messages")).await;
    assert_eq!(tl_resp.status(), StatusCode::NOT_FOUND,
        "cross-tenant timeline must return 404");
    let tl_headers = tl_resp.headers().clone();
    let tl_body = body_json(tl_resp).await;
    assert_eq!(tl_body["error"]["code"], "not_found");
    assert_error_has_request_id(&tl_headers, &tl_body).await;
}

// ---------------------------------------------------------------------------
// User Story 3 — Reply and Leave Internal Notes
//
// Tests for POST /tenant/conversations/{id}/messages.
// Marked #[ignore] until the add-message handler (T040/T041) is registered.
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on POST /tenant/conversations/{id}/messages (T040/T041)"]
async fn reply_message_appended_correctly() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Reply Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "reply@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Reply Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open",
        Some(admin.membership_id), Utc::now(),
    ).await;

    let payload = serde_json::json!({
        "kind": "reply",
        "body": "I am looking into this for you."
    });
    let response = send(pool.clone(), json_post(
        &format!("/api/v1/tenant/conversations/{conv_id}/messages"),
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::OK, "reply should succeed with 200");
    let body = body_json(response).await;

    let msg = &body["data"]["message"];
    assert_eq!(msg["kind"].as_str().unwrap(), "reply");
    assert_eq!(msg["body"].as_str().unwrap(), "I am looking into this for you.");
    assert_eq!(msg["sender"]["membership_id"].as_str().unwrap(), admin.membership_id.to_string());
    assert!(msg["sender"]["display_name"].is_string());
    assert!(msg["created_at"].is_string());
    assert!(msg["id"].is_string());
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on POST /tenant/conversations/{id}/messages (T040/T041)"]
async fn note_message_appended_correctly() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Note Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "note@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Note Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "email", "open", None, Utc::now(),
    ).await;

    let payload = serde_json::json!({
        "kind": "note",
        "body": "Internal: customer mentioned billing issue."
    });
    let response = send(pool.clone(), json_post(
        &format!("/api/v1/tenant/conversations/{conv_id}/messages"),
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let msg = &body["data"]["message"];
    assert_eq!(msg["kind"].as_str().unwrap(), "note");
    assert_eq!(msg["body"].as_str().unwrap(), "Internal: customer mentioned billing issue.");
    assert_eq!(msg["sender"]["membership_id"].as_str().unwrap(), admin.membership_id.to_string());
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on POST /tenant/conversations/{id}/messages (T040/T041)"]
async fn logged_customer_message_appended_correctly() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Logged Customer Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "logged-customer@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Logged Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "whatsapp", "open", None, Utc::now(),
    ).await;

    let payload = serde_json::json!({
        "kind": "customer",
        "body": "I need help with my order.",
        "logged_by_membership_id": admin.membership_id,
    });
    let response = send(pool.clone(), json_post(
        &format!("/api/v1/tenant/conversations/{conv_id}/messages"),
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let msg = &body["data"]["message"];
    assert_eq!(msg["kind"].as_str().unwrap(), "customer");
    assert_eq!(msg["body"].as_str().unwrap(), "I need help with my order.");
    assert!(msg["logged_by"].is_object(), "logged-by actor must be present for customer-kind messages");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on POST /tenant/conversations/{id}/messages (T040/T041)"]
async fn last_activity_at_bumps_on_message() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Bump Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "bump@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Bump Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open", None,
        Utc::now() - chrono::Duration::hours(2),
    ).await;

    let orig_activity: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT last_activity_at FROM conversations WHERE id = $1",
    ).bind(conv_id).fetch_one(&pool).await.unwrap();

    tokio::time::sleep(Duration::from_millis(10)).await;

    let payload = serde_json::json!({"kind": "reply", "body": "Bump!"});
    let response = send(pool.clone(), json_post(
        &format!("/api/v1/tenant/conversations/{conv_id}/messages"),
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::OK);

    let new_activity: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT last_activity_at FROM conversations WHERE id = $1",
    ).bind(conv_id).fetch_one(&pool).await.unwrap();
    assert!(new_activity > orig_activity,
        "last_activity_at must be bumped after a new message");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on POST /tenant/conversations/{id}/messages (T040/T041)"]
async fn empty_whitespace_body_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Empty Body Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "empty-body@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Empty Body Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open", None, Utc::now(),
    ).await;

    for (label, body_val) in [("empty string", ""), ("whitespace", "   ")] {
        let payload = serde_json::json!({"kind": "reply", "body": body_val});
        let response = send(pool.clone(), json_post(
            &format!("/api/v1/tenant/conversations/{conv_id}/messages"),
            admin.user_id, tenant_id, payload,
        )).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY,
            "{label} body should yield 422");
        let body = body_json(response).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(details.iter().any(|d| d["field"] == "body"),
            "{label} should report body field, got {details:?}");
    }
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on POST /tenant/conversations/{id}/messages (T040/T041)"]
async fn over_10000_char_body_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Long Body Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "long-body@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Long Body Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open", None, Utc::now(),
    ).await;

    let long_body = "x".repeat(10_001);
    let payload = serde_json::json!({"kind": "reply", "body": long_body});
    let response = send(pool.clone(), json_post(
        &format!("/api/v1/tenant/conversations/{conv_id}/messages"),
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY,
        "body > 10,000 chars should yield 422");
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "validation_failed");
    let details = body["error"]["details"].as_array().expect("details array");
    assert!(details.iter().any(|d| d["field"] == "body"),
        "should report body field, got {details:?}");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on POST /tenant/conversations/{id}/messages (T040/T041)"]
async fn reply_on_resolved_reopens_and_audits() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Reopen Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "reopen@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Reopen Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "resolved",
        Some(admin.membership_id), Utc::now(),
    ).await;

    let payload = serde_json::json!({"kind": "reply", "body": "Following up."});
    let response = send(pool.clone(), json_post(
        &format!("/api/v1/tenant/conversations/{conv_id}/messages"),
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;

    // Response includes the updated conversation status
    assert_eq!(body["data"]["conversation"]["status"].as_str().unwrap(), "open",
        "reply on resolved conversation must auto-reopen to open");

    // Verify the DB row was updated
    let db_status: String = sqlx::query_scalar(
        "SELECT status FROM conversations WHERE id = $1",
    ).bind(conv_id).fetch_one(&pool).await.unwrap();
    assert_eq!(db_status, "open", "DB row must reflect open status");

    // Audit row written
    let audit_count = fetch_audit_count(&pool, "conversation.status_changed", conv_id).await;
    assert_eq!(audit_count, 1, "status_changed audit must be written on auto-reopen");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on POST /tenant/conversations/{id}/messages (T040/T041)"]
async fn note_never_changes_status() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Note No Reopen Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "note-no-reopen@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Note No Reopen Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "closed", None, Utc::now(),
    ).await;

    let payload = serde_json::json!({"kind": "note", "body": "Internal note on closed."});
    let response = send(pool.clone(), json_post(
        &format!("/api/v1/tenant/conversations/{conv_id}/messages"),
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::OK);

    // Status must remain closed
    let db_status: String = sqlx::query_scalar(
        "SELECT status FROM conversations WHERE id = $1",
    ).bind(conv_id).fetch_one(&pool).await.unwrap();
    assert_eq!(db_status, "closed", "note must not change conversation status");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on POST /tenant/conversations/{id}/messages (T040/T041)"]
async fn viewer_403_for_message_post() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Viewer Msg Tenant").await;
    let viewer_id = seed_viewer(&pool, tenant_id, "viewer-msg@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Viewer Msg Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open", None, Utc::now(),
    ).await;

    let payload = serde_json::json!({"kind": "reply", "body": "Should be blocked."});
    let response = send(pool.clone(), json_post(
        &format!("/api/v1/tenant/conversations/{conv_id}/messages"),
        viewer_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN,
        "viewer must receive 403 on message post");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on POST /tenant/conversations/{id}/messages (T040/T041)"]
async fn cross_tenant_message_post_404() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_a = seed_tenant(&pool, "Cross Msg A").await;
    let tenant_b = seed_tenant(&pool, "Cross Msg B").await;
    let admin_b = seed_admin(&pool, tenant_b, "cross-msg-b@example.com").await;
    let customer_a = seed_customer(&pool, tenant_a, "Cross Msg Cust A", None, None).await;
    let conv_a = seed_conversation(
        &pool, tenant_a, customer_a, "web_chat", "open", None, Utc::now(),
    ).await;

    for kind in ["reply", "note", "customer"] {
        let payload = serde_json::json!({"kind": kind, "body": "Cross tenant test"});
        let response = send(pool.clone(), json_post(
            &format!("/api/v1/tenant/conversations/{conv_a}/messages"),
            admin_b.user_id, tenant_b, payload,
        )).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND,
            "cross-tenant {kind} post must return 404");
        let body = body_json(response).await;
        assert_eq!(body["error"]["code"], "not_found",
            "cross-tenant {kind} post error code");
    }
}

// ---------------------------------------------------------------------------
// User Story 4 — Manage Status and Assignment
//
// Tests for PATCH /tenant/conversations/{id}.
// Marked #[ignore] until the patch handler (T050/T051) is registered.
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on PATCH /tenant/conversations/{id} (T050/T051)"]
async fn patch_status_any_to_any() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Patch Status Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "patch-status@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Patch Status Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open", None, Utc::now(),
    ).await;

    for status in ["pending", "resolved", "closed", "open"] {
        let payload = serde_json::json!({"status": status});
        let response = send(pool.clone(), json_patch(
            &format!("/api/v1/tenant/conversations/{conv_id}"),
            admin.user_id, tenant_id, payload,
        )).await;
        assert_eq!(response.status(), StatusCode::OK,
            "PATCH status to {status} should succeed");
        let body = body_json(response).await;
        assert_eq!(body["data"]["status"].as_str().unwrap(), status,
            "response status must be {status}");

        let db_status: String = sqlx::query_scalar(
            "SELECT status FROM conversations WHERE id = $1",
        ).bind(conv_id).fetch_one(&pool).await.unwrap();
        assert_eq!(db_status, status, "DB status must be {status}");
    }
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on PATCH /tenant/conversations/{id} (T050/T051)"]
async fn patch_assign_to_active_member() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Assign Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "assign@example.com").await;
    let agent = seed_member(&pool, tenant_id, "assign-agent@example.com", "agent").await;
    let customer_id = seed_customer(&pool, tenant_id, "Assign Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open", None, Utc::now(),
    ).await;

    let payload = serde_json::json!({"assigned_membership_id": agent.membership_id});
    let response = send(pool.clone(), json_patch(
        &format!("/api/v1/tenant/conversations/{conv_id}"),
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(
        body["data"]["assignee"]["membership_id"].as_str().unwrap(),
        agent.membership_id.to_string(),
    );

    let db_assignee: Option<Uuid> = sqlx::query_scalar(
        "SELECT assigned_membership_id FROM conversations WHERE id = $1",
    ).bind(conv_id).fetch_one(&pool).await.unwrap();
    assert_eq!(db_assignee, Some(agent.membership_id));
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on PATCH /tenant/conversations/{id} (T050/T051)"]
async fn patch_unassign() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Unassign Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "unassign@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Unassign Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open",
        Some(admin.membership_id), Utc::now(),
    ).await;

    let payload = serde_json::json!({"assigned_membership_id": null});
    let response = send(pool.clone(), json_patch(
        &format!("/api/v1/tenant/conversations/{conv_id}"),
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert!(body["data"]["assignee"].is_null(),
        "assignee must be null after unassignment");

    let db_assignee: Option<Uuid> = sqlx::query_scalar(
        "SELECT assigned_membership_id FROM conversations WHERE id = $1",
    ).bind(conv_id).fetch_one(&pool).await.unwrap();
    assert_eq!(db_assignee, None, "DB assigned_membership_id must be null");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on PATCH /tenant/conversations/{id} (T050/T051)"]
async fn patch_inactive_membership_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Inactive Assign Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "inactive-assign@example.com").await;
    let inactive = seed_inactive_member(&pool, tenant_id, "inactive@example.com", "agent").await;
    let customer_id = seed_customer(&pool, tenant_id, "Inactive Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open", None, Utc::now(),
    ).await;

    let payload = serde_json::json!({"assigned_membership_id": inactive.membership_id});
    let response = send(pool.clone(), json_patch(
        &format!("/api/v1/tenant/conversations/{conv_id}"),
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "validation_failed");
    let details = body["error"]["details"].as_array().expect("details array");
    assert!(details.iter().any(|d| d["field"] == "assigned_membership_id"),
        "should report assigned_membership_id field, got {details:?}");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on PATCH /tenant/conversations/{id} (T050/T051)"]
async fn patch_cross_tenant_membership_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_a = seed_tenant(&pool, "Cross Assign A").await;
    let tenant_b = seed_tenant(&pool, "Cross Assign B").await;
    let admin_a = seed_admin(&pool, tenant_a, "cross-assign-a@example.com").await;
    let admin_b = seed_admin(&pool, tenant_b, "cross-assign-b@example.com").await;
    let customer_b = seed_customer(&pool, tenant_b, "Cross Assign Cust B", None, None).await;
    let conv_b = seed_conversation(
        &pool, tenant_b, customer_b, "web_chat", "open", None, Utc::now(),
    ).await;

    // Tenant B's conversation, but using tenant A's membership
    let payload = serde_json::json!({"assigned_membership_id": admin_a.membership_id});
    let response = send(pool.clone(), json_patch(
        &format!("/api/v1/tenant/conversations/{conv_b}"),
        admin_b.user_id, tenant_b, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "validation_failed");
    let details = body["error"]["details"].as_array().expect("details array");
    assert!(details.iter().any(|d| d["field"] == "assigned_membership_id"),
        "should report assigned_membership_id field");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on PATCH /tenant/conversations/{id} (T050/T051)"]
async fn status_changed_audit_row_written() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Audit Status Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "audit-status@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Audit Status Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open", None, Utc::now(),
    ).await;

    let payload = serde_json::json!({"status": "resolved"});
    let _response = send(pool.clone(), json_patch(
        &format!("/api/v1/tenant/conversations/{conv_id}"),
        admin.user_id, tenant_id, payload,
    )).await;

    let audit_count = fetch_audit_count(&pool, "conversation.status_changed", conv_id).await;
    assert_eq!(audit_count, 1, "status_changed audit must be written");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on PATCH /tenant/conversations/{id} (T050/T051)"]
async fn assignment_changed_audit_row_written() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Audit Assign Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "audit-assign@example.com").await;
    let agent = seed_member(&pool, tenant_id, "audit-assign-agent@example.com", "agent").await;
    let customer_id = seed_customer(&pool, tenant_id, "Audit Assign Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open", None, Utc::now(),
    ).await;

    let payload = serde_json::json!({"assigned_membership_id": agent.membership_id});
    let _response = send(pool.clone(), json_patch(
        &format!("/api/v1/tenant/conversations/{conv_id}"),
        admin.user_id, tenant_id, payload,
    )).await;

    let audit_count = fetch_audit_count(&pool, "conversation.assignment_changed", conv_id).await;
    assert_eq!(audit_count, 1, "assignment_changed audit must be written");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on PATCH /tenant/conversations/{id} (T050/T051)"]
async fn no_audit_on_noop_patch() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Noop Audit Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "noop-audit@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Noop Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open",
        Some(admin.membership_id), Utc::now(),
    ).await;

    // PATCH with same status
    let payload = serde_json::json!({"status": "open"});
    let _response = send(pool.clone(), json_patch(
        &format!("/api/v1/tenant/conversations/{conv_id}"),
        admin.user_id, tenant_id, payload,
    )).await;

    let status_audit = fetch_audit_count(&pool, "conversation.status_changed", conv_id).await;
    assert_eq!(status_audit, 0, "no status_changed audit on no-op");

    // PATCH with same assignee
    let payload = serde_json::json!({"assigned_membership_id": admin.membership_id});
    let _response = send(pool.clone(), json_patch(
        &format!("/api/v1/tenant/conversations/{conv_id}"),
        admin.user_id, tenant_id, payload,
    )).await;

    let assign_audit = fetch_audit_count(&pool, "conversation.assignment_changed", conv_id).await;
    assert_eq!(assign_audit, 0, "no assignment_changed audit on no-op");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on PATCH /tenant/conversations/{id} (T050/T051)"]
async fn missing_both_fields_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Missing Fields Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "missing-fields@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Missing Fields Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open", None, Utc::now(),
    ).await;

    let payload = serde_json::json!({});
    let response = send(pool.clone(), json_patch(
        &format!("/api/v1/tenant/conversations/{conv_id}"),
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY,
        "empty PATCH body must yield 422");
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "validation_failed");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on PATCH /tenant/conversations/{id} (T050/T051)"]
async fn viewer_403_for_patch() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Viewer Patch Tenant").await;
    let viewer_id = seed_viewer(&pool, tenant_id, "viewer-patch@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Viewer Patch Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open", None, Utc::now(),
    ).await;

    let payload = serde_json::json!({"status": "resolved"});
    let response = send(pool.clone(), json_patch(
        &format!("/api/v1/tenant/conversations/{conv_id}"),
        viewer_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN,
        "viewer must receive 403 on PATCH");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on PATCH /tenant/conversations/{id} (T050/T051)"]
async fn cross_tenant_patch_404() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_a = seed_tenant(&pool, "Cross Patch A").await;
    let tenant_b = seed_tenant(&pool, "Cross Patch B").await;
    let admin_b = seed_admin(&pool, tenant_b, "cross-patch-b@example.com").await;
    let customer_a = seed_customer(&pool, tenant_a, "Cross Patch Cust A", None, None).await;
    let conv_a = seed_conversation(
        &pool, tenant_a, customer_a, "web_chat", "open", None, Utc::now(),
    ).await;

    let payload = serde_json::json!({"status": "resolved"});
    let response = send(pool.clone(), json_patch(
        &format!("/api/v1/tenant/conversations/{conv_a}"),
        admin_b.user_id, tenant_b, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND,
        "cross-tenant PATCH must return 404");
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "not_found");
}

// ---------------------------------------------------------------------------
// User Story 5 — Start a New Conversation
//
// Tests for POST /tenant/conversations (create).
// Marked #[ignore] until the create handler (T059/T060) is registered.
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on POST /tenant/conversations (T059/T060)"]
async fn create_conversation_returns_open_unassigned_with_message() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Create Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "create@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Create Cust", None, None).await;

    let payload = serde_json::json!({
        "customer_id": customer_id,
        "channel": "web_chat",
        "message": {
            "body": "I need help with my account."
        }
    });
    let response = send(pool.clone(), json_post(
        "/api/v1/tenant/conversations",
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::CREATED,
        "create conversation should return 201");
    let body = body_json(response).await;
    let data = &body["data"];

    assert_eq!(data["status"].as_str().unwrap(), "open",
        "new conversation must be open");
    assert!(data["assignee"].is_null(),
        "new conversation must be unassigned");
    assert_eq!(data["customer"]["id"].as_str().unwrap(), customer_id.to_string());
    assert_eq!(data["channel"].as_str().unwrap(), "web_chat");

    // First message present as kind: reply
    let msgs = data["messages"].as_array()
        .or_else(|| data["participants"].and_then(|_| None))
        .unwrap_or(&serde_json::json!([]));
    // For the create response, the first message may be in a `messages` field
    // or elsewhere; check at least one message exists
    let timeline_resp = send_get(&pool, admin.user_id, tenant_id,
        &format!("/api/v1/tenant/conversations/{}/messages", data["id"].as_str().unwrap())).await;
    assert_eq!(timeline_resp.status(), StatusCode::OK);
    let tl_body = body_json(timeline_resp).await;
    let tl_msgs = tl_body["data"].as_array().expect("timeline data");
    assert_eq!(tl_msgs.len(), 1, "timeline must have one message");
    assert_eq!(tl_msgs[0]["kind"].as_str().unwrap(), "reply",
        "first message kind must be reply");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on POST /tenant/conversations (T059/T060)"]
async fn conversation_created_audit_row() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Create Audit Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "create-audit@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Create Audit Cust", None, None).await;

    let payload = serde_json::json!({
        "customer_id": customer_id,
        "channel": "email",
        "message": {"body": "Help"}
    });
    let response = send(pool.clone(), json_post(
        "/api/v1/tenant/conversations",
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = body_json(response).await;
    let conv_id = Uuid::parse_str(body["data"]["id"].as_str().unwrap()).unwrap();

    let audit_count = fetch_audit_count(&pool, "conversation.created", conv_id).await;
    assert_eq!(audit_count, 1, "conversation.created audit must be written");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on POST /tenant/conversations (T059/T060)"]
async fn create_missing_customer_id_channel_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Missing Fields Create Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "missing-create@example.com").await;

    // Missing customer_id
    let payload = serde_json::json!({
        "channel": "web_chat",
        "message": {"body": "Hello"}
    });
    let response = send(pool.clone(), json_post(
        "/api/v1/tenant/conversations",
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "validation_failed");
    let details = body["error"]["details"].as_array().expect("details array");
    assert!(details.iter().any(|d| d["field"] == "customer_id"),
        "should report missing customer_id, got {details:?}");

    // Missing channel
    let payload = serde_json::json!({
        "customer_id": Uuid::new_v4(),
        "message": {"body": "Hello"}
    });
    let response = send(pool.clone(), json_post(
        "/api/v1/tenant/conversations",
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(response).await;
    let details = body["error"]["details"].as_array().expect("details array");
    assert!(details.iter().any(|d| d["field"] == "channel"),
        "should report missing channel, got {details:?}");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on POST /tenant/conversations (T059/T060)"]
async fn create_empty_message_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Empty Msg Create Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "empty-msg-create@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Empty Msg Cust", None, None).await;

    let payload = serde_json::json!({
        "customer_id": customer_id,
        "channel": "web_chat",
        "message": {"body": ""}
    });
    let response = send(pool.clone(), json_post(
        "/api/v1/tenant/conversations",
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "validation_failed");
    let details = body["error"]["details"].as_array().expect("details array");
    assert!(details.iter().any(|d| d["field"] == "message.body" || d["field"].as_str().map_or(false, |f| f.contains("message"))),
        "should report message validation, got {details:?}");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on POST /tenant/conversations (T059/T060)"]
async fn create_unknown_customer_404() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Unknown Cust Create Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "unknown-cust@example.com").await;
    let unknown_id = Uuid::new_v4();

    let payload = serde_json::json!({
        "customer_id": unknown_id,
        "channel": "web_chat",
        "message": {"body": "Hello"}
    });
    let response = send(pool.clone(), json_post(
        "/api/v1/tenant/conversations",
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND,
        "unknown customer_id must return 404");
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on POST /tenant/conversations (T059/T060)"]
async fn create_cross_tenant_customer_404() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_a = seed_tenant(&pool, "Cross Cust Create A").await;
    let tenant_b = seed_tenant(&pool, "Cross Cust Create B").await;
    let admin_b = seed_admin(&pool, tenant_b, "cross-cust-create@example.com").await;
    let customer_a = seed_customer(&pool, tenant_a, "Cross Cust A", None, None).await;

    let payload = serde_json::json!({
        "customer_id": customer_a,
        "channel": "web_chat",
        "message": {"body": "Hello"}
    });
    let response = send(pool.clone(), json_post(
        "/api/v1/tenant/conversations",
        admin_b.user_id, tenant_b, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND,
        "cross-tenant customer_id must return 404 (FR-016)");
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on POST /tenant/conversations (T059/T060)"]
async fn second_concurrent_open_conversation_succeeds() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Dup Open Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "dup-open@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Dup Open Cust", None, None).await;

    // First conversation
    let payload = serde_json::json!({
        "customer_id": customer_id,
        "channel": "web_chat",
        "message": {"body": "First"}
    });
    let resp1 = send(pool.clone(), json_post(
        "/api/v1/tenant/conversations",
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(resp1.status(), StatusCode::CREATED);

    // Second concurrent open conversation — should succeed (Q3)
    let payload2 = serde_json::json!({
        "customer_id": customer_id,
        "channel": "web_chat",
        "message": {"body": "Second"}
    });
    let resp2 = send(pool.clone(), json_post(
        "/api/v1/tenant/conversations",
        admin.user_id, tenant_id, payload2,
    )).await;
    assert_eq!(resp2.status(), StatusCode::CREATED,
        "second open conversation for same customer+channel must succeed");
    let body2 = body_json(resp2).await;
    assert_ne!(body2["data"]["id"].as_str().unwrap(),
        body_json(resp1).await["data"]["id"].as_str().unwrap(),
        "second conversation must have a different id");
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on POST /tenant/conversations (T059/T060)"]
async fn new_conversation_appears_in_customer_history() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "History Appear Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "history-appear@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "History Appear Cust", None, None).await;

    let payload = serde_json::json!({
        "customer_id": customer_id,
        "channel": "web_chat",
        "message": {"body": "Appear in history"}
    });
    let response = send(pool.clone(), json_post(
        "/api/v1/tenant/conversations",
        admin.user_id, tenant_id, payload,
    )).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let conv_id = body_json(response).await["data"]["id"].as_str().unwrap().to_string();

    // Check that the conversation appears in customer history (FR-018)
    let history_resp = send_get(&pool, admin.user_id, tenant_id,
        &format!("/api/v1/tenant/customers/{customer_id}/conversations")).await;
    assert_eq!(history_resp.status(), StatusCode::OK);
    let history_body = body_json(history_resp).await;
    let history_ids: Vec<&str> = history_body["data"]
        .as_array().expect("history data array")
        .iter()
        .map(|item| item["id"].as_str().expect("conversation id"))
        .collect();
    assert!(history_ids.contains(&conv_id.as_str()),
        "new conversation must appear in customer history, got {history_ids:?}");
}

// ---------------------------------------------------------------------------
// Seeded-volume performance checks (SC-002)
//
// Tests that create many conversations / messages and assert response times
// stay under 1 second. Marked #[ignore] until the relevant handlers are
// registered, gated behind REQUIRE_DB_TESTS=1.
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations route (T013/T014); volume check"]
async fn inbox_list_with_10k_conversations_stays_under_1s() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Volume Inbox Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "volume-inbox@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Volume Inbox Cust", None, None).await;

    let base = Utc::now() - chrono::Duration::hours(48);
    for i in 0..10_000 {
        seed_conversation(
            &pool, tenant_id, customer_id, "web_chat", "open", None,
            base + chrono::Duration::minutes(i as i64),
        ).await;
    }

    let start = std::time::Instant::now();
    let response = get_inbox_list(&pool, admin.user_id, tenant_id, "limit=50").await;
    let elapsed = start.elapsed();

    assert_eq!(response.status(), StatusCode::OK,
        "inbox with 10k conversations must return 200");

    assert!(elapsed < std::time::Duration::from_secs(1),
        "inbox list with 10k conversations took {elapsed:?}, expected <1s (SC-002)");

    if elapsed >= std::time::Duration::from_secs(1) {
        eprintln!("SLOW INBOX (SC-002): {:?} — record query plan for diagnosis", elapsed);
    }
}

#[tokio::test]
#[serial_test::serial(conversations_db)]
#[ignore = "depends on GET /tenant/conversations/{id}/messages (T028/T029); volume check"]
async fn timeline_with_1k_messages_stays_under_1s() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Volume Timeline Tenant").await;
    let admin = seed_admin(&pool, tenant_id, "volume-timeline@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "Volume Timeline Cust", None, None).await;
    let conv_id = seed_conversation(
        &pool, tenant_id, customer_id, "web_chat", "open", None,
        Utc::now() - chrono::Duration::hours(2),
    ).await;

    let base = Utc::now() - chrono::Duration::hours(1);
    for i in 0..1_000 {
        seed_message(
            &pool, tenant_id, conv_id, "reply",
            Some(admin.membership_id), None,
            &format!("Volume message {i}"),
            base + chrono::Duration::seconds(i as i64),
        ).await;
    }

    let start = std::time::Instant::now();
    let response = send_get(&pool, admin.user_id, tenant_id,
        &format!("/api/v1/tenant/conversations/{conv_id}/messages?limit=50")).await;
    let elapsed = start.elapsed();

    assert_eq!(response.status(), StatusCode::OK,
        "timeline with 1k messages must return 200");

    assert!(elapsed < std::time::Duration::from_secs(1),
        "timeline with 1k messages took {elapsed:?}, expected <1s (SC-002)");

    if elapsed >= std::time::Duration::from_secs(1) {
        eprintln!("SLOW TIMELINE (SC-002): {:?} — record query plan for diagnosis", elapsed);
    }
}
