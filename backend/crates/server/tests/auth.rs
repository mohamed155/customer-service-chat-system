use std::sync::Arc;
use std::time::Duration;

use argon2::{Argon2, PasswordHasher};
use axum::body::Body;
use axum::http::{Method, Request};
use axum::response::Response;
use serde_json::json;
use server::router;
use server::state::AppState;
use tower::ServiceExt;

/// Live-gated pool: returns `None` when `DATABASE_URL` is missing or
/// unreachable, so auth integration tests can be skipped without a database.
pub async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("skipping auth test: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping auth test: could not connect to DATABASE_URL");
        return None;
    }
    Some(pool)
}

/// The dashboard origin the test config allows. Requests that start a session or carry the session
/// cookie must come from an allowed origin and carry a custom header (MOH-36).
pub const TEST_ORIGIN: &str = "http://localhost:4200";

/// Build an `AppConfig` with test-friendly defaults.
pub fn test_config() -> config::AppConfig {
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

/// Build an `AppState` from a live pool and a test config.
pub fn test_app_state(pool: sqlx::PgPool) -> AppState {
    AppState {
        config: Arc::new(test_config()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool, &test_config()).unwrap(),
        budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
    }
}

/// Build the app router and send a single request via `tower::ServiceExt::oneshot`.
pub async fn send_request(pool: sqlx::PgPool, req: Request<Body>) -> Response {
    let state = test_app_state(pool);
    let app = router::app(state);
    app.oneshot(req).await.expect("request should succeed")
}

pub fn test_app_state_with_config(pool: sqlx::PgPool, config: config::AppConfig) -> AppState {
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

pub async fn send_request_with_config(
    pool: sqlx::PgPool,
    config: config::AppConfig,
    req: Request<Body>,
) -> Response {
    let state = test_app_state_with_config(pool, config);
    let app = router::app(state);
    app.oneshot(req).await.expect("request should succeed")
}

/// Insert a unique user with a real Argon2id password hash and return its id.
///
/// If `platform_role` is `Some`, the user is created with that role;
/// otherwise the column is omitted (uses the DB default).
pub async fn seed_user_with_password(
    pool: &sqlx::PgPool,
    email: &str,
    platform_role: Option<&str>,
    password: &str,
) -> uuid::Uuid {
    let password_hash = hash_password(password);

    match platform_role {
        Some(role) => {
            sqlx::query_scalar::<_, uuid::Uuid>(
                "INSERT INTO users (email, display_name, platform_role, password_hash) VALUES ($1, $2, $3, $4) RETURNING id",
            )
            .bind(email)
            .bind("Seed User")
            .bind(role)
            .bind(&password_hash)
            .fetch_one(pool)
            .await
            .expect("seed user with password and platform_role")
        }
        None => {
            sqlx::query_scalar::<_, uuid::Uuid>(
                "INSERT INTO users (email, display_name, password_hash) VALUES ($1, $2, $3) RETURNING id",
            )
            .bind(email)
            .bind("Seed User")
            .bind(&password_hash)
            .fetch_one(pool)
            .await
            .expect("seed user with password")
        }
    }
}

/// Generate a unique email for per-test auth seeds.
pub fn unique_test_email() -> String {
    format!("test_{}@example.com", uuid::Uuid::new_v4())
}

fn hash_password(password: &str) -> String {
    Argon2::default()
        .hash_password(password.as_bytes())
        .expect("hash password")
        .to_string()
}

async fn body_bytes(res: &mut Response) -> Vec<u8> {
    use http_body_util::BodyExt;
    BodyExt::collect(res.body_mut())
        .await
        .unwrap()
        .to_bytes()
        .to_vec()
}

async fn body_json(res: &mut Response) -> serde_json::Value {
    serde_json::from_slice(&body_bytes(res).await).unwrap()
}

fn login_request(email: &str, password: &str) -> Request<Body> {
    Request::builder()
        .uri("/api/v1/auth/login")
        .method("POST")
        .header("origin", TEST_ORIGIN)
        .header("x-requested-with", "XMLHttpRequest")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "email": email, "password": password }).to_string(),
        ))
        .unwrap()
}

fn signup_request(
    workspace_name: &str,
    display_name: &str,
    email: &str,
    password: &str,
) -> Request<Body> {
    Request::builder()
        .uri("/api/v1/auth/signup")
        .method("POST")
        .header("origin", TEST_ORIGIN)
        .header("x-requested-with", "XMLHttpRequest")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "workspaceName": workspace_name,
                "displayName": display_name,
                "email": email,
                "password": password,
            })
            .to_string(),
        ))
        .unwrap()
}

fn strip_request_id(mut body: serde_json::Value) -> serde_json::Value {
    if let Some(error) = body
        .get_mut("error")
        .and_then(|value| value.as_object_mut())
    {
        error.remove("request_id");
    }
    body
}

fn session_cookie_for(config: &config::AppConfig, user_id: uuid::Uuid, ttl: u64) -> String {
    let (jwt, _, _) =
        identity::session::issue_token(&config.auth_jwt_secret, ttl, user_id).unwrap();
    format!("app_session={jwt}")
}

fn get_me_request(cookie: &str) -> Request<Body> {
    Request::builder()
        .uri("/api/v1/me")
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn login_success_sets_session_cookie_and_cookie_authenticates_me() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let email = unique_test_email();
        let user_id =
            seed_user_with_password(&pool, &email, Some("super_admin"), "Passw0rd!").await;

        let mut res = send_request(pool.clone(), login_request(&email, "Passw0rd!")).await;

        assert_eq!(res.status(), 200);
        let cookie = res
            .headers()
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .expect("login should set session cookie")
            .to_owned();
        assert!(cookie.starts_with("app_session="));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("Secure"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("Max-Age=28800"));

        let body = body_json(&mut res).await;
        assert_eq!(
            body,
            json!({
                "id": user_id,
                "email": email,
                "displayName": "Seed User",
                "platformRole": "super_admin",
                // Derived from the authz matrix so new permissions don't break this.
                "platformPermissions": authz::platform_role_permissions(identity::PlatformRole::SuperAdmin),
                "staffTenantPermissions": authz::staff_tenant_permissions(identity::PlatformRole::SuperAdmin, false),
                "memberships": []
            })
        );

        let mut me_res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/me")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(me_res.status(), 200);
        let me = body_json(&mut me_res).await;
        assert_eq!(me, body);
    }

    #[tokio::test]
    async fn login_response_matches_me_with_membership_permissions() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;
        let email = unique_test_email();
        let user_id = seed_user_with_password(&pool, &email, None, "Passw0rd!").await;
        let tenant_id: uuid::Uuid =
            sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
                .bind("Auth Tenant")
                .bind(format!("auth-{}", uuid::Uuid::new_v4().simple()))
                .fetch_one(&pool)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, 'viewer')",
        )
        .bind(tenant_id)
        .bind(user_id)
        .execute(&db::testing::seed_pool())
        .await
        .unwrap();

        let mut login = send_request(pool.clone(), login_request(&email, "Passw0rd!")).await;
        assert_eq!(login.status(), 200);
        let cookie = login.headers()["set-cookie"].to_str().unwrap().to_owned();
        let login_body = body_json(&mut login).await;
        let mut me = send_request(pool, get_me_request(&cookie)).await;
        assert_eq!(me.status(), 200);
        let me_body = body_json(&mut me).await;

        assert_eq!(login_body, me_body);
        assert_eq!(login_body["platformPermissions"], json!([]));
        assert_eq!(login_body["staffTenantPermissions"], json!(null));
        assert_eq!(
            login_body["memberships"][0]["permissions"],
            json!(authz::tenant_role_permissions(authz::TenantRole::Viewer))
        );
    }

    #[tokio::test]
    async fn invalid_login_paths_return_byte_identical_401_bodies() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let active_email = unique_test_email();
        seed_user_with_password(&pool, &active_email, None, "Passw0rd!").await;

        let deleted_email = unique_test_email();
        let deleted_user = seed_user_with_password(&pool, &deleted_email, None, "Passw0rd!").await;
        sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
            .bind(deleted_user)
            .execute(&pool)
            .await
            .unwrap();

        let null_hash_email = unique_test_email();
        sqlx::query("INSERT INTO users (email, display_name) VALUES ($1, $2)")
            .bind(&null_hash_email)
            .bind("No Credential")
            .execute(&pool)
            .await
            .unwrap();

        let cases = [
            login_request(&active_email, "wrong-password"),
            login_request(&unique_test_email(), "Passw0rd!"),
            login_request(&deleted_email, "Passw0rd!"),
            login_request(&null_hash_email, "Passw0rd!"),
        ];

        let mut normalized = Vec::new();
        for req in cases {
            let mut res = send_request(pool.clone(), req).await;
            assert_eq!(res.status(), 401);
            normalized.push(strip_request_id(body_json(&mut res).await));
        }

        for body in &normalized[1..] {
            assert_eq!(body, &normalized[0]);
        }
    }

    #[tokio::test]
    async fn login_validation_errors_return_400() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let requests = [
            Request::builder()
                .uri("/api/v1/auth/login")
                .method("POST")
                .header("origin", TEST_ORIGIN)
                .header("x-requested-with", "XMLHttpRequest")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "email": "", "password": "x" }).to_string(),
                ))
                .unwrap(),
            Request::builder()
                .uri("/api/v1/auth/login")
                .method("POST")
                .header("origin", TEST_ORIGIN)
                .header("x-requested-with", "XMLHttpRequest")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "email": "a@test.com", "password": "" }).to_string(),
                ))
                .unwrap(),
            Request::builder()
                .uri("/api/v1/auth/login")
                .method("POST")
                .header("origin", TEST_ORIGIN)
                .header("x-requested-with", "XMLHttpRequest")
                .header("content-type", "application/json")
                .body(Body::from("not-json"))
                .unwrap(),
        ];

        for request in requests {
            let mut res = send_request(pool.clone(), request).await;
            assert_eq!(res.status(), 400);
            let body = body_json(&mut res).await;
            assert_eq!(body["error"]["code"], "validation_failed");
        }
    }

    #[tokio::test]
    async fn login_ignores_x_tenant_id_and_refreshes_existing_session() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let email = unique_test_email();
        seed_user_with_password(&pool, &email, None, "Passw0rd!").await;

        let first = send_request(pool.clone(), login_request(&email, "Passw0rd!")).await;
        let first_cookie = first
            .headers()
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .expect("first login should set cookie")
            .to_owned();

        let mut second = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/auth/login")
                .method("POST")
                .header("origin", TEST_ORIGIN)
                .header("x-requested-with", "XMLHttpRequest")
                .header("content-type", "application/json")
                .header("cookie", first_cookie)
                .header("X-Tenant-ID", uuid::Uuid::new_v4().to_string())
                .body(Body::from(
                    json!({ "email": email, "password": "Passw0rd!" }).to_string(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(second.status(), 200);
        assert!(second.headers().get("set-cookie").is_some());
        let body = body_json(&mut second).await;
        assert_eq!(body["email"], email);
    }

    #[tokio::test]
    async fn login_attempts_write_audit_rows() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let email = unique_test_email();
        let user_id = seed_user_with_password(&pool, &email, None, "Passw0rd!").await;

        let success = send_request(pool.clone(), login_request(&email, "Passw0rd!")).await;
        assert_eq!(success.status(), 200);
        let failure = send_request(pool.clone(), login_request(&email, "bad")).await;
        assert_eq!(failure.status(), 401);

        let success_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE action = 'auth.login_succeeded' AND actor_user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap();
        let failure_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE action = 'auth.login_failed' AND actor_user_id IS NULL AND details->>'email' = $1",
        )
        .bind(&email)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap();

        assert_eq!(success_count, 1);
        assert_eq!(failure_count, 1);
    }
    #[tokio::test]
    async fn invalid_session_cookies_return_401() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let config = test_config();
        let email = unique_test_email();
        let user_id = seed_user_with_password(&pool, &email, None, "Passw0rd!").await;

        let expired_cookie = session_cookie_for(&config, user_id, 0);
        let valid_cookie = session_cookie_for(&config, user_id, 28_800);
        let mut tampered_cookie = valid_cookie.clone();
        tampered_cookie.push('x');

        let requests = [
            get_me_request(&expired_cookie),
            get_me_request(&tampered_cookie),
            get_me_request("app_session=not-a-jwt"),
        ];

        for request in requests {
            let mut res = send_request(pool.clone(), request).await;
            assert_eq!(res.status(), 401);
            let body = body_json(&mut res).await;
            assert_eq!(body["error"]["code"], "unauthenticated");
        }
    }

    #[tokio::test]
    async fn soft_deleted_user_token_returns_401() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let config = test_config();
        let email = unique_test_email();
        let user_id = seed_user_with_password(&pool, &email, None, "Passw0rd!").await;
        let cookie = session_cookie_for(&config, user_id, 28_800);

        sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();

        let mut res = send_request(pool.clone(), get_me_request(&cookie)).await;

        assert_eq!(res.status(), 401);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "unauthenticated");
    }

    #[tokio::test]
    async fn production_ignores_dev_header_but_accepts_valid_cookie() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let mut config = test_config();
        config.environment = config::Environment::Production;

        let email = unique_test_email();
        let user_id = seed_user_with_password(&pool, &email, None, "Passw0rd!").await;
        let cookie = session_cookie_for(&config, user_id, 28_800);

        let dev_header_res = send_request_with_config(
            pool.clone(),
            config.clone(),
            Request::builder()
                .uri("/api/v1/me")
                .header("X-Dev-User-Id", user_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(dev_header_res.status(), 401);

        let cookie_res =
            send_request_with_config(pool.clone(), config, get_me_request(&cookie)).await;
        assert_eq!(cookie_res.status(), 200);
    }

    const DASHBOARD: &str = "https://dashboard.example";

    fn csrf_config() -> config::AppConfig {
        let mut config = test_config();
        config.cors_allowed_origins = vec![DASHBOARD.into()];
        config
    }

    /// A state-changing request authenticated by the session cookie, with the given extra headers.
    fn cookie_post(cookie: &str, headers: &[(&str, &str)]) -> Request<Body> {
        let mut builder = Request::builder()
            .uri("/api/v1/auth/logout")
            .method(Method::POST)
            .header("cookie", cookie);
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        builder.body(Body::empty()).unwrap()
    }

    async fn csrf_status(
        pool: &sqlx::PgPool,
        config: &config::AppConfig,
        request: Request<Body>,
    ) -> u16 {
        send_request_with_config(pool.clone(), config.clone(), request)
            .await
            .status()
            .as_u16()
    }

    async fn csrf_session(pool: &sqlx::PgPool, config: &config::AppConfig) -> String {
        let email = unique_test_email();
        let user_id = seed_user_with_password(pool, &email, None, "Passw0rd!").await;
        session_cookie_for(config, user_id, 28_800)
    }

    #[tokio::test]
    async fn cookie_request_from_a_foreign_origin_is_rejected() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;
        let config = csrf_config();
        let cookie = csrf_session(&pool, &config).await;

        let mut res = send_request_with_config(
            pool.clone(),
            config.clone(),
            cookie_post(
                &cookie,
                &[
                    ("origin", "https://evil.example"),
                    ("x-requested-with", "XMLHttpRequest"),
                ],
            ),
        )
        .await;
        assert_eq!(res.status(), 403);
        assert_eq!(body_json(&mut res).await["error"]["code"], "unauthorized");
    }

    #[tokio::test]
    async fn cookie_request_with_no_origin_and_no_referer_is_rejected() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;
        let config = csrf_config();
        let cookie = csrf_session(&pool, &config).await;

        let status = csrf_status(
            &pool,
            &config,
            cookie_post(&cookie, &[("x-requested-with", "XMLHttpRequest")]),
        )
        .await;
        assert_eq!(status, 403);
    }

    #[tokio::test]
    async fn cookie_request_without_the_custom_header_is_rejected() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;
        let config = csrf_config();
        let cookie = csrf_session(&pool, &config).await;

        let status = csrf_status(
            &pool,
            &config,
            cookie_post(&cookie, &[("origin", DASHBOARD)]),
        )
        .await;
        assert_eq!(status, 403);
    }

    #[tokio::test]
    async fn cookie_request_from_the_allowed_origin_with_the_header_is_accepted() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;
        let config = csrf_config();
        let cookie = csrf_session(&pool, &config).await;

        let with_requested_with = csrf_status(
            &pool,
            &config,
            cookie_post(
                &cookie,
                &[
                    ("origin", DASHBOARD),
                    ("x-requested-with", "XMLHttpRequest"),
                ],
            ),
        )
        .await;
        assert_eq!(with_requested_with, 204);

        // X-Tenant-ID is the other accepted custom header.
        let cookie = csrf_session(&pool, &config).await;
        let with_tenant_header = csrf_status(
            &pool,
            &config,
            cookie_post(
                &cookie,
                &[
                    ("origin", DASHBOARD),
                    ("x-tenant-id", &uuid::Uuid::new_v4().to_string()),
                ],
            ),
        )
        .await;
        assert_eq!(with_tenant_header, 204);
    }

    #[tokio::test]
    async fn referer_stands_in_for_a_missing_origin() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;
        let config = csrf_config();

        let cookie = csrf_session(&pool, &config).await;
        let allowed = csrf_status(
            &pool,
            &config,
            cookie_post(
                &cookie,
                &[
                    ("referer", "https://dashboard.example/settings/team?tab=1"),
                    ("x-requested-with", "XMLHttpRequest"),
                ],
            ),
        )
        .await;
        assert_eq!(allowed, 204);

        let cookie = csrf_session(&pool, &config).await;
        let foreign = csrf_status(
            &pool,
            &config,
            cookie_post(
                &cookie,
                &[
                    ("referer", "https://evil.example/page"),
                    ("x-requested-with", "XMLHttpRequest"),
                ],
            ),
        )
        .await;
        assert_eq!(foreign, 403);

        // An allowed host as a prefix of a foreign one is not the allowed origin.
        let cookie = csrf_session(&pool, &config).await;
        let lookalike = csrf_status(
            &pool,
            &config,
            cookie_post(
                &cookie,
                &[
                    ("referer", "https://dashboard.example.evil.test/page"),
                    ("x-requested-with", "XMLHttpRequest"),
                ],
            ),
        )
        .await;
        assert_eq!(lookalike, 403);
    }

    #[tokio::test]
    async fn safe_methods_and_the_dev_header_principal_are_not_subject_to_the_cookie_rule() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;
        let config = csrf_config();
        let cookie = csrf_session(&pool, &config).await;

        // A GET with a foreign origin is not state-changing.
        let get = send_request_with_config(
            pool.clone(),
            config.clone(),
            Request::builder()
                .uri("/api/v1/me")
                .method(Method::GET)
                .header("origin", "https://evil.example")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(get.status(), 200);

        // The development header carries no cookie to ride on: no Origin is fine, a foreign one is
        // still refused.
        let email = unique_test_email();
        let user_id = seed_user_with_password(&pool, &email, None, "Passw0rd!").await;
        let dev_post = |origin: Option<&str>| {
            let mut builder = Request::builder()
                .uri("/api/v1/auth/logout")
                .method(Method::POST)
                .header("x-dev-user-id", user_id.to_string());
            if let Some(origin) = origin {
                builder = builder.header("origin", origin);
            }
            builder.body(Body::empty()).unwrap()
        };
        assert_eq!(csrf_status(&pool, &config, dev_post(None)).await, 204);
        assert_eq!(
            csrf_status(&pool, &config, dev_post(Some("https://evil.example"))).await,
            403
        );
    }

    /// Login CSRF: a foreign page signs the victim in as the attacker. There is no session yet, so
    /// the check has to apply without a principal.
    #[tokio::test]
    async fn login_and_signup_require_an_allowed_origin_and_the_custom_header() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;
        let config = csrf_config();
        let email = unique_test_email();
        seed_user_with_password(&pool, &email, None, "Passw0rd!").await;

        let login = |headers: &[(&str, &str)]| {
            let mut builder = Request::builder()
                .uri("/api/v1/auth/login")
                .method(Method::POST)
                .header("content-type", "application/json");
            for (name, value) in headers {
                builder = builder.header(*name, *value);
            }
            builder
                .body(Body::from(
                    json!({ "email": email, "password": "Passw0rd!" }).to_string(),
                ))
                .unwrap()
        };

        for (case, headers, expected) in [
            (
                "foreign origin",
                vec![
                    ("origin", "https://evil.example"),
                    ("x-requested-with", "XMLHttpRequest"),
                ],
                403,
            ),
            (
                "no origin or referer",
                vec![("x-requested-with", "XMLHttpRequest")],
                403,
            ),
            ("no custom header", vec![("origin", DASHBOARD)], 403),
            (
                "allowed origin and header",
                vec![
                    ("origin", DASHBOARD),
                    ("x-requested-with", "XMLHttpRequest"),
                ],
                200,
            ),
            (
                "allowed referer and header",
                vec![
                    ("referer", "https://dashboard.example/login"),
                    ("x-requested-with", "XMLHttpRequest"),
                ],
                200,
            ),
        ] {
            let status = csrf_status(&pool, &config, login(&headers)).await;
            assert_eq!(status, expected, "login: {case}");
        }

        let signup = |headers: &[(&str, &str)]| {
            let mut builder = Request::builder()
                .uri("/api/v1/auth/signup")
                .method(Method::POST)
                .header("content-type", "application/json");
            for (name, value) in headers {
                builder = builder.header(*name, *value);
            }
            builder
                .body(Body::from(
                    json!({
                        "workspaceName": "Csrf Co",
                        "displayName": "Csrf",
                        "email": unique_test_email(),
                        "password": "Passw0rd!x",
                    })
                    .to_string(),
                ))
                .unwrap()
        };
        assert_eq!(
            csrf_status(
                &pool,
                &config,
                signup(&[
                    ("origin", "https://evil.example"),
                    ("x-requested-with", "XMLHttpRequest")
                ])
            )
            .await,
            403,
            "signup from a foreign origin"
        );
        assert_eq!(
            csrf_status(&pool, &config, signup(&[])).await,
            403,
            "signup without proof"
        );
    }

    #[tokio::test]
    async fn preflight_for_the_custom_header_is_allowed_from_the_allowed_origin() {
        let Some(pool) = get_pool().await else { return };
        let config = csrf_config();
        let res = send_request_with_config(
            pool,
            config,
            Request::builder()
                .uri("/api/v1/auth/login")
                .method(Method::OPTIONS)
                .header("origin", DASHBOARD)
                .header("access-control-request-method", "POST")
                .header(
                    "access-control-request-headers",
                    "content-type,x-requested-with",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert!(
            res.status().is_success(),
            "preflight status {}",
            res.status()
        );
        let allowed = res
            .headers()
            .get("access-control-allow-headers")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        assert!(allowed.contains("x-requested-with"), "{allowed}");
    }

    #[tokio::test]
    async fn logout_revokes_cookie_session_and_clears_cookie() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let email = unique_test_email();
        let user_id = seed_user_with_password(&pool, &email, None, "Passw0rd!").await;
        let login = send_request(pool.clone(), login_request(&email, "Passw0rd!")).await;
        let cookie = login
            .headers()
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .expect("login should set cookie")
            .to_owned();

        let logout = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/auth/logout")
                .method(Method::POST)
                .header("origin", TEST_ORIGIN)
                .header("x-requested-with", "XMLHttpRequest")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(logout.status(), 204);
        let clear_cookie = logout
            .headers()
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .expect("logout should clear cookie");
        assert!(clear_cookie.contains("app_session="));
        assert!(clear_cookie.contains("Max-Age=0"));

        let revoked_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM revoked_sessions WHERE user_id = $1")
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE action = 'auth.logged_out' AND actor_user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap();

        assert_eq!(revoked_count, 1);
        assert_eq!(audit_count, 1);

        let replay = send_request(pool.clone(), get_me_request(&cookie)).await;
        assert_eq!(replay.status(), 401);
    }

    #[tokio::test]
    async fn logout_requires_principal() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/auth/logout")
                .method(Method::POST)
                .header("origin", TEST_ORIGIN)
                .header("x-requested-with", "XMLHttpRequest")
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 401);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "unauthenticated");
    }

    #[tokio::test]
    async fn logout_with_dev_header_clears_cookie_without_revocation() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let email = unique_test_email();
        let user_id = seed_user_with_password(&pool, &email, None, "Passw0rd!").await;

        let logout = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/auth/logout")
                .method(Method::POST)
                .header("origin", TEST_ORIGIN)
                .header("x-requested-with", "XMLHttpRequest")
                .header("X-Dev-User-Id", user_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(logout.status(), 204);
        let clear_cookie = logout
            .headers()
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .expect("logout should clear cookie");
        assert!(clear_cookie.contains("Max-Age=0"));

        let revoked_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM revoked_sessions WHERE user_id = $1")
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(revoked_count, 0);
    }

    #[tokio::test]
    async fn signup_creates_tenant_owner_and_session() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let email = unique_test_email();
        let mut res = send_request(
            pool.clone(),
            signup_request("Acme Corp", "Jane Doe", &email, "correct horse battery"),
        )
        .await;

        assert_eq!(res.status(), 201);
        let cookie = res
            .headers()
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .expect("signup should set session cookie")
            .to_owned();
        assert!(cookie.starts_with("app_session="));

        let body = body_json(&mut res).await;
        assert_eq!(body["email"], email);
        assert_eq!(body["displayName"], "Jane Doe");
        assert_eq!(body["memberships"][0]["tenantName"], "Acme Corp");
        assert_eq!(body["memberships"][0]["role"], "owner");

        let tenant_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM tenants WHERE name = $1")
            .bind("Acme Corp")
            .fetch_one(&pool)
            .await
            .unwrap();
        let slug: String = sqlx::query_scalar("SELECT slug FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(slug, "acme-corp");

        let audit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE action = 'tenant.signup_completed' AND tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap();
        assert_eq!(audit_count, 1);

        let mut me = send_request(pool, get_me_request(&cookie)).await;
        assert_eq!(me.status(), 200);
        let me_body = body_json(&mut me).await;
        assert_eq!(me_body, body);
    }

    #[tokio::test]
    async fn signup_disambiguates_colliding_slugs() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let first = send_request(
            pool.clone(),
            signup_request(
                "Collide Inc",
                "Alice",
                &unique_test_email(),
                "correct horse battery",
            ),
        )
        .await;
        assert_eq!(first.status(), 201);

        let mut second = send_request(
            pool.clone(),
            signup_request(
                "Collide Inc",
                "Bob",
                &unique_test_email(),
                "correct horse battery",
            ),
        )
        .await;
        assert_eq!(second.status(), 201);
        let second_body = body_json(&mut second).await;
        let second_tenant_id: uuid::Uuid =
            uuid::Uuid::parse_str(second_body["memberships"][0]["tenantId"].as_str().unwrap())
                .unwrap();
        let slug: String = sqlx::query_scalar("SELECT slug FROM tenants WHERE id = $1")
            .bind(second_tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_ne!(slug, "collide-inc");
        assert!(slug.starts_with("collide-inc-"));
    }

    #[tokio::test]
    async fn signup_rejects_duplicate_email_with_409() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let email = unique_test_email();
        seed_user_with_password(&pool, &email, None, "Passw0rd!").await;

        let mut res = send_request(
            pool.clone(),
            signup_request("Another Co", "Someone", &email, "correct horse battery"),
        )
        .await;

        assert_eq!(res.status(), 409);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "conflict");
    }

    #[tokio::test]
    async fn signup_validation_errors_return_422_with_field_details() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let mut res = send_request(
            pool.clone(),
            signup_request("", "", "not-an-email", "short"),
        )
        .await;

        assert_eq!(res.status(), 422);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let fields: Vec<String> = body["error"]["details"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["field"].as_str().unwrap().to_string())
            .collect();
        assert!(fields.contains(&"workspaceName".to_string()));
        assert!(fields.contains(&"displayName".to_string()));
        assert!(fields.contains(&"email".to_string()));
        assert!(fields.contains(&"password".to_string()));
    }

    #[tokio::test]
    async fn signup_malformed_json_returns_400() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/auth/signup")
                .method("POST")
                .header("origin", TEST_ORIGIN)
                .header("x-requested-with", "XMLHttpRequest")
                .header("content-type", "application/json")
                .body(Body::from("not-json"))
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 400);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
    }

    /// Hash `password` with Argon2id at explicit (weaker) parameters, like an older deployment did.
    fn hash_with_params(m: u32, t: u32, p: u32, password: &str) -> String {
        let params = argon2::Params::new(m, t, p, None).unwrap();
        Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
            .hash_password(password.as_bytes())
            .unwrap()
            .to_string()
    }

    async fn stored_hash(pool: &sqlx::PgPool, user_id: uuid::Uuid) -> String {
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn login_rehashes_a_password_hashed_with_older_parameters() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let email = unique_test_email();
        let user_id = seed_user_with_password(&pool, &email, None, "Passw0rd!").await;
        let old_hash = hash_with_params(8_192, 1, 1, "Passw0rd!");
        sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
            .bind(&old_hash)
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(identity::password::needs_rehash(&old_hash));

        let res = send_request(pool.clone(), login_request(&email, "Passw0rd!")).await;
        assert_eq!(res.status(), 200);

        let new_hash = stored_hash(&pool, user_id).await;
        assert_ne!(new_hash, old_hash, "the old hash was kept");
        assert!(
            !identity::password::needs_rehash(&new_hash),
            "the stored hash is not at the current parameters: {new_hash}"
        );
        assert!(identity::password::verify_password("Passw0rd!", &new_hash).unwrap());

        // The next login still works, and a current hash is left alone.
        let res = send_request(pool.clone(), login_request(&email, "Passw0rd!")).await;
        assert_eq!(res.status(), 200);
        assert_eq!(stored_hash(&pool, user_id).await, new_hash);
    }

    #[tokio::test]
    async fn failed_login_does_not_rehash_or_change_the_stored_hash() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let email = unique_test_email();
        let user_id = seed_user_with_password(&pool, &email, None, "Passw0rd!").await;
        let old_hash = hash_with_params(8_192, 1, 1, "Passw0rd!");
        sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
            .bind(&old_hash)
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();

        let res = send_request(pool.clone(), login_request(&email, "wrong-password")).await;
        assert_eq!(res.status(), 401);
        assert_eq!(stored_hash(&pool, user_id).await, old_hash);
    }

    /// Before the fix an unknown email took about three times as long as a known one (the dummy
    /// hash used `m=65536` against `m=19456`). Medians of several attempts keep this stable; the
    /// bounds are far inside the old ratio of about 3.4.
    #[tokio::test]
    async fn known_and_unknown_email_logins_take_comparable_time() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let known_email = unique_test_email();
        seed_user_with_password(&pool, &known_email, None, "Passw0rd!").await;

        // Warm the connection pool and the code paths so the first request isn't an outlier.
        send_request(pool.clone(), login_request(&known_email, "wrong-password")).await;
        send_request(
            pool.clone(),
            login_request(&unique_test_email(), "wrong-password"),
        )
        .await;

        let mut known = Vec::new();
        let mut unknown = Vec::new();
        async fn timed_failed_login(pool: &sqlx::PgPool, email: &str) -> std::time::Duration {
            let started = std::time::Instant::now();
            let res = send_request(pool.clone(), login_request(email, "wrong-password")).await;
            assert_eq!(res.status(), 401);
            started.elapsed()
        }

        // Alternate which side goes first so cache and clock effects can't favour one of them.
        for i in 0..7 {
            let unknown_email = unique_test_email();
            if i % 2 == 0 {
                known.push(timed_failed_login(&pool, &known_email).await);
                unknown.push(timed_failed_login(&pool, &unknown_email).await);
            } else {
                unknown.push(timed_failed_login(&pool, &unknown_email).await);
                known.push(timed_failed_login(&pool, &known_email).await);
            }
        }
        known.sort();
        unknown.sort();
        let (known, unknown) = (known[3].as_secs_f64(), unknown[3].as_secs_f64());
        let ratio = unknown / known;
        assert!(
            (0.6..=1.6).contains(&ratio),
            "unknown/known login time ratio {ratio:.2} (known {known:.3}s, unknown {unknown:.3}s)"
        );
    }
}
