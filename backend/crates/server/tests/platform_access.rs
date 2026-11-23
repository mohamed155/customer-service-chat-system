#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // integration test assertions

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use axum::Router;
use chrono::{DateTime, Utc};
use config::Environment;
use http_body_util::BodyExt;
use identity::sessions::Issued;
use serde_json::{json, Value};
use server::{router, state::AppState};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

mod session_fixture;

const ORIGIN: &str = "http://localhost:4200";
const REASON: &str = "Investigate the reported delivery issue";
const SESSION: &str = "/api/v1/platform/access-session";
const END: &str = "/api/v1/platform/access-session/end";
const TENANT: &str = "/api/v1/tenant";
const STARTED: &str = "platform.tenant_access_started";
const ENDED: &str = "platform.tenant_access_ended";
const EXPIRED: &str = "platform.tenant_access_expired";

fn test_config(redis_url: String) -> config::AppConfig {
    config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url,
        port: 0,
        bind_address: "0.0.0.0".into(),
        environment: config::Environment::Test,
        cors_allowed_origins: vec![ORIGIN.into()],
        log_format: config::LogFormat::Pretty,
        smtp_url: None,
        smtp_from: None,
        public_dashboard_url: ORIGIN.into(),
        db_max_connections: 2,
        db_acquire_timeout_ms: 5000,
        ready_probe_timeout_ms: 5000,
        shutdown_grace_seconds: 1,
        docs_enabled: false,
        rate_limit_enabled: false,
        rate_limit_login_max: 10,
        rate_limit_login_window_secs: 60,
        password_reset_limits: Default::default(),
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
        ai_key_encryption_key: None,
        integration_secrets_key: None,
        secrets_root_key: None,
        secrets_root_key_previous: None,
        auth_mfa_required: false,
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
        trusted_proxy: false,
        outbound_http: config::OutboundHttpConfig::default(),
    }
}

#[derive(sqlx::FromRow)]
struct AuditRecord {
    tenant_id: Uuid,
    resource_id: Option<String>,
    platform_access_session_id: Option<Uuid>,
    details: Value,
}

struct Fixture {
    pool: PgPool,
    app: Router,
    tenant: Uuid,
    staff: Uuid,
    sign_in: Issued,
}

impl Fixture {
    async fn new() -> Option<Self> {
        let required = std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1");
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            assert!(!required, "REQUIRE_DB_TESTS=1 requires DATABASE_URL");
            eprintln!("platform access tests need DATABASE_URL");
            return None;
        };
        let pool = db::lazy_pool(&database_url, 6, Duration::from_secs(5));
        if let Err(error) = sqlx::query("SELECT 1").execute(&pool).await {
            assert!(
                !required,
                "platform access tests require PostgreSQL: {error}"
            );
            eprintln!("platform access tests need live PostgreSQL");
            return None;
        }
        db::testing::migrate().await;

        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
        let config = test_config(redis_url);
        let cache = cache::Cache::new(&config.redis_url).unwrap();
        if let Err(error) = cache.ping().await {
            assert!(!required, "platform access tests require Redis: {error}");
            eprintln!("platform access tests need live Redis");
            return None;
        }
        let app = router::app(AppState {
            config: Arc::new(config.clone()),
            db: pool.clone(),
            cache: Arc::new(cache),
            health_checks: vec![],
            escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
            ai: ai::AiService::from_config(pool.clone(), &config).unwrap(),
            budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
        });
        let tenant = seed_tenant(&pool).await;
        let staff = seed_user(&pool, Some("super_admin")).await;
        let sign_in = session_fixture::start(&pool, &Environment::Test, staff).await;
        Some(Self {
            pool,
            app,
            tenant,
            staff,
            sign_in,
        })
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        sign_in: &Issued,
        tenant: Option<Uuid>,
        payload: Option<Value>,
    ) -> Response {
        let mut request = Request::builder()
            .uri(path)
            .method(method)
            .header(
                "cookie",
                format!("__Host-app_session={}", sign_in.access_token),
            )
            .header("accept", "application/problem+json")
            .header("origin", ORIGIN)
            .header("x-requested-with", "XMLHttpRequest");
        if let Some(tenant) = tenant {
            request = request.header("x-tenant-id", tenant.to_string());
        }
        let body = match payload {
            Some(payload) => {
                request = request.header("content-type", "application/json");
                Body::from(payload.to_string())
            }
            None => Body::empty(),
        };
        self.app
            .clone()
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap()
    }

    async fn switch(&self, tenant: Uuid, payload: Value) -> Response {
        self.send(
            Method::POST,
            &format!("/api/v1/platform/tenants/{tenant}/switch"),
            &self.sign_in,
            None,
            Some(payload),
        )
        .await
    }

    async fn tenant_request(&self, sign_in: &Issued, tenant: Uuid) -> Response {
        self.send(Method::GET, TENANT, sign_in, Some(tenant), None)
            .await
    }

    async fn current_for(&self, sign_in: &Issued) -> Response {
        self.send(Method::GET, SESSION, sign_in, None, None).await
    }

    async fn end_for(&self, sign_in: &Issued) -> Response {
        self.send(Method::POST, END, sign_in, None, None).await
    }

    async fn start(&self, tenant: Uuid) -> Uuid {
        let response = self.switch(tenant, json!({"reason": REASON})).await;
        assert_eq!(response.status(), StatusCode::OK);
        uuid(&body(response).await["accessSession"]["id"])
    }

    async fn audit(&self, action: &str) -> Vec<AuditRecord> {
        sqlx::query_as(
            "SELECT tenant_id, resource_id, platform_access_session_id, details FROM audit_logs
             WHERE actor_user_id = $1 AND action = $2 ORDER BY created_at, id",
        )
        .bind(self.staff)
        .bind(action)
        .fetch_all(&db::testing::seed_pool())
        .await
        .unwrap()
    }

    async fn count_open(&self) -> i64 {
        sqlx::query_scalar(
            "SELECT count(*) FROM platform_access_sessions WHERE user_id = $1 AND ended_at IS NULL",
        )
        .bind(self.staff)
        .fetch_one(&self.pool)
        .await
        .unwrap()
    }
}

async fn seed_user(pool: &PgPool, role: Option<&str>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name, platform_role)
         VALUES ($1, 'Platform Access Fixture', $2) RETURNING id",
    )
    .bind(format!("platform-access-{}@example.com", Uuid::new_v4()))
    .bind(role)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_tenant(pool: &PgPool) -> Uuid {
    db::testing::insert_tenant(
        pool,
        "Platform Access Tenant",
        &format!("platform-access-{}", Uuid::new_v4().simple()),
    )
    .await
}

async fn body(response: Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

async fn access_denial(response: Response, code: &str) {
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );
    let problem = body(response).await;
    assert_eq!(problem["code"], code, "{problem}");
}

#[tokio::test]
async fn invalid_input_creates_neither_sessions_nor_lifecycle_audit_rows() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let cases = [
        (json!({}), "reason", Some("required")),
        (json!({"reason": "123456789"}), "reason", Some("too_short")),
        (json!({"reason": " \t\n "}), "reason", Some("required")),
        (
            json!({"reason": "é".repeat(501)}),
            "reason",
            Some("too_long"),
        ),
        (
            json!({"reason": REASON, "durationMinutes": 4}),
            "durationMinutes",
            Some("out_of_range"),
        ),
        (
            json!({"reason": REASON, "durationMinutes": 241}),
            "durationMinutes",
            Some("out_of_range"),
        ),
        (
            json!({"reason": REASON, "durationMinutes": 15.5}),
            "<root>",
            Some("invalid_value"),
        ),
        (
            json!({"reason": REASON, "unexpected": true}),
            "unexpected",
            None,
        ),
    ];
    for (payload, field, code) in cases {
        let response = fixture.switch(fixture.tenant, payload).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{field}"
        );
        let problem = body(response).await;
        let issues = problem["errors"].as_array().unwrap();
        assert!(
            issues
                .iter()
                .any(|issue| issue["field"] == field && code.is_none_or(|c| issue["code"] == c)),
            "{problem}"
        );
    }
    let sessions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM platform_access_sessions WHERE user_id = $1")
            .bind(fixture.staff)
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
    assert_eq!(sessions, 0);
    let audit_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_logs WHERE actor_user_id = $1")
            .bind(fixture.staff)
            .fetch_one(&db::testing::seed_pool())
            .await
            .unwrap();
    assert_eq!(audit_rows, 0);
}

#[tokio::test]
async fn switching_returns_tenant_summary_and_current_session_with_default_expiry() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let response = fixture
        .switch(fixture.tenant, json!({"reason": format!("  {REASON}  ")}))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let summary = body(response).await;
    assert_eq!(summary["id"], json!(fixture.tenant));
    let session = &summary["accessSession"];
    let id = uuid(&session["id"]);
    assert_eq!(session["tenantId"], json!(fixture.tenant));
    assert_eq!(session["reason"], REASON);

    let (auth_session_id, reason, started_at, expires_at): (Uuid, String, DateTime<Utc>, DateTime<Utc>) =
        sqlx::query_as("SELECT auth_session_id, reason, started_at, expires_at FROM platform_access_sessions WHERE id = $1")
        .bind(id)
        .fetch_one(&fixture.pool)
        .await
        .unwrap();
    assert_eq!(auth_session_id, fixture.sign_in.sid);
    assert_eq!(reason, REASON);
    assert_eq!(expires_at - started_at, chrono::Duration::minutes(60));
    assert_eq!(
        session["startedAt"]
            .as_str()
            .unwrap()
            .parse::<DateTime<Utc>>()
            .unwrap(),
        started_at
    );
    assert_eq!(
        session["expiresAt"]
            .as_str()
            .unwrap()
            .parse::<DateTime<Utc>>()
            .unwrap(),
        expires_at
    );

    let response = fixture.current_for(&fixture.sign_in).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let current = body(response).await;
    assert_eq!(current["tenantName"], "Platform Access Tenant");
    for key in ["id", "tenantId", "reason", "expiresAt", "startedAt"] {
        assert_eq!(current[key], session[key]);
    }

    let audit = fixture.audit(STARTED).await;
    assert_eq!(audit.len(), 1);
    let row = &audit[0];
    assert_eq!(row.tenant_id, fixture.tenant);
    assert_eq!(row.resource_id, Some(id.to_string()));
    assert_eq!(row.platform_access_session_id, Some(id));
    assert_eq!(row.details["session_id"], json!(id));
    assert_eq!(row.details["reason"], REASON);
}

#[tokio::test]
async fn access_is_bound_to_the_tenant_staff_user_and_sign_in() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let other_tenant = seed_tenant(&fixture.pool).await;
    let required = "platform_access_required";
    access_denial(
        fixture
            .tenant_request(&fixture.sign_in, fixture.tenant)
            .await,
        required,
    )
    .await;
    assert_eq!(
        fixture
            .send(
                Method::GET,
                "/api/v1/platform/tenants",
                &fixture.sign_in,
                None,
                None
            )
            .await
            .status(),
        StatusCode::OK,
    );
    let no_session = fixture.current_for(&fixture.sign_in).await;
    assert_eq!(no_session.status(), StatusCode::NOT_FOUND);
    assert_eq!(no_session.headers()["cache-control"], "no-store");

    fixture.start(fixture.tenant).await;
    assert_eq!(
        fixture
            .tenant_request(&fixture.sign_in, fixture.tenant)
            .await
            .status(),
        StatusCode::OK
    );
    access_denial(
        fixture.tenant_request(&fixture.sign_in, other_tenant).await,
        required,
    )
    .await;

    let denial_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs
         WHERE action = 'tenant.access_denied' AND actor_user_id = $1 AND resource_id = $2",
    )
    .bind(fixture.staff)
    .bind(other_tenant.to_string())
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();
    assert_eq!(denial_audits, 1);

    let other_staff = seed_user(&fixture.pool, Some("support")).await;
    let other_staff_sign_in =
        session_fixture::start(&fixture.pool, &Environment::Test, other_staff).await;
    access_denial(
        fixture
            .tenant_request(&other_staff_sign_in, fixture.tenant)
            .await,
        required,
    )
    .await;

    let another_sign_in =
        session_fixture::start(&fixture.pool, &Environment::Test, fixture.staff).await;
    access_denial(
        fixture
            .tenant_request(&another_sign_in, fixture.tenant)
            .await,
        required,
    )
    .await;
    assert_eq!(
        fixture.current_for(&another_sign_in).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fixture.end_for(&another_sign_in).await.status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        fixture.count_open().await,
        1,
        "another sign-in cannot end the active access session"
    );

    let owner = seed_owner(&fixture.pool, fixture.tenant).await;
    assert_eq!(
        fixture
            .tenant_request(&owner, fixture.tenant)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        fixture.current_for(&owner).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        fixture.end_for(&owner).await.status(),
        StatusCode::FORBIDDEN
    );
}

async fn seed_owner(pool: &PgPool, tenant: Uuid) -> Issued {
    let user = seed_user(pool, None).await;
    sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, 'owner')",
    )
    .bind(tenant)
    .bind(user)
    .execute(&db::testing::seed_pool())
    .await
    .unwrap();
    session_fixture::start(pool, &Environment::Test, user).await
}

async fn expire(pool: &PgPool, id: Uuid) {
    sqlx::query(
        "UPDATE platform_access_sessions
         SET started_at = now() - interval '1 hour', expires_at = now() - interval '1 second'
         WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn expiry_is_audited_once_across_requests_and_sweeps_and_access_can_restart() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let id = fixture.start(fixture.tenant).await;
    expire(&fixture.pool, id).await;

    let tenant_request = || fixture.tenant_request(&fixture.sign_in, fixture.tenant);
    let (first, second, third) = tokio::join!(tenant_request(), tenant_request(), tenant_request());
    for response in [first, second, third] {
        access_denial(response, "platform_access_expired").await;
    }
    tenancy::sweep_platform_access_sessions(&fixture.pool)
        .await
        .unwrap();
    let audit = fixture.audit(EXPIRED).await;
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].tenant_id, fixture.tenant);
    assert_eq!(audit[0].resource_id, Some(id.to_string()));
    assert_eq!(audit[0].platform_access_session_id, Some(id));
    assert_eq!(audit[0].details["session_id"], json!(id));
    assert_eq!(audit[0].details["reason"], REASON);

    assert_eq!(
        fixture.current_for(&fixture.sign_in).await.status(),
        StatusCode::NOT_FOUND
    );
    let other_tenant = seed_tenant(&fixture.pool).await;
    fixture.start(other_tenant).await;
    access_denial(tenant_request().await, "platform_access_required").await;

    assert_ne!(fixture.start(fixture.tenant).await, id);
    assert_eq!(tenant_request().await.status(), StatusCode::OK);
}

#[tokio::test]
async fn end_is_idempotent_and_tenant_requests_require_a_new_session() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let id = fixture.start(fixture.tenant).await;
    for _ in 0..2 {
        assert_eq!(
            fixture.end_for(&fixture.sign_in).await.status(),
            StatusCode::NO_CONTENT
        );
    }
    access_denial(
        fixture
            .tenant_request(&fixture.sign_in, fixture.tenant)
            .await,
        "platform_access_required",
    )
    .await;
    assert_eq!(fixture.count_open().await, 0);

    let audit = fixture.audit(ENDED).await;
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].tenant_id, fixture.tenant);
    assert_eq!(audit[0].resource_id, Some(id.to_string()));
    assert_eq!(audit[0].platform_access_session_id, Some(id));
    assert_eq!(audit[0].details["session_id"], json!(id));
    assert_eq!(audit[0].details["reason"], REASON);
    assert_eq!(audit[0].details["end_reason"], "ended");
}

#[tokio::test]
async fn switching_tenants_replaces_existing_access_and_audits_the_original_tenant() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let other_tenant = seed_tenant(&fixture.pool).await;
    let original = fixture.start(fixture.tenant).await;
    let replacement = fixture.start(other_tenant).await;
    assert_ne!(original, replacement);
    assert_eq!(fixture.count_open().await, 1);
    access_denial(
        fixture
            .tenant_request(&fixture.sign_in, fixture.tenant)
            .await,
        "platform_access_required",
    )
    .await;
    assert_eq!(
        fixture
            .tenant_request(&fixture.sign_in, other_tenant)
            .await
            .status(),
        StatusCode::OK
    );

    let audit = fixture.audit(ENDED).await;
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].tenant_id, fixture.tenant);
    assert_eq!(audit[0].platform_access_session_id, Some(original));
    assert_eq!(audit[0].details["end_reason"], "replaced");
}

#[tokio::test]
async fn signed_out_access_is_closed_once_by_the_sweep() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let id = fixture.start(fixture.tenant).await;
    assert_eq!(
        fixture
            .send(
                Method::POST,
                "/api/v1/auth/logout",
                &fixture.sign_in,
                None,
                None
            )
            .await
            .status(),
        StatusCode::NO_CONTENT,
    );
    let new_sign_in =
        session_fixture::start(&fixture.pool, &Environment::Test, fixture.staff).await;
    access_denial(
        fixture.tenant_request(&new_sign_in, fixture.tenant).await,
        "platform_access_required",
    )
    .await;
    let (first, second) = tokio::join!(
        tenancy::sweep_platform_access_sessions(&fixture.pool),
        tenancy::sweep_platform_access_sessions(&fixture.pool),
    );
    first.unwrap();
    second.unwrap();
    let audit = fixture.audit(ENDED).await;
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].platform_access_session_id, Some(id));
    assert_eq!(audit[0].details["end_reason"], "signed_out");
    assert_eq!(fixture.count_open().await, 0);
}

#[tokio::test]
async fn customer_audit_actions_link_to_access_and_the_tenant_owner_can_see_the_reason() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let id = fixture.start(fixture.tenant).await;
    let customers = "/api/v1/tenant/customers";
    let created = fixture.send(
        Method::POST, customers, &fixture.sign_in, Some(fixture.tenant),
        Some(json!({"display_name": "Session Customer", "email": "session-customer@example.com"})),
    ).await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let customer_id = uuid(&body(created).await["data"]["id"]);
    let updated = fixture
        .send(
            Method::PATCH,
            &format!("{customers}/{customer_id}"),
            &fixture.sign_in,
            Some(fixture.tenant),
            Some(json!({"display_name": "Updated Session Customer"})),
        )
        .await;
    assert_eq!(updated.status(), StatusCode::OK);

    let owner = seed_owner(&fixture.pool, fixture.tenant).await;
    let owner_view = fixture
        .send(
            Method::GET,
            "/api/v1/tenant/audit-logs",
            &owner,
            Some(fixture.tenant),
            None,
        )
        .await;
    assert_eq!(owner_view.status(), StatusCode::OK);
    let owner_log = body(owner_view).await;
    let rows = owner_log["data"].as_array().unwrap();
    for action in ["customer.created", "customer.updated", STARTED] {
        let row = rows.iter().find(|row| row["action"] == action).unwrap();
        assert_eq!(row["platform_access_session_id"], json!(id));
        assert_eq!(row["actor"]["is_platform_staff"], true);
        if action == STARTED {
            assert_eq!(row["details"]["reason"], REASON);
        }
    }

    assert_eq!(
        fixture.end_for(&fixture.sign_in).await.status(),
        StatusCode::NO_CONTENT
    );
    let platform_edit = fixture
        .send(
            Method::PATCH,
            &format!("/api/v1/platform/tenants/{}", fixture.tenant),
            &fixture.sign_in,
            None,
            Some(json!({"name": "Edited tenant name"})),
        )
        .await;
    assert_eq!(platform_edit.status(), StatusCode::OK);

    let route = format!("/api/v1/platform/audit-logs?tenant_id={}", fixture.tenant);
    let platform_view = fixture
        .send(Method::GET, &route, &fixture.sign_in, None, None)
        .await;
    assert_eq!(platform_view.status(), StatusCode::OK);
    let platform_log = body(platform_view).await;
    let rows = platform_log["data"].as_array().unwrap();
    for action in ["customer.created", "customer.updated", STARTED, ENDED] {
        let row = rows.iter().find(|row| row["action"] == action).unwrap();
        assert_eq!(row["platform_access_session_id"], json!(id));
    }
    let outside = rows
        .iter()
        .find(|row| row["action"] == "platform.tenant_updated")
        .unwrap();
    assert!(outside["platform_access_session_id"].is_null());
}

#[tokio::test]
async fn audit_uses_the_session_that_admitted_a_request_even_if_access_is_replaced_mid_request() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let admitted_session = fixture.start(fixture.tenant).await;
    let created = fixture
        .send(
            Method::POST,
            "/api/v1/tenant/customers",
            &fixture.sign_in,
            Some(fixture.tenant),
            Some(
                json!({"display_name": "Locked Customer", "email": "locked-customer@example.com"}),
            ),
        )
        .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let customer = uuid(&body(created).await["data"]["id"]);

    // Wait until the mutation reaches its database write, which runs after the access gate.
    let mut customer_lock = db::testing::seed_pool().begin().await.unwrap();
    let blocker_pid: i32 = sqlx::query_scalar(
        "SELECT pg_backend_pid() FROM customers WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(fixture.tenant)
    .bind(customer)
    .fetch_one(&mut *customer_lock)
    .await
    .unwrap();
    let route = format!("/api/v1/tenant/customers/{customer}");
    let update = fixture.send(
        Method::PATCH,
        &route,
        &fixture.sign_in,
        Some(fixture.tenant),
        Some(json!({"display_name": "Updated Locked Customer"})),
    );
    tokio::pin!(update);
    let wait_until_blocked = async {
        loop {
            let blocked: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                    SELECT 1 FROM pg_stat_activity
                    WHERE datname = current_database() AND $1 = ANY(pg_blocking_pids(pid))
                )",
            )
            .bind(blocker_pid)
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
            if blocked {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::select! {
        blocked = tokio::time::timeout(Duration::from_secs(5), wait_until_blocked) => {
            blocked.expect("the update should reach the locked customer row");
        }
        result = &mut update => panic!("customer update completed before the row lock was released: {}", result.status()),
    }

    let replacement = fixture.start(fixture.tenant).await;
    assert_ne!(replacement, admitted_session);
    customer_lock.rollback().await.unwrap();
    assert_eq!(update.await.status(), StatusCode::OK);

    let audit = fixture.audit("customer.updated").await;
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].resource_id, Some(customer.to_string()));
    assert_eq!(audit[0].platform_access_session_id, Some(admitted_session));
}

#[tokio::test]
async fn concurrent_switches_create_one_open_session_and_one_start_audit() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let other_tenant = seed_tenant(&fixture.pool).await;

    // Let both requests reach the database while keeping the winning transaction open.
    // The lock permits reads and blocks access-session mutations until the loser responds.
    let owner = db::testing::owner_pool();
    let mut blocker = owner.begin().await.unwrap();
    sqlx::query("LOCK TABLE platform_access_sessions IN SHARE MODE")
        .execute(&mut *blocker)
        .await
        .unwrap();

    let first = fixture.switch(fixture.tenant, json!({"reason": REASON}));
    let second = fixture.switch(other_tenant, json!({"reason": REASON}));
    tokio::pin!(first, second);
    let (failed_first, loser) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::select! {
            result = &mut first => (true, result),
            result = &mut second => (false, result),
        }
    })
    .await
    .expect("one overlapping start should fail before the winning write completes");
    assert_eq!(loser.status(), StatusCode::CONFLICT);
    blocker.rollback().await.unwrap();

    let winner = if failed_first {
        second.await
    } else {
        first.await
    };
    assert_eq!(winner.status(), StatusCode::OK);
    assert_eq!(fixture.count_open().await, 1);
    assert_eq!(fixture.audit(STARTED).await.len(), 1);
    assert!(fixture.audit(ENDED).await.is_empty());
}

#[tokio::test]
async fn starting_again_after_expiry_records_expiry_instead_of_replacement() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    let previous = fixture.start(fixture.tenant).await;
    expire(&fixture.pool, previous).await;

    let response = fixture
        .switch(
            fixture.tenant,
            json!({"reason": "界".repeat(10), "durationMinutes": 240}),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let summary = body(response).await;
    let session = &summary["accessSession"];
    assert_eq!(session["reason"], "界".repeat(10));
    let start: DateTime<Utc> = session["startedAt"].as_str().unwrap().parse().unwrap();
    let expiry: DateTime<Utc> = session["expiresAt"].as_str().unwrap().parse().unwrap();
    assert_eq!(expiry - start, chrono::Duration::hours(4));
    assert_ne!(uuid(&session["id"]), previous);

    let audit = fixture.audit(EXPIRED).await;
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].platform_access_session_id, Some(previous));
    assert!(fixture.audit(ENDED).await.is_empty());
}

#[tokio::test]
async fn a_platform_write_outside_the_access_gate_is_not_linked_to_a_session() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    // The access session belongs to the first sign-in. The same staff user then edits the tenant
    // through a platform route from a second sign-in. No access session authorized that request,
    // so its audit row must not be attached to the open session.
    let opened = fixture.start(fixture.tenant).await;
    let other_sign_in =
        session_fixture::start(&fixture.pool, &Environment::Test, fixture.staff).await;

    let updated = fixture
        .send(
            Method::PATCH,
            &format!("/api/v1/platform/tenants/{}", fixture.tenant),
            &other_sign_in,
            None,
            Some(json!({"contactName": "Edited Without Access"})),
        )
        .await;
    assert_eq!(updated.status(), StatusCode::OK);

    let edits = fixture.audit("platform.tenant_updated").await;
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].platform_access_session_id, None);

    let started = fixture.audit(STARTED).await;
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].platform_access_session_id, Some(opened));
}

#[tokio::test]
async fn closing_platform_access_ends_an_existing_event_stream() {
    let Some(fixture) = Fixture::new().await else {
        return;
    };
    sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, 'agent')",
    )
    .bind(fixture.tenant)
    .bind(fixture.staff)
    .execute(&db::testing::seed_pool())
    .await
    .unwrap();

    for explicit_end in [true, false] {
        let session_id = fixture.start(fixture.tenant).await;
        let response = fixture
            .send(
                Method::GET,
                "/api/v1/tenant/events",
                &fixture.sign_in,
                Some(fixture.tenant),
                None,
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "text/event-stream");
        let mut reader = tokio::spawn(response.into_body().collect());

        // The stream must be open before access changes, or a stream that closes by itself would
        // pass the check below without showing that ending access closed it.
        assert!(
            tokio::time::timeout(Duration::from_millis(500), &mut reader)
                .await
                .is_err(),
            "the event stream closed before tenant access ended"
        );

        if explicit_end {
            assert_eq!(
                fixture.end_for(&fixture.sign_in).await.status(),
                StatusCode::NO_CONTENT
            );
        } else {
            expire(&fixture.pool, session_id).await;
        }

        let result = tokio::time::timeout(Duration::from_secs(5), reader)
            .await
            .expect("event stream remained open after tenant access ended")
            .unwrap();
        assert!(result.is_ok());
    }
}
