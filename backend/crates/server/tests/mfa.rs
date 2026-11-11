//! Multi-factor authentication (MOH-56), against real PostgreSQL: enrolment, the two-step sign-in,
//! replay, lockout, recovery codes, the challenge cookie, rate limiting, audit and key shredding.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use http_body_util::BodyExt;
use identity::{session, totp};
use serde_json::{json, Value};
use server::router;
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_ORIGIN: &str = "http://localhost:4200";
const PASSWORD: &str = "correct-horse-battery-1";
const JWT_SECRET: &str = "test-auth-jwt-secret-at-least-32-bytes";

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
        rate_limit_enabled: false,
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
        ai_key_encryption_key: Some("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=".into()),
        integration_secrets_key: None,
        secrets_root_key: None,
        secrets_root_key_previous: None,
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
        "TRUNCATE TABLE mfa_recovery_codes, mfa_factors, revoked_sessions, tenant_memberships, \
         tenants, users, audit_logs, secret_keks RESTART IDENTITY CASCADE",
    )
    .execute(&db::testing::owner_pool())
    .await
    .expect("failed to reset test tables");
}

fn hash(password: &str) -> String {
    identity::password::hash_password(password).unwrap()
}

async fn seed_user() -> (Uuid, String) {
    let email = format!("mfa-{}@example.test", Uuid::new_v4().simple());
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name, password_hash) VALUES ($1, 'Mfa User', $2) \
         RETURNING id",
    )
    .bind(&email)
    .bind(hash(PASSWORD))
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();
    (id, email)
}

fn json_request(method: Method, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(method)
        .header("origin", TEST_ORIGIN)
        .header("x-requested-with", "XMLHttpRequest")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn as_user(mut request: Request<Body>, user: Uuid) -> Request<Body> {
    request
        .headers_mut()
        .insert("X-Dev-User-Id", user.to_string().parse().unwrap());
    request
}

fn with_cookie(mut request: Request<Body>, cookie: &str) -> Request<Body> {
    request
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    request
}

async fn send(app: &axum::Router, request: Request<Body>) -> Response {
    app.clone().oneshot(request).await.unwrap()
}

async fn json_body(response: Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

/// The `name=value` pairs of every `Set-Cookie` header.
fn set_cookies(response: &Response) -> Vec<String> {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .collect()
}

fn cookie_value(response: &Response, name: &str) -> Option<String> {
    set_cookies(response).into_iter().find_map(|c| {
        let first = c.split(';').next()?.to_owned();
        let (n, v) = first.split_once('=')?;
        (n == name && !v.is_empty()).then(|| v.to_owned())
    })
}

fn now() -> u64 {
    chrono::Utc::now().timestamp() as u64
}

async fn secret_of(user: Uuid) -> Vec<u8> {
    // Read the sealed secret back the way the service does, to compute valid codes in the test.
    let (ct, nonce, dek, kek, ver, alg): (Vec<u8>, Vec<u8>, Vec<u8>, String, i32, String) =
        sqlx::query_as(
            "SELECT ciphertext, nonce, wrapped_dek, kek_id, kek_version, algorithm \
             FROM mfa_factors WHERE user_id = $1",
        )
        .bind(user)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap();
    let sealed = secrets::Sealed {
        ciphertext: ct,
        nonce,
        wrapped_dek: dek,
        kek_id: kek,
        kek_version: ver,
        algorithm: alg,
    };
    let cfg = test_config();
    let pool = db::testing::seed_pool();
    let vault = secrets::Vault::from_config(&pool, None, cfg.secrets_root_key(), None).unwrap();
    vault
        .secret_box()
        .unwrap()
        .open(&format!("mfa-totp|{user}"), &sealed)
        .await
        .unwrap()
        .to_vec()
}

/// A user with a confirmed factor: returns the id, email, secret and the ten recovery codes.
async fn enrolled_user(app: &axum::Router) -> (Uuid, String, Vec<u8>, Vec<String>) {
    let (user, email) = seed_user().await;
    let begin = send(
        app,
        as_user(
            json_request(
                Method::POST,
                "/api/v1/auth/mfa/enrol",
                json!({ "password": PASSWORD }),
            ),
            user,
        ),
    )
    .await;
    assert_eq!(begin.status(), StatusCode::CREATED);
    let secret = secret_of(user).await;
    let code = totp::code_at(&secret, now(), 6);
    let confirm = send(
        app,
        as_user(
            json_request(
                Method::POST,
                "/api/v1/auth/mfa/enrol/confirm",
                json!({ "code": code }),
            ),
            user,
        ),
    )
    .await;
    assert_eq!(confirm.status(), StatusCode::OK);
    let codes: Vec<String> = json_body(confirm).await["recoveryCodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    (user, email, secret, codes)
}

/// The password step: returns the pending cookie as `name=value`.
async fn password_step(app: &axum::Router, email: &str) -> String {
    let response = send(
        app,
        json_request(
            Method::POST,
            "/api/v1/auth/login",
            json!({ "email": email, "password": PASSWORD }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    format!(
        "app_mfa_pending={}",
        cookie_value(&response, "app_mfa_pending").expect("a challenge cookie")
    )
}

fn verify_request(pending: &str, body: Value) -> Request<Body> {
    with_cookie(
        json_request(Method::POST, "/api/v1/auth/mfa/verify", body),
        pending,
    )
}

async fn audit_actions(user: Uuid) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT action FROM audit_logs WHERE actor_user_id = $1 AND action LIKE 'auth.mfa_%' \
         ORDER BY created_at, id",
    )
    .bind(user)
    .fetch_all(&db::testing::seed_pool())
    .await
    .unwrap()
}

macro_rules! setup {
    () => {{
        let Some(pool) = get_pool().await else { return };
        let _serial = DB_LOCK.lock().await;
        reset().await;
        let app = router::app(state(pool.clone(), test_config()));
        (pool, app, _serial)
    }};
}

#[tokio::test]
async fn enrolment_shows_the_secret_once_and_confirming_activates_the_factor() {
    let (_pool, app, _lock) = setup!();
    let (user, email) = seed_user().await;

    let begin = send(
        &app,
        as_user(
            json_request(
                Method::POST,
                "/api/v1/auth/mfa/enrol",
                json!({ "password": PASSWORD }),
            ),
            user,
        ),
    )
    .await;
    assert_eq!(begin.status(), StatusCode::CREATED);
    let body = json_body(begin).await;
    let secret_b32 = body["secret"].as_str().unwrap().to_owned();
    let uri = body["otpauthUri"].as_str().unwrap();
    assert!(uri.starts_with("otpauth://totp/") && uri.contains(&secret_b32));
    assert!(
        uri.contains("algorithm=SHA1") && uri.contains("digits=6") && uri.contains("period=30")
    );

    // Not active yet: sign-in still needs no second step, and the status says so.
    let status = send(
        &app,
        as_user(
            Request::get("/api/v1/auth/mfa/status")
                .body(Body::empty())
                .unwrap(),
            user,
        ),
    )
    .await;
    assert_eq!(json_body(status).await["enrolled"], false);

    // The secret is stored sealed: neither the base32 nor the raw bytes are in the row.
    let secret = totp::base32_decode(&secret_b32).unwrap();
    let (ct,): (Vec<u8>,) = sqlx::query_as("SELECT ciphertext FROM mfa_factors WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap();
    assert!(!ct.windows(secret.len()).any(|w| w == secret.as_slice()));
    assert_eq!(
        secret_of(user).await,
        secret,
        "and it opens to what was shown"
    );
    let kek: String = sqlx::query_scalar("SELECT kek_id FROM mfa_factors WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap();
    assert_eq!(kek, format!("user:{user}"));

    // A wrong code does not activate it.
    let wrong = send(
        &app,
        as_user(
            json_request(
                Method::POST,
                "/api/v1/auth/mfa/enrol/confirm",
                json!({ "code": "000000" }),
            ),
            user,
        ),
    )
    .await;
    assert_eq!(wrong.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // The right one does: recovery codes once, and a session that records the second factor.
    let code = totp::code_at(&secret, now(), 6);
    let confirm = send(
        &app,
        as_user(
            json_request(
                Method::POST,
                "/api/v1/auth/mfa/enrol/confirm",
                json!({ "code": code }),
            ),
            user,
        ),
    )
    .await;
    assert_eq!(confirm.status(), StatusCode::OK);
    let jwt = cookie_value(&confirm, "app_session").expect("a replacement session");
    let claims = session::validate_token(JWT_SECRET, &jwt).unwrap();
    assert_eq!(claims.sub, user);
    assert_eq!(claims.amr, vec!["pwd", "otp"]);
    assert!(claims.second_factor_fresh(chrono::Utc::now().timestamp(), 300));
    let codes = json_body(confirm).await["recoveryCodes"].clone();
    let codes: Vec<&str> = codes
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    assert_eq!(codes.len(), 10);
    assert!(codes
        .iter()
        .all(|c| c.len() == 11 && c.as_bytes()[5] == b'-'));

    // Only hashes are stored.
    let hashes: Vec<String> =
        sqlx::query_scalar("SELECT code_hash FROM mfa_recovery_codes WHERE user_id = $1")
            .bind(user)
            .fetch_all(&db::testing::seed_pool())
            .await
            .unwrap();
    assert_eq!(hashes.len(), 10);
    assert!(hashes.iter().all(|h| h.starts_with("$argon2id$")));
    for code in &codes {
        assert!(!hashes.iter().any(|h| h.contains(&code.replace('-', ""))));
    }

    // Status shows it is on and never a secret; starting again is refused.
    let status = send(
        &app,
        as_user(
            Request::get("/api/v1/auth/mfa/status")
                .body(Body::empty())
                .unwrap(),
            user,
        ),
    )
    .await;
    let status = json_body(status).await;
    assert_eq!(status["enrolled"], true);
    assert_eq!(status["recoveryCodesRemaining"], 10);
    assert!(status.get("secret").is_none());
    let again = send(
        &app,
        as_user(
            json_request(
                Method::POST,
                "/api/v1/auth/mfa/enrol",
                json!({ "password": PASSWORD }),
            ),
            user,
        ),
    )
    .await;
    assert_eq!(again.status(), StatusCode::CONFLICT);
    assert_eq!(
        audit_actions(user).await,
        vec!["auth.mfa_failed", "auth.mfa_enrolled"],
        "the wrong code and the enrolment are audited"
    );
    let _ = email;
}

#[tokio::test]
async fn starting_enrolment_needs_the_current_password() {
    let (_pool, app, _lock) = setup!();
    let (user, _email) = seed_user().await;
    let enrol = |body: Value| {
        as_user(
            json_request(Method::POST, "/api/v1/auth/mfa/enrol", body),
            user,
        )
    };

    let wrong = send(&app, enrol(json!({ "password": "not-the-password" }))).await;
    assert_eq!(wrong.status(), StatusCode::UNPROCESSABLE_ENTITY);
    for body in [json!({}), json!({ "password": PASSWORD, "extra": 1 })] {
        let r = send(&app, enrol(body)).await;
        assert!(
            r.status() == StatusCode::BAD_REQUEST || r.status() == StatusCode::UNPROCESSABLE_ENTITY,
            "got {}",
            r.status()
        );
    }
    let factors: i64 = sqlx::query_scalar("SELECT count(*) FROM mfa_factors WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap();
    assert_eq!(factors, 0, "nothing was created without the password");

    assert_eq!(
        send(&app, enrol(json!({ "password": PASSWORD })))
            .await
            .status(),
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn a_user_with_no_password_cannot_start_enrolment() {
    let (_pool, app, _lock) = setup!();
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name) VALUES ($1, 'No Password') RETURNING id",
    )
    .bind(format!("nopw-{}@example.test", Uuid::new_v4().simple()))
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();
    let r = send(
        &app,
        as_user(
            json_request(
                Method::POST,
                "/api/v1/auth/mfa/enrol",
                json!({ "password": "" }),
            ),
            user,
        ),
    )
    .await;
    assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_session_that_skipped_the_second_factor_stops_working_once_the_user_enrols() {
    let (_pool, app, _lock) = setup!();
    let (user, _email) = seed_user().await;
    // A password-only session, such as one stolen before the user enrolled.
    let (old, _, _) = session::issue_token(JWT_SECRET, 600, user).unwrap();
    let me = |jwt: &str| {
        with_cookie(
            Request::get("/api/v1/me").body(Body::empty()).unwrap(),
            &format!("app_session={jwt}"),
        )
    };
    assert_eq!(
        send(&app, me(&old)).await.status(),
        StatusCode::OK,
        "fine while not enrolled"
    );

    // The user enrols with that session.
    let begin = send(
        &app,
        with_cookie(
            json_request(
                Method::POST,
                "/api/v1/auth/mfa/enrol",
                json!({ "password": PASSWORD }),
            ),
            &format!("app_session={old}"),
        ),
    )
    .await;
    assert_eq!(begin.status(), StatusCode::CREATED);
    let code = totp::code_at(&secret_of(user).await, now(), 6);
    let confirm = send(
        &app,
        with_cookie(
            json_request(
                Method::POST,
                "/api/v1/auth/mfa/enrol/confirm",
                json!({ "code": code }),
            ),
            &format!("app_session={old}"),
        ),
    )
    .await;
    assert_eq!(confirm.status(), StatusCode::OK);
    let fresh = cookie_value(&confirm, "app_session").unwrap();

    // The session that confirmed is replaced by one that records the factor; the old one is gone,
    // and so would be any other password-only session of this user.
    assert_eq!(send(&app, me(&fresh)).await.status(), StatusCode::OK);
    assert_eq!(
        send(&app, me(&old)).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let (other, _, _) = session::issue_token(JWT_SECRET, 600, user).unwrap();
    assert_eq!(
        send(&app, me(&other)).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn enrolment_needs_a_session() {
    let (_pool, app, _lock) = setup!();
    for (method, uri) in [
        (Method::POST, "/api/v1/auth/mfa/enrol"),
        (Method::POST, "/api/v1/auth/mfa/enrol/confirm"),
        (Method::GET, "/api/v1/auth/mfa/status"),
    ] {
        let response = send(&app, json_request(method, uri, json!({ "code": "123456" }))).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri}");
    }
}

#[tokio::test]
async fn sign_in_is_two_steps_and_the_session_records_the_second_factor() {
    let (_pool, app, _lock) = setup!();
    let (user, email, secret, _) = enrolled_user(&app).await;

    // Step one: the password is right, but there is no session.
    let login = send(
        &app,
        json_request(
            Method::POST,
            "/api/v1/auth/login",
            json!({ "email": email, "password": PASSWORD }),
        ),
    )
    .await;
    assert_eq!(login.status(), StatusCode::ACCEPTED);
    assert!(
        cookie_value(&login, "app_session").is_none(),
        "no session yet"
    );
    let pending = cookie_value(&login, "app_mfa_pending").expect("a challenge");
    let set = set_cookies(&login)
        .into_iter()
        .find(|c| c.starts_with("app_mfa_pending="))
        .unwrap();
    assert!(
        set.contains("HttpOnly")
            && set.contains("Path=/api/v1/auth/mfa")
            && set.contains("Max-Age=300")
    );
    assert_eq!(json_body(login).await["mfaRequired"], true);
    let pending = format!("app_mfa_pending={pending}");

    // Without the challenge the second step is refused.
    let none = send(
        &app,
        json_request(
            Method::POST,
            "/api/v1/auth/mfa/verify",
            json!({ "code": "123456" }),
        ),
    )
    .await;
    assert_eq!(none.status(), StatusCode::UNAUTHORIZED);

    // With a code from the next step (inside the window) it signs in.
    let code = totp::code_at(&secret, now() + 30, 6);
    let verify = send(&app, verify_request(&pending, json!({ "code": code }))).await;
    assert_eq!(verify.status(), StatusCode::OK);
    let jwt = cookie_value(&verify, "app_session").expect("a session");
    let claims = session::validate_token(JWT_SECRET, &jwt).unwrap();
    assert_eq!(
        (claims.sub, claims.amr.clone()),
        (user, vec!["pwd".to_owned(), "otp".to_owned()])
    );
    assert!(set_cookies(&verify)
        .iter()
        .any(|c| c.starts_with("app_mfa_pending=;") && c.contains("Max-Age=0")));
    assert_eq!(json_body(verify).await["email"], email);

    // The session works; the challenge is spent.
    let me = send(
        &app,
        with_cookie(
            Request::get("/api/v1/me").body(Body::empty()).unwrap(),
            &format!("app_session={jwt}"),
        ),
    )
    .await;
    assert_eq!(me.status(), StatusCode::OK);
    let reuse = send(
        &app,
        verify_request(
            &pending,
            json!({ "code": totp::code_at(&secret, now() + 60, 6) }),
        ),
    )
    .await;
    assert_eq!(
        reuse.status(),
        StatusCode::UNAUTHORIZED,
        "a challenge signs in once"
    );
    assert_eq!(
        audit_actions(user).await,
        vec![
            "auth.mfa_enrolled",
            "auth.mfa_challenged",
            "auth.mfa_verified"
        ]
    );
}

#[tokio::test]
async fn an_accepted_code_cannot_be_used_again() {
    let (_pool, app, _lock) = setup!();
    let (user, email, secret, _) = enrolled_user(&app).await;
    // Enrolment already used the current step; the next step's code is the only one that works.
    let next = totp::code_at(&secret, now() + 30, 6);

    let first = send(
        &app,
        verify_request(&password_step(&app, &email).await, json!({ "code": next })),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    let replay = send(
        &app,
        verify_request(&password_step(&app, &email).await, json!({ "code": next })),
    )
    .await;
    assert_eq!(
        replay.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "the same step again"
    );
    // The code the enrolment used (an earlier step) is refused too.
    let old = totp::code_at(&secret, now(), 6);
    let old = send(
        &app,
        verify_request(&password_step(&app, &email).await, json!({ "code": old })),
    )
    .await;
    assert_eq!(old.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let last: i64 = sqlx::query_scalar("SELECT last_used_step FROM mfa_factors WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap();
    assert_eq!(last as u64, totp::step_at(now() + 30));
}

#[tokio::test]
async fn two_requests_with_the_same_code_cannot_both_succeed() {
    let (_pool, app, _lock) = setup!();
    let (_user, email, secret, _) = enrolled_user(&app).await;
    let code = totp::code_at(&secret, now() + 30, 6);
    let a = password_step(&app, &email).await;
    let b = password_step(&app, &email).await;
    let (first, second) = tokio::join!(
        send(&app, verify_request(&a, json!({ "code": code }))),
        send(&app, verify_request(&b, json!({ "code": code }))),
    );
    let mut statuses = [first.status(), second.status()];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::UNPROCESSABLE_ENTITY]);
}

#[tokio::test]
async fn five_wrong_codes_lock_the_second_factor_for_fifteen_minutes() {
    let (pool, app, _lock) = setup!();
    let (user, email, secret, _) = enrolled_user(&app).await;
    let pending = password_step(&app, &email).await;

    for attempt in 1..=5 {
        let response = send(&app, verify_request(&pending, json!({ "code": "000000" }))).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "attempt {attempt}"
        );
    }
    // Locked: even the right code is refused, with a retry time.
    let good = totp::code_at(&secret, now() + 30, 6);
    let locked = send(&app, verify_request(&pending, json!({ "code": good }))).await;
    assert_eq!(locked.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry: u64 = locked.headers()["retry-after"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        (850..=900).contains(&retry),
        "about fifteen minutes, got {retry}s"
    );

    let actions = audit_actions(user).await;
    assert_eq!(
        actions.iter().filter(|a| *a == "auth.mfa_failed").count(),
        5
    );
    assert_eq!(
        actions.iter().filter(|a| *a == "auth.mfa_locked").count(),
        1
    );

    // After the lock runs out the right code works.
    sqlx::query(
        "UPDATE mfa_factors SET locked_until = now() - interval '1 second' WHERE user_id = $1",
    )
    .bind(user)
    .execute(&db::testing::owner_pool())
    .await
    .unwrap();
    let ok = send(&app, verify_request(&pending, json!({ "code": good }))).await;
    assert_eq!(ok.status(), StatusCode::OK);
    let _ = pool;
}

#[tokio::test]
async fn a_right_code_resets_the_count_of_wrong_ones() {
    let (_pool, app, _lock) = setup!();
    let (user, email, secret, _) = enrolled_user(&app).await;
    let pending = password_step(&app, &email).await;
    for _ in 0..4 {
        send(&app, verify_request(&pending, json!({ "code": "000000" }))).await;
    }
    let ok = send(
        &app,
        verify_request(
            &pending,
            json!({ "code": totp::code_at(&secret, now() + 30, 6) }),
        ),
    )
    .await;
    assert_eq!(ok.status(), StatusCode::OK);
    let attempts: i32 =
        sqlx::query_scalar("SELECT failed_attempts FROM mfa_factors WHERE user_id = $1")
            .bind(user)
            .fetch_one(&db::testing::seed_pool())
            .await
            .unwrap();
    assert_eq!(attempts, 0);
}

#[tokio::test]
async fn a_recovery_code_signs_in_once() {
    let (_pool, app, _lock) = setup!();
    let (user, email, _secret, codes) = enrolled_user(&app).await;

    let first = send(
        &app,
        verify_request(
            &password_step(&app, &email).await,
            json!({ "recoveryCode": codes[0] }),
        ),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    let claims =
        session::validate_token(JWT_SECRET, &cookie_value(&first, "app_session").unwrap()).unwrap();
    assert_eq!(claims.amr, vec!["pwd", "rcv"]);

    // Used: refused. Lower case and spaces in what the person typed are fine for an unused one.
    let again = send(
        &app,
        verify_request(
            &password_step(&app, &email).await,
            json!({ "recoveryCode": codes[0] }),
        ),
    )
    .await;
    assert_eq!(again.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let typed = codes[1].to_lowercase().replace('-', " ");
    let other = send(
        &app,
        verify_request(
            &password_step(&app, &email).await,
            json!({ "recoveryCode": typed }),
        ),
    )
    .await;
    assert_eq!(other.status(), StatusCode::OK);

    let remaining: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM mfa_recovery_codes WHERE user_id = $1 AND used_at IS NULL",
    )
    .bind(user)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();
    assert_eq!(remaining, 8);
    let actions = audit_actions(user).await;
    assert_eq!(
        actions
            .iter()
            .filter(|a| *a == "auth.mfa_recovery_used")
            .count(),
        2
    );
    assert!(actions.iter().any(|a| a == "auth.mfa_failed"));
}

#[tokio::test]
async fn wrong_recovery_codes_count_toward_the_lockout() {
    let (_pool, app, _lock) = setup!();
    let (_user, email, _secret, codes) = enrolled_user(&app).await;
    let pending = password_step(&app, &email).await;
    for _ in 0..5 {
        let r = send(
            &app,
            verify_request(&pending, json!({ "recoveryCode": "AAAAA-AAAAA" })),
        )
        .await;
        assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
    let locked = send(
        &app,
        verify_request(&pending, json!({ "recoveryCode": codes[0] })),
    )
    .await;
    assert_eq!(locked.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn the_second_step_needs_exactly_one_credential() {
    let (_pool, app, _lock) = setup!();
    let (_user, email, secret, codes) = enrolled_user(&app).await;
    let pending = password_step(&app, &email).await;
    for body in [
        json!({}),
        json!({ "code": totp::code_at(&secret, now() + 30, 6), "recoveryCode": codes[0] }),
        json!({ "code": "123456", "extra": true }),
    ] {
        let r = send(&app, verify_request(&pending, body)).await;
        assert!(
            r.status() == StatusCode::BAD_REQUEST || r.status() == StatusCode::UNPROCESSABLE_ENTITY,
            "got {}",
            r.status()
        );
    }
}

#[tokio::test]
async fn an_expired_or_foreign_challenge_is_refused() {
    let (_pool, app, _lock) = setup!();
    let (user, _email, secret, _) = enrolled_user(&app).await;
    let code = totp::code_at(&secret, now() + 30, 6);

    let past = chrono::Utc::now().timestamp() - 400;
    let expired = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &session::PendingClaims {
            sub: user,
            jti: Uuid::new_v4(),
            iat: past,
            exp: past + 300,
            aud: "mfa_pending".into(),
        },
        &jsonwebtoken::EncodingKey::from_secret(JWT_SECRET.as_bytes()),
    )
    .unwrap();
    let r = send(
        &app,
        verify_request(
            &format!("app_mfa_pending={expired}"),
            json!({ "code": code }),
        ),
    )
    .await;
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED, "expired");

    let (foreign, _) =
        session::issue_pending("another-secret-with-at-least-32-bytes!!", user).unwrap();
    let r = send(
        &app,
        verify_request(
            &format!("app_mfa_pending={foreign}"),
            json!({ "code": code }),
        ),
    )
    .await;
    assert_eq!(
        r.status(),
        StatusCode::UNAUTHORIZED,
        "signed with another secret"
    );
    let r = send(
        &app,
        verify_request("app_mfa_pending=not-a-token", json!({ "code": code })),
    )
    .await;
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED, "garbage");
}

#[tokio::test]
async fn a_challenge_is_not_a_session_and_a_session_is_not_a_challenge() {
    let (_pool, app, _lock) = setup!();
    let (user, email, secret, _) = enrolled_user(&app).await;
    let pending = password_step(&app, &email).await;
    let token = pending.trim_start_matches("app_mfa_pending=");

    // The challenge in the session cookie opens nothing.
    let me = send(
        &app,
        with_cookie(
            Request::get("/api/v1/me").body(Body::empty()).unwrap(),
            &format!("app_session={token}"),
        ),
    )
    .await;
    assert_eq!(me.status(), StatusCode::UNAUTHORIZED);
    // A session in the challenge cookie completes nothing.
    let (jwt, _, _) = session::issue_token(JWT_SECRET, 60, user).unwrap();
    let code = totp::code_at(&secret, now() + 30, 6);
    let r = send(
        &app,
        verify_request(&format!("app_mfa_pending={jwt}"), json!({ "code": code })),
    )
    .await;
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_user_without_a_factor_still_signs_in_with_a_password() {
    let (_pool, app, _lock) = setup!();
    let (_user, email) = seed_user().await;
    let login = send(
        &app,
        json_request(
            Method::POST,
            "/api/v1/auth/login",
            json!({ "email": email, "password": PASSWORD }),
        ),
    )
    .await;
    assert_eq!(login.status(), StatusCode::OK);
    let jwt = cookie_value(&login, "app_session").unwrap();
    assert_eq!(
        session::validate_token(JWT_SECRET, &jwt).unwrap().amr,
        vec!["pwd"]
    );
    // An unconfirmed enrolment does not change that.
    let (user, email2) = seed_user().await;
    send(
        &app,
        as_user(
            json_request(
                Method::POST,
                "/api/v1/auth/mfa/enrol",
                json!({ "password": PASSWORD }),
            ),
            user,
        ),
    )
    .await;
    let login = send(
        &app,
        json_request(
            Method::POST,
            "/api/v1/auth/login",
            json!({ "email": email2, "password": PASSWORD }),
        ),
    )
    .await;
    assert_eq!(login.status(), StatusCode::OK);
}

#[tokio::test]
async fn attempts_at_the_second_step_are_rate_limited_per_client() {
    let Some(pool) = get_pool().await else { return };
    let _serial = DB_LOCK.lock().await;
    reset().await;
    let mut limited = test_config();
    limited.rate_limit_enabled = true;
    limited.rate_limit_login_max = 3;
    limited.rate_limit_login_window_secs = 60;
    limited.trusted_proxy = true;
    let app = router::app(state(pool, limited));
    // A client address of its own, so other tests' buckets do not count.
    let client = format!("203.0.113.{}", (Uuid::new_v4().as_u128() % 200) + 1);
    let request = || {
        let mut r = json_request(
            Method::POST,
            "/api/v1/auth/mfa/verify",
            json!({ "code": "123456" }),
        );
        r.headers_mut()
            .insert("x-forwarded-for", client.parse().unwrap());
        r
    };
    for _ in 0..3 {
        // No challenge: refused before any code is looked at, but it still counts.
        assert_eq!(
            send(&app, request()).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        send(&app, request()).await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn deleting_a_user_destroys_the_key_that_protects_their_secret() {
    let (_pool, app, _lock) = setup!();
    let (user, _email, _secret, _) = enrolled_user(&app).await;
    let kek = format!("user:{user}");
    let state: String = sqlx::query_scalar("SELECT state FROM secret_keks WHERE kek_id = $1")
        .bind(&kek)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap();
    assert_eq!(state, "active");

    sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
        .bind(user)
        .execute(&db::testing::owner_pool())
        .await
        .unwrap();
    let row: (String, Option<Vec<u8>>) =
        sqlx::query_as("SELECT state, wrapped_kek FROM secret_keks WHERE kek_id = $1")
            .bind(&kek)
            .fetch_one(&db::testing::seed_pool())
            .await
            .unwrap();
    assert_eq!(row.0, "destroyed");
    assert!(row.1.is_none());
}
