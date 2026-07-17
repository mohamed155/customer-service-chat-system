use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use http_body_util::BodyExt;
use rand::RngCore;
use server::router;
use server::state::AppState;
use sha2::Digest;
use tokio::sync::Notify;
use tower::ServiceExt;
use uuid::Uuid;

struct DeterministicEmailSender {
    succeeds: bool,
}

struct BlockingEmailSender {
    started: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait::async_trait]
impl notifications::EmailSender for BlockingEmailSender {
    fn is_configured(&self) -> bool {
        true
    }

    async fn send(
        &self,
        _message: notifications::EmailMessage,
    ) -> notifications::EmailDeliveryStatus {
        self.started.notify_one();
        self.release.notified().await;
        notifications::EmailDeliveryStatus::Sent
    }
}

#[async_trait::async_trait]
impl notifications::EmailSender for DeterministicEmailSender {
    fn is_configured(&self) -> bool {
        true
    }

    async fn send(
        &self,
        _message: notifications::EmailMessage,
    ) -> notifications::EmailDeliveryStatus {
        if self.succeeds {
            notifications::EmailDeliveryStatus::Sent
        } else {
            notifications::EmailDeliveryStatus::Failed("deterministic failure".into())
        }
    }
}

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
    AppState {
        config: Arc::new(test_config()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool, &test_config()).unwrap(),
    }
}

/// Whether the caller has demanded that DB-backed tests must actually run
/// (Constitution VII: mandatory DB integration tests). CI sets this so a
/// broken/unreachable Postgres service fails the build instead of letting
/// every test in this file silently no-op via `let Some(pool) = get_pool()
/// else { return }`.
fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            if require_db_tests() {
                panic!(
                    "REQUIRE_DB_TESTS=1 but DATABASE_URL is not set — refusing to silently skip team_members tests"
                );
            }
            eprintln!("skipping team_members live tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!(
                "REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable — refusing to silently skip team_members tests"
            );
        }
        eprintln!("skipping team_members live tests: DATABASE_URL is unreachable");
        return None;
    }

    // Many tests in this file seed fixed literal emails (e.g.
    // "owner@example.com") rather than UUID-suffixed ones, so they can only
    // coexist across a full run if each test starts from a clean slate.
    // Every `#[tokio::test]` in this file is also tagged
    // `#[serial_test::serial(team_members_db)]` so they never run
    // concurrently with each other (regardless of the harness's
    // `--test-threads`), making this truncate-on-entry safe: no other test
    // in this binary is touching the database while it runs, and other
    // test binaries in the workspace run as separate sequential processes.
    sqlx::query(
        "TRUNCATE TABLE outbox_events, audit_logs, tenant_invitations, tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(&pool)
    .await
    .expect("failed to reset team_members test tables");

    Some(pool)
}

async fn send(pool: sqlx::PgPool, request: Request<Body>) -> Response {
    router::app_with_test_routes(app_state(pool))
        .oneshot(request)
        .await
        .expect("request should complete")
}

async fn send_with_email_sender(
    pool: sqlx::PgPool,
    request: Request<Body>,
    succeeds: bool,
) -> Response {
    router::app_with_test_routes_and_email_sender(
        app_state(pool),
        Arc::new(DeterministicEmailSender { succeeds }),
    )
    .oneshot(request)
    .await
    .expect("request should complete")
}

fn authenticated_request(
    uri: &str,
    method: Method,
    user_id: Uuid,
    tenant_id: Option<Uuid>,
) -> Request<Body> {
    let mut builder = Request::builder().uri(uri).method(method);
    builder = builder.header("X-Dev-User-Id", user_id.to_string());
    if let Some(tenant_id) = tenant_id {
        builder = builder.header("X-Tenant-ID", tenant_id.to_string());
    }
    builder.body(Body::empty()).unwrap()
}

/// Percent-encode a cursor value for safe embedding in a test-constructed
/// query string. Cursors from the `members`/`invitations` list handlers are
/// `"{RFC3339 timestamp}_{uuid}"`, and RFC3339's UTC offset renders as a
/// literal `+00:00` — a raw `+` in a query string decodes as a space
/// (application/x-www-form-urlencoded convention), breaking the round-trip.
/// A real HTTP client (browser, Angular `HttpParams`) encodes this
/// automatically; test code building raw URI strings must do so explicitly.
fn encode_cursor(cursor: &str) -> String {
    cursor.replace('+', "%2B")
}

async fn body_json(response: Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn assert_validation_error(response: Response, expected_fields: &[(&str, &str)]) {
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "validation_failed");
    let details = body["error"]["details"]
        .as_array()
        .expect("validation error should include details");

    for (field, code) in expected_fields {
        assert!(
            details
                .iter()
                .any(|detail| { detail["field"] == *field && detail["code"] == *code }),
            "expected validation detail for {field} with code {code}, got: {details:?}"
        );
    }
}

async fn assert_audit_event(
    pool: &sqlx::PgPool,
    action: &str,
    actor_user_id: Uuid,
    tenant_id: Uuid,
    resource_type: &str,
    resource_id: Uuid,
    expected_details: serde_json::Value,
) {
    let row: (Option<Uuid>, Uuid, String, String, serde_json::Value) = sqlx::query_as(
        "SELECT actor_user_id, tenant_id, resource_type, resource_id, details FROM audit_logs WHERE action = $1 AND resource_id = $2 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(action)
    .bind(resource_id.to_string())
    .fetch_one(pool)
    .await
    .unwrap();

    assert_eq!(row.0, Some(actor_user_id));
    assert_eq!(row.1, tenant_id);
    assert_eq!(row.2, resource_type);
    assert_eq!(row.3, resource_id.to_string());
    assert_eq!(row.4, expected_details);
}

async fn seed_user(pool: &sqlx::PgPool, display_name: &str, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind(display_name)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Team Members Tenant")
        .bind(format!("team-members-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_membership(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
    role: &str,
    status: &str,
) {
    sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) VALUES ($1, $2, $3, $4)",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
}

async fn assert_ok_response(
    pool: &sqlx::PgPool,
    uri: &str,
    user_id: Uuid,
    tenant_id: Uuid,
) -> serde_json::Value {
    let response = send(
        pool.clone(),
        authenticated_request(uri, Method::GET, user_id, Some(tenant_id)),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "expected 200 for {uri}, got {}",
        response.status()
    );
    body_json(response).await
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn full_field_set_returned() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "Alice Smith", "alice@example.com").await;
    seed_membership(&pool, tenant_id, user_id, "admin", "active").await;

    let body = assert_ok_response(&pool, "/api/v1/tenant/members", user_id, tenant_id).await;

    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    let member = &items[0];
    assert_eq!(member["displayName"], "Alice Smith");
    assert_eq!(member["email"], "alice@example.com");
    assert_eq!(member["role"], "admin");
    assert_eq!(member["status"], "active");
    assert!(member["id"].is_string());
    assert!(member["userId"].is_string());
    assert!(member["joinedAt"].is_string());
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn search_q_matches_name_and_email() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let alice = seed_user(&pool, "Alice Smith", "alice@example.com").await;
    let bob = seed_user(&pool, "Bob Jones", "bob@example.com").await;
    let charlie = seed_user(&pool, "Charlie Brown", "charlie@example.com").await;
    seed_membership(&pool, tenant_id, alice, "admin", "active").await;
    seed_membership(&pool, tenant_id, bob, "manager", "active").await;
    seed_membership(&pool, tenant_id, charlie, "agent", "active").await;

    let body = assert_ok_response(&pool, "/api/v1/tenant/members?q=alice", alice, tenant_id).await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "q=alice should match 1 member");
    assert_eq!(items[0]["displayName"], "Alice Smith");

    let body = assert_ok_response(&pool, "/api/v1/tenant/members?q=smith", alice, tenant_id).await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "q=smith should match 1 member");
    assert_eq!(items[0]["displayName"], "Alice Smith");

    let body = assert_ok_response(
        &pool,
        "/api/v1/tenant/members?q=example.com",
        alice,
        tenant_id,
    )
    .await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 3, "q=example.com should match all 3 members");
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn status_filter_works() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let active_user = seed_user(&pool, "Active User", "active@example.com").await;
    let disabled_user = seed_user(&pool, "Disabled User", "disabled@example.com").await;
    seed_membership(&pool, tenant_id, active_user, "admin", "active").await;
    seed_membership(&pool, tenant_id, disabled_user, "viewer", "disabled").await;

    let body = assert_ok_response(
        &pool,
        "/api/v1/tenant/members?status=active",
        active_user,
        tenant_id,
    )
    .await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["status"], "active");

    let body = assert_ok_response(
        &pool,
        "/api/v1/tenant/members?status=disabled",
        active_user,
        tenant_id,
    )
    .await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["status"], "disabled");
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn roster_query_validation_errors_return_422() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-query-validation@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let cases = vec![
        (
            "/api/v1/tenant/members?status=bogus".to_string(),
            vec![("status", "invalid_value")],
        ),
        (
            "/api/v1/tenant/members?limit=0".to_string(),
            vec![("limit", "invalid_range")],
        ),
        (
            "/api/v1/tenant/members?limit=101".to_string(),
            vec![("limit", "invalid_range")],
        ),
        (
            "/api/v1/tenant/members?cursor=not-a-cursor".to_string(),
            vec![("cursor", "invalid_value")],
        ),
        (
            format!("/api/v1/tenant/members?q={}", "x".repeat(255)),
            vec![("q", "too_long")],
        ),
    ];

    for (uri, expected_fields) in cases {
        let response = send(
            pool.clone(),
            authenticated_request(&uri, Method::GET, owner, Some(tenant_id)),
        )
        .await;
        assert_validation_error(response, &expected_fields).await;
    }
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn cursor_pagination_works() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let mut member_ids = Vec::new();
    for i in 0..3 {
        let user_id = seed_user(&pool, &format!("User {i}"), &format!("user{i}@example.com")).await;
        // members.view requires owner/admin/manager; agent cannot list the roster.
        seed_membership(&pool, tenant_id, user_id, "admin", "active").await;
        member_ids.push(user_id);
    }

    let body = assert_ok_response(
        &pool,
        "/api/v1/tenant/members?limit=1",
        member_ids[0],
        tenant_id,
    )
    .await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    let cursor = body["nextCursor"].as_str().unwrap().to_string();
    assert!(body["hasMore"].as_bool().unwrap());

    let body = assert_ok_response(
        &pool,
        &format!(
            "/api/v1/tenant/members?limit=1&cursor={}",
            encode_cursor(&cursor)
        ),
        member_ids[0],
        tenant_id,
    )
    .await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    let cursor2 = body["nextCursor"].as_str().unwrap().to_string();
    assert!(body["hasMore"].as_bool().unwrap());
    assert_ne!(cursor, cursor2);

    let body = assert_ok_response(
        &pool,
        &format!(
            "/api/v1/tenant/members?limit=1&cursor={}",
            encode_cursor(&cursor2)
        ),
        member_ids[0],
        tenant_id,
    )
    .await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert!(!body["hasMore"].as_bool().unwrap());
    assert!(body["nextCursor"].is_null());
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn cross_tenant_isolation() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let user_a = seed_user(&pool, "Tenant A User", "a@example.com").await;
    let user_b = seed_user(&pool, "Tenant B User", "b@example.com").await;
    seed_membership(&pool, tenant_a, user_a, "admin", "active").await;
    seed_membership(&pool, tenant_b, user_b, "admin", "active").await;

    let body = assert_ok_response(&pool, "/api/v1/tenant/members", user_a, tenant_a).await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["email"], "a@example.com");

    let body = assert_ok_response(&pool, "/api/v1/tenant/members", user_b, tenant_b).await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["email"], "b@example.com");
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn empty_roster() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "Lonely User", "lonely@example.com").await;
    seed_membership(&pool, tenant_id, user_id, "admin", "active").await;

    // A truly member-less tenant can only be queried by someone without a
    // membership row via platform staff permissions (members.view has no
    // per-tenant carve-out for "your own empty tenant" — cross-tenant access
    // without staff rank is correctly refused per FR-002).
    let staff = seed_user(&pool, "Platform Staff", "staff-empty-roster@example.com").await;
    sqlx::query("UPDATE users SET platform_role = 'super_admin' WHERE id = $1")
        .bind(staff)
        .execute(&pool)
        .await
        .unwrap();

    let other_tenant = seed_tenant(&pool).await;
    let body = assert_ok_response(&pool, "/api/v1/tenant/members", staff, other_tenant).await;
    let items = body["items"].as_array().unwrap();
    assert!(items.is_empty());
    assert!(body["nextCursor"].is_null());
    assert!(!body["hasMore"].as_bool().unwrap());
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn scale_500_members() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let admin_id = seed_user(&pool, "Admin", "admin@example.com").await;
    seed_membership(&pool, tenant_id, admin_id, "admin", "active").await;

    for i in 0..500 {
        let user_id = seed_user(&pool, &format!("User {i}"), &format!("user{i}@example.com")).await;
        seed_membership(&pool, tenant_id, user_id, "agent", "active").await;
    }

    let mut cursor: Option<String> = None;
    let mut total = 0usize;
    loop {
        let uri = match &cursor {
            Some(c) => format!(
                "/api/v1/tenant/members?limit=100&cursor={}",
                encode_cursor(c)
            ),
            None => "/api/v1/tenant/members?limit=100".to_string(),
        };
        let body = assert_ok_response(&pool, &uri, admin_id, tenant_id).await;
        let items = body["items"].as_array().unwrap();
        total += items.len();
        if !body["hasMore"].as_bool().unwrap() {
            break;
        }
        cursor = body["nextCursor"].as_str().map(|s| s.to_string());
    }
    assert_eq!(total, 501);

    let body = assert_ok_response(
        &pool,
        "/api/v1/tenant/members?q=User 42",
        admin_id,
        tenant_id,
    )
    .await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["displayName"], "User 42");
}

// ---------------------------------------------------------------------------
// Invitation lifecycle (T029)
// ---------------------------------------------------------------------------

fn json_request(
    uri: &str,
    method: Method,
    user_id: Uuid,
    tenant_id: Option<Uuid>,
    body: serde_json::Value,
) -> Request<Body> {
    let bytes = serde_json::to_vec(&body).unwrap();
    let mut builder = Request::builder().uri(uri).method(method);
    builder = builder
        .header("X-Dev-User-Id", user_id.to_string())
        .header("content-type", "application/json");
    if let Some(tenant_id) = tenant_id {
        builder = builder.header("X-Tenant-ID", tenant_id.to_string());
    }
    builder.body(Body::from(bytes)).unwrap()
}

async fn seed_invitation(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    email: &str,
    role: &str,
    invited_by: Uuid,
) -> Uuid {
    let (id, _raw) = seed_invitation_with_token(pool, tenant_id, email, role, invited_by).await;
    id
}

async fn seed_invitation_with_token(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    email: &str,
    role: &str,
    invited_by: Uuid,
) -> (Uuid, String) {
    use sha2::Sha256;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let raw = hex::encode(bytes);
    let hash = hex::encode(Sha256::digest(bytes));
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_invitations (tenant_id, email, role, token_hash, invited_by, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(tenant_id)
    .bind(email)
    .bind(role)
    .bind(&hash)
    .bind(invited_by)
    .bind(expires_at)
    .fetch_one(pool)
    .await
    .unwrap();
    (id, raw)
}

async fn create_invitation(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    tenant_id: Uuid,
    email: &str,
    role: &str,
) -> (StatusCode, serde_json::Value) {
    let body = serde_json::json!({ "email": email, "role": role });
    let request = json_request(
        "/api/v1/tenant/members/invitations",
        Method::POST,
        user_id,
        Some(tenant_id),
        body,
    );
    let response = send(pool.clone(), request).await;
    let status = response.status();
    let body = body_json(response).await;
    (status, body)
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn create_invitation_success() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let (status, body) =
        create_invitation(&pool, owner, tenant_id, "newmember@example.com", "agent").await;

    assert_eq!(status, StatusCode::CREATED, "expected 201, got {body:?}");
    assert!(
        body["acceptUrl"].as_str().unwrap().contains("/invite/"),
        "missing acceptUrl"
    );
    assert!(body["invitation"]["id"].is_string());
    assert_eq!(body["invitation"]["email"], "newmember@example.com");
    assert_eq!(body["invitation"]["role"], "agent");
    assert_eq!(body["invitation"]["status"], "pending");
    assert_eq!(body["emailSent"], serde_json::json!(false));
    assert_eq!(
        body["emailDeliveryStatus"],
        serde_json::json!("unconfigured")
    );

    let invitation_id = Uuid::parse_str(body["invitation"]["id"].as_str().unwrap()).unwrap();

    assert_audit_event(
        &pool,
        "member.invited",
        owner,
        tenant_id,
        "invitation",
        invitation_id,
        serde_json::json!({"email": "newmember@example.com", "role": "agent"}),
    )
    .await;
}

async fn assert_configured_delivery_result(succeeds: bool, expected: &str) {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    let email = format!("{expected}@example.com");
    let request = json_request(
        "/api/v1/tenant/members/invitations",
        Method::POST,
        owner,
        Some(tenant_id),
        serde_json::json!({ "email": email, "role": "agent" }),
    );

    let response = send_with_email_sender(pool.clone(), request, succeeds).await;
    let status = response.status();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::CREATED, "expected 201, got {body:?}");
    assert_eq!(body["emailDeliveryStatus"], "queued");
    assert_eq!(body["emailSent"], false);
    let invitation_id = Uuid::parse_str(body["invitation"]["id"].as_str().unwrap()).unwrap();

    let attempt_count = if succeeds { 1 } else { 3 };
    for attempt in 0..attempt_count {
        tenancy::invitations::process_invitation_deliveries_once(
            &pool,
            Arc::new(DeterministicEmailSender { succeeds }),
        )
        .await
        .unwrap();
        if !succeeds && attempt + 1 < attempt_count {
            assert_eq!(
                sqlx::query_scalar::<_, String>(
                    "SELECT email_delivery_status FROM tenant_invitations WHERE id = $1",
                )
                .bind(invitation_id)
                .fetch_one(&pool)
                .await
                .unwrap(),
                "queued"
            );
            sqlx::query("UPDATE outbox_events SET available_at = now() WHERE aggregate_id = $1")
                .bind(invitation_id.to_string())
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    let persisted = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let status: String = sqlx::query_scalar(
                "SELECT email_delivery_status FROM tenant_invitations WHERE tenant_id = $1 AND id = $2",
            )
            .bind(tenant_id)
            .bind(invitation_id)
            .fetch_one(&pool)
            .await
            .unwrap();
            if status == expected {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("delivery status should reach its eventual result");
    assert_eq!(persisted, expected);
    if !succeeds {
        let outbox: (i32, bool, bool) = sqlx::query_as(
            "SELECT attempts, processed_at IS NOT NULL, dead_lettered_at IS NOT NULL \
             FROM outbox_events WHERE aggregate_id = $1",
        )
        .bind(invitation_id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(outbox, (3, true, true));
    }

    let list_request = authenticated_request(
        "/api/v1/tenant/members/invitations",
        Method::GET,
        owner,
        Some(tenant_id),
    );
    let list = body_json(send(pool, list_request).await).await;
    assert_eq!(list["items"][0]["id"], invitation_id.to_string());
    assert_eq!(list["items"][0]["emailDeliveryStatus"], expected);
}

#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial(team_members_db)]
async fn configured_email_success_is_persisted_and_listed() {
    assert_configured_delivery_result(true, "sent").await;
}

#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial(team_members_db)]
async fn configured_email_failure_is_persisted_and_listed_without_losing_invitation() {
    assert_configured_delivery_result(false, "failed").await;
}

#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial(team_members_db)]
async fn blocked_sender_does_not_block_invitation_creation() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    let started = Arc::new(Notify::new());
    let sender: Arc<dyn notifications::EmailSender> = Arc::new(BlockingEmailSender {
        started: started.clone(),
        release: Arc::new(Notify::new()),
    });
    let request = json_request(
        "/api/v1/tenant/members/invitations",
        Method::POST,
        owner,
        Some(tenant_id),
        serde_json::json!({"email": "blocked@example.com", "role": "agent"}),
    );

    let response = tokio::time::timeout(
        Duration::from_millis(250),
        router::app_with_test_routes_and_email_sender(app_state(pool.clone()), sender)
            .oneshot(request),
    )
    .await
    .expect("creation must not wait for transport")
    .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM outbox_events WHERE event_type = 'invitation.email_delivery'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), started.notified())
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial(team_members_db)]
async fn queued_outbox_delivery_is_recovered() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    let invitation_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tenant_invitations (id, tenant_id, email, role, token_hash, invited_by, expires_at, email_delivery_status) \
         VALUES ($1,$2,'recover@example.com','agent',$3,$4,now() + interval '7 days','queued')",
    )
    .bind(invitation_id).bind(tenant_id).bind("a".repeat(64)).bind(owner)
    .execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1,'tenant_invitation',$2,$3,'invitation.email_delivery',$4,now())",
    )
    .bind(Uuid::new_v4()).bind(invitation_id.to_string()).bind(tenant_id.to_string())
    .bind(serde_json::json!({"to":"recover@example.com","acceptUrl":"https://app.test/invite/token"}))
    .execute(&pool).await.unwrap();
    sqlx::query(
        "UPDATE outbox_events SET claimed_at = now() - interval '6 minutes', claim_token = $1 \
         WHERE aggregate_id = $2",
    )
    .bind(Uuid::new_v4())
    .bind(invitation_id.to_string())
    .execute(&pool)
    .await
    .unwrap();

    let processed = tenancy::invitations::process_invitation_deliveries_once(
        &pool,
        Arc::new(DeterministicEmailSender { succeeds: true }),
    )
    .await
    .unwrap();

    assert_eq!(processed, 1);
    let state: (String, bool) = sqlx::query_as(
        "SELECT ti.email_delivery_status, oe.processed_at IS NOT NULL \
         FROM tenant_invitations ti JOIN outbox_events oe ON oe.aggregate_id = ti.id::text \
         WHERE ti.tenant_id = $1 AND ti.id = $2",
    )
    .bind(tenant_id)
    .bind(invitation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, ("sent".into(), true));
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn create_invitation_returns_full_camel_case_inviter_shape() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner Name", "owner@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    let (_, body) = create_invitation(&pool, owner, tenant_id, "shape@example.com", "agent").await;
    assert_eq!(body["invitation"]["invitedByName"], "Owner Name");
    assert!(body.get("acceptUrl").is_some());
    assert!(body.get("emailDeliveryStatus").is_some());
    assert!(body.get("email_delivery_status").is_none());
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn invitation_delivery_status_is_targeted_and_tenant_scoped() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    let manager = seed_user(&pool, "Manager", "manager@example.com").await;
    seed_membership(&pool, tenant_id, manager, "manager", "active").await;
    let (_, created) =
        create_invitation(&pool, owner, tenant_id, "status@example.com", "agent").await;
    let invitation_id = created["invitation"]["id"].as_str().unwrap();

    let request = authenticated_request(
        &format!("/api/v1/tenant/members/invitations/{invitation_id}/delivery"),
        Method::GET,
        manager,
        Some(tenant_id),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        body_json(response).await["emailDeliveryStatus"],
        "unconfigured"
    );

    let other_tenant = seed_tenant(&pool).await;
    seed_membership(&pool, other_tenant, owner, "owner", "active").await;
    let cross_tenant = authenticated_request(
        &format!("/api/v1/tenant/members/invitations/{invitation_id}/delivery"),
        Method::GET,
        owner,
        Some(other_tenant),
    );
    assert_eq!(
        send(pool, cross_tenant).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial(team_members_db)]
async fn poison_delivery_is_terminal_and_does_not_starve_next_event() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    let poison_id = Uuid::new_v4();
    let valid_id = Uuid::new_v4();
    for (id, email, hash) in [
        (poison_id, "poison@example.com", "b".repeat(64)),
        (valid_id, "valid@example.com", "c".repeat(64)),
    ] {
        sqlx::query(
            "INSERT INTO tenant_invitations (id,tenant_id,email,role,token_hash,invited_by,expires_at,email_delivery_status) \
             VALUES ($1,$2,$3,'agent',$4,$5,now()+interval '7 days','queued')",
        ).bind(id).bind(tenant_id).bind(email).bind(hash).bind(owner).execute(&pool).await.unwrap();
    }
    for (id, payload, created_offset) in [
        (poison_id, serde_json::json!({}), 0_i64),
        (
            valid_id,
            serde_json::json!({"to":"valid@example.com","acceptUrl":"https://app.test/invite/token"}),
            1_i64,
        ),
    ] {
        sqlx::query(
            "INSERT INTO outbox_events (id,aggregate_type,aggregate_id,tenant_id,event_type,payload,created_at) \
             VALUES ($1,'tenant_invitation',$2,$3,'invitation.email_delivery',$4,now()+($5*interval '1 millisecond'))",
        ).bind(Uuid::new_v4()).bind(id.to_string()).bind(tenant_id.to_string()).bind(payload)
        .bind(created_offset).execute(&pool).await.unwrap();
    }
    let sender: Arc<dyn notifications::EmailSender> =
        Arc::new(DeterministicEmailSender { succeeds: true });

    assert_eq!(
        tenancy::invitations::process_invitation_deliveries_once(&pool, sender.clone())
            .await
            .unwrap(),
        1
    );
    let poison: (String, bool) = sqlx::query_as(
        "SELECT ti.email_delivery_status, oe.dead_lettered_at IS NOT NULL \
         FROM tenant_invitations ti JOIN outbox_events oe ON oe.aggregate_id=ti.id::text WHERE ti.id=$1",
    ).bind(poison_id).fetch_one(&pool).await.unwrap();
    assert_eq!(poison, ("failed".into(), true));

    assert_eq!(
        tenancy::invitations::process_invitation_deliveries_once(&pool, sender)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT email_delivery_status FROM tenant_invitations WHERE id=$1"
        )
        .bind(valid_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        "sent"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial(team_members_db)]
async fn smtp_wait_does_not_hold_the_outbox_row_lock() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    let invitation_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tenant_invitations (id,tenant_id,email,role,token_hash,invited_by,expires_at,email_delivery_status) \
         VALUES ($1,$2,'lock@example.com','agent',$3,$4,now()+interval '7 days','queued')",
    ).bind(invitation_id).bind(tenant_id).bind("d".repeat(64)).bind(owner).execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO outbox_events (id,aggregate_type,aggregate_id,tenant_id,event_type,payload,created_at) \
         VALUES ($1,'tenant_invitation',$2,$3,'invitation.email_delivery',$4,now())",
    ).bind(Uuid::new_v4()).bind(invitation_id.to_string()).bind(tenant_id.to_string())
    .bind(serde_json::json!({"to":"lock@example.com","acceptUrl":"https://app.test/invite/token"}))
    .execute(&pool).await.unwrap();
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let sender: Arc<dyn notifications::EmailSender> = Arc::new(BlockingEmailSender {
        started: started.clone(),
        release: release.clone(),
    });
    let worker_pool = pool.clone();
    let worker = tokio::spawn(async move {
        tenancy::invitations::process_invitation_deliveries_once(&worker_pool, sender).await
    });
    started.notified().await;

    tokio::time::timeout(
        Duration::from_millis(250),
        sqlx::query("UPDATE outbox_events SET last_error='lock probe' WHERE aggregate_id=$1")
            .bind(invitation_id.to_string())
            .execute(&pool),
    )
    .await
    .expect("SMTP must run after the claim transaction commits")
    .unwrap();
    release.notify_one();
    worker.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[serial_test::serial(team_members_db)]
async fn stale_third_attempt_claim_is_terminally_failed_and_dead_lettered() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    let invitation_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tenant_invitations (id,tenant_id,email,role,token_hash,invited_by,expires_at,email_delivery_status) \
         VALUES ($1,$2,'stale-third@example.com','agent',$3,$4,now()+interval '7 days','queued')",
    )
    .bind(invitation_id)
    .bind(tenant_id)
    .bind("e".repeat(64))
    .bind(owner)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO outbox_events \
         (id,aggregate_type,aggregate_id,tenant_id,event_type,payload,created_at,attempts,claimed_at,claim_token) \
         VALUES ($1,'tenant_invitation',$2,$3,'invitation.email_delivery',$4,now(),3,now()-interval '6 minutes',$5)",
    )
    .bind(Uuid::new_v4())
    .bind(invitation_id.to_string())
    .bind(tenant_id.to_string())
    .bind(serde_json::json!({"to":"stale-third@example.com","acceptUrl":"https://app.test/invite/token"}))
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap();

    let processed = tenancy::invitations::process_invitation_deliveries_once(
        &pool,
        Arc::new(DeterministicEmailSender { succeeds: true }),
    )
    .await
    .unwrap();

    assert_eq!(processed, 1);
    let state: (String, i32, bool, bool) = sqlx::query_as(
        "SELECT ti.email_delivery_status, oe.attempts, oe.processed_at IS NOT NULL, \
         oe.dead_lettered_at IS NOT NULL FROM tenant_invitations ti \
         JOIN outbox_events oe ON oe.aggregate_id=ti.id::text WHERE ti.id=$1",
    )
    .bind(invitation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, ("failed".into(), 3, true, true));
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn create_invitation_rejects_invalid_email() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let (status, body) = create_invitation(&pool, owner, tenant_id, "user@example", "agent").await;

    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "expected 422, got {body:?}"
    );
    assert_eq!(body["error"]["code"], "validation_failed");
    let details = body["error"]["details"].as_array().expect("details array");
    assert!(
        !details.is_empty(),
        "expected at least one detail entry, got: {details:?}"
    );
    assert_eq!(
        details[0]["field"], "email",
        "expected details[0].field == email, got: {details:?}"
    );
    assert_eq!(details[0]["code"], "invalid_format");
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn create_invitation_duplicate_active_member_409() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    let member = seed_user(&pool, "Existing Member", "existing@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    seed_membership(&pool, tenant_id, member, "agent", "active").await;

    let (status, body) =
        create_invitation(&pool, owner, tenant_id, "existing@example.com", "agent").await;

    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "expected 409 for existing member, got {body:?}"
    );
    assert_eq!(body["error"]["code"], "conflict");
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn create_invitation_duplicate_pending_409() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let (status, _body) =
        create_invitation(&pool, owner, tenant_id, "pending@example.com", "agent").await;
    assert_eq!(status, StatusCode::CREATED, "first invite should succeed");

    let (status, body) =
        create_invitation(&pool, owner, tenant_id, "pending@example.com", "agent").await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "second invite should be 409, got {body:?}"
    );
    assert_eq!(body["error"]["code"], "conflict");
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn create_invitation_hierarchy_refusal() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let manager = seed_user(&pool, "Manager", "manager@example.com").await;
    seed_membership(&pool, tenant_id, manager, "manager", "active").await;

    let (status, body) =
        create_invitation(&pool, manager, tenant_id, "newadmin@example.com", "admin").await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "manager should not be able to invite admin, got {body:?}"
    );

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE action = 'member.invited' AND tenant_id = $1 AND details->>'email' = $2",
    )
    .bind(tenant_id)
    .bind("newadmin@example.com")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 0);
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn create_invitation_disabled_member_conflict() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-disabled@example.com").await;
    let disabled = seed_user(&pool, "Disabled", "disabled-invite@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    seed_membership(&pool, tenant_id, disabled, "agent", "disabled").await;

    let (status, body) = create_invitation(
        &pool,
        owner,
        tenant_id,
        "disabled-invite@example.com",
        "agent",
    )
    .await;

    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "disabled member should block invites"
    );
    assert_eq!(body["error"]["code"], "conflict");
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn create_invitation_duplicate_race_returns_one_success() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-race@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let req_a = tokio::spawn({
        let pool = pool.clone();
        async move { create_invitation(&pool, owner, tenant_id, "race@example.com", "agent").await }
    });
    let req_b = tokio::spawn({
        let pool = pool.clone();
        async move { create_invitation(&pool, owner, tenant_id, "race@example.com", "agent").await }
    });

    let (res_a, res_b) = tokio::join!(req_a, req_b);
    let statuses = [res_a.unwrap().0, res_b.unwrap().0];
    assert!(statuses.contains(&StatusCode::CREATED));
    assert!(statuses.contains(&StatusCode::CONFLICT));
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn list_invitations_filters_status_and_paginates() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-list@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let (_pending_id, _pending_token) =
        seed_invitation_with_token(&pool, tenant_id, "pending@example.com", "agent", owner).await;
    let (expired_id, _expired_token) =
        seed_invitation_with_token(&pool, tenant_id, "expired@example.com", "agent", owner).await;
    let (persisted_expired_id, _persisted_expired_token) = seed_invitation_with_token(
        &pool,
        tenant_id,
        "persisted-expired@example.com",
        "agent",
        owner,
    )
    .await;
    let (accepted_id, _accepted_token) =
        seed_invitation_with_token(&pool, tenant_id, "accepted@example.com", "agent", owner).await;

    sqlx::query(
        "UPDATE tenant_invitations SET expires_at = now() - interval '1 hour' WHERE id = $1",
    )
    .bind(expired_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE tenant_invitations SET expires_at = now() - interval '1 hour', status = 'expired' WHERE id = $1",
    )
    .bind(persisted_expired_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE tenant_invitations SET status = 'accepted', accepted_at = now(), accepted_user_id = $1 WHERE id = $2")
        .bind(owner)
        .bind(accepted_id)
        .execute(&pool)
        .await
        .unwrap();

    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/members/invitations",
            Method::GET,
            owner,
            Some(tenant_id),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let statuses: Vec<String> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["status"].as_str().unwrap().to_string())
        .collect();
    assert!(statuses.contains(&"pending".to_string()));
    assert!(statuses.contains(&"expired".to_string()));
    assert!(!statuses.contains(&"accepted".to_string()));
    assert_eq!(
        statuses
            .iter()
            .filter(|status| *status == "expired")
            .count(),
        2
    );

    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/members/invitations?status=pending",
            Method::GET,
            owner,
            Some(tenant_id),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 3);

    let response = send(
        pool.clone(),
        Request::get("/api/v1/tenant/members/invitations?status=expired")
            .header("X-Dev-User-Id", owner.to_string())
            .header("X-Tenant-ID", tenant_id.to_string())
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    assert!(body["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item["status"] == "expired"));

    let response = send(
        pool.clone(),
        Request::get("/api/v1/tenant/members/invitations?limit=1")
            .header("X-Dev-User-Id", owner.to_string())
            .header("X-Tenant-ID", tenant_id.to_string())
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert!(body["hasMore"].as_bool().unwrap());
    let cursor = body["nextCursor"].as_str().unwrap().to_string();

    let response = send(
        pool.clone(),
        Request::get(format!(
            "/api/v1/tenant/members/invitations?limit=1&cursor={}",
            encode_cursor(&cursor)
        ))
        .header("X-Dev-User-Id", owner.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .body(Body::empty())
        .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn invitation_query_validation_errors_return_422() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-invite-query-validation@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let (_pending_id, _pending_token) =
        seed_invitation_with_token(&pool, tenant_id, "pending@example.com", "agent", owner).await;

    let cases = [
        (
            "/api/v1/tenant/members/invitations?status=bogus",
            vec![("status", "invalid_value")],
        ),
        (
            "/api/v1/tenant/members/invitations?limit=0",
            vec![("limit", "invalid_range")],
        ),
        (
            "/api/v1/tenant/members/invitations?limit=101",
            vec![("limit", "invalid_range")],
        ),
        (
            "/api/v1/tenant/members/invitations?cursor=not-a-cursor",
            vec![("cursor", "invalid_value")],
        ),
    ];

    for (uri, expected_fields) in cases {
        let response = send(
            pool.clone(),
            authenticated_request(uri, Method::GET, owner, Some(tenant_id)),
        )
        .await;
        assert_validation_error(response, &expected_fields).await;
    }
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn accept_invitation_race_consumes_single_use_token() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-race-accept@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let (inv_id, raw_token) =
        seed_invitation_with_token(&pool, tenant_id, "race-accept@example.com", "agent", owner)
            .await;
    assert!(!inv_id.is_nil());

    let body = serde_json::to_vec(&serde_json::json!({
        "displayName": "Race User",
        "password": "securePassword123!"
    }))
    .unwrap();

    let req_a = Request::post(format!("/api/v1/invitations/{raw_token}/accept"))
        .header("content-type", "application/json")
        .body(Body::from(body.clone()))
        .unwrap();
    let req_b = Request::post(format!("/api/v1/invitations/{raw_token}/accept"))
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();

    let (res_a, res_b) = tokio::join!(send(pool.clone(), req_a), send(pool.clone(), req_b));
    let statuses = [res_a.status(), res_b.status()];
    assert!(statuses.contains(&StatusCode::OK));
    assert!(statuses.contains(&StatusCode::GONE));

    // Exactly one membership transition and exactly one acceptance audit row
    // must be committed — the single-use guard must not produce partial or
    // duplicate side effects under concurrency.
    let membership_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tenant_memberships WHERE tenant_id = $1 \
         AND user_id = (SELECT id FROM users WHERE email = 'race-accept@example.com')",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        membership_count, 1,
        "exactly one membership row must exist after the race"
    );

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE action = 'member.invitation_accepted' AND resource_id = $1",
    )
    .bind(inv_id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        audit_count, 1,
        "exactly one member.invitation_accepted audit row must exist after the race"
    );
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn accept_invitation_still_works_after_inviter_is_disabled() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-inviter@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let (inv_id, raw_token) = seed_invitation_with_token(
        &pool,
        tenant_id,
        "inviter-change@example.com",
        "agent",
        owner,
    )
    .await;

    sqlx::query(
        "UPDATE tenant_memberships SET status = 'disabled' WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(owner)
    .execute(&pool)
    .await
    .unwrap();

    let request = Request::post(format!("/api/v1/invitations/{raw_token}/accept"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "displayName": "Invited User",
                "password": "securePassword123!"
            }))
            .unwrap(),
        ))
        .unwrap();

    let response = send(pool.clone(), request).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "inviter state should not block acceptance"
    );

    let status: String = sqlx::query_scalar("SELECT status FROM tenant_invitations WHERE id = $1")
        .bind(inv_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "accepted");
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn list_invitations() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let email1 = format!("invite1-{}@example.com", Uuid::new_v4().simple());
    let email2 = format!("invite2-{}@example.com", Uuid::new_v4().simple());
    create_invitation(&pool, owner, tenant_id, &email1, "agent").await;
    create_invitation(&pool, owner, tenant_id, &email2, "manager").await;

    let request = authenticated_request(
        "/api/v1/tenant/members/invitations",
        Method::GET,
        owner,
        Some(tenant_id),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "expected 2 invitations, got {items:?}");
    let emails: Vec<&str> = items.iter().map(|i| i["email"].as_str().unwrap()).collect();
    assert!(emails.contains(&email1.as_str()));
    assert!(emails.contains(&email2.as_str()));
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn revoke_invitation() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let inv_id = seed_invitation(&pool, tenant_id, "revoke@example.com", "agent", owner).await;

    let request = authenticated_request(
        &format!("/api/v1/tenant/members/invitations/{inv_id}"),
        Method::DELETE,
        owner,
        Some(tenant_id),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "expected 204 on revoke"
    );

    let status: String = sqlx::query_scalar("SELECT status FROM tenant_invitations WHERE id = $1")
        .bind(inv_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "revoked");

    assert_audit_event(
        &pool,
        "member.invitation_revoked",
        owner,
        tenant_id,
        "invitation",
        inv_id,
        serde_json::json!({"email": "revoke@example.com", "role": "agent"}),
    )
    .await;
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn revoked_token_unacceptable() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let email = format!("revoked-{}@example.com", Uuid::new_v4().simple());
    let (inv_id, raw_token) =
        seed_invitation_with_token(&pool, tenant_id, &email, "agent", owner).await;

    sqlx::query(
        "UPDATE tenant_invitations SET status = 'revoked', revoked_at = now(), revoked_by = $1 WHERE id = $2",
    )
    .bind(owner)
    .bind(inv_id)
    .execute(&pool)
    .await
    .unwrap();

    let request = Request::post(format!("/api/v1/invitations/{raw_token}/accept"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "displayName": "Test User",
                "password": "password123"
            }))
            .unwrap(),
        ))
        .unwrap();
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Bug fixes: revoke status codes, reissue-after-expiry, rank documentation,
// cross-tenant regressions, soft-deleted/suspended tenant exclusion
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn revoke_accepted_invitation_returns_409_not_404() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-revoke-accepted@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let inv_id = seed_invitation(
        &pool,
        tenant_id,
        "already-accepted@example.com",
        "agent",
        owner,
    )
    .await;
    sqlx::query(
        "UPDATE tenant_invitations SET status = 'accepted', accepted_at = now(), accepted_user_id = $1 WHERE id = $2",
    )
    .bind(owner)
    .bind(inv_id)
    .execute(&pool)
    .await
    .unwrap();

    let request = authenticated_request(
        &format!("/api/v1/tenant/members/invitations/{inv_id}"),
        Method::DELETE,
        owner,
        Some(tenant_id),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "revoking an already-accepted invitation must be 409, not 404 (it was found, just terminal)"
    );
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn revoke_already_revoked_invitation_returns_409() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-revoke-revoked@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let inv_id = seed_invitation(
        &pool,
        tenant_id,
        "already-revoked@example.com",
        "agent",
        owner,
    )
    .await;
    sqlx::query(
        "UPDATE tenant_invitations SET status = 'revoked', revoked_at = now(), revoked_by = $1 WHERE id = $2",
    )
    .bind(owner)
    .bind(inv_id)
    .execute(&pool)
    .await
    .unwrap();

    let request = authenticated_request(
        &format!("/api/v1/tenant/members/invitations/{inv_id}"),
        Method::DELETE,
        owner,
        Some(tenant_id),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "revoking an already-revoked invitation must be 409, not 404"
    );
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn revoke_expired_pending_invitation_succeeds_as_noop_hardening() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-revoke-expired@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let inv_id = seed_invitation(
        &pool,
        tenant_id,
        "expired-revoke@example.com",
        "agent",
        owner,
    )
    .await;
    sqlx::query(
        "UPDATE tenant_invitations SET expires_at = now() - interval '1 hour' WHERE id = $1",
    )
    .bind(inv_id)
    .execute(&pool)
    .await
    .unwrap();

    let request = authenticated_request(
        &format!("/api/v1/tenant/members/invitations/{inv_id}"),
        Method::DELETE,
        owner,
        Some(tenant_id),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "revoking an expired-pending invitation is a no-op hardening success, not blocked by expiry"
    );

    let persisted_id = seed_invitation(
        &pool,
        tenant_id,
        "persisted-expired-revoke@example.com",
        "agent",
        owner,
    )
    .await;
    sqlx::query(
        "UPDATE tenant_invitations SET expires_at = now() - interval '1 hour', status = 'expired' WHERE id = $1",
    )
    .bind(persisted_id)
    .execute(&pool)
    .await
    .unwrap();
    let response = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/members/invitations/{persisted_id}"),
            Method::DELETE,
            owner,
            Some(tenant_id),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let persisted_status: String =
        sqlx::query_scalar("SELECT status FROM tenant_invitations WHERE id = $1")
            .bind(persisted_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(persisted_status, "revoked");
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE action = 'member.invitation_revoked' AND resource_id = $1",
    )
    .bind(persisted_id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 1);
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn revoke_unknown_invitation_id_still_404() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-revoke-unknown@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let request = authenticated_request(
        &format!("/api/v1/tenant/members/invitations/{}", Uuid::new_v4()),
        Method::DELETE,
        owner,
        Some(tenant_id),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "a truly unknown invitation id must still 404"
    );
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn create_invitation_reissue_after_expiry_succeeds() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-reissue@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let (status, body) =
        create_invitation(&pool, owner, tenant_id, "reissue@example.com", "agent").await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "first invite should succeed, got {body:?}"
    );
    let old_id = Uuid::parse_str(body["invitation"]["id"].as_str().unwrap()).unwrap();

    // Simulate the 7-day expiry window passing. There is no distinct
    // "expired" status stored — the row is still `status = 'pending'`.
    sqlx::query(
        "UPDATE tenant_invitations SET expires_at = now() - interval '1 hour' WHERE id = $1",
    )
    .bind(old_id)
    .execute(&pool)
    .await
    .unwrap();

    let (status, body) =
        create_invitation(&pool, owner, tenant_id, "reissue@example.com", "agent").await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "reissuing after the old invitation expired should succeed, got {body:?}"
    );
    let new_id = Uuid::parse_str(body["invitation"]["id"].as_str().unwrap()).unwrap();
    assert_ne!(old_id, new_id);

    let old_status: String =
        sqlx::query_scalar("SELECT status FROM tenant_invitations WHERE id = $1")
            .bind(old_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        old_status, "expired",
        "automatic expiry must not be persisted as an admin revocation"
    );

    let new_status: String =
        sqlx::query_scalar("SELECT status FROM tenant_invitations WHERE id = $1")
            .bind(new_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(new_status, "pending");

    let expired_response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/members/invitations?status=expired",
            Method::GET,
            owner,
            Some(tenant_id),
        ),
    )
    .await;
    assert_eq!(expired_response.status(), StatusCode::OK);
    let expired_body = body_json(expired_response).await;
    assert_eq!(expired_body["items"].as_array().unwrap().len(), 1);
    assert_eq!(expired_body["items"][0]["id"], old_id.to_string());
    assert_eq!(expired_body["items"][0]["status"], "expired");

    let audit_counts: (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE action = 'member.invited'), \
                COUNT(*) FILTER (WHERE action = 'member.invitation_revoked') \
         FROM audit_logs WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_counts, (2, 0));
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn create_invitation_still_blocks_active_unexpired_duplicate() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-still-blocks@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let (status, _body) = create_invitation(
        &pool,
        owner,
        tenant_id,
        "still-pending@example.com",
        "agent",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Not expired — must still block.
    let (status, body) = create_invitation(
        &pool,
        owner,
        tenant_id,
        "still-pending@example.com",
        "agent",
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "an active unexpired pending invitation must still block reissue, got {body:?}"
    );
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn create_invitation_reissue_after_expiry_concurrent_race_returns_one_success() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-reissue-race@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let (status, body) =
        create_invitation(&pool, owner, tenant_id, "reissue-race@example.com", "agent").await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "seed invite should succeed, got {body:?}"
    );
    let old_id = Uuid::parse_str(body["invitation"]["id"].as_str().unwrap()).unwrap();
    sqlx::query(
        "UPDATE tenant_invitations SET expires_at = now() - interval '1 hour' WHERE id = $1",
    )
    .bind(old_id)
    .execute(&pool)
    .await
    .unwrap();

    let req_a = tokio::spawn({
        let pool = pool.clone();
        async move {
            create_invitation(&pool, owner, tenant_id, "reissue-race@example.com", "agent").await
        }
    });
    let req_b = tokio::spawn({
        let pool = pool.clone();
        async move {
            create_invitation(&pool, owner, tenant_id, "reissue-race@example.com", "agent").await
        }
    });

    let (res_a, res_b) = tokio::join!(req_a, req_b);
    let statuses = [res_a.unwrap().0, res_b.unwrap().0];
    assert!(
        statuses.contains(&StatusCode::CREATED),
        "expected exactly one success, got {statuses:?}"
    );
    assert!(
        statuses.contains(&StatusCode::CONFLICT),
        "expected exactly one conflict, got {statuses:?}"
    );

    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, status FROM tenant_invitations \
         WHERE tenant_id = $1 AND email = $2 ORDER BY created_at, id",
    )
    .bind(tenant_id)
    .bind("reissue-race@example.com")
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        rows.len(),
        2,
        "only the winning request may persist a replacement"
    );
    assert!(rows.contains(&(old_id, "expired".to_string())));
    assert_eq!(
        rows.iter()
            .filter(|(_, status)| status == "pending")
            .count(),
        1
    );

    let audit_counts: (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE action = 'member.invited'), \
                COUNT(*) FILTER (WHERE action = 'member.invitation_revoked') \
         FROM audit_logs WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        audit_counts,
        (2, 0),
        "the original and winning replacement each need one create audit; automatic expiry is not revocation"
    );
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn create_invitation_equal_rank_succeeds_by_design() {
    // contracts/permissions.md rule 2 ("assign-at-or-below"): an Admin actor
    // may invite another Admin-rank invitation. This is intentionally NOT
    // the strict-below "manage" rule (rule 1) — do not regress this to
    // strict-below.
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let admin = seed_user(&pool, "Admin", "admin-equal-invite@example.com").await;
    seed_membership(&pool, tenant_id, admin, "admin", "active").await;

    let (status, body) = create_invitation(
        &pool,
        admin,
        tenant_id,
        "equal-rank-invite@example.com",
        "admin",
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "admin inviting admin-rank should succeed by design (assign-at-or-below), got {body:?}"
    );
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn revoke_invitation_equal_rank_succeeds_by_design() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let admin = seed_user(&pool, "Admin", "admin-equal-revoke@example.com").await;
    seed_membership(&pool, tenant_id, admin, "admin", "active").await;

    let invitation_id = seed_invitation(
        &pool,
        tenant_id,
        "equal-rank-revoke@example.com",
        "admin",
        admin,
    )
    .await;

    let request = authenticated_request(
        &format!("/api/v1/tenant/members/invitations/{invitation_id}"),
        Method::DELETE,
        admin,
        Some(tenant_id),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "admin revoking an admin-rank invitation should succeed by design (assign-at-or-below)"
    );
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn revoke_invitation_lower_rank_actor_refused() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let manager = seed_user(&pool, "Manager", "manager-revoke-lower@example.com").await;
    seed_membership(&pool, tenant_id, manager, "manager", "active").await;
    let owner = seed_user(&pool, "Owner", "owner-revoke-lower@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let invitation_id = seed_invitation(
        &pool,
        tenant_id,
        "lower-rank-revoke@example.com",
        "admin",
        owner,
    )
    .await;

    let request = authenticated_request(
        &format!("/api/v1/tenant/members/invitations/{invitation_id}"),
        Method::DELETE,
        manager,
        Some(tenant_id),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let status: String = sqlx::query_scalar("SELECT status FROM tenant_invitations WHERE id = $1")
        .bind(invitation_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "pending", "refused revoke must not mutate the row");
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn patch_member_cross_tenant_id_404_no_mutation_no_audit() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let owner_a = seed_user(&pool, "Owner A", "owner-cta@example.com").await;
    seed_membership(&pool, tenant_a, owner_a, "owner", "active").await;
    let target_b = seed_user(&pool, "Target B", "target-ctb@example.com").await;
    seed_membership(&pool, tenant_b, target_b, "agent", "active").await;

    let target_b_membership: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_b)
    .bind(target_b)
    .fetch_one(&pool)
    .await
    .unwrap();

    // owner_a acts with X-Tenant-ID = tenant_a but targets a membership id
    // that actually belongs to tenant_b.
    let request = json_request(
        &format!("/api/v1/tenant/members/{target_b_membership}"),
        Method::PATCH,
        owner_a,
        Some(tenant_a),
        serde_json::json!({ "status": "disabled" }),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let stored_status: String =
        sqlx::query_scalar("SELECT status FROM tenant_memberships WHERE id = $1")
            .bind(target_b_membership)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        stored_status, "active",
        "cross-tenant PATCH must not mutate the foreign row"
    );

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE resource_id = $1 \
         AND action IN ('member.role_changed','member.disabled','member.enabled')",
    )
    .bind(target_b_membership.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        audit_count, 0,
        "cross-tenant refusal must write no audit row"
    );
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn revoke_invitation_cross_tenant_id_404_no_mutation_no_audit() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let owner_a = seed_user(&pool, "Owner A", "owner-cti@example.com").await;
    seed_membership(&pool, tenant_a, owner_a, "owner", "active").await;
    let owner_b = seed_user(&pool, "Owner B", "owner-ctib@example.com").await;
    seed_membership(&pool, tenant_b, owner_b, "owner", "active").await;

    let invitation_b = seed_invitation(
        &pool,
        tenant_b,
        "cross-tenant-invite@example.com",
        "agent",
        owner_b,
    )
    .await;

    let request = authenticated_request(
        &format!("/api/v1/tenant/members/invitations/{invitation_b}"),
        Method::DELETE,
        owner_a,
        Some(tenant_a),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let status: String = sqlx::query_scalar("SELECT status FROM tenant_invitations WHERE id = $1")
        .bind(invitation_b)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        status, "pending",
        "cross-tenant DELETE must not mutate the foreign invitation"
    );

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE resource_id = $1 AND action = 'member.invitation_revoked'",
    )
    .bind(invitation_b.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        audit_count, 0,
        "cross-tenant refusal must write no audit row"
    );
}

async fn suspend_tenant(pool: &sqlx::PgPool, tenant_id: Uuid) {
    sqlx::query("UPDATE tenants SET status = 'suspended' WHERE id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn soft_delete_tenant(pool: &sqlx::PgPool, tenant_id: Uuid) {
    sqlx::query("UPDATE tenants SET deleted_at = now() WHERE id = $1")
        .bind(tenant_id)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn preview_invitation_suspended_tenant_404() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-sus-preview@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    let (_inv_id, raw_token) =
        seed_invitation_with_token(&pool, tenant_id, "sus-preview@example.com", "agent", owner)
            .await;

    suspend_tenant(&pool, tenant_id).await;

    let request = Request::get(format!("/api/v1/invitations/{raw_token}"))
        .body(Body::empty())
        .unwrap();
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn preview_invitation_soft_deleted_tenant_404() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-del-preview@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    let (_inv_id, raw_token) =
        seed_invitation_with_token(&pool, tenant_id, "del-preview@example.com", "agent", owner)
            .await;

    soft_delete_tenant(&pool, tenant_id).await;

    let request = Request::get(format!("/api/v1/invitations/{raw_token}"))
        .body(Body::empty())
        .unwrap();
    let response = send(pool.clone(), request).await;
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "a soft-deleted tenant's invitation must be indistinguishable from unknown (404)"
    );
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn accept_invitation_suspended_tenant_404_no_mutation() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-sus-accept@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    let (inv_id, raw_token) =
        seed_invitation_with_token(&pool, tenant_id, "sus-accept@example.com", "agent", owner)
            .await;

    suspend_tenant(&pool, tenant_id).await;

    let request = Request::post(format!("/api/v1/invitations/{raw_token}/accept"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "displayName": "Should Not Work",
                "password": "securePassword123!"
            }))
            .unwrap(),
        ))
        .unwrap();
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let status: String = sqlx::query_scalar("SELECT status FROM tenant_invitations WHERE id = $1")
        .bind(inv_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        status, "pending",
        "refused acceptance must not mutate the invitation"
    );

    let user_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users WHERE email = 'sus-accept@example.com')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!user_exists, "no account should be created on refusal");
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn accept_invitation_suspended_tenant_race_404_no_mutation() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-sus-race@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    let (inv_id, raw_token) =
        seed_invitation_with_token(&pool, tenant_id, "sus-race@example.com", "agent", owner).await;

    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE tenants SET status = 'suspended' WHERE id = $1")
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .unwrap();

    let request = Request::post(format!("/api/v1/invitations/{raw_token}/accept"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "displayName": "Should Not Work",
                "password": "securePassword123!"
            }))
            .unwrap(),
        ))
        .unwrap();
    let response_task = tokio::spawn({
        let pool = pool.clone();
        async move { send(pool, request).await }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    tx.commit().await.unwrap();

    let response = response_task.await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let status: String = sqlx::query_scalar("SELECT status FROM tenant_invitations WHERE id = $1")
        .bind(inv_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "pending");

    let user_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users WHERE email = 'sus-race@example.com')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!user_exists, "no account should be created on refusal");
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn accept_invitation_soft_deleted_tenant_404_no_mutation() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-del-accept@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    let (inv_id, raw_token) =
        seed_invitation_with_token(&pool, tenant_id, "del-accept@example.com", "agent", owner)
            .await;

    soft_delete_tenant(&pool, tenant_id).await;

    let request = Request::post(format!("/api/v1/invitations/{raw_token}/accept"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "displayName": "Should Not Work",
                "password": "securePassword123!"
            }))
            .unwrap(),
        ))
        .unwrap();
    let response = send(pool.clone(), request).await;
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "a soft-deleted tenant's invitation must be indistinguishable from unknown (404)"
    );

    let status: String = sqlx::query_scalar("SELECT status FROM tenant_invitations WHERE id = $1")
        .bind(inv_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        status, "pending",
        "refused acceptance must not mutate the invitation"
    );
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn accept_invitation_soft_deleted_tenant_race_404_no_mutation() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-del-race@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    let (inv_id, raw_token) =
        seed_invitation_with_token(&pool, tenant_id, "del-race@example.com", "agent", owner).await;

    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE tenants SET deleted_at = now() WHERE id = $1")
        .bind(tenant_id)
        .execute(&mut *tx)
        .await
        .unwrap();

    let request = Request::post(format!("/api/v1/invitations/{raw_token}/accept"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "displayName": "Should Not Work",
                "password": "securePassword123!"
            }))
            .unwrap(),
        ))
        .unwrap();
    let response_task = tokio::spawn({
        let pool = pool.clone();
        async move { send(pool, request).await }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    tx.commit().await.unwrap();

    let response = response_task.await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "a soft-deleted tenant's invitation must be indistinguishable from unknown (404)"
    );

    let status: String = sqlx::query_scalar("SELECT status FROM tenant_invitations WHERE id = $1")
        .bind(inv_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "pending");

    let user_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users WHERE email = 'del-race@example.com')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!user_exists, "no account should be created on refusal");
}

// ---------------------------------------------------------------------------
// Public invitation acceptance (T030)
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn accept_invitation_anonymous() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let raw_token = hex::encode(bytes);
    let token_hash = hex::encode(sha2::Sha256::digest(bytes));
    let email = format!("anon-{}@example.com", Uuid::new_v4().simple());
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

    let inv_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_invitations (tenant_id, email, role, token_hash, invited_by, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(tenant_id)
    .bind(&email)
    .bind("agent")
    .bind(&token_hash)
    .bind(owner)
    .bind(expires_at)
    .fetch_one(&pool)
    .await
    .unwrap();

    let request = Request::post(format!("/api/v1/invitations/{raw_token}/accept"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "displayName": "New Anon User",
                "password": "securePassword123!"
            }))
            .unwrap(),
        ))
        .unwrap();
    let response = send(pool.clone(), request).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "expected 200 for anonymous accept"
    );

    let set_cookie = response.headers().get("set-cookie").cloned();
    assert!(
        set_cookie.is_some(),
        "anonymous accept must set session cookie"
    );

    let body = body_json(response).await;
    assert_eq!(body["email"], email);
    assert_eq!(body["displayName"], "New Anon User");
    // Accept returns the canonical MeResponse shape (rest-api.md: "same shape
    // as POST /auth/login"), not a flat {role} — the role lives on the
    // membership entry for the accepted tenant.
    assert_eq!(body["memberships"][0]["role"], "agent");

    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(&email)
        .fetch_one(&pool)
        .await
        .unwrap();
    let audited_email = email.clone();
    assert_audit_event(
        &pool,
        "member.invitation_accepted",
        user_id,
        tenant_id,
        "invitation",
        inv_id,
        serde_json::json!({"email": audited_email, "role": "agent", "user_id": user_id}),
    )
    .await;

    let membership_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM tenant_memberships WHERE tenant_id = $1 \
         AND user_id = (SELECT id FROM users WHERE email = $2))",
    )
    .bind(tenant_id)
    .bind(&email)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(membership_exists, "membership should exist");
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn accept_invitation_anonymous_existing_account_returns_409_without_mutation() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-existing-account@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let email = format!("existing-account-{}@example.com", Uuid::new_v4().simple());
    let _existing_account = seed_user(&pool, "Existing Account", &email).await;

    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let raw_token = hex::encode(bytes);
    let token_hash = hex::encode(sha2::Sha256::digest(bytes));
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

    let inv_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_invitations (tenant_id, email, role, token_hash, invited_by, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(tenant_id)
    .bind(&email)
    .bind("agent")
    .bind(&token_hash)
    .bind(owner)
    .bind(expires_at)
    .fetch_one(&pool)
    .await
    .unwrap();

    let request = Request::post(format!("/api/v1/invitations/{raw_token}/accept"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "displayName": "New Account",
                "password": "securePassword123!"
            }))
            .unwrap(),
        ))
        .unwrap();

    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "conflict");

    let invitation_status: String =
        sqlx::query_scalar("SELECT status FROM tenant_invitations WHERE id = $1")
            .bind(inv_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(invitation_status, "pending");

    let membership_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tenant_memberships WHERE tenant_id = $1 AND user_id = (SELECT id FROM users WHERE email = $2)",
    )
    .bind(tenant_id)
    .bind(&email)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(membership_count, 0);

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE action = 'member.invitation_accepted' AND resource_id = $1",
    )
    .bind(inv_id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 0);
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn accept_invitation_signed_in() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let user = seed_user(&pool, "Invited User", "signedin@example.com").await;

    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let raw_token = hex::encode(bytes);
    let token_hash = hex::encode(sha2::Sha256::digest(bytes));
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

    sqlx::query(
        "INSERT INTO tenant_invitations (tenant_id, email, role, token_hash, invited_by, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(tenant_id)
    .bind("signedin@example.com")
    .bind("manager")
    .bind(&token_hash)
    .bind(owner)
    .bind(expires_at)
    .execute(&pool)
    .await
    .unwrap();

    let request = Request::post(format!("/api/v1/invitations/{raw_token}/accept"))
        .header("X-Dev-User-Id", user.to_string())
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({})).unwrap(),
        ))
        .unwrap();
    let response = send(pool.clone(), request).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "expected 200 for signed-in accept"
    );

    let body = body_json(response).await;
    // Accept returns the canonical MeResponse shape, not a flat {role}.
    assert_eq!(body["memberships"][0]["role"], "manager");

    let membership_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2)",
    )
    .bind(tenant_id)
    .bind(user)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(membership_exists);
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn accept_expired_token_410() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let raw_token = hex::encode(bytes);
    let token_hash = hex::encode(sha2::Sha256::digest(bytes));
    let expires_at = chrono::Utc::now() - chrono::Duration::hours(1);

    sqlx::query(
        "INSERT INTO tenant_invitations (tenant_id, email, role, token_hash, invited_by, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(tenant_id)
    .bind("expired@example.com")
    .bind("agent")
    .bind(&token_hash)
    .bind(owner)
    .bind(expires_at)
    .execute(&pool)
    .await
    .unwrap();

    let request = Request::post(format!("/api/v1/invitations/{raw_token}/accept"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "displayName": "Late User",
                "password": "password123"
            }))
            .unwrap(),
        ))
        .unwrap();
    let response = send(pool.clone(), request).await;
    assert_eq!(
        response.status(),
        StatusCode::GONE,
        "expired token should be 410"
    );
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn preview_derived_and_persisted_expired_tokens_return_410() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-expired-preview@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    for (email, persisted) in [
        ("derived-expired-preview@example.com", false),
        ("persisted-expired-preview@example.com", true),
    ] {
        let (invitation_id, token) =
            seed_invitation_with_token(&pool, tenant_id, email, "agent", owner).await;
        sqlx::query(
            "UPDATE tenant_invitations SET expires_at = now() - interval '1 hour', \
             status = CASE WHEN $2 THEN 'expired' ELSE status END WHERE id = $1",
        )
        .bind(invitation_id)
        .bind(persisted)
        .execute(&pool)
        .await
        .unwrap();

        let response = send(
            pool.clone(),
            Request::get(format!("/api/v1/invitations/{token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::GONE, "persisted={persisted}");

        let response = send(
            pool.clone(),
            Request::post(format!("/api/v1/invitations/{token}/accept"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "displayName": "Expired Invitee",
                        "password": "securePassword123!"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::GONE, "persisted={persisted}");
    }
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn accept_unknown_token_404() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let request = Request::post("/api/v1/invitations/abcdef1234567890abcdef1234567890/accept")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "displayName": "Ghost",
                "password": "password123"
            }))
            .unwrap(),
        ))
        .unwrap();
    let response = send(pool.clone(), request).await;
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "unknown token should be 404"
    );
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn accept_email_mismatch_403() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let user = seed_user(&pool, "Wrong User", "wrong@example.com").await;

    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let raw_token = hex::encode(bytes);
    let token_hash = hex::encode(sha2::Sha256::digest(bytes));
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

    sqlx::query(
        "INSERT INTO tenant_invitations (tenant_id, email, role, token_hash, invited_by, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(tenant_id)
    .bind("invited@example.com")
    .bind("agent")
    .bind(&token_hash)
    .bind(owner)
    .bind(expires_at)
    .execute(&pool)
    .await
    .unwrap();

    let request = Request::post(format!("/api/v1/invitations/{raw_token}/accept"))
        .header("X-Dev-User-Id", user.to_string())
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({})).unwrap(),
        ))
        .unwrap();
    let response = send(pool.clone(), request).await;
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "email mismatch should be 403"
    );
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn accept_consumed_token_410() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let raw_token = hex::encode(bytes);
    let token_hash = hex::encode(sha2::Sha256::digest(bytes));
    let email = format!("consumed-{}@example.com", Uuid::new_v4().simple());
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

    sqlx::query(
        "INSERT INTO tenant_invitations (tenant_id, email, role, token_hash, invited_by, expires_at, status, accepted_at, accepted_user_id) \
         VALUES ($1, $2, $3, $4, $5, $6, 'accepted', now(), $5)",
    )
    .bind(tenant_id)
    .bind(&email)
    .bind("agent")
    .bind(&token_hash)
    .bind(owner)
    .bind(expires_at)
    .execute(&pool)
    .await
    .unwrap();

    let request = Request::post(format!("/api/v1/invitations/{raw_token}/accept"))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "displayName": "Latecomer",
                "password": "password123"
            }))
            .unwrap(),
        ))
        .unwrap();
    let response = send(pool.clone(), request).await;
    assert_eq!(
        response.status(),
        StatusCode::GONE,
        "consumed token should be 410"
    );
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn accept_disabled_member_409() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;

    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let raw_token = hex::encode(bytes);
    let token_hash = hex::encode(sha2::Sha256::digest(bytes));
    let email = "disabledmember@example.com";
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);

    let owner = seed_user(&pool, "Owner", "owner@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    let disabled_user = seed_user(&pool, "Disabled Member", email).await;
    seed_membership(&pool, tenant_id, disabled_user, "agent", "disabled").await;

    sqlx::query(
        "INSERT INTO tenant_invitations (tenant_id, email, role, token_hash, invited_by, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(tenant_id)
    .bind(email)
    .bind("agent")
    .bind(&token_hash)
    .bind(owner)
    .bind(expires_at)
    .execute(&pool)
    .await
    .unwrap();

    let request = Request::post(format!("/api/v1/invitations/{raw_token}/accept"))
        .header("X-Dev-User-Id", disabled_user.to_string())
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({})).unwrap(),
        ))
        .unwrap();
    let response = send(pool.clone(), request).await;
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "disabled member should be 409"
    );
}

// ---------------------------------------------------------------------------
// US3 — Member role change (T047)
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn role_change_success() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-rcs@example.com").await;
    let target = seed_user(&pool, "Target", "target-rcs@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    seed_membership(&pool, tenant_id, target, "manager", "active").await;

    let target_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(target)
    .fetch_one(&pool)
    .await
    .unwrap();

    let request = json_request(
        &format!("/api/v1/tenant/members/{target_id}"),
        Method::PATCH,
        owner,
        Some(tenant_id),
        serde_json::json!({ "role": "admin" }),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "role change should succeed"
    );
    let body = body_json(response).await;
    assert_eq!(body["role"], "admin");
    assert_eq!(body["userId"], target.to_string());

    let stored_role: String =
        sqlx::query_scalar("SELECT role FROM tenant_memberships WHERE id = $1")
            .bind(target_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored_role, "admin");

    assert_audit_event(
        &pool,
        "member.role_changed",
        owner,
        tenant_id,
        "membership",
        target_id,
        serde_json::json!({"previous_role": "manager", "new_role": "admin"}),
    )
    .await;
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn role_change_hierarchy_refused() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let manager = seed_user(&pool, "Manager", "manager-hr@example.com").await;
    let admin = seed_user(&pool, "Admin", "admin-hr@example.com").await;
    seed_membership(&pool, tenant_id, manager, "manager", "active").await;
    seed_membership(&pool, tenant_id, admin, "admin", "active").await;

    let target_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(admin)
    .fetch_one(&pool)
    .await
    .unwrap();

    let request = json_request(
        &format!("/api/v1/tenant/members/{target_id}"),
        Method::PATCH,
        manager,
        Some(tenant_id),
        serde_json::json!({ "role": "viewer" }),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE action = 'member.role_changed' AND resource_id = $1",
    )
    .bind(target_id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 0);
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn role_change_non_owner_assigning_owner_refused() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let admin = seed_user(&pool, "Admin", "admin-noa@example.com").await;
    let target = seed_user(&pool, "Target", "target-noa@example.com").await;
    seed_membership(&pool, tenant_id, admin, "admin", "active").await;
    seed_membership(&pool, tenant_id, target, "viewer", "active").await;

    let target_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(target)
    .fetch_one(&pool)
    .await
    .unwrap();

    let request = json_request(
        &format!("/api/v1/tenant/members/{target_id}"),
        Method::PATCH,
        admin,
        Some(tenant_id),
        serde_json::json!({ "role": "owner" }),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn role_change_last_owner_demotion_refused() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let target = seed_user(&pool, "Only Owner", "only-owner@example.com").await;
    seed_membership(&pool, tenant_id, target, "owner", "active").await;

    let platform = seed_user(&pool, "Platform Admin", "platform-lodr@example.com").await;
    sqlx::query("UPDATE users SET platform_role = 'super_admin' WHERE id = $1")
        .bind(platform)
        .execute(&pool)
        .await
        .unwrap();

    let membership_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(target)
    .fetch_one(&pool)
    .await
    .unwrap();

    let request = json_request(
        &format!("/api/v1/tenant/members/{membership_id}"),
        Method::PATCH,
        platform,
        Some(tenant_id),
        serde_json::json!({ "role": "admin" }),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn role_change_self_refused() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-self@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let membership_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(owner)
    .fetch_one(&pool)
    .await
    .unwrap();

    let request = json_request(
        &format!("/api/v1/tenant/members/{membership_id}"),
        Method::PATCH,
        owner,
        Some(tenant_id),
        serde_json::json!({ "role": "admin" }),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn role_change_immediate_effect() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-ie@example.com").await;
    // Start as manager (has members.manage) so the pre-demotion check below
    // is meaningful; agent never has members.manage in the first place.
    let agent = seed_user(&pool, "Agent", "agent-ie@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    seed_membership(&pool, tenant_id, agent, "manager", "active").await;

    let agent_can_manage = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/test/tenant/members/manage",
            Method::GET,
            agent,
            Some(tenant_id),
        ),
    )
    .await;
    assert_eq!(
        agent_can_manage.status(),
        StatusCode::OK,
        "manager should be able to manage members before demotion"
    );

    let membership_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(agent)
    .fetch_one(&pool)
    .await
    .unwrap();

    let request = json_request(
        &format!("/api/v1/tenant/members/{membership_id}"),
        Method::PATCH,
        owner,
        Some(tenant_id),
        serde_json::json!({ "role": "viewer" }),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::OK);

    let agent_denied = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/test/tenant/members/manage",
            Method::GET,
            agent,
            Some(tenant_id),
        ),
    )
    .await;
    assert_eq!(
        agent_denied.status(),
        StatusCode::FORBIDDEN,
        "demoted viewer should be denied"
    );
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn role_change_uses_active_tenant_membership() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let actor = seed_user(&pool, "Actor", "actor-active-tenant@example.com").await;
    let target = seed_user(&pool, "Target", "target-active-tenant@example.com").await;
    seed_membership(&pool, tenant_a, actor, "owner", "active").await;
    seed_membership(&pool, tenant_b, actor, "viewer", "active").await;
    seed_membership(&pool, tenant_b, target, "agent", "active").await;

    let target_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_b)
    .bind(target)
    .fetch_one(&pool)
    .await
    .unwrap();

    let request = json_request(
        &format!("/api/v1/tenant/members/{target_id}"),
        Method::PATCH,
        actor,
        Some(tenant_b),
        serde_json::json!({ "role": "viewer" }),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "active tenant membership should drive management permissions"
    );

    let stored_role: String =
        sqlx::query_scalar("SELECT role FROM tenant_memberships WHERE id = $1")
            .bind(target_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored_role, "agent");

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE resource_id = $1 AND action = 'member.role_changed'",
    )
    .bind(target_id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 0);
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn role_change_concurrent_demotions() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner_a = seed_user(&pool, "Owner A", "owner-a@example.com").await;
    let owner_b = seed_user(&pool, "Owner B", "owner-b@example.com").await;
    seed_membership(&pool, tenant_id, owner_a, "owner", "active").await;
    seed_membership(&pool, tenant_id, owner_b, "owner", "active").await;

    let membership_a: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(owner_a)
    .fetch_one(&pool)
    .await
    .unwrap();
    let membership_b: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(owner_b)
    .fetch_one(&pool)
    .await
    .unwrap();

    let platform = seed_user(&pool, "Platform", "platform-cd@example.com").await;
    sqlx::query("UPDATE users SET platform_role = 'super_admin' WHERE id = $1")
        .bind(platform)
        .execute(&pool)
        .await
        .unwrap();

    let req_a = json_request(
        &format!("/api/v1/tenant/members/{membership_a}"),
        Method::PATCH,
        platform,
        Some(tenant_id),
        serde_json::json!({ "role": "admin" }),
    );
    let req_b = json_request(
        &format!("/api/v1/tenant/members/{membership_b}"),
        Method::PATCH,
        platform,
        Some(tenant_id),
        serde_json::json!({ "role": "admin" }),
    );

    let (res_a, res_b) = tokio::join!(send(pool.clone(), req_a), send(pool.clone(), req_b));
    let ok_count = [res_a.status(), res_b.status()]
        .iter()
        .filter(|s| **s == StatusCode::OK)
        .count();
    let conflict_count = [res_a.status(), res_b.status()]
        .iter()
        .filter(|s| **s == StatusCode::CONFLICT)
        .count();

    assert_eq!(ok_count, 1, "exactly one concurrent demotion must succeed");
    assert_eq!(
        conflict_count, 1,
        "exactly one concurrent demotion must conflict"
    );
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn last_owner_changes_remain_conflicted_under_concurrency() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner_a = seed_user(&pool, "Owner A", "owner-a-lock@example.com").await;
    let owner_b = seed_user(&pool, "Owner B", "owner-b-lock@example.com").await;
    seed_membership(&pool, tenant_id, owner_a, "owner", "active").await;
    seed_membership(&pool, tenant_id, owner_b, "owner", "active").await;

    let membership_a: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(owner_a)
    .fetch_one(&pool)
    .await
    .unwrap();
    let membership_b: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(owner_b)
    .fetch_one(&pool)
    .await
    .unwrap();

    let platform = seed_user(&pool, "Platform", "platform-last-owner-lock@example.com").await;
    sqlx::query("UPDATE users SET platform_role = 'super_admin' WHERE id = $1")
        .bind(platform)
        .execute(&pool)
        .await
        .unwrap();

    let req_a = json_request(
        &format!("/api/v1/tenant/members/{membership_a}"),
        Method::PATCH,
        platform,
        Some(tenant_id),
        serde_json::json!({ "role": "admin" }),
    );
    let req_b = json_request(
        &format!("/api/v1/tenant/members/{membership_b}"),
        Method::PATCH,
        platform,
        Some(tenant_id),
        serde_json::json!({ "status": "disabled" }),
    );

    let (res_a, res_b) = tokio::join!(send(pool.clone(), req_a), send(pool.clone(), req_b));
    let statuses = [res_a.status(), res_b.status()];
    let ok_count = statuses.iter().filter(|s| **s == StatusCode::OK).count();
    let conflict_count = statuses
        .iter()
        .filter(|s| **s == StatusCode::CONFLICT)
        .count();

    assert_eq!(
        ok_count, 1,
        "exactly one concurrent last-owner change must succeed"
    );
    assert_eq!(
        conflict_count, 1,
        "exactly one concurrent last-owner change must conflict"
    );
}

// ---------------------------------------------------------------------------
// US4 — Disable / re-enable members (T054)
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn disable_member() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-dm@example.com").await;
    let target = seed_user(&pool, "Target", "target-dm@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    seed_membership(&pool, tenant_id, target, "agent", "active").await;

    let target_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(target)
    .fetch_one(&pool)
    .await
    .unwrap();

    let request = json_request(
        &format!("/api/v1/tenant/members/{target_id}"),
        Method::PATCH,
        owner,
        Some(tenant_id),
        serde_json::json!({ "status": "disabled" }),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["status"], "disabled");
    assert_eq!(body["role"], "agent");

    let stored_status: String =
        sqlx::query_scalar("SELECT status FROM tenant_memberships WHERE id = $1")
            .bind(target_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored_status, "disabled");

    assert_audit_event(
        &pool,
        "member.disabled",
        owner,
        tenant_id,
        "membership",
        target_id,
        serde_json::json!({"role": "agent", "previous_status": "active", "new_status": "disabled"}),
    )
    .await;
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn disabled_member_immediate_access_loss() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-dmal@example.com").await;
    let target = seed_user(&pool, "Target", "target-dmal@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    seed_membership(&pool, tenant_id, target, "agent", "active").await;

    let target_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(target)
    .fetch_one(&pool)
    .await
    .unwrap();

    let request = json_request(
        &format!("/api/v1/tenant/members/{target_id}"),
        Method::PATCH,
        owner,
        Some(tenant_id),
        serde_json::json!({ "status": "disabled" }),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::OK);

    let target_request = authenticated_request(
        "/api/v1/tenant/members",
        Method::GET,
        target,
        Some(tenant_id),
    );
    let target_response = send(pool.clone(), target_request).await;
    assert_eq!(
        target_response.status(),
        StatusCode::FORBIDDEN,
        "disabled member's next request must be refused"
    );
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn re_enable_member() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-rem@example.com").await;
    let target = seed_user(&pool, "Target", "target-rem@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    seed_membership(&pool, tenant_id, target, "manager", "disabled").await;

    let target_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(target)
    .fetch_one(&pool)
    .await
    .unwrap();

    let request = json_request(
        &format!("/api/v1/tenant/members/{target_id}"),
        Method::PATCH,
        owner,
        Some(tenant_id),
        serde_json::json!({ "status": "active" }),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["status"], "active");
    assert_eq!(body["role"], "manager");

    assert_audit_event(
        &pool,
        "member.enabled",
        owner,
        tenant_id,
        "membership",
        target_id,
        serde_json::json!({"role": "manager", "previous_status": "disabled", "new_status": "active"}),
    )
    .await;
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn patch_member_same_role_conflict_no_audit() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-same-role@example.com").await;
    let target = seed_user(&pool, "Target", "target-same-role@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    seed_membership(&pool, tenant_id, target, "agent", "active").await;

    let target_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(target)
    .fetch_one(&pool)
    .await
    .unwrap();

    let request = json_request(
        &format!("/api/v1/tenant/members/{target_id}"),
        Method::PATCH,
        owner,
        Some(tenant_id),
        serde_json::json!({ "role": "agent" }),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE action = 'member.role_changed' AND resource_id = $1",
    )
    .bind(target_id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 0);
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn patch_member_validation_errors_return_422_without_mutation() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-patch-validation@example.com").await;
    let target = seed_user(&pool, "Target", "target-patch-validation@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;
    seed_membership(&pool, tenant_id, target, "agent", "active").await;

    let target_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(target)
    .fetch_one(&pool)
    .await
    .unwrap();

    let invalid_requests = [
        (serde_json::json!({}), None),
        (
            serde_json::json!({"role": "admin", "status": "disabled"}),
            None,
        ),
        (
            serde_json::json!({"role": "not-a-role"}),
            Some(("role", "invalid_value")),
        ),
        (
            serde_json::json!({"status": "not-a-status"}),
            Some(("status", "invalid_value")),
        ),
    ];

    for (body, expected_detail) in invalid_requests {
        let request = json_request(
            &format!("/api/v1/tenant/members/{target_id}"),
            Method::PATCH,
            owner,
            Some(tenant_id),
            body,
        );
        let response = send(pool.clone(), request).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = body_json(response).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        if let Some((field, code)) = expected_detail {
            let details = body["error"]["details"].as_array().unwrap();
            assert!(
                details
                    .iter()
                    .any(|detail| detail["field"] == field && detail["code"] == code),
                "expected validation detail for {field} with code {code}, got: {details:?}"
            );
        }
    }

    let stored: (String, String) =
        sqlx::query_as("SELECT role, status FROM tenant_memberships WHERE id = $1")
            .bind(target_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored.0, "agent");
    assert_eq!(stored.1, "active");

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE resource_id = $1 AND action IN ('member.role_changed','member.disabled','member.enabled')",
    )
    .bind(target_id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 0);
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn disable_last_owner_refused() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let target = seed_user(&pool, "Only Owner", "only-owner-dlo@example.com").await;
    seed_membership(&pool, tenant_id, target, "owner", "active").await;

    let platform = seed_user(&pool, "Platform Admin", "platform-dlo@example.com").await;
    sqlx::query("UPDATE users SET platform_role = 'super_admin' WHERE id = $1")
        .bind(platform)
        .execute(&pool)
        .await
        .unwrap();

    let membership_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(target)
    .fetch_one(&pool)
    .await
    .unwrap();

    let request = json_request(
        &format!("/api/v1/tenant/members/{membership_id}"),
        Method::PATCH,
        platform,
        Some(tenant_id),
        serde_json::json!({ "status": "disabled" }),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn self_disable_refused() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, "Owner", "owner-sd@example.com").await;
    seed_membership(&pool, tenant_id, owner, "owner", "active").await;

    let membership_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(owner)
    .fetch_one(&pool)
    .await
    .unwrap();

    let request = json_request(
        &format!("/api/v1/tenant/members/{membership_id}"),
        Method::PATCH,
        owner,
        Some(tenant_id),
        serde_json::json!({ "status": "disabled" }),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial_test::serial(team_members_db)]
async fn disable_in_tenant_a_does_not_affect_tenant_b() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let owner_a = seed_user(&pool, "Owner A", "owner-ta@example.com").await;
    let shared = seed_user(&pool, "Shared User", "shared@example.com").await;
    seed_membership(&pool, tenant_a, owner_a, "owner", "active").await;
    // admin (not agent) so the tenant-B access check below can exercise
    // members.view — agent never has roster visibility, disabled or not.
    seed_membership(&pool, tenant_a, shared, "admin", "active").await;
    seed_membership(&pool, tenant_b, shared, "admin", "active").await;

    let membership_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_a)
    .bind(shared)
    .fetch_one(&pool)
    .await
    .unwrap();

    let request = json_request(
        &format!("/api/v1/tenant/members/{membership_id}"),
        Method::PATCH,
        owner_a,
        Some(tenant_a),
        serde_json::json!({ "status": "disabled" }),
    );
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status(), StatusCode::OK);

    let shared_can_access_b = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/members",
            Method::GET,
            shared,
            Some(tenant_b),
        ),
    )
    .await;
    assert_eq!(
        shared_can_access_b.status(),
        StatusCode::OK,
        "member disabled in tenant A must still access tenant B"
    );
}
