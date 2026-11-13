//! Browser error reports (MOH-168), against real PostgreSQL and Redis: who may send them, what
//! is limited, what is logged and counted, and what is not.

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use server::router;
use server::state::AppState;
use tower::ServiceExt;
use tracing_subscriber::fmt::MakeWriter;
use uuid::Uuid;

const TEST_ORIGIN: &str = "http://localhost:4200";
const PASSWORD: &str = "correct-horse-battery-1";

// Every test resets the same tables, so they take turns.
static DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn test_config() -> config::AppConfig {
    config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: "redis://127.0.0.1:6379".into(),
        auth_jwt_secret: "test-auth-jwt-secret-at-least-32-bytes".into(),
        auth_session_ttl_seconds: 28_800,
        port: 0,
        bind_address: "0.0.0.0".into(),
        environment: config::Environment::Test,
        cors_allowed_origins: vec![TEST_ORIGIN.into()],
        log_format: config::LogFormat::Pretty,
        smtp_url: None,
        smtp_from: None,
        public_dashboard_url: "http://localhost:4200".into(),
        db_max_connections: 2,
        db_acquire_timeout_ms: 5000,
        ready_probe_timeout_ms: 5000,
        shutdown_grace_seconds: 1,
        docs_enabled: false,
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
        release_sha: Some("test-release".into()),
        ai_key_encryption_key: Some("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=".into()),
        integration_secrets_key: None,
        secrets_root_key: None,
        secrets_root_key_previous: None,
        auth_mfa_required: false,
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
        trusted_proxy: false,
    }
}

fn state(pool: sqlx::PgPool, config: config::AppConfig) -> AppState {
    AppState {
        config: Arc::new(config.clone()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool, &config).unwrap(),
        budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
    }
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let require = std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1");
    let Ok(url) = std::env::var("DATABASE_URL") else {
        assert!(!require, "REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
        return None;
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        assert!(
            !require,
            "REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable"
        );
        return None;
    }
    Some(pool)
}

async fn reset() {
    db::testing::migrate().await;
    sqlx::query(
        "TRUNCATE TABLE revoked_sessions, tenant_memberships, tenants, users, audit_logs \
         RESTART IDENTITY CASCADE",
    )
    .execute(&db::testing::owner_pool())
    .await
    .expect("failed to reset test tables");
}

fn hash(password: &str) -> String {
    identity::password::hash_password(password).unwrap()
}

async fn seed_user() -> Uuid {
    let email = format!("client-errors-{}@example.test", Uuid::new_v4().simple());
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name, password_hash) VALUES ($1, 'Reporter', $2) \
         RETURNING id",
    )
    .bind(&email)
    .bind(hash(PASSWORD))
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap()
}

macro_rules! setup {
    () => {{
        let Some(pool) = get_pool().await else { return };
        let _serial = DB_LOCK.lock().await;
        reset().await;
        let app = router::app(state(pool.clone(), test_config()));
        let user = seed_user().await;
        (app, user, _serial)
    }};
}

fn report_request(body: Value) -> Request<Body> {
    Request::builder()
        .uri("/api/v1/client-errors")
        .method(Method::POST)
        .header("origin", TEST_ORIGIN)
        .header("x-requested-with", "XMLHttpRequest")
        .header("content-type", "application/json")
        .header("user-agent", "Mozilla/5.0 (X11; Linux) TestBrowser/1.0")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn as_user(mut request: Request<Body>, user: Uuid) -> Request<Body> {
    request
        .headers_mut()
        .insert("X-Dev-User-Id", user.to_string().parse().unwrap());
    request
}

async fn send(app: &axum::Router, request: Request<Body>) -> Response {
    app.clone().oneshot(request).await.unwrap()
}

async fn json_body(response: Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

fn report(message: &str) -> Value {
    json!({ "kind": "error", "message": message })
}

/// Collects what `tracing` writes, so a test can read the log lines of the handler.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Captured {
    type Writer = Captured;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

impl Captured {
    /// The JSON log events of target `frontend`.
    fn frontend_events(&self) -> Vec<Value> {
        let text = String::from_utf8(self.0.lock().unwrap().clone()).unwrap();
        text.lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|v| v["target"] == "frontend")
            .collect()
    }
}

fn capture_logs() -> (Captured, tracing::subscriber::DefaultGuard) {
    let captured = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_writer(captured.clone())
        .with_max_level(tracing::Level::TRACE)
        .finish();
    (captured, tracing::subscriber::set_default(subscriber))
}

#[tokio::test]
async fn a_signed_in_user_reports_errors_and_each_one_is_logged_with_its_context() {
    let (app, user, _lock) = setup!();
    let (logs, _guard) = capture_logs();
    let tenant = Uuid::new_v4();
    let request_id = "req_0191f3a2-6b1e-7c3d-8e4f-5a6b7c8d9e0f";

    let response = send(
        &app,
        as_user(
            report_request(json!({ "reports": [
                {
                    "kind": "chunk_load",
                    "message": "Failed to fetch dynamically imported module",
                    "stack": "TypeError: x\n    at main-ABCD1234.js:1:2",
                    "route": "tenant/customers/:id",
                    "release": "4TYQZ6FJ",
                    "request_id": request_id,
                    "tenant_id": tenant,
                    "count": 3
                },
                { "kind": "unhandled_rejection", "message": "boom" }
            ] })),
            user,
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let events = logs.frontend_events();
    assert_eq!(events.len(), 2, "one log event per report: {events:?}");
    let first = &events[0]["fields"];
    assert_eq!(first["kind"], "chunk_load");
    assert_eq!(
        first["error_message"],
        "Failed to fetch dynamically imported module"
    );
    // The event's own text stays what the log query and the runbook search for.
    assert_eq!(first["message"], "frontend error");
    assert_eq!(first["route"], "tenant/customers/:id");
    assert_eq!(first["client_release"], "4TYQZ6FJ");
    assert_eq!(first["client_request_id"], request_id);
    assert_eq!(first["user_id"], user.to_string());
    assert_eq!(first["count"], 3);
    assert_eq!(
        first["user_agent"],
        "Mozilla/5.0 (X11; Linux) TestBrowser/1.0"
    );
    assert_eq!(events[1]["fields"]["kind"], "unhandled_rejection");
}

#[tokio::test]
async fn a_tenant_the_browser_names_is_logged_as_a_claim_never_as_the_tenant() {
    let (app, user, _lock) = setup!();
    let (logs, _guard) = capture_logs();
    let foreign = Uuid::new_v4(); // a workspace the user does not belong to

    let response = send(
        &app,
        as_user(
            report_request(json!({ "reports": [
                { "kind": "error", "message": "x", "tenant_id": foreign }
            ] })),
            user,
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let fields = &logs.frontend_events()[0]["fields"];
    assert_eq!(fields["claimed_tenant_id"], foreign.to_string());
    assert!(
        fields.get("tenant_id").is_none(),
        "the unverified claim must not look like a verified tenant: {fields}"
    );
}

#[tokio::test]
async fn control_characters_cannot_forge_a_log_line() {
    let (app, user, _lock) = setup!();
    let (logs, _guard) = capture_logs();

    let response = send(
        &app,
        as_user(
            report_request(json!({ "reports": [
                report("first\n{\"level\":\"ERROR\",\"fake\":true}\r\u{0}end")
            ] })),
            user,
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let message = logs.frontend_events()[0]["fields"]["error_message"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(!message.contains(['\n', '\r', '\u{0}']), "{message:?}");
    assert!(message.starts_with("first"));
}

#[tokio::test]
async fn reporting_needs_a_session() {
    let (app, _user, _lock) = setup!();

    let response = send(&app, report_request(json!({ "reports": [report("x")] }))).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

fn cookie_report(user: Uuid, headers: &[(&str, &str)]) -> Request<Body> {
    let (jwt, _, _) =
        identity::session::issue_token(&test_config().auth_jwt_secret, 28_800, user).unwrap();
    let mut builder = Request::builder()
        .uri("/api/v1/client-errors")
        .method(Method::POST)
        .header("cookie", format!("app_session={jwt}"))
        .header("content-type", "application/json");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    builder
        .body(Body::from(json!({ "reports": [report("x")] }).to_string()))
        .unwrap()
}

#[tokio::test]
async fn a_session_cookie_request_needs_the_allowed_origin_and_the_csrf_header() {
    let (app, user, _lock) = setup!();
    let (logs, _guard) = capture_logs();

    for (what, headers) in [
        ("no origin", vec![("x-requested-with", "XMLHttpRequest")]),
        (
            "a foreign origin",
            vec![
                ("origin", "https://evil.example"),
                ("x-requested-with", "XMLHttpRequest"),
            ],
        ),
        ("no custom header", vec![("origin", TEST_ORIGIN)]),
    ] {
        let response = send(&app, cookie_report(user, &headers)).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{what}");
    }
    assert!(
        logs.frontend_events().is_empty(),
        "a refused request logs nothing"
    );

    // The same request with both is accepted: the browser path the dashboard uses.
    let response = send(
        &app,
        cookie_report(
            user,
            &[
                ("origin", TEST_ORIGIN),
                ("x-requested-with", "XMLHttpRequest"),
            ],
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(logs.frontend_events().len(), 1);
}

#[tokio::test]
async fn a_body_over_the_limit_is_refused_and_nothing_is_logged() {
    let (app, user, _lock) = setup!();
    let (logs, _guard) = capture_logs();
    let big = "x".repeat(server::client_errors::MAX_BODY_BYTES + 1);

    let response = send(
        &app,
        as_user(report_request(json!({ "reports": [report(&big)] })), user),
    )
    .await;

    // `ApiJson` answers every unreadable body, the oversize one included, with 400.
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(logs.frontend_events().is_empty());
}

#[tokio::test]
async fn invalid_reports_are_refused_and_nothing_is_logged() {
    let (app, user, _lock) = setup!();
    let (logs, _guard) = capture_logs();
    let long = |n: usize| "x".repeat(n);
    let twenty_one: Vec<Value> = (0..21).map(|i| report(&format!("e{i}"))).collect();
    let cases: Vec<(&str, Value)> = vec![
        ("no reports", json!({ "reports": [] })),
        ("too many reports", json!({ "reports": twenty_one })),
        (
            "an unknown kind",
            json!({ "reports": [{ "kind": "http", "message": "x" }] }),
        ),
        ("an empty message", json!({ "reports": [report("   ")] })),
        (
            "a long message",
            json!({ "reports": [report(&long(1001))] }),
        ),
        (
            "a long stack",
            json!({ "reports": [{ "kind": "error", "message": "x", "stack": long(8001) }] }),
        ),
        (
            "a long route",
            json!({ "reports": [{ "kind": "error", "message": "x", "route": long(201) }] }),
        ),
        (
            "a release with a space",
            json!({ "reports": [{ "kind": "error", "message": "x", "release": "a b" }] }),
        ),
        (
            "a request id that is not one",
            json!({ "reports": [{ "kind": "error", "message": "x", "request_id": "abc" }] }),
        ),
        (
            "a tenant id that is not a UUID",
            json!({ "reports": [{ "kind": "error", "message": "x", "tenant_id": "acme" }] }),
        ),
        (
            "a count of zero",
            json!({ "reports": [{ "kind": "error", "message": "x", "count": 0 }] }),
        ),
        (
            "a huge count",
            json!({ "reports": [{ "kind": "error", "message": "x", "count": 1001 }] }),
        ),
        (
            "one valid report beside an invalid one",
            json!({ "reports": [report("fine"), report("")] }),
        ),
    ];

    for (what, body) in cases {
        let response = send(&app, as_user(report_request(body), user)).await;
        assert!(
            response.status().is_client_error(),
            "{what}: {}",
            response.status()
        );
    }
    let malformed = Request::builder()
        .uri("/api/v1/client-errors")
        .method(Method::POST)
        .header("origin", TEST_ORIGIN)
        .header("x-requested-with", "XMLHttpRequest")
        .header("content-type", "application/json")
        .body(Body::from("{not json"))
        .unwrap();
    let response = send(&app, as_user(malformed, user)).await;
    assert!(response.status().is_client_error(), "{}", response.status());

    assert!(
        logs.frontend_events().is_empty(),
        "an invalid batch must not be logged, not even its valid part"
    );
}

#[tokio::test]
async fn a_user_has_a_budget_of_requests() {
    let (app, user, _lock) = setup!();
    let body = json!({ "reports": [report("x")] });

    for i in 0..server::client_errors::REQUESTS_PER_USER {
        let response = send(&app, as_user(report_request(body.clone()), user)).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT, "request {i}");
    }
    let refused = send(&app, as_user(report_request(body.clone()), user)).await;

    assert_eq!(refused.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(refused.headers().contains_key("retry-after"));
    assert_eq!(json_body(refused).await["error"]["code"], "rate_limited");

    // Another user is not affected by this one's budget.
    let other = seed_user().await;
    let response = send(&app, as_user(report_request(body), other)).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn reports_are_counted_by_kind_and_release_only() {
    let (app, user, _lock) = setup!();
    observability::metrics::init();

    let response = send(
        &app,
        as_user(
            report_request(json!({ "reports": [
                { "kind": "chunk_load", "message": "x", "release": "client-chosen-label",
                  "route": "tenant/x", "tenant_id": Uuid::new_v4(), "count": 4 }
            ] })),
            user,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let text = observability::metrics::prometheus_handle().render();
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("frontend_errors_total{"))
        .collect();
    assert!(
        lines
            .iter()
            .any(|l| l.contains("kind=\"chunk_load\"") && l.contains("release=\"test-release\"")),
        "{lines:?}"
    );
    for line in lines {
        assert!(!line.contains("client-chosen-label"), "{line}");
        assert!(!line.contains("tenant"), "{line}");
        assert!(!line.contains(&user.to_string()), "{line}");
    }
}

/// The value of `frontend_errors_total{kind="<kind>", release="test-release"}` now, 0 if absent.
fn counter(kind: &str) -> f64 {
    let text = observability::metrics::prometheus_handle().render();
    let prefix = format!("frontend_errors_total{{kind=\"{kind}\",release=\"test-release\"}} ");
    text.lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0.0)
}

#[tokio::test]
async fn one_user_can_add_only_a_small_budget_to_the_alert_counter() {
    let (app, user, _lock) = setup!();
    let (logs, _guard) = capture_logs();
    observability::metrics::init();
    let before = counter("chunk_load");
    let budget = server::client_errors::COUNTED_PER_USER;
    // A request is as large as it can be: twenty reports, each claiming a huge count.
    let reports: Vec<Value> = (0..20)
        .map(|i| json!({ "kind": "chunk_load", "message": format!("e{i}"), "count": 1000 }))
        .collect();

    for _ in 0..3 {
        let response = send(
            &app,
            as_user(report_request(json!({ "reports": reports })), user),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    // Sixty reports, each claiming 1000: the counter moved by the user's budget, no more.
    assert_eq!(counter("chunk_load") - before, f64::from(budget));
    // Every report is still logged, with the claim as sent.
    let events = logs.frontend_events();
    assert_eq!(events.len(), 60);
    assert_eq!(events[0]["fields"]["count"], 1000);

    // Another user has a budget of their own: a real spike is many users.
    let other = seed_user().await;
    let response = send(
        &app,
        as_user(
            report_request(json!({ "reports": [
                { "kind": "chunk_load", "message": "z", "count": 1000 }
            ] })),
            other,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(counter("chunk_load") - before, f64::from(budget) + 1.0);
}
