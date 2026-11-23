#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod session_fixture;

use axum::body::{to_bytes, Body};
use axum::extract::connect_info::ConnectInfo;
use axum::http::{header, Request, StatusCode};
use axum::response::Response;
use axum::Router;
use serde_json::{json, Value};
use server::{router, state::AppState};
use sqlx::PgPool;
use std::net::{Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

const ORIGIN: &str = "http://localhost:4200";
const OLD_PASSWORD: &str = "previous password";
const NEW_PASSWORD: &str = "new password";

async fn pool() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL");
    if std::env::var("REQUIRE_DB_TESTS").as_deref() != Ok("1") && url.is_err() {
        return None;
    }
    let pool = db::lazy_pool(&url.expect("DATABASE_URL"), 8, Duration::from_secs(5));
    static MIGRATED: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
    MIGRATED.get_or_init(db::testing::migrate).await;
    Some(pool)
}

fn app(pool: &PgPool, redis_url: &str) -> Router {
    let config = test_config(redis_url);
    router::app(AppState {
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new(redis_url).unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool.clone(), &config).unwrap(),
        budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
        config: Arc::new(config),
    })
}

async fn seed(pool: &PgPool, password_enabled: bool) -> (Uuid, String) {
    let email = format!("reset-{}@example.com", Uuid::new_v4());
    let hash = password_enabled.then(|| identity::password::hash_password(OLD_PASSWORD).unwrap());
    let id = sqlx::query_scalar(
        "INSERT INTO users (email, display_name, password_hash, platform_role)
         VALUES ($1, 'Reset test', $2, 'support') RETURNING id",
    )
    .bind(&email)
    .bind(hash)
    .fetch_one(pool)
    .await
    .unwrap();
    (id, email)
}

fn redis_url() -> String {
    std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into())
}

// A separate address per fixture keeps real Redis counters independent.
fn ip() -> SocketAddr {
    (Ipv6Addr::from(*Uuid::new_v4().as_bytes()), 12345).into()
}

fn request(path: &str, body: Value, addr: SocketAddr) -> Request<Body> {
    Request::post(format!("/api/v1/auth/{path}"))
        .extension(ConnectInfo(addr))
        .header(header::ORIGIN, ORIGIN)
        .header("x-requested-with", "XMLHttpRequest")
        .header(header::ACCEPT, "application/problem+json")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn send(app: &Router, req: Request<Body>) -> Response {
    app.clone().oneshot(req).await.unwrap()
}

async fn body(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 16384).await.unwrap()).unwrap()
}

#[tokio::test]
async fn forgot_enqueues_the_same_work_for_known_and_unknown_email() {
    let Some(pool) = pool().await else { return };
    let _serial = QUEUE.lock().await;
    let app = app(&pool, &redis_url());
    let (_, known) = seed(&pool, true).await;
    let unknown = format!("reset-{}@example.com", Uuid::new_v4());
    let addr = ip();
    let forgot = |email: &str| request("password/forgot", json!({"email": email}), addr);
    let known_response = send(&app, forgot(&format!(" {} ", known.to_uppercase()))).await;
    let unknown_response = send(&app, forgot(&unknown)).await;
    assert_eq!(known_response.status(), StatusCode::ACCEPTED);
    assert_eq!(unknown_response.status(), known_response.status());
    let stable_headers = |response: &Response| {
        let mut headers = response.headers().clone();
        headers.remove("x-request-id");
        headers
    };
    assert_eq!(
        stable_headers(&known_response),
        stable_headers(&unknown_response)
    );
    assert_eq!(body(known_response).await, body(unknown_response).await);
    let queued: Vec<(String, i32, bool)> = sqlx::query_as(
        "SELECT email, attempts, ip = $3::text::inet
         FROM password_reset_requests WHERE email IN ($1,$2) ORDER BY email",
    )
    .bind(&known)
    .bind(&unknown)
    .bind(addr.ip().to_string())
    .fetch_all(&pool)
    .await
    .unwrap();
    let mut expected = vec![(known, 0, true), (unknown, 0, true)];
    expected.sort();
    assert_eq!(queued, expected);
    let (depth, oldest_age) = identity::password_reset::queue_health(&pool).await.unwrap();
    assert!(depth >= 2);
    assert!(oldest_age >= 0.0);
    for (email, _, _) in expected {
        sqlx::query("DELETE FROM password_reset_requests WHERE email = $1")
            .bind(email)
            .execute(&pool)
            .await
            .unwrap();
    }
}

fn test_config(redis_url: &str) -> config::AppConfig {
    config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: redis_url.into(),
        port: 0,
        bind_address: "0.0.0.0".into(),
        environment: config::Environment::Test,
        cors_allowed_origins: vec!["http://localhost:4200".into()],
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
        password_reset_limits: Default::default(),
        rate_limit_max_override: 10000,
        rate_limit_fail_mode: "auth_closed".into(),
        security_headers_enabled: false,
        csp_policy: String::new(),
        csp_report_only: false,
        hsts_max_age: 0,
        otlp_endpoint: None,
        trace_sample_ratio: 0.0,
        sentry_dsn: None,
        release_sha: None,
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
        outbound_http: config::OutboundHttpConfig::default(),
    }
}

static QUEUE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Default)]
struct Capture(std::sync::Mutex<Vec<email::EmailMessage>>);

#[async_trait::async_trait]
impl email::EmailSender for Capture {
    fn is_configured(&self) -> bool {
        true
    }

    async fn send(&self, message: email::EmailMessage) -> email::EmailDeliveryStatus {
        self.0.lock().unwrap().push(message);
        email::EmailDeliveryStatus::Sent
    }
}

async fn deliver(pool: &PgPool, capture: &Capture) {
    while identity::password_reset::process_delivery_once(pool, capture, ORIGIN)
        .await
        .unwrap()
    {}
}

fn email_token(capture: &Capture, to: &str) -> String {
    let messages = capture.0.lock().unwrap();
    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.to, to);
    assert!(message.body_text.contains("30 minutes"));
    assert!(message.body_text.contains("ignore"));
    let link = message.body_text.lines().nth(1).unwrap();
    link.strip_prefix(&format!("{ORIGIN}/reset-password/"))
        .unwrap()
        .to_owned()
}

async fn forgot(app: &Router, email: &str, addr: SocketAddr) -> Response {
    send(
        app,
        request("password/forgot", json!({"email": email}), addr),
    )
    .await
}

async fn issue(pool: &PgPool, app: &Router, email: &str) -> String {
    assert_eq!(
        forgot(app, email, ip()).await.status(),
        StatusCode::ACCEPTED
    );
    let capture = Capture::default();
    deliver(pool, &capture).await;
    email_token(&capture, email)
}

async fn reset(app: &Router, token: &str, password: &str) -> Response {
    send(
        app,
        request(
            "password/reset",
            json!({"token": token, "password": password}),
            ip(),
        ),
    )
    .await
}

async fn generic_token_failure(response: Response) -> Value {
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/problem+json"
    );
    let mut problem = body(response).await;
    assert_eq!(problem["code"], "reset_token_invalid");
    problem.as_object_mut().unwrap().remove("request_id");
    problem
}

#[tokio::test]
async fn worker_delivers_only_to_password_enabled_non_deleted_users_and_stores_only_the_hash() {
    let Some(pool) = pool().await else { return };
    let _serial = QUEUE.lock().await;
    let app = app(&pool, &redis_url());
    let (known, email) = seed(&pool, true).await;
    let (disabled, disabled_email) = seed(&pool, false).await;
    let (deleted, deleted_email) = seed(&pool, true).await;
    sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
        .bind(deleted)
        .execute(&pool)
        .await
        .unwrap();
    let unknown = format!("unknown-{}@example.com", Uuid::new_v4());
    let emails = [&email, &disabled_email, &deleted_email, &unknown];
    for address in emails {
        assert_eq!(
            forgot(&app, address, ip()).await.status(),
            StatusCode::ACCEPTED
        );
    }
    let capture = Capture::default();
    deliver(&pool, &capture).await;
    let token = email_token(&capture, &email);

    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use sha2::{Digest, Sha256};
    assert_eq!(token.len(), 43);
    assert_eq!(URL_SAFE_NO_PAD.decode(&token).unwrap().len(), 32);
    let rows: Vec<(Uuid, String, bool, String)> = sqlx::query_as(
        "SELECT user_id, token_hash, expires_at - created_at <= interval '30 minutes', t::text
         FROM password_reset_tokens t WHERE user_id IN ($1,$2,$3)",
    )
    .bind(known)
    .bind(disabled)
    .bind(deleted)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, known);
    assert_eq!(rows[0].1, hex::encode(Sha256::digest(token.as_bytes())));
    assert!(rows[0].2);
    assert!(!rows[0].3.contains(&token));
    let pending: i64 =
        sqlx::query_scalar("SELECT count(*) FROM password_reset_requests WHERE email = ANY($1)")
            .bind(emails.map(String::as_str).to_vec())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pending, 0);
}

#[tokio::test]
async fn reset_links_are_single_use_and_generic_for_expired_malformed_unknown_and_superseded_tokens(
) {
    let Some(pool) = pool().await else { return };
    let _serial = QUEUE.lock().await;
    let app = app(&pool, &redis_url());
    let (user, email) = seed(&pool, true).await;

    let expired = issue(&pool, &app, &email).await;
    sqlx::query(
        "UPDATE password_reset_tokens SET created_at = now() - interval '31 minutes', expires_at = now() - interval '1 minute'
         WHERE user_id = $1",
    )
    .bind(user).execute(&pool).await.unwrap();
    let expected = generic_token_failure(reset(&app, &expired, NEW_PASSWORD).await).await;
    for token in ["malformed", &"A".repeat(43)] {
        assert_eq!(
            generic_token_failure(reset(&app, token, NEW_PASSWORD).await).await,
            expected
        );
    }

    let superseded = issue(&pool, &app, &email).await;
    let current = issue(&pool, &app, &email).await;
    assert_eq!(
        generic_token_failure(reset(&app, &superseded, NEW_PASSWORD).await).await,
        expected
    );
    let weak = reset(&app, &current, "short").await;
    assert_eq!(weak.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body(weak).await["errors"],
        json!([{"field":"password","code":"too_short"}])
    );
    let response = reset(&app, &current, NEW_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(!response.headers().contains_key(header::SET_COOKIE));
    assert_eq!(
        generic_token_failure(reset(&app, &current, NEW_PASSWORD).await).await,
        expected
    );
}

async fn login(app: &Router, email: &str, password: &str) -> Response {
    send(
        app,
        request("login", json!({"email": email, "password": password}), ip()),
    )
    .await
}

fn access_request(token: &str) -> Request<Body> {
    Request::get("/api/v1/me")
        .header(header::COOKIE, format!("__Host-app_session={token}"))
        .body(Body::empty())
        .unwrap()
}

fn refresh_request(token: &str) -> Request<Body> {
    let mut req = request("refresh", json!({}), ip());
    req.headers_mut().insert(
        header::COOKIE,
        format!("__Secure-app_refresh={token}").parse().unwrap(),
    );
    req
}

#[tokio::test]
async fn reset_changes_credentials_revokes_every_session_and_records_a_secret_free_audit_event() {
    let Some(pool) = pool().await else { return };
    let _serial = QUEUE.lock().await;
    let app = app(&pool, &redis_url());
    let (user, email) = seed(&pool, true).await;
    let (other, _) = seed(&pool, true).await;
    let environment = config::Environment::Test;
    let first = session_fixture::start(&pool, &environment, user).await;
    let second = session_fixture::start(&pool, &environment, user).await;
    let untouched = session_fixture::start(&pool, &environment, other).await;
    assert_eq!(
        send(&app, access_request(&first.access_token))
            .await
            .status(),
        StatusCode::OK
    );
    let token = issue(&pool, &app, &email).await;
    assert_eq!(
        reset(&app, &token, NEW_PASSWORD).await.status(),
        StatusCode::NO_CONTENT
    );

    for session in [first, second] {
        assert_eq!(
            send(&app, access_request(&session.access_token))
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(&app, refresh_request(&session.refresh_token))
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let reason: String =
            sqlx::query_scalar("SELECT revoke_reason FROM auth_sessions WHERE id = $1")
                .bind(session.sid)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(reason, "password_reset");
    }
    assert_eq!(
        send(&app, access_request(&untouched.access_token))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(&app, refresh_request(&untouched.refresh_token))
            .await
            .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        login(&app, &email, OLD_PASSWORD).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&app, &email, NEW_PASSWORD).await.status(),
        StatusCode::OK
    );

    let audits: Vec<(String, Value)> = sqlx::query_as(
        "SELECT resource_type, details FROM audit_logs
         WHERE action = 'auth.password_reset' AND actor_user_id = $1 AND resource_id = $2",
    )
    .bind(user)
    .bind(user.to_string())
    .fetch_all(&db::testing::seed_pool())
    .await
    .unwrap();
    assert_eq!(audits, vec![("user".into(), json!({}))]);
}

fn pending_cookie(response: &Response) -> String {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("app_mfa_pending="))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn mfa_survives_reset_and_old_challenges_stop_working() {
    let Some(pool) = pool().await else { return };
    let _serial = QUEUE.lock().await;
    let redis = redis_url();
    let app = app(&pool, &redis);
    let (user, email) = seed(&pool, true).await;
    use identity::{mfa, totp};
    let keys = mfa::MfaKeys::from_config(&pool, &test_config(&redis)).unwrap();
    let enrolment = mfa::begin_enrolment(&pool, &keys, user, &email)
        .await
        .unwrap();
    let now = chrono::Utc::now().timestamp() as u64;
    let secret = totp::base32_decode(&enrolment.secret).unwrap();
    let code = totp::code_at(&secret, now, 6);
    let recovery = mfa::confirm_enrolment(&pool, &keys, user, &code, now)
        .await
        .unwrap();
    let before = login(&app, &email, OLD_PASSWORD).await;
    assert_eq!(before.status(), StatusCode::ACCEPTED);
    let stale_cookie = pending_cookie(&before);
    let token = issue(&pool, &app, &email).await;
    assert_eq!(
        reset(&app, &token, NEW_PASSWORD).await.status(),
        StatusCode::NO_CONTENT
    );
    assert!(mfa::is_enrolled(&pool, user).await.unwrap());

    let verify = |cookie: &str| {
        let mut req = request("mfa/verify", json!({"recoveryCode": recovery[0]}), ip());
        req.headers_mut()
            .insert(header::COOKIE, cookie.parse().unwrap());
        req
    };
    assert_eq!(
        send(&app, verify(&stale_cookie)).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let after = login(&app, &email, NEW_PASSWORD).await;
    assert_eq!(after.status(), StatusCode::ACCEPTED);
    assert_eq!(
        send(&app, verify(&pending_cookie(&after))).await.status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn concurrent_redemption_changes_the_password_once() {
    let Some(pool) = pool().await else { return };
    let _serial = QUEUE.lock().await;
    let app = app(&pool, &redis_url());
    let (_, email) = seed(&pool, true).await;
    let token = issue(&pool, &app, &email).await;
    let (a, b) = tokio::join!(
        reset(&app, &token, NEW_PASSWORD),
        reset(&app, &token, NEW_PASSWORD)
    );
    let statuses = [a.status(), b.status()];
    assert!(statuses.contains(&StatusCode::NO_CONTENT));
    assert!(statuses.contains(&StatusCode::BAD_REQUEST));
}

#[tokio::test]
async fn retention_removes_expired_and_used_tokens_and_keeps_a_live_link() {
    let Some(pool) = pool().await else { return };
    let _serial = QUEUE.lock().await;
    let app = app(&pool, &redis_url());
    let (user, email) = seed(&pool, true).await;
    issue(&pool, &app, &email).await;
    issue(&pool, &app, &email).await;
    sqlx::query(
        "UPDATE password_reset_tokens SET created_at = now() - interval '31 minutes', expires_at = now() - interval '1 minute'
         WHERE user_id = $1 AND used_at IS NOT NULL",
    )
    .bind(user).execute(&pool).await.unwrap();
    assert!(identity::password_reset::sweep_tokens(&pool).await.unwrap() >= 1);
    let remaining: Vec<bool> =
        sqlx::query_scalar("SELECT used_at IS NULL FROM password_reset_tokens WHERE user_id = $1")
            .bind(user)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, vec![true]);
}

#[tokio::test]
async fn forgot_has_separate_email_and_ip_budgets_and_reset_has_its_own_ip_budget() {
    let Some(pool) = pool().await else { return };
    let _serial = QUEUE.lock().await;
    let redis = redis_url();
    let app = app(&pool, &redis);
    let limits = test_config(&redis).password_reset_limits;
    let (_, known) = seed(&pool, true).await;
    let unknown = format!("unknown-{}@example.com", Uuid::new_v4());
    let mut refused = Vec::new();
    for email in [&known, &unknown] {
        for _ in 0..limits.forgot_email_max {
            assert_eq!(
                forgot(&app, email, ip()).await.status(),
                StatusCode::ACCEPTED
            );
        }
        let response = forgot(&app, &email.to_uppercase(), ip()).await;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(response.headers().contains_key(header::RETRY_AFTER));
        let mut answer = body(response).await;
        answer.as_object_mut().unwrap().remove("request_id");
        refused.push(answer);
    }
    assert_eq!(refused[0], refused[1]);

    let address = ip();
    let prefix = Uuid::new_v4();
    for i in 0..limits.forgot_ip_max {
        assert_eq!(
            forgot(&app, &format!("{prefix}-{i}@example.com"), address)
                .await
                .status(),
            StatusCode::ACCEPTED
        );
    }
    assert_eq!(
        forgot(&app, &format!("{prefix}@example.com"), address)
            .await
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );

    // The forgot address limit never blocks redeeming the link from the same address.
    let redeem = || {
        request(
            "password/reset",
            json!({"token": "invalid", "password": NEW_PASSWORD}),
            address,
        )
    };
    for _ in 0..limits.reset_ip_max {
        assert_eq!(send(&app, redeem()).await.status(), StatusCode::BAD_REQUEST);
    }
    assert_eq!(
        send(&app, redeem()).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    deliver(&pool, &Capture::default()).await;
}

#[tokio::test]
async fn password_recovery_fails_closed_during_a_redis_outage() {
    let Some(pool) = pool().await else { return };
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let unused = listener.local_addr().unwrap();
    drop(listener);
    let app = app(&pool, &format!("redis://{unused}"));
    assert_eq!(
        forgot(&app, "someone@example.com", ip()).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        reset(&app, "invalid", NEW_PASSWORD).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn recovery_requires_the_origin_and_ajax_header() {
    let Some(pool) = pool().await else { return };
    let app = app(&pool, &redis_url());
    for action in ["forgot", "reset"] {
        for missing in ["origin", "x-requested-with"] {
            let mut req = request(&format!("password/{action}"), json!({}), ip());
            req.headers_mut().remove(missing);
            req.headers_mut()
                .insert("x-tenant-id", "untrusted".parse().unwrap());
            assert_eq!(send(&app, req).await.status(), StatusCode::FORBIDDEN);
        }
    }
}

struct DeferredDelivery;

#[async_trait::async_trait]
impl email::EmailSender for DeferredDelivery {
    fn is_configured(&self) -> bool {
        true
    }

    async fn send(&self, _message: email::EmailMessage) -> email::EmailDeliveryStatus {
        email::EmailDeliveryStatus::Failed("Local test delivery did not complete".into())
    }
}

#[tokio::test]
async fn newer_queued_request_supersedes_a_link_and_an_older_smtp_retry() {
    let Some(pool) = pool().await else { return };
    let _serial = QUEUE.lock().await;
    let app = app(&pool, &redis_url());
    let (user, email) = seed(&pool, true).await;
    let prior = issue(&pool, &app, &email).await;
    assert_eq!(
        forgot(&app, &email, ip()).await.status(),
        StatusCode::ACCEPTED
    );
    generic_token_failure(reset(&app, &prior, NEW_PASSWORD).await).await;

    identity::password_reset::process_delivery_once(&pool, &DeferredDelivery, ORIGIN)
        .await
        .unwrap();
    let retry: Uuid = sqlx::query_scalar(
        "SELECT id FROM password_reset_requests WHERE email = $1 AND attempts = 1
         AND available_at > clock_timestamp()",
    )
    .bind(&email)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        forgot(&app, &email, ip()).await.status(),
        StatusCode::ACCEPTED
    );
    let capture = Capture::default();
    let dashboard_base = format!("{ORIGIN}/app/");
    identity::password_reset::process_delivery_once(&pool, &capture, &dashboard_base)
        .await
        .unwrap();
    let link = capture.0.lock().unwrap()[0]
        .body_text
        .lines()
        .nth(1)
        .unwrap()
        .to_owned();
    let token = link
        .strip_prefix(&format!("{ORIGIN}/app/reset-password/"))
        .unwrap();

    sqlx::query(
        "UPDATE password_reset_requests SET available_at = clock_timestamp() WHERE id = $1",
    )
    .bind(retry)
    .execute(&pool)
    .await
    .unwrap();
    let ignored = Capture::default();
    deliver(&pool, &ignored).await;
    assert!(ignored.0.lock().unwrap().is_empty());
    let active_request: Uuid = sqlx::query_scalar(
        "SELECT request_id FROM password_reset_tokens WHERE user_id = $1 AND used_at IS NULL",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_ne!(active_request, retry);
    assert_eq!(
        reset(&app, token, NEW_PASSWORD).await.status(),
        StatusCode::NO_CONTENT
    );
}
