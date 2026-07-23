use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use config::Environment;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use server::router;
use server::state::AppState;
use cache::Cache;
use notifications::worker::process_notification_outbox_once;

use notifications::emit::{self, NotificationRequest, dedupe_key_tool_approval, dedupe_key_ai_failed};
use notifications::model::{NotificationKind, SubjectType};

fn test_config(environment: Environment) -> config::AppConfig {
    config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: "redis://127.0.0.1:6379".into(),
        auth_jwt_secret: "test-auth-jwt-secret-at-least-32-bytes".into(),
        auth_session_ttl_seconds: 28_800,
        port: 0,
        bind_address: "0.0.0.0".into(),
        environment,
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

fn app_state(pool: sqlx::PgPool, environment: Environment) -> AppState {
    let cfg = test_config(environment);
    AppState {
        config: Arc::new(cfg.clone()),
        db: pool.clone(),
        cache: Arc::new(Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool, &cfg).unwrap(),
    }
}

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("skipping notification tests: DATABASE_URL not set");
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping notification tests: DATABASE_URL is unreachable");
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        return None;
    }
    Some(pool)
}

async fn send(pool: sqlx::PgPool, environment: Environment, request: Request<Body>) -> Response {
    router::app_with_test_routes(app_state(pool, environment))
        .oneshot(request)
        .await
        .expect("request should complete")
}

fn authenticated_request(
    uri: &str,
    method: Method,
    user_id: Uuid,
    tenant_id: Option<Uuid>,
    environment: Environment,
) -> Request<Body> {
    let mut builder = Request::builder().uri(uri).method(method);
    if environment == Environment::Production {
        let config = test_config(environment.clone());
        let (token, _, _) = identity::session::issue_token(
            &config.auth_jwt_secret,
            config.auth_session_ttl_seconds,
            user_id,
        )
        .unwrap();
        builder = builder.header("cookie", format!("app_session={token}"));
    } else {
        builder = builder.header("X-Dev-User-Id", user_id.to_string());
    }
    if let Some(tenant_id) = tenant_id {
        builder = builder.header("X-Tenant-ID", tenant_id.to_string());
    }
    builder.body(Body::empty()).unwrap()
}

async fn body_json(response: Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

// ── Seed helpers ────────────────────────────────────────────────────────────

async fn seed_user(pool: &sqlx::PgPool, platform_role: Option<&str>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name, platform_role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(format!("notif_{}@example.com", Uuid::new_v4()))
    .bind("Notification User")
    .bind(platform_role)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Notification Tenant")
        .bind(format!("notif-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_membership(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
    role: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_membership_with_status(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
    role: &str,
    status: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .bind(status)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_notification(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    recipient_membership_id: Uuid,
    kind: &str,
    state: &str,
    title: &str,
    subject_type: &str,
    subject_id: Uuid,
    actor_membership_id: Option<Uuid>,
    created_at: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO notifications \
         (tenant_id, recipient_membership_id, kind, state, title, body, subject_type, subject_id, \
          dedupe_key, actor_membership_id, created_at) \
         VALUES ($1, $2, $3, $4, $5, NULL, $6, $7, $8, $9, $10::timestamptz) RETURNING id",
    )
    .bind(tenant_id)
    .bind(recipient_membership_id)
    .bind(kind)
    .bind(state)
    .bind(title)
    .bind(subject_type)
    .bind(subject_id)
    .bind(format!("test-{}-{}", kind, Uuid::new_v4()))
    .bind(actor_membership_id)
    .bind(created_at)
    .fetch_one(pool)
    .await
    .unwrap()
}

struct SeededMembership {
    user_id: Uuid,
    membership_id: Uuid,
}

async fn seed_customer(pool: &sqlx::PgPool, tenant_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Notifications Test Customer")
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_conversation(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    customer_id: Uuid,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, last_activity_at) \
         VALUES ($1, $2, 'web_chat', 'open', now()) RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_actor(pool: &sqlx::PgPool, tenant_id: Uuid, email: &str, role: &str) -> SeededMembership {
    let unique_email = format!("{}-{}", email, Uuid::new_v4().simple());
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(&unique_email)
    .bind("Notifications Test User")
    .fetch_one(pool)
    .await
    .unwrap();
    let membership_id = seed_membership(pool, tenant_id, user_id, role).await;
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

fn setup_state(pool: sqlx::PgPool) -> AppState {
    app_state(pool, Environment::Test)
}

async fn send_state(state: &AppState, request: Request<Body>) -> Response {
    router::app_with_test_routes(state.clone())
        .oneshot(request)
        .await
        .expect("request should complete")
}

async fn drain_pending_notifications(state: &AppState) {
    while let Ok(true) = process_notification_outbox_once(&state.db, &state.escalations).await {}
}

// ── T019: Core inbox CRUD tests ─────────────────────────────────────────────

#[tokio::test]
async fn list_returns_newest_first() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, None).await;
    let mem_id = seed_membership(&pool, tenant_id, user_id, "admin").await;

    let subj_id = Uuid::new_v4();
    seed_notification(
        &pool, tenant_id, mem_id, "escalation.new", "unread", "Oldest",
        "escalation", subj_id, None, "2026-07-18T14:00:00Z",
    )
    .await;
    seed_notification(
        &pool, tenant_id, mem_id, "escalation.new", "unread", "Middle",
        "escalation", subj_id, None, "2026-07-18T15:00:00Z",
    )
    .await;
    seed_notification(
        &pool, tenant_id, mem_id, "escalation.new", "unread", "Newest",
        "escalation", subj_id, None, "2026-07-18T16:00:00Z",
    )
    .await;

    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/notifications",
            Method::GET,
            user_id,
            Some(tenant_id),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let titles: Vec<&str> = json["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, vec!["Newest", "Middle", "Oldest"]);
}

#[tokio::test]
async fn cursor_pagination_returns_all_pages() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, None).await;
    let mem_id = seed_membership(&pool, tenant_id, user_id, "admin").await;

    let subj_id = Uuid::new_v4();
    for i in 0..5 {
        seed_notification(
            &pool, tenant_id, mem_id, "escalation.new", "unread", &format!("Item {i}"),
            "escalation", subj_id, None,
            &format!("2026-07-18T{:02}:00:00Z", 14 + i),
        )
        .await;
    }

    let all_ids = {
        let response = send(
            pool.clone(),
            Environment::Test,
            authenticated_request(
                "/api/v1/tenant/notifications?limit=5",
                Method::GET,
                user_id,
                Some(tenant_id),
                Environment::Test,
            ),
        )
        .await;
        let json = body_json(response).await;
        json["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    };

    let mut collected = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let uri = match &cursor {
            Some(c) => format!("/api/v1/tenant/notifications?limit=2&cursor={c}"),
            None => "/api/v1/tenant/notifications?limit=2".to_string(),
        };
        let response = send(
            pool.clone(),
            Environment::Test,
            authenticated_request(&uri, Method::GET, user_id, Some(tenant_id), Environment::Test),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        let entries = json["data"].as_array().unwrap();
        for entry in entries {
            let id = entry["id"].as_str().unwrap();
            assert!(!collected.contains(&id.to_string()), "duplicate id: {id}");
            collected.push(id.to_string());
        }
        let has_more = json["pagination"]["has_more"].as_bool().unwrap();
        cursor = json["pagination"]["next_cursor"]
            .as_str()
            .map(|s| s.to_string());
        if !has_more {
            break;
        }
    }

    assert_eq!(collected.len(), 5);
    assert_eq!(collected, all_ids);
    assert!(cursor.is_none(), "last page must return next_cursor: null");
}

#[tokio::test]
async fn unread_count_counts_only_unread() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, None).await;
    let mem_id = seed_membership(&pool, tenant_id, user_id, "admin").await;

    let subj_id = Uuid::new_v4();
    seed_notification(
        &pool, tenant_id, mem_id, "escalation.new", "unread",
        "Unread 1", "escalation", subj_id, None, "2026-07-18T14:00:00Z",
    )
    .await;
    seed_notification(
        &pool, tenant_id, mem_id, "escalation.new", "unread",
        "Unread 2", "escalation", subj_id, None, "2026-07-18T14:01:00Z",
    )
    .await;
    seed_notification(
        &pool, tenant_id, mem_id, "escalation.new", "read",
        "Read 1", "escalation", subj_id, None, "2026-07-18T14:02:00Z",
    )
    .await;
    seed_notification(
        &pool, tenant_id, mem_id, "escalation.new", "resolved",
        "Resolved 1", "escalation", subj_id, None, "2026-07-18T14:03:00Z",
    )
    .await;

    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/notifications/unread-count",
            Method::GET,
            user_id,
            Some(tenant_id),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["count"].as_i64().unwrap(), 2);
}

#[tokio::test]
async fn mark_read_is_idempotent() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, None).await;
    let mem_id = seed_membership(&pool, tenant_id, user_id, "admin").await;

    let subj_id = Uuid::new_v4();
    let nid = seed_notification(
        &pool, tenant_id, mem_id, "escalation.new", "unread",
        "Test", "escalation", subj_id, None, "2026-07-18T14:00:00Z",
    )
    .await;

    // First mark
    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            &format!("/api/v1/tenant/notifications/{nid}/read"),
            Method::POST,
            user_id,
            Some(tenant_id),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["state"], "read");

    // Second mark — idempotent
    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            &format!("/api/v1/tenant/notifications/{nid}/read"),
            Method::POST,
            user_id,
            Some(tenant_id),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["state"], "read");
}

#[tokio::test]
async fn mark_all_read_returns_count_and_is_idempotent() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, None).await;
    let mem_id = seed_membership(&pool, tenant_id, user_id, "admin").await;

    let subj_id = Uuid::new_v4();
    for i in 0..3 {
        seed_notification(
            &pool, tenant_id, mem_id, "escalation.new", "unread",
            &format!("Unread {i}"), "escalation", subj_id, None,
            &format!("2026-07-18T{:02}:00:00Z", 14 + i),
        )
        .await;
    }

    // First mark-all
    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/notifications/read-all",
            Method::POST,
            user_id,
            Some(tenant_id),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["marked"].as_u64().unwrap(), 3);

    // Second mark-all — idempotent
    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/notifications/read-all",
            Method::POST,
            user_id,
            Some(tenant_id),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["marked"].as_u64().unwrap(), 0);
}

#[tokio::test]
async fn reading_another_members_notification_returns_404_not_403() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let user_a = seed_user(&pool, None).await;
    let user_b = seed_user(&pool, None).await;
    let mem_a = seed_membership(&pool, tenant_id, user_a, "admin").await;
    seed_membership(&pool, tenant_id, user_b, "agent").await;

    let subj_id = Uuid::new_v4();
    let nid = seed_notification(
        &pool, tenant_id, mem_a, "escalation.new", "unread",
        "Secret", "escalation", subj_id, None, "2026-07-18T14:00:00Z",
    )
    .await;

    // User B tries to read user A's notification
    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            &format!("/api/v1/tenant/notifications/{nid}/read"),
            Method::POST,
            user_b,
            Some(tenant_id),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn tenant_isolation() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, None).await;
    let mem_a = seed_membership(&pool, tenant_a, user_id, "admin").await;
    seed_membership(&pool, tenant_b, user_id, "admin").await;

    let subj_id = Uuid::new_v4();
    seed_notification(
        &pool, tenant_a, mem_a, "escalation.new", "unread",
        "Only in A", "escalation", subj_id, None, "2026-07-18T14:00:00Z",
    )
    .await;

    // Query tenant B — should see zero
    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/notifications",
            Method::GET,
            user_id,
            Some(tenant_b),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["data"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn deactivated_membership_cannot_read_inbox() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, None).await;
    // status = 'disabled' is not 'active'
    seed_membership_with_status(&pool, tenant_id, user_id, "agent", "disabled").await;

    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/notifications",
            Method::GET,
            user_id,
            Some(tenant_id),
            Environment::Test,
        ),
    )
    .await;
    // The tenant-context middleware returns 403 when no active membership is found,
    // before the request reaches the notification handler.
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// ── T020: Role-access test (FR-012a) ────────────────────────────────────────

#[tokio::test]
async fn all_five_tenant_roles_can_access_inbox() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;

    for role in ["owner", "admin", "manager", "agent", "viewer"] {
        let user_id = seed_user(&pool, None).await;
        let mem_id = seed_membership(&pool, tenant_id, user_id, role).await;

        // Seed one notification so the list is non-empty
        seed_notification(
            &pool, tenant_id, mem_id, "escalation.new", "unread",
            &format!("{role} notif"), "escalation", Uuid::new_v4(), None,
            "2026-07-18T14:00:00Z",
        )
        .await;

        // GET /tenant/notifications
        let response = send(
            pool.clone(),
            Environment::Test,
            authenticated_request(
                "/api/v1/tenant/notifications",
                Method::GET,
                user_id,
                Some(tenant_id),
                Environment::Test,
            ),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "role {role} expected 200 from list, got {}",
            response.status()
        );

        // GET /tenant/notifications/unread-count
        let response = send(
            pool.clone(),
            Environment::Test,
            authenticated_request(
                "/api/v1/tenant/notifications/unread-count",
                Method::GET,
                user_id,
                Some(tenant_id),
                Environment::Test,
            ),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "role {role} expected 200 from unread-count, got {}",
            response.status()
        );
    }
}

// ── T021: Performance test (SC-004) ─────────────────────────────────────────

#[tokio::test]
#[ignore]
async fn list_under_one_second_with_one_thousand_notifications() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, None).await;
    let mem_id = seed_membership(&pool, tenant_id, user_id, "admin").await;

    // Bulk insert 1,000 notifications
    let batch_size = 500;
    let total = 1_000;
    let mut inserted = 0;
    while inserted < total {
        let mut builder = sqlx::QueryBuilder::new(
            "INSERT INTO notifications \
             (tenant_id, recipient_membership_id, kind, state, title, body, subject_type, subject_id, \
              dedupe_key, actor_membership_id, created_at) ",
        );
        builder.push_values(0..batch_size.min(total - inserted), |mut b, i| {
            let ts = format!("2026-07-18T{:02}:{:02}:{:02}Z", 0, 0, inserted + i);
            b.push_bind(tenant_id)
                .push_bind(mem_id)
                .push_bind("escalation.new")
                .push_bind("unread")
                .push_bind(format!("Perf notification {}", inserted + i))
                .push_bind(None::<String>)
                .push_bind("escalation")
                .push_bind(Uuid::new_v4())
                .push_bind(format!("perf-{}", inserted + i))
                .push_bind(None::<Uuid>)
                .push_bind(ts);
        });
        builder.build().execute(&pool).await.unwrap();
        inserted += batch_size;
    }

    let start = std::time::Instant::now();
    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/notifications?limit=20",
            Method::GET,
            user_id,
            Some(tenant_id),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(1),
        "list with 1000 notifications took {:?}",
        elapsed
    );
}

// ── T040: Tool approval + AI failure trigger tests ─────────────────────────

#[tokio::test]
async fn tool_approval_notifies_conversations_manage_holders() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, None).await;
    let admin = seed_user(&pool, None).await;
    let manager = seed_user(&pool, None).await;
    let agent = seed_user(&pool, None).await;
    let viewer = seed_user(&pool, None).await;

    seed_membership(&pool, tenant_id, owner, "owner").await;
    seed_membership(&pool, tenant_id, admin, "admin").await;
    seed_membership(&pool, tenant_id, manager, "manager").await;
    seed_membership(&pool, tenant_id, agent, "agent").await;
    seed_membership(&pool, tenant_id, viewer, "viewer").await;

    let tool_request_id = Uuid::new_v4();

    emit::emit_requested_on_pool(
        &pool,
        &NotificationRequest {
            tenant_id,
            kind: NotificationKind::ToolApprovalRequired,
            subject_type: SubjectType::ToolRequest,
            subject_id: tool_request_id,
            actor_membership_id: None,
            target_membership_id: None,
            dedupe_key: dedupe_key_tool_approval(tool_request_id),
            title: "Tool approval required".into(),
            body: Some("A tool action requires your approval.".into()),
        },
    )
    .await;

    let presence = Arc::new(escalations::presence::Runtime::new(
        pool.clone(),
        Duration::from_secs(45),
    ));
    let processed =
        notifications::worker::process_notification_outbox_once(&pool, &presence)
            .await
            .unwrap();
    assert!(processed, "expected outbox event to be processed");

    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT n.recipient_membership_id, tm.role \
         FROM notifications n \
         JOIN tenant_memberships tm ON n.recipient_membership_id = tm.id \
         WHERE n.tenant_id = $1 AND n.subject_id = $2 AND n.kind = 'tool.approval_required'",
    )
    .bind(tenant_id)
    .bind(tool_request_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    let roles: Vec<&str> = rows.iter().map(|(_, r)| r.as_str()).collect();
    assert!(roles.contains(&"owner"));
    assert!(roles.contains(&"admin"));
    assert!(roles.contains(&"manager"));
    assert!(roles.contains(&"agent"));
    assert!(!roles.contains(&"viewer"));
    assert_eq!(rows.len(), 4, "expected 4 notification rows for manage holders");
}

#[tokio::test]
async fn tool_decision_resolves_other_holders_notifications() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let owner_user = seed_user(&pool, None).await;
    let admin_user = seed_user(&pool, None).await;
    let owner_mem = seed_membership(&pool, tenant_id, owner_user, "owner").await;
    let admin_mem = seed_membership(&pool, tenant_id, admin_user, "admin").await;

    let tool_request_id = Uuid::new_v4();

    for &mem_id in &[owner_mem, admin_mem] {
        sqlx::query(
            "INSERT INTO notifications (tenant_id, recipient_membership_id, kind, state, title, \
             body, subject_type, subject_id, dedupe_key) \
             VALUES ($1, $2, 'tool.approval_required', 'unread', 'Tool approval required', \
             'A tool action requires your approval.', 'tool_request', $3, $4)",
        )
        .bind(tenant_id)
        .bind(mem_id)
        .bind(tool_request_id)
        .bind(format!("test-resolve-{}", mem_id))
        .execute(&pool)
        .await
        .unwrap();
    }

    let mut tx = pool.begin().await.unwrap();
    emit::emit_resolved_in_tx(
        &mut tx,
        tenant_id,
        &SubjectType::ToolRequest,
        tool_request_id,
        Some(admin_mem),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let presence = Arc::new(escalations::presence::Runtime::new(
        pool.clone(),
        Duration::from_secs(45),
    ));
    let processed =
        notifications::worker::process_notification_outbox_once(&pool, &presence)
            .await
            .unwrap();
    assert!(processed, "expected resolve outbox event to be processed");

    let owner_state: String = sqlx::query_scalar(
        "SELECT state FROM notifications \
         WHERE tenant_id = $1 AND recipient_membership_id = $2 AND subject_id = $3",
    )
    .bind(tenant_id)
    .bind(owner_mem)
    .bind(tool_request_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(owner_state, "resolved", "owner's notification should be resolved");

    let admin_state: String = sqlx::query_scalar(
        "SELECT state FROM notifications \
         WHERE tenant_id = $1 AND recipient_membership_id = $2 AND subject_id = $3",
    )
    .bind(tenant_id)
    .bind(admin_mem)
    .bind(tool_request_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(admin_state, "unread", "admin's (decider's) notification should remain unread");
}

#[tokio::test]
async fn multiple_ai_failures_within_15_min_produce_one_notification() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, None).await;
    seed_membership(&pool, tenant_id, owner, "owner").await;

    let conversation_id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let dedupe = dedupe_key_ai_failed(conversation_id, &now);

    for _ in 0..3 {
        emit::emit_requested_on_pool(
            &pool,
            &NotificationRequest {
                tenant_id,
                kind: NotificationKind::AiResponseFailed,
                subject_type: SubjectType::Conversation,
                subject_id: conversation_id,
                actor_membership_id: None,
                target_membership_id: None,
                dedupe_key: dedupe.clone(),
                title: "AI response failed".into(),
                body: Some("The AI was unable to generate a response for this conversation.".into()),
            },
        )
        .await;
    }

    let presence = Arc::new(escalations::presence::Runtime::new(
        pool.clone(),
        Duration::from_secs(45),
    ));
    for _ in 0..3 {
        let processed =
            notifications::worker::process_notification_outbox_once(&pool, &presence)
                .await
                .unwrap();
        assert!(processed, "expected outbox event to be processed");
    }

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE tenant_id = $1 AND dedupe_key = $2",
    )
    .bind(tenant_id)
    .bind(&dedupe)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "expected exactly 1 notification despite 3 emits");

    emit::emit_requested_on_pool(
        &pool,
        &NotificationRequest {
            tenant_id,
            kind: NotificationKind::AiResponseFailed,
            subject_type: SubjectType::Conversation,
            subject_id: conversation_id,
            actor_membership_id: None,
            target_membership_id: None,
            dedupe_key: dedupe.clone(),
            title: "AI response failed".into(),
            body: Some("The AI was unable to generate a response for this conversation.".into()),
        },
    )
    .await;

    let processed =
        notifications::worker::process_notification_outbox_once(&pool, &presence)
            .await
            .unwrap();
    assert!(processed, "expected outbox event to be processed");

    let count_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE tenant_id = $1 AND dedupe_key = $2",
    )
    .bind(tenant_id)
    .bind(&dedupe)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count_after, 1, "still expected exactly 1 notification after 4th emit");
}

#[tokio::test]
async fn ai_failure_in_later_bucket_produces_second_notification() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let owner = seed_user(&pool, None).await;
    seed_membership(&pool, tenant_id, owner, "owner").await;

    let conversation_id = Uuid::new_v4();

    let t1 = chrono::DateTime::parse_from_rfc3339("2026-07-20T14:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let t2 = chrono::DateTime::parse_from_rfc3339("2026-07-20T14:20:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);

    let dedupe_1 = dedupe_key_ai_failed(conversation_id, &t1);
    let dedupe_2 = dedupe_key_ai_failed(conversation_id, &t2);

    assert_ne!(dedupe_1, dedupe_2, "buckets must differ");

    emit::emit_requested_on_pool(
        &pool,
        &NotificationRequest {
            tenant_id,
            kind: NotificationKind::AiResponseFailed,
            subject_type: SubjectType::Conversation,
            subject_id: conversation_id,
            actor_membership_id: None,
            target_membership_id: None,
            dedupe_key: dedupe_1.clone(),
            title: "AI response failed".into(),
            body: Some("The AI was unable to generate a response for this conversation.".into()),
        },
    )
    .await;

    emit::emit_requested_on_pool(
        &pool,
        &NotificationRequest {
            tenant_id,
            kind: NotificationKind::AiResponseFailed,
            subject_type: SubjectType::Conversation,
            subject_id: conversation_id,
            actor_membership_id: None,
            target_membership_id: None,
            dedupe_key: dedupe_2.clone(),
            title: "AI response failed".into(),
            body: Some("The AI was unable to generate a response for this conversation.".into()),
        },
    )
    .await;

    let presence = Arc::new(escalations::presence::Runtime::new(
        pool.clone(),
        Duration::from_secs(45),
    ));
    for _ in 0..2 {
        let processed =
            notifications::worker::process_notification_outbox_once(&pool, &presence)
                .await
                .unwrap();
        assert!(processed, "expected outbox event to be processed");
    }

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications n \
         JOIN tenant_memberships tm ON n.recipient_membership_id = tm.id \
         WHERE n.tenant_id = $1 AND tm.role = 'owner'",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 2, "expected 2 notifications across buckets");
}

// ── T036: Assignment + escalation notification tests ─────────────────────────

#[tokio::test]
async fn assignment_notifies_only_new_assignee() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let assigner = seed_actor(&pool, tenant_id, "assign-notify@test.com", "admin").await;
    let assignee = seed_actor(&pool, tenant_id, "assign-rec@test.com", "agent").await;
    let customer_id = seed_customer(&pool, tenant_id).await;
    let conv_id = seed_conversation(&pool, tenant_id, customer_id).await;

    let state = setup_state(pool.clone());

    let payload = json!({"assigned_membership_id": assignee.membership_id});
    let response = send_state(
        &state,
        json_patch(
            &format!("/api/v1/tenant/conversations/{conv_id}"),
            assigner.user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    drain_pending_notifications(&state).await;

    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT n.recipient_membership_id, n.state FROM notifications n \
         WHERE n.tenant_id = $1 AND n.subject_id = $2 AND n.kind = 'conversation.assigned'",
    )
    .bind(tenant_id)
    .bind(conv_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(rows.len(), 1, "expected exactly 1 notification for assignee");
    assert_eq!(rows[0].0, assignee.membership_id, "notification must go to the new assignee");
}

#[tokio::test]
async fn self_assignment_notifies_nobody() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let actor = seed_actor(&pool, tenant_id, "self-assign@test.com", "admin").await;
    let customer_id = seed_customer(&pool, tenant_id).await;
    let conv_id = seed_conversation(&pool, tenant_id, customer_id).await;

    // PATCH assign to self
    let payload = json!({"assigned_membership_id": actor.membership_id});
    let response = send(
        pool.clone(),
        Environment::Test,
        json_patch(
            &format!("/api/v1/tenant/conversations/{conv_id}"),
            actor.user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    // No notification.requested should be emitted for self-assignment
    let req_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events \
         WHERE tenant_id = $1::text AND event_type = 'notification.requested'",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(req_count, 0, "self-assignment must not emit any notification.requested");
}

#[tokio::test]
async fn escalation_routing_produces_one_notification() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let agent = seed_actor(&pool, tenant_id, "esc-routed@test.com", "agent").await;
    let customer_id = seed_customer(&pool, tenant_id).await;
    let conv_id = seed_conversation(&pool, tenant_id, customer_id).await;

    // Create a skill and assign it to the agent
    let skill_id: Uuid = sqlx::query_scalar(
        "INSERT INTO skills (tenant_id, name) VALUES ($1, 'billing') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO agent_skills (tenant_id, membership_id, skill_id) VALUES ($1, $2, $3)",
    )
    .bind(tenant_id)
    .bind(agent.membership_id)
    .bind(skill_id)
    .execute(&pool)
    .await
    .unwrap();

    let state = setup_state(pool.clone());

    sqlx::query(
        "INSERT INTO agent_availability (tenant_id, membership_id, state) \
         VALUES ($1, $2, 'available') \
         ON CONFLICT (tenant_id, membership_id) DO UPDATE SET state = 'available', state_changed_at = now()",
    )
    .bind(tenant_id)
    .bind(agent.membership_id)
    .execute(&pool)
    .await
    .unwrap();

    let (_guard, _rx) = tokio::task::spawn_blocking({
        let rt = state.escalations.clone();
        move || rt.connect(tenant_id, agent.membership_id)
    })
    .await
    .unwrap();

    // Escalate with matching skill → routed directly to agent
    let response = send_state(
        &state,
        json_post(
            &format!("/api/v1/tenant/conversations/{conv_id}/escalate"),
            agent.user_id,
            tenant_id,
            json!({"reason": "billing issue", "requiredSkillIds": [skill_id]}),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    drain_pending_notifications(&state).await;

    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT n.recipient_membership_id, n.state FROM notifications n \
         WHERE n.tenant_id = $1 AND n.subject_type = 'escalation' AND n.kind = 'escalation.new'",
    )
    .bind(tenant_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(rows.len(), 1, "expected exactly 1 notification for routed escalation");
    assert_eq!(rows[0].0, agent.membership_id, "notification must go to the routed agent");
}

#[tokio::test]
async fn queued_escalation_fans_out_to_manage_holders() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let agent_a = seed_actor(&pool, tenant_id, "queued-a@test.com", "agent").await;
    let agent_b = seed_actor(&pool, tenant_id, "queued-b@test.com", "agent").await;
    let actor = seed_actor(&pool, tenant_id, "queued-trigger@test.com", "admin").await;
    let customer_id = seed_customer(&pool, tenant_id).await;
    let conv_id = seed_conversation(&pool, tenant_id, customer_id).await;

    let state = setup_state(pool.clone());

    // Create a real skill that no agent has → escalation should be queued
    let no_skill_id: Uuid = sqlx::query_scalar(
        "INSERT INTO skills (tenant_id, name) VALUES ($1, 'unmatched') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let response = send_state(
        &state,
        json_post(
            &format!("/api/v1/tenant/conversations/{conv_id}/escalate"),
            actor.user_id,
            tenant_id,
            json!({"reason": "no match", "requiredSkillIds": [no_skill_id]}),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    drain_pending_notifications(&state).await;

    let rows: Vec<Uuid> = sqlx::query_scalar(
        "SELECT n.recipient_membership_id FROM notifications n \
         WHERE n.tenant_id = $1 AND n.subject_type = 'escalation' AND n.kind = 'escalation.new'",
    )
    .bind(tenant_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(rows.len(), 2, "expected 2 notifications (one per manage holder)");
    assert!(rows.contains(&agent_a.membership_id), "agent_a must be notified");
    assert!(rows.contains(&agent_b.membership_id), "agent_b must be notified");
}

#[tokio::test]
async fn claiming_resolves_others_rows() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let agent_a = seed_actor(&pool, tenant_id, "claim-other-a@test.com", "agent").await;
    let agent_b = seed_actor(&pool, tenant_id, "claim-other-b@test.com", "agent").await;
    let actor = seed_actor(&pool, tenant_id, "claim-trigger@test.com", "admin").await;
    let customer_id = seed_customer(&pool, tenant_id).await;
    let conv_id = seed_conversation(&pool, tenant_id, customer_id).await;

    // Escalate with no-match skill → queued
    let no_skill_id: Uuid = sqlx::query_scalar(
        "INSERT INTO skills (tenant_id, name) VALUES ($1, 'unmatched-claim') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let state = setup_state(pool.clone());
    let response = send_state(
        &state,
        json_post(
            &format!("/api/v1/tenant/conversations/{conv_id}/escalate"),
            actor.user_id,
            tenant_id,
            json!({"reason": "for claim test", "requiredSkillIds": [no_skill_id]}),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    // Find the escalation id
    let esc_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM escalations WHERE tenant_id = $1 AND conversation_id = $2",
    )
    .bind(tenant_id)
    .bind(conv_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Seed a notification for agent_a about this escalation (simulating existing unread)
    sqlx::query(
        "INSERT INTO notifications (tenant_id, recipient_membership_id, kind, state, title, \
         body, subject_type, subject_id, dedupe_key) \
         VALUES ($1, $2, 'escalation.new', 'unread', 'Escalation queued', NULL, \
         'escalation', $3, $4)",
    )
    .bind(tenant_id)
    .bind(agent_a.membership_id)
    .bind(esc_id)
    .bind(format!("test-claim-resolve-{esc_id}"))
    .execute(&pool)
    .await
    .unwrap();

    // Agent B claims the escalation
    let claim_response = send_state(
        &state,
        json_post(
            &format!("/api/v1/tenant/escalations/{esc_id}/claim"),
            agent_b.user_id,
            tenant_id,
            json!({}),
        ),
    )
    .await;
    assert_eq!(claim_response.status(), StatusCode::OK, "claim should succeed");

    drain_pending_notifications(&state).await;

    // Agent A's notification should be resolved
    let a_state: String = sqlx::query_scalar(
        "SELECT state FROM notifications WHERE tenant_id = $1 AND recipient_membership_id = $2 \
         AND subject_id = $3",
    )
    .bind(tenant_id)
    .bind(agent_a.membership_id)
    .bind(esc_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(a_state, "resolved", "agent_a's notification should be resolved after claim");
}

#[tokio::test]
async fn auto_drain_resolves_others_rows() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let agent_a = seed_actor(&pool, tenant_id, "drain-other-a@test.com", "agent").await;
    let agent_b = seed_actor(&pool, tenant_id, "drain-other-b@test.com", "agent").await;
    let customer_id = seed_customer(&pool, tenant_id).await;
    let conv_id = seed_conversation(&pool, tenant_id, customer_id).await;

    // Create queued escalation directly
    let esc_id: Uuid = sqlx::query_scalar(
        "INSERT INTO escalations (tenant_id, conversation_id, reason, required_skill_ids, \
         required_skill_names, status) \
         VALUES ($1, $2, 'drain test', '{}', '{}', 'queued') RETURNING id",
    )
    .bind(tenant_id)
    .bind(conv_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Seed notification for agent_a (unread)
    sqlx::query(
        "INSERT INTO notifications (tenant_id, recipient_membership_id, kind, state, title, \
         body, subject_type, subject_id, dedupe_key) \
         VALUES ($1, $2, 'escalation.new', 'unread', 'Escalation queued', NULL, \
         'escalation', $3, $4)",
    )
    .bind(tenant_id)
    .bind(agent_a.membership_id)
    .bind(esc_id)
    .bind(format!("test-drain-resolve-{esc_id}"))
    .execute(&pool)
    .await
    .unwrap();

    // Also seed a read notification for agent_a (should stay read, FR-011a)
    sqlx::query(
        "INSERT INTO notifications (tenant_id, recipient_membership_id, kind, state, title, \
         body, subject_type, subject_id, dedupe_key) \
         VALUES ($1, $2, 'escalation.new', 'read', 'Escalation queued', NULL, \
         'escalation', $3, $4)",
    )
    .bind(tenant_id)
    .bind(agent_a.membership_id)
    .bind(esc_id)
    .bind(format!("test-drain-read-{esc_id}"))
    .execute(&pool)
    .await
    .unwrap();

    // Call drain_one_for_membership_in_tx directly
    let mut tx = pool.begin().await.unwrap();
    let drained = escalations::routing::drain_one_for_membership_in_tx(
        &mut tx,
        tenant_id,
        agent_b.membership_id,
        &[],
        agent_b.user_id,
    )
    .await
    .unwrap();
    assert!(drained.is_some(), "drain should pick up the queued escalation");
    tx.commit().await.unwrap();

    let state = setup_state(pool.clone());
    drain_pending_notifications(&state).await;

    // Agent A's unread notification should be resolved
    let a_state: String = sqlx::query_scalar(
        "SELECT state FROM notifications WHERE tenant_id = $1 AND recipient_membership_id = $2 \
         AND subject_id = $3 AND dedupe_key = $4",
    )
    .bind(tenant_id)
    .bind(agent_a.membership_id)
    .bind(esc_id)
    .bind(format!("test-drain-resolve-{esc_id}"))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(a_state, "resolved", "agent_a's unread notification should be resolved");

    // Agent A's already-read notification should remain read (FR-011a)
    let a_read_state: String = sqlx::query_scalar(
        "SELECT state FROM notifications WHERE tenant_id = $1 AND recipient_membership_id = $2 \
         AND subject_id = $3 AND dedupe_key = $4",
    )
    .bind(tenant_id)
    .bind(agent_a.membership_id)
    .bind(esc_id)
    .bind(format!("test-drain-read-{esc_id}"))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(a_read_state, "read", "already-read notification must remain read (FR-011a)");
}

#[tokio::test]
async fn replay_dedup_produces_one_row_per_recipient() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let recipient = seed_actor(&pool, tenant_id, "dedup-rec@test.com", "agent").await;
    let dedupe_key = format!("test-dedup-{}", Uuid::new_v4());

    // Insert the same notification.requested payload twice
    for _ in 0..2 {
        let payload = json!({
            "tenantId": tenant_id,
            "kind": "escalation.new",
            "subjectType": "escalation",
            "subjectId": Uuid::new_v4(),
            "actorMembershipId": null,
            "targetMembershipId": recipient.membership_id,
            "dedupeKey": dedupe_key,
            "title": "Dedup test",
            "body": null,
        });
        sqlx::query(
            "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
             VALUES ($1, 'notification', $2, $3, 'notification.requested', $4, now())",
        )
        .bind(Uuid::new_v4())
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(payload)
        .execute(&pool)
        .await
        .unwrap();
    }

    let state = setup_state(pool.clone());
    drain_pending_notifications(&state).await;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE tenant_id = $1 AND dedupe_key = $2",
    )
    .bind(tenant_id)
    .bind(&dedupe_key)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "expected exactly 1 notification row despite 2 identical outbox events");

    // Process again — should not create a duplicate
    drain_pending_notifications(&state).await;

    let count_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE tenant_id = $1 AND dedupe_key = $2",
    )
    .bind(tenant_id)
    .bind(&dedupe_key)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count_after, 1, "still expected exactly 1 notification row after replay");
}

// ── T046: Regression test for removed-then-re-added member ──────────────────

#[tokio::test]
async fn deactivated_member_gets_no_inbox_re_add_gets_zero_unread() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, None).await;
    let old_mem_id = seed_membership(&pool, tenant_id, user_id, "agent").await;

    // Seed an unread notification for the active membership
    let nid = seed_notification(
        &pool, tenant_id, old_mem_id, "escalation.new", "unread",
        "Old notification", "escalation", Uuid::new_v4(), None,
        "2026-07-18T14:00:00Z",
    )
    .await;

    // Deactivate membership
    sqlx::query("UPDATE tenant_memberships SET status = 'disabled' WHERE id = $1")
        .bind(old_mem_id)
        .execute(&pool)
        .await
        .unwrap();

    // Inbox is unreachable — tenant-context middleware returns 403 when the
    // user has no active membership in the tenant.
    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/notifications",
            Method::GET,
            user_id,
            Some(tenant_id),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Old notification still exists and is tied to old membership
    let old_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE recipient_membership_id = $1",
    )
    .bind(old_mem_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(old_count, 1, "old notification must remain bound to old membership id");

    // Re-add the same user → new membership id
    // First soft-delete the old membership so the unique constraint allows
    // a new active row for the same (tenant_id, user_id).
    sqlx::query(
        "UPDATE tenant_memberships SET deleted_at = now() WHERE id = $1",
    )
    .bind(old_mem_id)
    .execute(&pool)
    .await
    .unwrap();
    let new_mem_id = seed_membership(&pool, tenant_id, user_id, "agent").await;
    assert_ne!(old_mem_id, new_mem_id, "re-add must produce a new membership id");

    // Unread count for new membership is 0 (old notifications don't carry over)
    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/notifications/unread-count",
            Method::GET,
            user_id,
            Some(tenant_id),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert_eq!(json["count"].as_i64().unwrap(), 0);

    // Verify no notification rows point to the new membership
    let new_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE recipient_membership_id = $1",
    )
    .bind(new_mem_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(new_count, 0, "new membership must have zero notifications");
}

#[tokio::test]
async fn assignment_by_actor_without_membership_notifies_assignee() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let assignee_user_id = seed_user(&pool, None).await;
    let assignee_mid = seed_membership(&pool, tenant_id, assignee_user_id, "agent").await;
    // Actor is a platform user (super_admin) with NO tenant membership.
    // The tenant_context_middleware grants staff_tenant_permissions to platform
    // users, bypassing the membership check so the API call succeeds, while the
    // notification path must still resolve actorMembershipId: null.
    let actor_user_id = seed_user(&pool, Some("super_admin")).await;
    let customer_id = seed_customer(&pool, tenant_id).await;
    let conv_id = seed_conversation(&pool, tenant_id, customer_id).await;

    let state = setup_state(pool.clone());

    let payload = json!({"assigned_membership_id": assignee_mid});
    let response = send_state(
        &state,
        json_patch(
            &format!("/api/v1/tenant/conversations/{conv_id}"),
            actor_user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    drain_pending_notifications(&state).await;

    let rows: Vec<(Uuid, String, Option<Uuid>)> = sqlx::query_as(
        "SELECT n.recipient_membership_id, n.state, n.actor_membership_id FROM notifications n \
         WHERE n.tenant_id = $1 AND n.subject_id = $2 AND n.kind = 'conversation.assigned'",
    )
    .bind(tenant_id)
    .bind(conv_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(rows.len(), 1, "expected exactly 1 notification for assignee");
    assert_eq!(rows[0].0, assignee_mid, "notification must go to the assignee");
    assert!(rows[0].2.is_none(), "actor_membership_id must be NULL when actor has no membership");

    let req_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events \
         WHERE tenant_id = $1::text AND event_type = 'notification.requested'",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(req_count, 0, "all notification.requested outbox events must be drained");
}
