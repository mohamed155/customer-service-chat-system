use std::sync::Arc;
use std::time::Duration;

use argon2::password_hash::{rand_core::OsRng, SaltString};
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
        cors_allowed_origins: vec![],
        log_format: config::LogFormat::Pretty,
        db_max_connections: 2,
        db_acquire_timeout_ms: 5000,
        ready_probe_timeout_ms: 5000,
        shutdown_grace_seconds: 1,
    }
}

/// Build an `AppState` from a live pool and a test config.
pub fn test_app_state(pool: sqlx::PgPool) -> AppState {
    AppState {
        config: Arc::new(test_config()),
        db: pool,
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
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
        config: Arc::new(config),
        db: pool,
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
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
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
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
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "email": email, "password": password }).to_string(),
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
        db::run_migrations(&pool).await.unwrap();

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
                "platformPermissions": [
                    "platform.tenants.list",
                    "platform.tenants.switch",
                    "platform.tenants.manage",
                    "platform.admin",
                    "platform.billing.view",
                    "platform.diagnostics.view"
                ],
                "staffTenantPermissions": [
                    "overview.view",
                    "conversations.view",
                    "conversations.manage",
                    "customers.view",
                    "customers.manage",
                    "ai_agent.view",
                    "ai_agent.manage",
                    "knowledge_base.view",
                    "knowledge_base.manage",
                    "integrations.view",
                    "integrations.manage",
                    "analytics.view",
                    "members.view",
                    "members.manage",
                    "settings.view",
                    "settings.manage",
                    "billing.view",
                    "billing.manage",
                    "tenant.delete",
                    "owner.assign"
                ],
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
        db::run_migrations(&pool).await.unwrap();
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
        .execute(&pool)
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
            json!([
                "overview.view",
                "conversations.view",
                "customers.view",
                "ai_agent.view",
                "knowledge_base.view",
                "integrations.view",
                "analytics.view"
            ])
        );
    }

    #[tokio::test]
    async fn invalid_login_paths_return_byte_identical_401_bodies() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

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
        db::run_migrations(&pool).await.unwrap();

        let requests = [
            Request::builder()
                .uri("/api/v1/auth/login")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "email": "", "password": "x" }).to_string(),
                ))
                .unwrap(),
            Request::builder()
                .uri("/api/v1/auth/login")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "email": "a@test.com", "password": "" }).to_string(),
                ))
                .unwrap(),
            Request::builder()
                .uri("/api/v1/auth/login")
                .method("POST")
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
        db::run_migrations(&pool).await.unwrap();

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
        db::run_migrations(&pool).await.unwrap();

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
        .fetch_one(&pool)
        .await
        .unwrap();
        let failure_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE action = 'auth.login_failed' AND actor_user_id IS NULL AND details->>'email' = $1",
        )
        .bind(&email)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(success_count, 1);
        assert_eq!(failure_count, 1);
    }
    #[tokio::test]
    async fn invalid_session_cookies_return_401() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

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
        db::run_migrations(&pool).await.unwrap();

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
        db::run_migrations(&pool).await.unwrap();

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

    #[tokio::test]
    async fn csrf_origin_policy_blocks_foreign_state_changing_requests_only() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let mut config = test_config();
        config.cors_allowed_origins = vec!["https://dashboard.example".into()];

        let email = unique_test_email();
        let user_id = seed_user_with_password(&pool, &email, None, "Passw0rd!").await;
        let cookie = session_cookie_for(&config, user_id, 28_800);

        let mut blocked = send_request_with_config(
            pool.clone(),
            config.clone(),
            Request::builder()
                .uri("/api/v1/auth/login")
                .method(Method::POST)
                .header("origin", "https://evil.example")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "email": email, "password": "Passw0rd!" }).to_string(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(blocked.status(), 403);
        let body = body_json(&mut blocked).await;
        assert_eq!(body["error"]["code"], "unauthorized");

        let allowed = send_request_with_config(
            pool.clone(),
            config.clone(),
            Request::builder()
                .uri("/api/v1/auth/login")
                .method(Method::POST)
                .header("origin", "https://dashboard.example")
                .header("cookie", &cookie)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "email": email, "password": "Passw0rd!" }).to_string(),
                ))
                .unwrap(),
        )
        .await;
        assert_ne!(allowed.status(), 403);

        let get_res = send_request_with_config(
            pool.clone(),
            config,
            Request::builder()
                .uri("/api/v1/me")
                .method(Method::GET)
                .header("origin", "https://evil.example")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(get_res.status(), 200);
    }
    #[tokio::test]
    async fn logout_revokes_cookie_session_and_clears_cookie() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

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
        .fetch_one(&pool)
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
        db::run_migrations(&pool).await.unwrap();

        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/auth/logout")
                .method(Method::POST)
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
        db::run_migrations(&pool).await.unwrap();

        let email = unique_test_email();
        let user_id = seed_user_with_password(&pool, &email, None, "Passw0rd!").await;

        let logout = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/auth/logout")
                .method(Method::POST)
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
}
