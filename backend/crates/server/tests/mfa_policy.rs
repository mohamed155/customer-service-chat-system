//! The second-factor policy (MOH-34), against real PostgreSQL: enforcement for every account,
//! step-up for sensitive actions, recovery-code regeneration and resetting someone's factor.

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

fn policy_config() -> config::AppConfig {
    let mut cfg = test_config();
    cfg.auth_mfa_required = true;
    cfg
}

macro_rules! setup {
    () => {{
        let Some(pool) = get_pool().await else { return };
        let _serial = DB_LOCK.lock().await;
        reset().await;
        let app = router::app(state(pool.clone(), policy_config()));
        (pool, app, _serial)
    }};
}

async fn seed_tenant() -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ('Policy Tenant', $1) RETURNING id")
        .bind(format!("policy-{}", Uuid::new_v4().simple()))
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap()
}

async fn seed_member(tenant: Uuid, role: &str) -> (Uuid, Uuid, String) {
    let (user, email) = seed_user().await;
    let membership: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant)
    .bind(user)
    .bind(role)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();
    (user, membership, email)
}

/// A session for `user` with the given second-factor state, as the cookie header value.
fn cookie_for(user: Uuid, auth: session::SessionAuth) -> String {
    let (jwt, _, _) = session::issue_session(JWT_SECRET, 3600, user, auth).unwrap();
    format!("app_session={jwt}")
}

fn password_only(user: Uuid) -> String {
    cookie_for(user, session::SessionAuth::password())
}

fn fresh(user: Uuid) -> String {
    cookie_for(user, session::SessionAuth::second_factor(session::AMR_OTP))
}

/// A second factor that was proven, but more than five minutes ago.
fn stale(user: Uuid) -> String {
    cookie_for(
        user,
        session::SessionAuth {
            amr: vec!["pwd".into(), "otp".into()],
            mfa_at: Some(chrono::Utc::now().timestamp() - 400),
        },
    )
}

fn tenant_get(uri: &str, tenant: Uuid, cookie: &str) -> Request<Body> {
    let mut r = Request::get(uri).body(Body::empty()).unwrap();
    r.headers_mut()
        .insert("x-tenant-id", tenant.to_string().parse().unwrap());
    with_cookie(r, cookie)
}

fn tenant_json(
    method: Method,
    uri: &str,
    tenant: Uuid,
    cookie: &str,
    body: Value,
) -> Request<Body> {
    let mut r = json_request(method, uri, body);
    r.headers_mut()
        .insert("x-tenant-id", tenant.to_string().parse().unwrap());
    with_cookie(r, cookie)
}

async fn error_code(response: Response) -> String {
    json_body(response).await["error"]["code"]
        .as_str()
        .unwrap_or("")
        .to_owned()
}

/// Enrol `user` directly (the API path is tested in mfa.rs) and return the secret.
async fn enrol(app: &axum::Router, user: Uuid) -> Vec<u8> {
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
    let confirm = send(
        app,
        as_user(
            json_request(
                Method::POST,
                "/api/v1/auth/mfa/enrol/confirm",
                json!({ "code": totp::code_at(&secret, now(), 6) }),
            ),
            user,
        ),
    )
    .await;
    assert_eq!(confirm.status(), StatusCode::OK);
    secret
}

#[tokio::test]
async fn a_session_without_a_second_factor_is_held_at_the_door_when_mfa_is_required() {
    let (_pool, app, _lock) = setup!();
    let tenant = seed_tenant().await;
    let (user, _, _) = seed_member(tenant, "owner").await;
    let cookie = password_only(user);

    // Tenant routes are refused with the code a client looks for.
    let blocked = send(&app, tenant_get("/api/v1/tenant/members", tenant, &cookie)).await;
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
    assert_eq!(error_code(blocked).await, "mfa_enrolment_required");

    // The way out stays open: the profile, the status, enrolment and sign-out.
    let me = send(
        &app,
        with_cookie(
            Request::get("/api/v1/me").body(Body::empty()).unwrap(),
            &cookie,
        ),
    )
    .await;
    assert_eq!(me.status(), StatusCode::OK);
    let status = send(
        &app,
        with_cookie(
            Request::get("/api/v1/auth/mfa/status")
                .body(Body::empty())
                .unwrap(),
            &cookie,
        ),
    )
    .await;
    let status = json_body(status).await;
    assert_eq!(
        (
            status["required"].clone(),
            status["sessionSecondFactor"].clone()
        ),
        (json!(true), json!(false))
    );
    let begin = send(
        &app,
        with_cookie(
            json_request(
                Method::POST,
                "/api/v1/auth/mfa/enrol",
                json!({ "password": PASSWORD }),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(begin.status(), StatusCode::CREATED);
    let secret = secret_of(user).await;
    let confirm = send(
        &app,
        with_cookie(
            json_request(
                Method::POST,
                "/api/v1/auth/mfa/enrol/confirm",
                json!({ "code": totp::code_at(&secret, now(), 6) }),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(confirm.status(), StatusCode::OK);

    // The session that confirmed carries the factor and is let in.
    let new_cookie = format!(
        "app_session={}",
        cookie_value(&confirm, "app_session").unwrap()
    );
    let ok = send(
        &app,
        tenant_get("/api/v1/tenant/members", tenant, &new_cookie),
    )
    .await;
    assert_eq!(ok.status(), StatusCode::OK);
}

#[tokio::test]
async fn platform_routes_need_a_second_factor_too() {
    let (_pool, app, _lock) = setup!();
    let (staff, _) = seed_user().await;
    sqlx::query("UPDATE users SET platform_role = 'super_admin' WHERE id = $1")
        .bind(staff)
        .execute(&db::testing::owner_pool())
        .await
        .unwrap();
    let blocked = send(
        &app,
        with_cookie(
            Request::get("/api/v1/platform/tenants")
                .body(Body::empty())
                .unwrap(),
            &password_only(staff),
        ),
    )
    .await;
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
    assert_eq!(error_code(blocked).await, "mfa_enrolment_required");
    let ok = send(
        &app,
        with_cookie(
            Request::get("/api/v1/platform/tenants")
                .body(Body::empty())
                .unwrap(),
            &fresh(staff),
        ),
    )
    .await;
    assert_eq!(ok.status(), StatusCode::OK);
}

#[tokio::test]
async fn nothing_is_enforced_when_mfa_is_not_required() {
    let Some(pool) = get_pool().await else { return };
    let _serial = DB_LOCK.lock().await;
    reset().await;
    let app = router::app(state(pool, test_config())); // auth_mfa_required: false
    let tenant = seed_tenant().await;
    let (user, _, _) = seed_member(tenant, "owner").await;
    let ok = send(
        &app,
        tenant_get("/api/v1/tenant/members", tenant, &password_only(user)),
    )
    .await;
    assert_eq!(ok.status(), StatusCode::OK);
    let status = send(
        &app,
        with_cookie(
            Request::get("/api/v1/auth/mfa/status")
                .body(Body::empty())
                .unwrap(),
            &password_only(user),
        ),
    )
    .await;
    assert_eq!(json_body(status).await["required"], false);
}

#[tokio::test]
async fn sensitive_actions_need_a_fresh_code_and_a_step_up_unlocks_them() {
    let (_pool, app, _lock) = setup!();
    let tenant = seed_tenant().await;
    let (owner, _, _) = seed_member(tenant, "owner").await;
    let (_member, member_membership, _) = seed_member(tenant, "agent").await;
    let secret = enrol(&app, owner).await;
    let cookie = stale(owner);

    // Reads are fine; secret changes and role changes are held for a fresh code.
    assert_eq!(
        send(&app, tenant_get("/api/v1/tenant/members", tenant, &cookie))
            .await
            .status(),
        StatusCode::OK
    );
    let put_secret = |cookie: &str| {
        tenant_json(
            Method::PUT,
            "/api/v1/tenant/ai/credentials/openai",
            tenant,
            cookie,
            json!({ "api_key": "sk-test-key-1234" }),
        )
    };
    let held = send(&app, put_secret(&cookie)).await;
    assert_eq!(held.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(error_code(held).await, "mfa_step_up_required");
    // An invitation names a role, so it is held the same way.
    let invite = |cookie: &str| {
        tenant_json(
            Method::POST,
            "/api/v1/tenant/members/invitations",
            tenant,
            cookie,
            json!({ "email": "new-colleague@example.test", "role": "admin" }),
        )
    };
    let held_invite = send(&app, invite(&cookie)).await;
    assert_eq!(held_invite.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(error_code(held_invite).await, "mfa_step_up_required");
    let role = send(
        &app,
        tenant_json(
            Method::PATCH,
            &format!("/api/v1/tenant/members/{member_membership}"),
            tenant,
            &cookie,
            json!({ "role": "manager" }),
        ),
    )
    .await;
    assert_eq!(role.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(error_code(role).await, "mfa_step_up_required");

    // A wrong code does not unlock it; a right one does, and replaces the session.
    let bad = send(
        &app,
        with_cookie(
            json_request(
                Method::POST,
                "/api/v1/auth/mfa/step-up",
                json!({ "code": "000000" }),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(bad.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let good_code = totp::code_at(&secret, now() + 30, 6);
    let up = send(
        &app,
        with_cookie(
            json_request(
                Method::POST,
                "/api/v1/auth/mfa/step-up",
                json!({ "code": good_code }),
            ),
            &cookie,
        ),
    )
    .await;
    assert_eq!(up.status(), StatusCode::NO_CONTENT);
    let fresh_cookie = format!("app_session={}", cookie_value(&up, "app_session").unwrap());
    let claims =
        session::validate_token(JWT_SECRET, fresh_cookie.trim_start_matches("app_session="))
            .unwrap();
    assert!(claims.second_factor_fresh(chrono::Utc::now().timestamp(), 300));
    assert_eq!(
        send(&app, put_secret(&fresh_cookie)).await.status(),
        StatusCode::OK,
        "retry works"
    );

    // The old session was replaced, and the same code cannot step up twice.
    assert_eq!(
        send(&app, tenant_get("/api/v1/tenant/members", tenant, &cookie))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let again = send(
        &app,
        with_cookie(
            json_request(
                Method::POST,
                "/api/v1/auth/mfa/step-up",
                json!({ "code": good_code }),
            ),
            &fresh_cookie,
        ),
    )
    .await;
    assert_eq!(again.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn stepping_up_needs_a_session_and_exactly_one_credential() {
    let (_pool, app, _lock) = setup!();
    let none = send(
        &app,
        json_request(
            Method::POST,
            "/api/v1/auth/mfa/step-up",
            json!({ "code": "123456" }),
        ),
    )
    .await;
    assert_eq!(none.status(), StatusCode::UNAUTHORIZED);
    let (user, _) = seed_user().await;
    let r = send(
        &app,
        with_cookie(
            json_request(Method::POST, "/api/v1/auth/mfa/step-up", json!({})),
            &password_only(user),
        ),
    )
    .await;
    assert!(
        r.status() == StatusCode::BAD_REQUEST || r.status() == StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn recovery_codes_can_be_replaced_after_a_step_up() {
    let (_pool, app, _lock) = setup!();
    let (user, email) = seed_user().await;
    enrol(&app, user).await;
    let old: Vec<String> =
        sqlx::query_scalar("SELECT code_hash FROM mfa_recovery_codes WHERE user_id = $1")
            .bind(user)
            .fetch_all(&db::testing::seed_pool())
            .await
            .unwrap();
    assert_eq!(old.len(), 10);

    let held = send(
        &app,
        with_cookie(
            json_request(Method::POST, "/api/v1/auth/mfa/recovery-codes", json!({})),
            &stale(user),
        ),
    )
    .await;
    assert_eq!(held.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(error_code(held).await, "mfa_step_up_required");

    let done = send(
        &app,
        with_cookie(
            json_request(Method::POST, "/api/v1/auth/mfa/recovery-codes", json!({})),
            &fresh(user),
        ),
    )
    .await;
    assert_eq!(done.status(), StatusCode::OK);
    let codes: Vec<String> = json_body(done).await["recoveryCodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(codes.len(), 10);
    let new: Vec<String> =
        sqlx::query_scalar("SELECT code_hash FROM mfa_recovery_codes WHERE user_id = $1")
            .bind(user)
            .fetch_all(&db::testing::seed_pool())
            .await
            .unwrap();
    assert_eq!(new.len(), 10);
    assert!(new.iter().all(|h| !old.contains(h)), "every code is new");
    assert!(audit_actions(user)
        .await
        .contains(&"auth.mfa_recovery_codes_regenerated".to_string()));

    // A new code signs in; the codes it replaced are dead.
    let pending = password_step(&app, &email).await;
    let ok = send(
        &app,
        verify_request(&pending, json!({ "recoveryCode": codes[0] })),
    )
    .await;
    assert_eq!(ok.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_tenant_owner_resets_a_members_factor_and_ends_their_sessions() {
    let (_pool, app, _lock) = setup!();
    let tenant = seed_tenant().await;
    let (owner, _, _) = seed_member(tenant, "owner").await;
    let (member, member_membership, member_email) = seed_member(tenant, "agent").await;
    enrol(&app, owner).await;
    enrol(&app, member).await;
    let member_session = fresh(member);
    assert_eq!(
        send(
            &app,
            with_cookie(
                Request::get("/api/v1/me").body(Body::empty()).unwrap(),
                &member_session
            )
        )
        .await
        .status(),
        StatusCode::OK
    );

    let url = format!("/api/v1/tenant/members/{member_membership}/mfa/reset");
    let reset = |cookie: &str| tenant_json(Method::POST, &url, tenant, cookie, json!({}));

    // Needs a fresh code from the owner first.
    let held = send(&app, reset(&stale(owner))).await;
    assert_eq!(held.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(error_code(held).await, "mfa_step_up_required");
    let done = send(&app, reset(&fresh(owner))).await;
    assert_eq!(done.status(), StatusCode::NO_CONTENT);

    // The factor and codes are gone, the member's sessions stopped, and it is audited.
    let factors: i64 = sqlx::query_scalar("SELECT count(*) FROM mfa_factors WHERE user_id = $1")
        .bind(member)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap();
    let codes: i64 =
        sqlx::query_scalar("SELECT count(*) FROM mfa_recovery_codes WHERE user_id = $1")
            .bind(member)
            .fetch_one(&db::testing::seed_pool())
            .await
            .unwrap();
    assert_eq!((factors, codes), (0, 0));
    assert_eq!(
        send(
            &app,
            with_cookie(
                Request::get("/api/v1/me").body(Body::empty()).unwrap(),
                &member_session
            )
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED,
        "an existing session is ended"
    );
    let row: (Uuid, String, Value) = sqlx::query_as(
        "SELECT actor_user_id, resource_id, details FROM audit_logs WHERE action = 'auth.mfa_reset'",
    )
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();
    assert_eq!(row.0, owner);
    assert_eq!(row.1, member.to_string());
    assert_eq!(row.2["by"], "tenant");

    // The member signs in with a password again (a second later: a session from the second of the
    // reset is one the reset ends) and is held until they set a factor up.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let login = send(
        &app,
        json_request(
            Method::POST,
            "/api/v1/auth/login",
            json!({ "email": member_email, "password": PASSWORD }),
        ),
    )
    .await;
    assert_eq!(login.status(), StatusCode::OK);
    let relogin = format!(
        "app_session={}",
        cookie_value(&login, "app_session").unwrap()
    );
    let blocked = send(&app, tenant_get("/api/v1/tenant/members", tenant, &relogin)).await;
    assert_eq!(error_code(blocked).await, "mfa_enrolment_required");
}

#[tokio::test]
async fn a_tenant_cannot_reset_a_user_other_workspaces_or_platform_depend_on() {
    let (_pool, app, _lock) = setup!();
    let tenant = seed_tenant().await;
    let other_tenant = seed_tenant().await;
    let (owner, owner_membership, _) = seed_member(tenant, "owner").await;
    let (shared, shared_membership, _) = seed_member(tenant, "agent").await;
    sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, 'agent')",
    )
    .bind(other_tenant)
    .bind(shared)
    .execute(&db::testing::seed_pool())
    .await
    .unwrap();
    let (staff, staff_membership, _) = seed_member(tenant, "agent").await;
    sqlx::query("UPDATE users SET platform_role = 'support' WHERE id = $1")
        .bind(staff)
        .execute(&db::testing::owner_pool())
        .await
        .unwrap();
    let (_admin, admin_membership, _) = seed_member(tenant, "admin").await;
    enrol(&app, owner).await;
    enrol(&app, shared).await;
    enrol(&app, staff).await;

    let attempt = |membership: Uuid, cookie: &str| {
        tenant_json(
            Method::POST,
            &format!("/api/v1/tenant/members/{membership}/mfa/reset"),
            tenant,
            cookie,
            json!({}),
        )
    };
    // A user who also belongs to another workspace, and platform staff: refused.
    for (name, membership) in [("shared", shared_membership), ("staff", staff_membership)] {
        let r = send(&app, attempt(membership, &fresh(owner))).await;
        assert_eq!(r.status(), StatusCode::FORBIDDEN, "{name}");
    }
    // Your own: refused. Someone who is not an owner: refused. An unknown member: not found.
    assert_eq!(
        send(&app, attempt(owner_membership, &fresh(owner)))
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (admin_user, _, _) = {
        let id: Uuid = sqlx::query_scalar("SELECT user_id FROM tenant_memberships WHERE id = $1")
            .bind(admin_membership)
            .fetch_one(&db::testing::seed_pool())
            .await
            .unwrap();
        (id, 0, 0)
    };
    assert_eq!(
        send(&app, attempt(shared_membership, &fresh(admin_user)))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&app, attempt(Uuid::new_v4(), &fresh(owner)))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );

    // Nothing was reset.
    let factors: i64 =
        sqlx::query_scalar("SELECT count(*) FROM mfa_factors WHERE user_id = ANY($1)")
            .bind(vec![shared, staff, owner])
            .fetch_one(&db::testing::seed_pool())
            .await
            .unwrap();
    assert_eq!(factors, 3);
}

#[tokio::test]
async fn a_platform_administrator_resets_any_users_factor() {
    let (_pool, app, _lock) = setup!();
    let tenant = seed_tenant().await;
    let (target, _, _) = seed_member(tenant, "owner").await;
    enrol(&app, target).await;
    let (admin, _) = seed_user().await;
    sqlx::query("UPDATE users SET platform_role = 'super_admin' WHERE id = $1")
        .bind(admin)
        .execute(&db::testing::owner_pool())
        .await
        .unwrap();
    let url = format!("/api/v1/platform/users/{target}/mfa/reset");
    let post =
        |url: &str, cookie: &str| with_cookie(json_request(Method::POST, url, json!({})), cookie);

    // A tenant owner has no say here.
    assert_eq!(
        send(&app, post(&url, &fresh(target))).await.status(),
        StatusCode::FORBIDDEN
    );
    // The administrator needs a fresh code; you cannot reset yourself; unknown users are not found.
    assert_eq!(
        send(&app, post(&url, &stale(admin))).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            &app,
            post(
                &format!("/api/v1/platform/users/{admin}/mfa/reset"),
                &fresh(admin)
            )
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        send(
            &app,
            post(
                &format!("/api/v1/platform/users/{}/mfa/reset", Uuid::new_v4()),
                &fresh(admin)
            )
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&app, post(&url, &fresh(admin))).await.status(),
        StatusCode::NO_CONTENT
    );
    let factors: i64 = sqlx::query_scalar("SELECT count(*) FROM mfa_factors WHERE user_id = $1")
        .bind(target)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap();
    assert_eq!(factors, 0);
    let by: String =
        sqlx::query_scalar("SELECT details->>'by' FROM audit_logs WHERE action = 'auth.mfa_reset'")
            .fetch_one(&db::testing::seed_pool())
            .await
            .unwrap();
    assert_eq!(by, "platform");
}
