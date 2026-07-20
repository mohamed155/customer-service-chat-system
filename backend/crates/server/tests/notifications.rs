use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use config::Environment;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

use server::router;
use server::state::AppState;
use cache::Cache;

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
    // status = 'invited' is not 'active'
    seed_membership_with_status(&pool, tenant_id, user_id, "agent", "invited").await;

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
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = body_json(response).await;
    assert_eq!(json["error"]["code"], "validation_failed");
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
