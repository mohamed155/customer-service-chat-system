#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    net::{Ipv6Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    body::{to_bytes, Body},
    extract::connect_info::ConnectInfo,
    http::{header, HeaderMap, Request, StatusCode},
    response::Response,
};
use email::{EmailDeliveryStatus, EmailMessage, EmailSender};
use serde_json::json;
use server::{router, state::AppState};
use tower::ServiceExt;
use uuid::Uuid;

const PASSWORD: &str = "CorrectHorse1!";
const ORIGIN: &str = "http://localhost:4200";
static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static MIGRATED: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();

async fn database() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            assert_ne!(
                std::env::var("REQUIRE_DB_TESTS").as_deref(),
                Ok("1"),
                "REQUIRE_DB_TESTS=1 but DATABASE_URL is not set"
            );
            eprintln!("skipping login lockout test: DATABASE_URL is not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 8, Duration::from_secs(5));
    if let Err(error) = sqlx::query("SELECT 1").execute(&pool).await {
        assert_ne!(
            std::env::var("REQUIRE_DB_TESTS").as_deref(),
            Ok("1"),
            "REQUIRE_DB_TESTS=1 but PostgreSQL is unreachable: {error}"
        );
        eprintln!("skipping login lockout test: PostgreSQL is unreachable");
        return None;
    }
    Some(pool)
}

async fn redis_url() -> Option<String> {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
    let cache = match cache::Cache::new(&url) {
        Ok(cache) => cache,
        Err(error) => {
            assert_ne!(
                std::env::var("REQUIRE_DB_TESTS").as_deref(),
                Ok("1"),
                "REQUIRE_DB_TESTS=1 but Redis client setup failed: {error}"
            );
            return None;
        }
    };
    if cache.ping().await.is_err() {
        assert_ne!(
            std::env::var("REQUIRE_DB_TESTS").as_deref(),
            Ok("1"),
            "Redis is unreachable"
        );
        eprintln!("skipping login lockout test: Redis is unreachable");
        return None;
    }
    Some(url)
}

async fn services() -> Option<(sqlx::PgPool, String)> {
    let pool = database().await?;
    let redis = redis_url().await?;
    MIGRATED.get_or_init(db::testing::migrate).await;
    Some((pool, redis))
}

fn config(redis_url: &str) -> config::AppConfig {
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
        login_lockout: Default::default(),
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

fn app(pool: sqlx::PgPool, redis_url: &str) -> axum::Router {
    let config = config(redis_url);
    router::app(AppState {
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new(redis_url).unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool, &config).unwrap(),
        budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
        config: Arc::new(config),
    })
}

fn ip() -> SocketAddr {
    (Ipv6Addr::from(*Uuid::new_v4().as_bytes()), 12345).into()
}

fn login(email: &str, password: &str, ip: SocketAddr) -> Request<Body> {
    Request::post("/api/v1/auth/login")
        .extension(ConnectInfo(ip))
        .header(header::ORIGIN, "http://localhost:4200")
        .header("x-requested-with", "XMLHttpRequest")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"email": email, "password": password}).to_string(),
        ))
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, password: &str) -> (Uuid, String) {
    let email = format!("lockout-{}@example.com", Uuid::new_v4());
    let password_hash = identity::password::hash_password(password).unwrap();
    let user_id = sqlx::query_scalar(
        "INSERT INTO users (email, display_name, password_hash)
         VALUES ($1, 'Lockout test', $2) RETURNING id",
    )
    .bind(&email)
    .bind(password_hash)
    .fetch_one(pool)
    .await
    .unwrap();
    (user_id, email)
}

async fn seed_passwordless_user(pool: &sqlx::PgPool) -> (Uuid, String) {
    let email = format!("passwordless-lockout-{}@example.com", Uuid::new_v4());
    let user_id = sqlx::query_scalar(
        "INSERT INTO users (email, display_name, password_hash)
         VALUES ($1, 'Passwordless lockout test', NULL) RETURNING id",
    )
    .bind(&email)
    .fetch_one(pool)
    .await
    .unwrap();
    (user_id, email)
}

async fn audit_count(action: &str, resource_id: &str, reason: Option<&str>) -> i64 {
    let mut tx = db::testing::owner_tx().await;
    db::testing::become_platform(&mut tx).await;
    let count = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs
         WHERE action = $1 AND resource_id = $2
           AND ($3::text IS NULL OR details->>'reason' = $3)",
    )
    .bind(action)
    .bind(resource_id)
    .bind(reason)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    count
}

async fn login_failure_count(email: &str, reason: &str) -> i64 {
    audit_count("auth.login_failed", email, Some(reason)).await
}

async fn outbox_count(pool: &sqlx::PgPool, user_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM login_lockout_notifications WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn error_signature(response: Response) -> (StatusCode, HeaderMap, serde_json::Value) {
    let status = response.status();
    let mut headers = response.headers().clone();
    headers.remove("x-request-id");
    let bytes = to_bytes(response.into_body(), 16 * 1024).await.unwrap();
    let mut body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    if let Some(error) = body
        .get_mut("error")
        .and_then(serde_json::Value::as_object_mut)
    {
        error.remove("request_id");
    }
    (status, headers, body)
}

struct CaptureSender {
    messages: Mutex<Vec<EmailMessage>>,
    result: EmailDeliveryStatus,
}

impl CaptureSender {
    fn new(result: EmailDeliveryStatus) -> Self {
        Self {
            messages: Mutex::new(Vec::new()),
            result,
        }
    }

    fn messages(&self) -> Vec<EmailMessage> {
        self.messages.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl EmailSender for CaptureSender {
    fn is_configured(&self) -> bool {
        true
    }

    async fn send(&self, message: EmailMessage) -> EmailDeliveryStatus {
        self.messages.lock().unwrap().push(message);
        self.result.clone()
    }
}

#[tokio::test]
async fn distributed_ip_guesses_lock_the_account_before_a_correct_password_can_start_a_session() {
    let _serial = TEST_LOCK.lock().await;
    let Some((pool, redis_url)) = services().await else {
        return;
    };
    let (user_id, email) = seed_user(&pool, PASSWORD).await;
    let app = app(pool.clone(), &redis_url);

    let unlocked_wrong = app
        .clone()
        .oneshot(login(&email, "wrong-password", ip()))
        .await
        .unwrap();
    assert_eq!(unlocked_wrong.status(), StatusCode::UNAUTHORIZED);
    for _ in 1..5 {
        let response = app
            .clone()
            .oneshot(login(&email, "wrong-password", ip()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let locked: (i32, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT failed_login_count, locked_until FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(locked.0, 5);
    assert!(locked.1.is_some());

    let wrong_locked = app
        .clone()
        .oneshot(login(&email, "wrong-password", ip()))
        .await
        .unwrap();
    let correct_locked = app
        .clone()
        .oneshot(login(&email, PASSWORD, ip()))
        .await
        .unwrap();
    let unknown = app
        .oneshot(login(
            &format!("missing-{}@example.com", Uuid::new_v4()),
            "wrong-password",
            ip(),
        ))
        .await
        .unwrap();
    let expected = error_signature(unlocked_wrong).await;
    assert_eq!(error_signature(wrong_locked).await, expected);
    assert_eq!(error_signature(correct_locked).await, expected);
    assert_eq!(error_signature(unknown).await, expected);
    assert!(expected.1.get(header::SET_COOKIE).is_none());
    assert!(expected.1.get("retry-after").is_none());

    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM auth_sessions WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(sessions, 0, "a locked account must not start a session");
    assert_eq!(
        audit_count("auth.account_locked", &user_id.to_string(), None).await,
        1
    );
    assert_eq!(outbox_count(&pool, user_id).await, 1);
    sqlx::query("DELETE FROM login_lockout_notifications WHERE user_id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn passwordless_accounts_follow_the_unknown_email_path_without_lockout_or_email() {
    let _serial = TEST_LOCK.lock().await;
    let Some((pool, redis_url)) = services().await else {
        return;
    };
    let (user_id, email) = seed_passwordless_user(&pool).await;
    let app = app(pool.clone(), &redis_url);

    let first = app
        .clone()
        .oneshot(login(&email, "wrong-password", ip()))
        .await
        .unwrap();
    let unknown = app
        .clone()
        .oneshot(login(
            &format!("missing-{}@example.com", Uuid::new_v4()),
            "wrong-password",
            ip(),
        ))
        .await
        .unwrap();
    let expected = error_signature(first).await;
    assert_eq!(error_signature(unknown).await, expected);

    for _ in 1..5 {
        let response = app
            .clone()
            .oneshot(login(&email, "wrong-password", ip()))
            .await
            .unwrap();
        assert_eq!(error_signature(response).await, expected);
    }

    let (failed_login_count, locked_until): (i32, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT failed_login_count, locked_until FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((failed_login_count, locked_until), (0, None));
    assert_eq!(outbox_count(&pool, user_id).await, 0);
    assert_eq!(
        audit_count("auth.account_locked", &user_id.to_string(), None).await,
        0
    );
}

#[tokio::test]
async fn parallel_burst_reserves_only_the_configured_number_of_password_checks() {
    let _serial = TEST_LOCK.lock().await;
    let Some((pool, redis_url)) = services().await else {
        return;
    };
    let (user_id, email) = seed_user(&pool, PASSWORD).await;
    let app = app(pool.clone(), &redis_url);
    let mut attempts = tokio::task::JoinSet::new();

    for _ in 0..25 {
        let app = app.clone();
        let email = email.clone();
        attempts.spawn(async move {
            app.oneshot(login(&email, "wrong-password", ip()))
                .await
                .unwrap()
                .status()
        });
    }
    while let Some(result) = attempts.join_next().await {
        assert_eq!(result.unwrap(), StatusCode::UNAUTHORIZED);
    }

    let failed_count: i32 =
        sqlx::query_scalar("SELECT failed_login_count FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(failed_count, 5);
    assert_eq!(login_failure_count(&email, "account_locked").await, 20);
    assert_eq!(
        audit_count("auth.account_locked", &user_id.to_string(), None).await,
        1
    );
    assert_eq!(outbox_count(&pool, user_id).await, 1);
    sqlx::query("DELETE FROM login_lockout_notifications WHERE user_id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_successful_password_step_resets_failures_and_mfa_still_returns_a_challenge() {
    let _serial = TEST_LOCK.lock().await;
    let Some((pool, redis_url)) = services().await else {
        return;
    };
    let app = app(pool.clone(), &redis_url);

    let (user_id, email) = seed_user(&pool, PASSWORD).await;
    for _ in 0..4 {
        let response = app
            .clone()
            .oneshot(login(&email, "wrong-password", ip()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let success = app
        .clone()
        .oneshot(login(&email, PASSWORD, ip()))
        .await
        .unwrap();
    assert_eq!(success.status(), StatusCode::OK);
    let counters: (
        i32,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT failed_login_count, last_failed_login_at, locked_until FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counters, (0, None, None));

    for _ in 0..4 {
        let response = app
            .clone()
            .oneshot(login(&email, "wrong-password", ip()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let count: i32 = sqlx::query_scalar("SELECT failed_login_count FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 4);

    let (mfa_user_id, mfa_email) = seed_user(&pool, PASSWORD).await;
    sqlx::query(
        "INSERT INTO mfa_factors
             (user_id, kind, ciphertext, nonce, wrapped_dek, kek_id, kek_version, algorithm, confirmed_at)
         VALUES ($1, 'totp', decode('00', 'hex'), decode('00', 'hex'), decode('00', 'hex'),
                 'lockout-test', 1, 'AES-256-GCM', clock_timestamp())",
    )
    .bind(mfa_user_id)
    .execute(&pool)
    .await
    .unwrap();
    for _ in 0..4 {
        let response = app
            .clone()
            .oneshot(login(&mfa_email, "wrong-password", ip()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let challenge = app
        .oneshot(login(&mfa_email, PASSWORD, ip()))
        .await
        .unwrap();
    assert_eq!(challenge.status(), StatusCode::ACCEPTED);
    let mfa_count: i32 = sqlx::query_scalar("SELECT failed_login_count FROM users WHERE id = $1")
        .bind(mfa_user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(mfa_count, 0);
}

#[tokio::test]
async fn expired_locks_double_to_the_cap_and_old_failures_start_a_new_streak() {
    let _serial = TEST_LOCK.lock().await;
    let Some((pool, redis_url)) = services().await else {
        return;
    };
    let (user_id, email) = seed_user(&pool, PASSWORD).await;
    let app = app(pool.clone(), &redis_url);
    for _ in 0..4 {
        let response = app
            .clone()
            .oneshot(login(&email, "wrong-password", ip()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let owner_pool = db::testing::owner_pool();
    for (index, expected_secs) in [60, 120, 240, 480, 900, 900].into_iter().enumerate() {
        let response = app
            .clone()
            .oneshot(login(&email, "wrong-password", ip()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let (count, remaining): (i32, Option<i32>) = sqlx::query_as(
            "SELECT failed_login_count,
                    floor(extract(epoch FROM locked_until - clock_timestamp()))::integer
             FROM users WHERE id = $1",
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 5 + index as i32);
        let remaining = remaining.expect("the threshold failure should lock the account");
        assert!(
            (expected_secs - 5..=expected_secs).contains(&remaining),
            "expected a {expected_secs}s lock, got {remaining}s remaining"
        );
        if index < 5 {
            sqlx::query(
                "UPDATE users SET locked_until = clock_timestamp() - interval '1 second' WHERE id = $1",
            )
            .bind(user_id)
            .execute(&owner_pool)
            .await
            .unwrap();
        }
    }

    sqlx::query(
        "UPDATE users
         SET failed_login_count = 4,
             last_failed_login_at = clock_timestamp() - interval '2 days',
             locked_until = NULL
         WHERE id = $1",
    )
    .bind(user_id)
    .execute(&owner_pool)
    .await
    .unwrap();
    let response = app
        .oneshot(login(&email, "wrong-password", ip()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let counters: (i32, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT failed_login_count, locked_until FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(counters.0, 1);
    assert!(counters.1.is_none());
    assert_eq!(
        audit_count(
            "auth.account_unlocked",
            &user_id.to_string(),
            Some("expired")
        )
        .await,
        5
    );
    sqlx::query("DELETE FROM login_lockout_notifications WHERE user_id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn an_expired_lock_allows_a_correct_password_and_audits_the_unlock() {
    let _serial = TEST_LOCK.lock().await;
    let Some((pool, redis_url)) = services().await else {
        return;
    };
    let (user_id, email) = seed_user(&pool, PASSWORD).await;
    let app = app(pool.clone(), &redis_url);
    for _ in 0..5 {
        let response = app
            .clone()
            .oneshot(login(&email, "wrong-password", ip()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let owner_pool = db::testing::owner_pool();
    sqlx::query(
        "UPDATE users SET locked_until = clock_timestamp() - interval '1 second' WHERE id = $1",
    )
    .bind(user_id)
    .execute(&owner_pool)
    .await
    .unwrap();
    let response = app.oneshot(login(&email, PASSWORD, ip())).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let counters: (
        i32,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT failed_login_count, last_failed_login_at, locked_until FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counters, (0, None, None));
    assert_eq!(
        audit_count(
            "auth.account_unlocked",
            &user_id.to_string(),
            Some("expired")
        )
        .await,
        1
    );
    sqlx::query("DELETE FROM login_lockout_notifications WHERE user_id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn password_reset_clears_lockout_state_and_audits_the_unlock() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use sha2::{Digest, Sha256};

    let _serial = TEST_LOCK.lock().await;
    let Some((pool, redis_url)) = services().await else {
        return;
    };
    let (user_id, email) = seed_user(&pool, PASSWORD).await;
    let app = app(pool.clone(), &redis_url);
    for _ in 0..5 {
        let response = app
            .clone()
            .oneshot(login(&email, "wrong-password", ip()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    assert_eq!(outbox_count(&pool, user_id).await, 1);

    let mut token_bytes = [0_u8; 32];
    token_bytes[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    token_bytes[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    let raw_token = URL_SAFE_NO_PAD.encode(token_bytes);
    let token_hash = hex::encode(Sha256::digest(raw_token.as_bytes()));
    let request_id = Uuid::new_v4();
    let owner_pool = db::testing::owner_pool();
    sqlx::query(
        "UPDATE users SET failed_login_count = 5, last_failed_login_at = clock_timestamp(),
                          locked_until = clock_timestamp() + interval '5 minutes'
         WHERE id = $1",
    )
    .bind(user_id)
    .execute(&owner_pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO password_reset_tokens
             (user_id, request_id, requested_at, token_hash, created_at, expires_at)
         VALUES ($1, $2, statement_timestamp(), $3, statement_timestamp(), statement_timestamp() + interval '30 minutes')",
    )
    .bind(user_id)
    .bind(request_id)
    .bind(token_hash)
    .execute(&pool)
    .await
    .unwrap();

    identity::password_reset::reset(&pool, &raw_token, "ReplacementPassword1!".into())
        .await
        .unwrap();
    let counters: (
        i32,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT failed_login_count, last_failed_login_at, locked_until FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counters, (0, None, None));
    assert_eq!(
        audit_count(
            "auth.account_unlocked",
            &user_id.to_string(),
            Some("password_reset")
        )
        .await,
        1
    );
    assert_eq!(
        outbox_count(&pool, user_id).await,
        1,
        "password reset must preserve a confirmed lockout alert"
    );

    let owner_pool = db::testing::owner_pool();
    sqlx::query(
        "UPDATE login_lockout_notifications
         SET available_at = clock_timestamp() - interval '100 years' WHERE user_id = $1",
    )
    .bind(user_id)
    .execute(&owner_pool)
    .await
    .unwrap();
    let sender = CaptureSender::new(EmailDeliveryStatus::Sent);
    assert!(
        identity::login_lockout::process_notification_once(&pool, &sender, ORIGIN)
            .await
            .unwrap()
    );
    assert_eq!(sender.messages().len(), 1);
    assert_eq!(sender.messages()[0].to, email);
    assert_eq!(outbox_count(&pool, user_id).await, 0);

    for _ in 0..4 {
        let response = app
            .clone()
            .oneshot(login(&email, PASSWORD, ip()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let success = app
        .oneshot(login(&email, "ReplacementPassword1!", ip()))
        .await
        .unwrap();
    assert_eq!(success.status(), StatusCode::OK);
    let failure_count: i32 =
        sqlx::query_scalar("SELECT failed_login_count FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(failure_count, 0);
}

#[tokio::test]
async fn lockout_notification_retries_delivery_and_is_queued_once_per_streak() {
    let _serial = TEST_LOCK.lock().await;
    let Some((pool, redis_url)) = services().await else {
        return;
    };
    let (user_id, email) = seed_user(&pool, PASSWORD).await;
    let app = app(pool.clone(), &redis_url);
    for _ in 0..5 {
        let response = app
            .clone()
            .oneshot(login(&email, "wrong-password", ip()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    assert_eq!(outbox_count(&pool, user_id).await, 1);

    let owner_pool = db::testing::owner_pool();
    sqlx::query(
        "UPDATE login_lockout_notifications
         SET available_at = clock_timestamp() - interval '100 years' WHERE user_id = $1",
    )
    .bind(user_id)
    .execute(&owner_pool)
    .await
    .unwrap();
    let sender = CaptureSender::new(EmailDeliveryStatus::Failed("temporary".into()));
    assert!(
        identity::login_lockout::process_notification_once(&pool, &sender, ORIGIN)
            .await
            .unwrap()
    );
    let failed_message = sender.messages().pop().unwrap();
    assert_eq!(failed_message.to, email);
    assert!(failed_message
        .body_text
        .contains("5 consecutive incorrect passwords"));
    assert!(failed_message
        .body_text
        .lines()
        .any(|line| line.ends_with("\x20http://localhost:4200/auth/forgot-password")));
    assert!(!failed_message.body_text.contains(PASSWORD));
    assert!(!failed_message.body_text.contains("reset-password/"));

    let attempts: i32 =
        sqlx::query_scalar("SELECT attempts FROM login_lockout_notifications WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attempts, 1);
    sqlx::query(
        "UPDATE login_lockout_notifications
         SET available_at = clock_timestamp() - interval '100 years' WHERE user_id = $1",
    )
    .bind(user_id)
    .execute(&owner_pool)
    .await
    .unwrap();
    let retry_sender = CaptureSender::new(EmailDeliveryStatus::Sent);
    assert!(
        identity::login_lockout::process_notification_once(&pool, &retry_sender, ORIGIN)
            .await
            .unwrap()
    );
    assert_eq!(retry_sender.messages().len(), 1);
    assert_eq!(outbox_count(&pool, user_id).await, 0);

    sqlx::query(
        "UPDATE users SET locked_until = clock_timestamp() - interval '1 second' WHERE id = $1",
    )
    .bind(user_id)
    .execute(&owner_pool)
    .await
    .unwrap();
    let response = app
        .oneshot(login(&email, "wrong-password", ip()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(outbox_count(&pool, user_id).await, 0);
}

#[tokio::test]
async fn dropping_an_unfinished_login_attempt_preserves_its_reservation() {
    let _serial = TEST_LOCK.lock().await;
    let Some((pool, _redis_url)) = services().await else {
        return;
    };
    let (user_id, email) = seed_user(&pool, PASSWORD).await;
    let owner_pool = db::testing::owner_pool();
    sqlx::query(
        "UPDATE users
         SET failed_login_count = 4, last_failed_login_at = clock_timestamp(), locked_until = NULL
         WHERE id = $1",
    )
    .bind(user_id)
    .execute(&owner_pool)
    .await
    .unwrap();

    {
        let attempt = identity::login_lockout::begin_login(
            &pool,
            &email,
            config::LoginLockoutLimits::default(),
        )
        .await
        .unwrap();
        assert!(attempt.can_authenticate());
    }

    let state: (i32, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as("SELECT failed_login_count, locked_until FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state.0, 5);
    assert!(state.1.is_some());
    assert_eq!(outbox_count(&pool, user_id).await, 0);
    assert_eq!(
        audit_count("auth.account_locked", &user_id.to_string(), None).await,
        0
    );
}

#[tokio::test]
async fn a_correct_password_serializes_with_a_concurrent_wrong_password() {
    let _serial = TEST_LOCK.lock().await;
    let Some((pool, redis_url)) = services().await else {
        return;
    };
    let (user_id, email) = seed_user(&pool, PASSWORD).await;
    let app = app(pool.clone(), &redis_url);
    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(login(&email, "wrong-password", ip()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let mut attempts = tokio::task::JoinSet::new();
    for password in [PASSWORD.to_owned(), "wrong-password".to_owned()] {
        let app = app.clone();
        let email = email.clone();
        attempts.spawn(async move {
            app.oneshot(login(&email, &password, ip()))
                .await
                .unwrap()
                .status()
        });
    }
    let mut statuses = Vec::new();
    while let Some(result) = attempts.join_next().await {
        statuses.push(result.unwrap());
    }
    assert_eq!(statuses.len(), 2);
    assert!(statuses.contains(&StatusCode::OK));
    assert!(statuses.contains(&StatusCode::UNAUTHORIZED));

    let failed_count: i32 =
        sqlx::query_scalar("SELECT failed_login_count FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        failed_count <= 1,
        "serialized correct login leaves at most one later failure"
    );
    assert_eq!(outbox_count(&pool, user_id).await, 0);
}

#[tokio::test]
async fn login_pressure_does_not_hold_database_connections_during_verification() {
    let _serial = TEST_LOCK.lock().await;
    let Some((seed_pool, redis_url)) = services().await else {
        return;
    };
    let (_, wrong_account) = seed_user(&seed_pool, PASSWORD).await;
    let (_, correct_account) = seed_user(&seed_pool, PASSWORD).await;
    let pool = db::lazy_pool(
        &std::env::var("DATABASE_URL").unwrap(),
        3,
        Duration::from_secs(2),
    );

    // Keep more reservations alive than there are connections, as if their verifiers were
    // still running. This detects connection retention without relying on Argon2 speed.
    let mut pending = Vec::new();
    for _ in 0..4 {
        let email = format!("missing-{}@example.com", Uuid::new_v4());
        pending.push(
            identity::login_lockout::begin_login(&pool, &email, Default::default())
                .await
                .unwrap(),
        );
    }
    let app = app(pool, &redis_url);
    let mut requests = tokio::task::JoinSet::new();
    for index in 0..30 {
        let app = app.clone();
        let (email, password, expected) = match index {
            0 | 11..=29 => (
                format!("missing-{}@example.com", Uuid::new_v4()),
                PASSWORD,
                StatusCode::UNAUTHORIZED,
            ),
            1..=10 => (
                wrong_account.clone(),
                "wrong-password",
                StatusCode::UNAUTHORIZED,
            ),
            _ => (correct_account.clone(), PASSWORD, StatusCode::OK),
        };
        requests.spawn(async move {
            let response = app.oneshot(login(&email, password, ip())).await.unwrap();
            assert_eq!(response.status(), expected);
            if expected == StatusCode::UNAUTHORIZED {
                let (_, headers, body) = error_signature(response).await;
                assert!(!headers.contains_key(header::SET_COOKIE));
                assert!(!headers.contains_key(header::RETRY_AFTER));
                return Some((headers, body));
            }
            None
        });
    }
    let mut expected_failure = None;
    while let Some(result) = requests.join_next().await {
        if let Some(failure) = result.unwrap() {
            assert_eq!(
                expected_failure.get_or_insert_with(|| failure.clone()),
                &failure
            );
        }
    }
    drop(pending);
}
