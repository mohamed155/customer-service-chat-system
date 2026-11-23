#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: a failed unwrap is the failed test
//! Sessions and rotating refresh tokens (MOH-93, ADR-005 §3): the access token's claims, rotation,
//! reuse detection, the concurrency grace, expiry and revocation.

use std::sync::Arc;
use std::time::Duration;

use argon2::{Argon2, PasswordHasher};
use axum::body::Body;
use axum::http::{Method, Request};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use server::router;
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;

const ORIGIN: &str = "http://localhost:4200";
const PASSWORD: &str = "Passw0rd!";
const ACCESS: &str = "__Host-app_session";
const REFRESH: &str = "__Secure-app_refresh";

fn test_config() -> config::AppConfig {
    config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: "redis://127.0.0.1:6379".into(),
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
        login_lockout: Default::default(),
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
        outbound_http: config::OutboundHttpConfig::default(),
    }
}

fn app_state(pool: sqlx::PgPool) -> AppState {
    let cfg = test_config();
    AppState {
        config: Arc::new(cfg.clone()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool, &cfg).unwrap(),
        budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
    }
}

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            assert!(
                !require_db_tests(),
                "REQUIRE_DB_TESTS=1 but DATABASE_URL is not set"
            );
            eprintln!("skipping sessions test: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        assert!(
            !require_db_tests(),
            "REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable"
        );
        eprintln!("skipping sessions test: could not connect to DATABASE_URL");
        return None;
    }
    db::testing::migrate().await;
    Some(pool)
}

async fn send(pool: &sqlx::PgPool, req: Request<Body>) -> Response {
    router::app(app_state(pool.clone()))
        .oneshot(req)
        .await
        .expect("request should complete")
}

async fn seed_user(pool: &sqlx::PgPool, platform_role: Option<&str>) -> (Uuid, String) {
    let email = format!("sessions_{}@example.com", Uuid::new_v4());
    let hash = Argon2::default()
        .hash_password(PASSWORD.as_bytes())
        .unwrap()
        .to_string();
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name, platform_role, password_hash) \
         VALUES ($1, 'Seed User', $2, $3) RETURNING id",
    )
    .bind(&email)
    .bind(platform_role)
    .bind(hash)
    .fetch_one(pool)
    .await
    .unwrap();
    (id, email)
}

fn login_request(email: &str) -> Request<Body> {
    Request::builder()
        .uri("/api/v1/auth/login")
        .method(Method::POST)
        .header("origin", ORIGIN)
        .header("x-requested-with", "XMLHttpRequest")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "email": email, "password": PASSWORD }).to_string(),
        ))
        .unwrap()
}

fn refresh_request(refresh_token: Option<&str>, with_proof: bool) -> Request<Body> {
    let mut builder = Request::builder()
        .uri("/api/v1/auth/refresh")
        .method(Method::POST);
    if with_proof {
        builder = builder
            .header("origin", ORIGIN)
            .header("x-requested-with", "XMLHttpRequest");
    }
    if let Some(token) = refresh_token {
        builder = builder.header("cookie", format!("{REFRESH}={token}"));
    }
    builder.body(Body::empty()).unwrap()
}

fn me_request(access_token: &str) -> Request<Body> {
    Request::builder()
        .uri("/api/v1/me")
        .header("cookie", format!("{ACCESS}={access_token}"))
        .body(Body::empty())
        .unwrap()
}

fn logout_request(access_token: &str) -> Request<Body> {
    Request::builder()
        .uri("/api/v1/auth/logout")
        .method(Method::POST)
        .header("origin", ORIGIN)
        .header("x-requested-with", "XMLHttpRequest")
        .header("cookie", format!("{ACCESS}={access_token}"))
        .body(Body::empty())
        .unwrap()
}

/// The value of the `Set-Cookie` named `name`, or `None` when the response sets none (or clears it).
fn cookie(response: &Response, name: &str) -> Option<String> {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|c| {
            let (n, v) = c.split(';').next()?.split_once('=')?;
            (n == name && !v.is_empty()).then(|| v.to_owned())
        })
}

fn cleared(response: &Response, name: &str) -> bool {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .any(|c| c.starts_with(&format!("{name}=;")) && c.contains("Max-Age=0"))
}

struct Tokens {
    access: String,
    refresh: String,
}

async fn sign_in(pool: &sqlx::PgPool, email: &str) -> Tokens {
    let response = send(pool, login_request(email)).await;
    assert_eq!(response.status(), 200);
    Tokens {
        access: cookie(&response, ACCESS).expect("an access cookie"),
        refresh: cookie(&response, REFRESH).expect("a refresh cookie"),
    }
}

fn keys() -> &'static identity::keys::KeySet {
    identity::keys::keys_for(&config::Environment::Test)
}

type SessionRow = (
    bool,
    Option<String>,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
);

async fn session_row(pool: &sqlx::PgPool, sid: Uuid) -> SessionRow {
    sqlx::query_as(
        "SELECT revoked_at IS NOT NULL, revoke_reason, idle_expires_at, absolute_expires_at \
         FROM auth_sessions WHERE id = $1",
    )
    .bind(sid)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn sid_of(access: &str) -> Uuid {
    identity::session::validate_access_token(keys(), access)
        .unwrap()
        .sid
}

fn hash_of(token: &str) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    Sha256::digest(token.as_bytes()).to_vec()
}

#[tokio::test]
async fn sign_in_starts_a_session_with_an_eddsa_access_token_and_a_hashed_refresh_token() {
    let Some(pool) = get_pool().await else { return };
    let (user, email) = seed_user(&pool, None).await;
    let tokens = sign_in(&pool, &email).await;

    let claims = identity::session::validate_access_token(keys(), &tokens.access).unwrap();
    assert_eq!(claims.sub, user);
    assert_eq!(claims.typ.as_deref(), Some("tenant_user"));
    assert_eq!(claims.exp - claims.iat, 600, "ten minutes");
    let header = jsonwebtoken::decode_header(&tokens.access).unwrap();
    assert_eq!(header.alg, jsonwebtoken::Algorithm::EdDSA);
    assert!(header.kid.is_some());

    // The session row, with the tenant-user lifetimes.
    let (revoked, _, idle, absolute) = session_row(&pool, claims.sid).await;
    assert!(!revoked);
    let now = chrono::Utc::now();
    assert!((idle - now).num_seconds().abs_diff(12 * 3600) < 30);
    assert!((absolute - now).num_seconds().abs_diff(7 * 24 * 3600) < 30);

    // The refresh token is opaque, prefixed, and only its hash is stored.
    assert!(tokens.refresh.starts_with("rt_"));
    let stored: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM refresh_tokens WHERE token_hash = $1")
            .bind(hash_of(&tokens.refresh))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, 1);
    let plaintext: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM refresh_tokens WHERE convert_from(token_hash, 'SQL_ASCII') = $1",
    )
    .bind(&tokens.refresh)
    .fetch_one(&pool)
    .await
    .unwrap_or(0);
    assert_eq!(plaintext, 0, "the plaintext token is never stored");

    assert_eq!(send(&pool, me_request(&tokens.access)).await.status(), 200);
}

#[tokio::test]
async fn sign_in_and_refresh_issue_only_current_session_cookies() {
    let Some(pool) = get_pool().await else { return };
    let (_, email) = seed_user(&pool, None).await;

    let login = send(&pool, login_request(&email)).await;
    assert!(cookie(&login, ACCESS).is_some());
    assert_eq!(login.headers().get_all("set-cookie").iter().count(), 2);
    let refresh = cookie(&login, REFRESH).unwrap();

    let refreshed = send(&pool, refresh_request(Some(&refresh), true)).await;
    assert!(cookie(&refreshed, ACCESS).is_some());
    assert!(cookie(&refreshed, REFRESH).is_some());
    assert_eq!(refreshed.headers().get_all("set-cookie").iter().count(), 2);
}

#[tokio::test]
async fn a_platform_user_gets_the_shorter_session() {
    let Some(pool) = get_pool().await else { return };
    let (_, email) = seed_user(&pool, Some("support")).await;
    let tokens = sign_in(&pool, &email).await;

    let claims = identity::session::validate_access_token(keys(), &tokens.access).unwrap();
    assert_eq!(claims.typ.as_deref(), Some("platform_user"));
    let (_, _, idle, absolute) = session_row(&pool, claims.sid).await;
    let now = chrono::Utc::now();
    assert!(
        (idle - now).num_seconds().abs_diff(3600) < 30,
        "one hour idle"
    );
    assert!(
        (absolute - now).num_seconds().abs_diff(12 * 3600) < 30,
        "twelve hours at most"
    );
}

#[tokio::test]
async fn refreshing_rotates_the_token_and_keeps_the_session() {
    let Some(pool) = get_pool().await else { return };
    let (_, email) = seed_user(&pool, None).await;
    let first = sign_in(&pool, &email).await;
    let sid = sid_of(&first.access);

    let response = send(&pool, refresh_request(Some(&first.refresh), true)).await;
    assert_eq!(response.status(), 204);
    let access = cookie(&response, ACCESS).expect("a new access token");
    let refresh = cookie(&response, REFRESH).expect("a new refresh token");
    assert_ne!(
        refresh, first.refresh,
        "the refresh token rotates on every use"
    );
    assert_ne!(access, first.access);
    assert_eq!(sid_of(&access), sid, "the session is the same");

    // The new pair works; the used token is marked used and its successor chained to it.
    assert_eq!(send(&pool, me_request(&access)).await.status(), 200);
    let chained: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM refresh_tokens c JOIN refresh_tokens p ON p.id = c.parent_id \
         WHERE c.token_hash = $1 AND p.token_hash = $2 AND p.used_at IS NOT NULL",
    )
    .bind(hash_of(&refresh))
    .bind(hash_of(&first.refresh))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(chained, 1);

    let (revoked, _, _, _) = session_row(&pool, sid).await;
    assert!(!revoked);
}

#[tokio::test]
async fn a_replayed_refresh_token_revokes_the_whole_session_and_is_audited() {
    let Some(pool) = get_pool().await else { return };
    let (user, email) = seed_user(&pool, None).await;
    let first = sign_in(&pool, &email).await;
    let sid = sid_of(&first.access);

    let rotated = send(&pool, refresh_request(Some(&first.refresh), true)).await;
    let second_access = cookie(&rotated, ACCESS).unwrap();
    let second_refresh = cookie(&rotated, REFRESH).unwrap();

    // The attacker replays the first token after the grace window.
    sqlx::query(
        "UPDATE refresh_tokens SET used_at = now() - interval '30 seconds' WHERE token_hash = $1",
    )
    .bind(hash_of(&first.refresh))
    .execute(&pool)
    .await
    .unwrap();
    let replay = send(&pool, refresh_request(Some(&first.refresh), true)).await;
    assert_eq!(replay.status(), 401);
    assert!(cleared(&replay, ACCESS) && cleared(&replay, REFRESH));

    let (revoked, reason, _, _) = session_row(&pool, sid).await;
    assert!(revoked);
    assert_eq!(reason.as_deref(), Some("refresh_reuse"));

    // Everyone holding the family is out: the legitimate client's newest refresh token and its
    // access token too.
    assert_eq!(
        send(&pool, refresh_request(Some(&second_refresh), true))
            .await
            .status(),
        401
    );
    assert_eq!(send(&pool, me_request(&second_access)).await.status(), 401);
    assert_eq!(send(&pool, me_request(&first.access)).await.status(), 401);

    let audited: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE action = 'auth.refresh_reuse_detected' \
         AND actor_user_id = $1 AND resource_id = $2",
    )
    .bind(user)
    .bind(sid.to_string())
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();
    assert_eq!(audited, 1);
}

#[tokio::test]
async fn two_tabs_refreshing_at_once_keep_the_session_and_only_one_sets_the_refresh_cookie() {
    let Some(pool) = get_pool().await else { return };
    let (_, email) = seed_user(&pool, None).await;
    let first = sign_in(&pool, &email).await;
    let sid = sid_of(&first.access);

    let (a, b) = tokio::join!(
        send(&pool, refresh_request(Some(&first.refresh), true)),
        send(&pool, refresh_request(Some(&first.refresh), true)),
    );
    assert_eq!(a.status(), 204);
    assert_eq!(b.status(), 204, "the second tab is not treated as theft");

    let refreshes = [cookie(&a, REFRESH), cookie(&b, REFRESH)];
    assert_eq!(
        refreshes.iter().filter(|r| r.is_some()).count(),
        1,
        "exactly one response carries the successor refresh token"
    );
    // Both get an access token for the same, still live session.
    for response in [&a, &b] {
        let access = cookie(response, ACCESS).expect("an access token");
        assert_eq!(sid_of(&access), sid);
        assert_eq!(send(&pool, me_request(&access)).await.status(), 200);
    }
    let (revoked, _, _, _) = session_row(&pool, sid).await;
    assert!(!revoked);

    // The successor the winner set is good for the next refresh.
    let successor = refreshes.into_iter().flatten().next().unwrap();
    assert_eq!(
        send(&pool, refresh_request(Some(&successor), true))
            .await
            .status(),
        204
    );
}

#[tokio::test]
async fn unknown_and_missing_refresh_tokens_are_refused_with_the_cookies_cleared() {
    let Some(pool) = get_pool().await else { return };

    let unknown = send(&pool, refresh_request(Some("rt_not-a-token"), true)).await;
    assert_eq!(unknown.status(), 401);
    assert!(cleared(&unknown, REFRESH));

    let missing = send(&pool, refresh_request(None, true)).await;
    assert_eq!(missing.status(), 401);
}

#[tokio::test]
async fn an_expired_refresh_token_is_refused_without_ending_the_session() {
    let Some(pool) = get_pool().await else { return };
    let (_, email) = seed_user(&pool, None).await;
    let tokens = sign_in(&pool, &email).await;
    let sid = sid_of(&tokens.access);

    sqlx::query(
        "UPDATE refresh_tokens SET expires_at = now() - interval '1 second' WHERE token_hash = $1",
    )
    .bind(hash_of(&tokens.refresh))
    .execute(&pool)
    .await
    .unwrap();
    let response = send(&pool, refresh_request(Some(&tokens.refresh), true)).await;
    assert_eq!(response.status(), 401);

    // Expiry is not theft: nothing is revoked and nothing is audited as reuse.
    let (revoked, _, _, _) = session_row(&pool, sid).await;
    assert!(!revoked);
}

#[tokio::test]
async fn a_session_idle_past_its_limit_cannot_be_refreshed() {
    let Some(pool) = get_pool().await else { return };
    let (_, email) = seed_user(&pool, None).await;
    let tokens = sign_in(&pool, &email).await;
    let sid = sid_of(&tokens.access);

    sqlx::query(
        "UPDATE auth_sessions SET idle_expires_at = now() - interval '1 second' WHERE id = $1",
    )
    .bind(sid)
    .execute(&pool)
    .await
    .unwrap();
    let response = send(&pool, refresh_request(Some(&tokens.refresh), true)).await;
    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn refreshing_never_extends_a_session_past_its_absolute_limit() {
    let Some(pool) = get_pool().await else { return };
    let (_, email) = seed_user(&pool, None).await;
    let tokens = sign_in(&pool, &email).await;
    let sid = sid_of(&tokens.access);

    sqlx::query(
        "UPDATE auth_sessions SET idle_expires_at = now() + interval '60 seconds', \
         absolute_expires_at = now() + interval '90 seconds' WHERE id = $1",
    )
    .bind(sid)
    .execute(&pool)
    .await
    .unwrap();
    let response = send(&pool, refresh_request(Some(&tokens.refresh), true)).await;
    assert_eq!(response.status(), 204);

    let (_, _, idle, absolute) = session_row(&pool, sid).await;
    assert_eq!(
        idle, absolute,
        "the idle limit is capped at the absolute one"
    );
    let newest: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT max(expires_at) FROM refresh_tokens \
         WHERE session_id = $1 AND parent_id IS NOT NULL",
    )
    .bind(sid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(newest <= absolute);
    let max_age: i64 = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|c| c.starts_with(REFRESH))
        .and_then(|c| c.split("Max-Age=").nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap();
    assert!(
        max_age <= 90,
        "the cookie does not outlive the session: {max_age}"
    );
}

#[tokio::test]
async fn refresh_needs_the_same_proof_of_origin_as_sign_in() {
    let Some(pool) = get_pool().await else { return };
    let (_, email) = seed_user(&pool, None).await;
    let tokens = sign_in(&pool, &email).await;

    let no_proof = send(&pool, refresh_request(Some(&tokens.refresh), false)).await;
    assert_eq!(no_proof.status(), 403);

    let foreign = Request::builder()
        .uri("/api/v1/auth/refresh")
        .method(Method::POST)
        .header("origin", "https://evil.example")
        .header("x-requested-with", "XMLHttpRequest")
        .header("cookie", format!("{REFRESH}={}", tokens.refresh))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&pool, foreign).await.status(), 403);

    // The refused attempts did not use the token up.
    assert_eq!(
        send(&pool, refresh_request(Some(&tokens.refresh), true))
            .await
            .status(),
        204
    );
}

#[tokio::test]
async fn logging_out_ends_the_session_for_its_access_and_refresh_tokens() {
    let Some(pool) = get_pool().await else { return };
    let (_, email) = seed_user(&pool, None).await;
    let tokens = sign_in(&pool, &email).await;
    let sid = sid_of(&tokens.access);

    let logout = send(&pool, logout_request(&tokens.access)).await;
    assert_eq!(logout.status(), 204);
    assert!(cleared(&logout, ACCESS) && cleared(&logout, REFRESH));

    let (revoked, reason, _, _) = session_row(&pool, sid).await;
    assert!(revoked);
    assert_eq!(reason.as_deref(), Some("logout"));
    // At once, not when the access token runs out.
    assert_eq!(send(&pool, me_request(&tokens.access)).await.status(), 401);
    assert_eq!(
        send(&pool, refresh_request(Some(&tokens.refresh), true))
            .await
            .status(),
        401
    );
}

#[tokio::test]
async fn ending_every_session_of_a_user_ends_each_of_them_except_the_one_kept() {
    let Some(pool) = get_pool().await else { return };
    let (user, email) = seed_user(&pool, None).await;
    let laptop = sign_in(&pool, &email).await;
    let phone = sign_in(&pool, &email).await;
    let (_, other_email) = seed_user(&pool, None).await;
    let stranger = sign_in(&pool, &other_email).await;

    let ended = identity::sessions::revoke_user_sessions(
        &pool,
        user,
        identity::sessions::reason::PASSWORD_RESET,
        Some(sid_of(&laptop.access)),
    )
    .await
    .unwrap();
    assert_eq!(ended, 1);
    assert_eq!(send(&pool, me_request(&laptop.access)).await.status(), 200);
    assert_eq!(send(&pool, me_request(&phone.access)).await.status(), 401);
    assert_eq!(
        send(&pool, me_request(&stranger.access)).await.status(),
        200,
        "other users are untouched"
    );

    let ended = identity::sessions::revoke_user_sessions(
        &pool,
        user,
        identity::sessions::reason::PASSWORD_RESET,
        None,
    )
    .await
    .unwrap();
    assert_eq!(ended, 1);
    assert_eq!(send(&pool, me_request(&laptop.access)).await.status(), 401);
    assert_eq!(
        send(&pool, refresh_request(Some(&laptop.refresh), true))
            .await
            .status(),
        401
    );
    // The user can sign in again straight away.
    let again = sign_in(&pool, &email).await;
    assert_eq!(send(&pool, me_request(&again.access)).await.status(), 200);
}

#[tokio::test]
async fn retired_hs256_sessions_and_the_old_cookie_are_refused() {
    let Some(pool) = get_pool().await else { return };
    let (user, email) = seed_user(&pool, None).await;
    let now = chrono::Utc::now().timestamp();
    let legacy = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &json!({ "sub": user, "jti": Uuid::new_v4(), "iat": now, "exp": now + 600 }),
        &jsonwebtoken::EncodingKey::from_secret(b"test-auth-jwt-secret-at-least-32-bytes").unwrap(),
    )
    .unwrap();
    assert_eq!(send(&pool, me_request(&legacy)).await.status(), 401);

    let tokens = sign_in(&pool, &email).await;
    assert_eq!(send(&pool, me_request(&tokens.access)).await.status(), 200);
    for token in [&legacy, &tokens.access] {
        let request = Request::builder()
            .uri("/api/v1/me")
            .header("cookie", format!("app_session={token}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(send(&pool, request).await.status(), 401);
    }
}

#[tokio::test]
async fn a_completed_mfa_challenge_can_create_only_one_session() {
    let Some(pool) = get_pool().await else { return };
    let (user, _) = seed_user(&pool, None).await;
    let (_, challenge) = identity::session::issue_pending(keys(), user).unwrap();
    let headers = axum::http::HeaderMap::new();
    let complete = || {
        identity::sessions::start_from_challenge(
            &pool,
            &config::Environment::Test,
            &challenge,
            identity::session::SessionAuth::second_factor(identity::session::AMR_OTP),
            &headers,
        )
    };

    let (first, second) = tokio::join!(complete(), complete());
    let failure = match (first, second) {
        (Ok(_), Err(error)) | (Err(error), Ok(_)) => error,
        _ => panic!("exactly one completion should create a session"),
    };
    assert_eq!(failure.into_response().status(), 401);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM auth_sessions WHERE user_id = $1")
        .bind(user)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn an_access_token_for_a_deleted_user_or_another_users_session_is_refused() {
    let Some(pool) = get_pool().await else { return };
    let (alice, alice_email) = seed_user(&pool, None).await;
    let (_, bob_email) = seed_user(&pool, None).await;
    let alice_tokens = sign_in(&pool, &alice_email).await;
    let bob_tokens = sign_in(&pool, &bob_email).await;

    // A token that names Alice but Bob's session id (signed with the real key, so only the
    // session check can catch it).
    let (kid, key) = keys().signing().unwrap();
    let mut claims =
        identity::session::validate_access_token(keys(), &alice_tokens.access).unwrap();
    claims.sid = sid_of(&bob_tokens.access);
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::EdDSA);
    header.kid = Some(kid.to_owned());
    let mixed = jsonwebtoken::encode(&header, &claims, key).unwrap();
    assert_eq!(send(&pool, me_request(&mixed)).await.status(), 401);

    sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
        .bind(alice)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        send(&pool, me_request(&alice_tokens.access)).await.status(),
        401
    );
    assert_eq!(
        send(&pool, refresh_request(Some(&alice_tokens.refresh), true))
            .await
            .status(),
        401
    );
}

#[tokio::test]
async fn the_sweep_deletes_old_ended_sessions_and_keeps_live_and_recent_ones() {
    let Some(pool) = get_pool().await else { return };
    let (_, email) = seed_user(&pool, None).await;
    let live = sign_in(&pool, &email).await;
    let recently_revoked = sign_in(&pool, &email).await;
    let long_revoked = sign_in(&pool, &email).await;
    let long_expired = sign_in(&pool, &email).await;

    sqlx::query(
        "UPDATE auth_sessions SET revoked_at = now() - interval '1 day', revoke_reason = 'logout' \
         WHERE id = $1",
    )
    .bind(sid_of(&recently_revoked.access))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE auth_sessions SET revoked_at = now() - interval '31 days', revoke_reason = 'logout' \
         WHERE id = $1",
    )
    .bind(sid_of(&long_revoked.access))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE auth_sessions SET idle_expires_at = now() - interval '40 days', \
         absolute_expires_at = now() - interval '31 days' WHERE id = $1",
    )
    .bind(sid_of(&long_expired.access))
    .execute(&pool)
    .await
    .unwrap();

    identity::sessions::sweep_ended(&pool).await.unwrap();

    let exists = |id: Uuid| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM auth_sessions WHERE id = $1)",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert!(exists(sid_of(&live.access)).await);
    assert!(exists(sid_of(&recently_revoked.access)).await);
    assert!(!exists(sid_of(&long_revoked.access)).await);
    assert!(!exists(sid_of(&long_expired.access)).await);

    // The refresh tokens of a deleted session went with it; a live one keeps its own.
    let gone: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM refresh_tokens WHERE token_hash = $1 OR token_hash = $2",
    )
    .bind(hash_of(&long_revoked.refresh))
    .bind(hash_of(&long_expired.refresh))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(gone, 0);
    let kept: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refresh_tokens WHERE token_hash = $1")
        .bind(hash_of(&live.refresh))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(kept, 1);
}

async fn tenant_with_owner(pool: &sqlx::PgPool, owner: Uuid) -> Uuid {
    let tenant: Uuid = sqlx::query_scalar(
        "INSERT INTO tenants (name, slug) VALUES ('Sessions Tenant', $1) RETURNING id",
    )
    .bind(format!("sessions-{}", Uuid::new_v4().simple()))
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();
    add_membership(pool, tenant, owner, "owner").await;
    tenant
}

async fn add_membership(_pool: &sqlx::PgPool, tenant: Uuid, user: Uuid, role: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, $3, 'active') RETURNING id",
    )
    .bind(tenant)
    .bind(user)
    .bind(role)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap()
}

fn disable_request(access: &str, tenant: Uuid, membership: Uuid) -> Request<Body> {
    Request::builder()
        .uri(format!("/api/v1/tenant/members/{membership}"))
        .method(Method::PATCH)
        .header("origin", ORIGIN)
        .header("x-requested-with", "XMLHttpRequest")
        .header("x-tenant-id", tenant.to_string())
        .header("content-type", "application/json")
        .header("cookie", format!("{ACCESS}={access}"))
        .body(Body::from(json!({ "status": "disabled" }).to_string()))
        .unwrap()
}

#[tokio::test]
async fn disabling_a_member_ends_their_sessions_only_when_they_have_no_other_access() {
    let Some(pool) = get_pool().await else { return };
    let (owner, owner_email) = seed_user(&pool, None).await;
    let tenant = tenant_with_owner(&pool, owner).await;
    let owner_tokens = sign_in(&pool, &owner_email).await;

    // Alone in this workspace.
    let (alone, alone_email) = seed_user(&pool, None).await;
    let alone_membership = add_membership(&pool, tenant, alone, "agent").await;
    let alone_tokens = sign_in(&pool, &alone_email).await;
    // Also a member of another workspace.
    let (both, both_email) = seed_user(&pool, None).await;
    let both_membership = add_membership(&pool, tenant, both, "agent").await;
    let (other_owner, _) = seed_user(&pool, None).await;
    let other_tenant = tenant_with_owner(&pool, other_owner).await;
    add_membership(&pool, other_tenant, both, "agent").await;
    let both_tokens = sign_in(&pool, &both_email).await;

    assert_eq!(
        send(&pool, me_request(&alone_tokens.access)).await.status(),
        200
    );
    let response = send(
        &pool,
        disable_request(&owner_tokens.access, tenant, alone_membership),
    )
    .await;
    assert_eq!(response.status(), 200);
    // Nothing left to sign in to: the session ends at once, and so does its refresh token.
    assert_eq!(
        send(&pool, me_request(&alone_tokens.access)).await.status(),
        401
    );
    assert_eq!(
        send(&pool, refresh_request(Some(&alone_tokens.refresh), true))
            .await
            .status(),
        401
    );
    let reason: Option<String> =
        sqlx::query_scalar("SELECT revoke_reason FROM auth_sessions WHERE id = $1")
            .bind(sid_of(&alone_tokens.access))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(reason.as_deref(), Some("member_removed"));

    // Still a member elsewhere: one workspace's admin cannot sign them out of the other.
    let response = send(
        &pool,
        disable_request(&owner_tokens.access, tenant, both_membership),
    )
    .await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        send(&pool, me_request(&both_tokens.access)).await.status(),
        200
    );
    assert_eq!(
        send(&pool, refresh_request(Some(&both_tokens.refresh), true))
            .await
            .status(),
        204
    );
    // ... but this workspace is closed to them.
    let into_tenant = Request::builder()
        .uri("/api/v1/tenant/members")
        .header("x-tenant-id", tenant.to_string())
        .header("cookie", format!("{ACCESS}={}", both_tokens.access))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&pool, into_tenant).await.status(), 403);
}

#[tokio::test]
async fn resetting_a_second_factor_ends_persisted_sessions_and_their_refresh_tokens() {
    let Some(pool) = get_pool().await else { return };
    let (user, email) = seed_user(&pool, None).await;
    let (admin, _) = seed_user(&pool, None).await;
    let tokens = sign_in(&pool, &email).await;
    assert_eq!(send(&pool, me_request(&tokens.access)).await.status(), 200);

    identity::mfa::reset(&pool, user, admin, "platform", None)
        .await
        .unwrap();

    // Revocation ends both the access token and the refresh token.
    let (revoked, reason, _, _) = session_row(&pool, sid_of(&tokens.access)).await;
    assert!(revoked);
    assert_eq!(reason.as_deref(), Some("mfa_reset"));
    assert_eq!(send(&pool, me_request(&tokens.access)).await.status(), 401);
    assert_eq!(
        send(&pool, refresh_request(Some(&tokens.refresh), true))
            .await
            .status(),
        401
    );
    // A new sign-in in the same second still works.
    let again = sign_in(&pool, &email).await;
    assert_eq!(send(&pool, me_request(&again.access)).await.status(), 200);
}
