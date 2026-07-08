use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use axum::response::Response;
use server::router;
use server::state::AppState;
use tower::ServiceExt;

/// Live-gated pool: returns `None` (with `eprintln!`) when `DATABASE_URL` is
/// unreachable, so the test is silently skipped in CI without a database.
pub async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("skipping tenancy test: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping tenancy test: could not connect to DATABASE_URL");
        return None;
    }
    Some(pool)
}

/// Build an `AppConfig` with test-friendly defaults.
pub fn test_config() -> config::AppConfig {
    config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: "redis://127.0.0.1:6379".into(),
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

/// Insert a unique user and return its id.
///
/// If `platform_role` is `Some`, the user is created with that role;
/// otherwise the column is omitted (uses the DB default).
pub async fn seed_user(pool: &sqlx::PgPool, platform_role: Option<&str>) -> uuid::Uuid {
    let email = format!("test_{}@example.com", uuid::Uuid::new_v4());
    match platform_role {
        Some(role) => {
            sqlx::query_scalar::<_, uuid::Uuid>(
                "INSERT INTO users (email, display_name, platform_role) VALUES ($1, $2, $3) RETURNING id",
            )
            .bind(&email)
            .bind("Seed User")
            .bind(role)
            .fetch_one(pool)
            .await
            .expect("seed user with platform_role")
        }
        None => {
            sqlx::query_scalar::<_, uuid::Uuid>(
                "INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id",
            )
            .bind(&email)
            .bind("Seed User")
            .fetch_one(pool)
            .await
            .expect("seed user")
        }
    }
}

/// Insert a unique tenant and return its id.
///
/// If `status` is `Some`, the tenant is created with that status;
/// otherwise defaults to `'active'`.
pub async fn seed_tenant(pool: &sqlx::PgPool, status: Option<&str>) -> uuid::Uuid {
    let slug = format!(
        "tenant-{}",
        uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
    );
    let status = status.unwrap_or("active");
    sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO tenants (name, slug, status) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind("Seed Tenant")
    .bind(&slug)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("seed tenant")
}

/// Insert a tenant membership and return its id.
pub async fn seed_membership(
    pool: &sqlx::PgPool,
    tenant_id: uuid::Uuid,
    user_id: uuid::Uuid,
    role: &str,
) -> uuid::Uuid {
    sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .fetch_one(pool)
    .await
    .expect("seed membership")
}
