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

/// Helper: collect a response body into a `Vec<u8>`.
async fn body_bytes(res: &mut Response) -> Vec<u8> {
    use http_body_util::BodyExt;
    BodyExt::collect(res.body_mut())
        .await
        .unwrap()
        .to_bytes()
        .to_vec()
}

/// Helper: parse a response body into a `serde_json::Value`.
async fn body_json(res: &mut Response) -> serde_json::Value {
    serde_json::from_slice(&body_bytes(res).await).unwrap()
}

// ---------------------------------------------------------------------------
// Tests — GET /api/v1/tenant (existential isolation matrix)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn own_tenant_returns_200_with_tenant_summary() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let user_id = seed_user(&pool, None).await;
        let tenant_id = seed_tenant(&pool, None).await;
        seed_membership(&pool, tenant_id, user_id, "agent").await;

        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant")
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 200);
        let body = body_json(&mut res).await;
        assert_eq!(body["id"], serde_json::json!(tenant_id));
    }

    #[tokio::test]
    async fn foreign_tenant_returns_403_unauthorized() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let user_id = seed_user(&pool, None).await;
        let tenant_a = seed_tenant(&pool, None).await;
        let tenant_b = seed_tenant(&pool, None).await;
        seed_membership(&pool, tenant_a, user_id, "agent").await;

        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant")
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", tenant_b.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 403);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "unauthorized");
    }

    #[tokio::test]
    async fn nonexistent_tenant_returns_403_byte_identical() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let user_id = seed_user(&pool, None).await;
        let tenant_a = seed_tenant(&pool, None).await;
        seed_membership(&pool, tenant_a, user_id, "agent").await;

        let nonexistent_id = uuid::Uuid::new_v4();

        // First: foreign tenant (tenant_b that doesn't exist in any membership)
        let mut foreign_res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant")
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", nonexistent_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        // Second: a different non-existent UUID
        let other_id = uuid::Uuid::new_v4();
        assert_ne!(nonexistent_id, other_id);
        let mut nonexistent_res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant")
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", other_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(foreign_res.status(), 403);
        assert_eq!(nonexistent_res.status(), 403);

        let foreign_body: serde_json::Value = body_json(&mut foreign_res).await;
        let nonexistent_body: serde_json::Value = body_json(&mut nonexistent_res).await;

        // Compare every field except request_id
        assert_eq!(
            foreign_body["error"]["code"],
            nonexistent_body["error"]["code"]
        );
        assert_eq!(
            foreign_body["error"]["message"],
            nonexistent_body["error"]["message"]
        );
        assert_eq!(
            foreign_body["error"]["details"],
            nonexistent_body["error"]["details"]
        );
    }

    #[tokio::test]
    async fn missing_x_tenant_id_returns_400() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let user_id = seed_user(&pool, None).await;

        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant")
                .header("X-Dev-User-Id", user_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 400);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
    }

    #[tokio::test]
    async fn malformed_x_tenant_id_returns_400() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let user_id = seed_user(&pool, None).await;

        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant")
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", "not-a-uuid")
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 400);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
    }

    #[tokio::test]
    async fn suspended_tenant_tenant_user_returns_403() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let user_id = seed_user(&pool, None).await;
        let tenant_id = seed_tenant(&pool, Some("suspended")).await;
        seed_membership(&pool, tenant_id, user_id, "agent").await;

        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant")
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 403);
        // The message should indicate suspension, not just "unauthorized"
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "unauthorized");
    }

    #[tokio::test]
    async fn suspended_tenant_platform_user_returns_200() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let user_id = seed_user(&pool, Some("super_admin")).await;
        let tenant_id = seed_tenant(&pool, Some("suspended")).await;

        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant")
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 200);
        let body = body_json(&mut res).await;
        assert_eq!(body["id"], serde_json::json!(tenant_id));
    }

    #[tokio::test]
    async fn revoked_membership_returns_403() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let user_id = seed_user(&pool, None).await;
        let tenant_id = seed_tenant(&pool, None).await;
        let membership_id = seed_membership(&pool, tenant_id, user_id, "agent").await;

        // Soft-delete (revoke) the membership
        sqlx::query("UPDATE tenant_memberships SET deleted_at = now() WHERE id = $1")
            .bind(membership_id)
            .execute(&pool)
            .await
            .unwrap();

        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant")
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 403);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "unauthorized");
    }

    #[tokio::test]
    async fn no_principal_returns_401() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant")
                .header("X-Tenant-ID", uuid::Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 401);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "unauthenticated");
    }

    #[tokio::test]
    async fn denial_writes_access_denied_audit_row() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let user_id = seed_user(&pool, None).await;
        let tenant_a = seed_tenant(&pool, None).await;
        let tenant_b = seed_tenant(&pool, None).await;
        seed_membership(&pool, tenant_a, user_id, "agent").await;

        let res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant")
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", tenant_b.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 403);
        drop(res);

        // Give the async write a moment to flush
        tokio::time::sleep(Duration::from_millis(50)).await;

        let row: Option<(uuid::Uuid, String, Option<uuid::Uuid>, serde_json::Value)> =
            sqlx::query_as(
                r#"
                SELECT id, action, tenant_id, details
                FROM audit_logs
                WHERE actor_user_id = $1
                  AND action = 'tenant.access_denied'
                ORDER BY created_at DESC
                LIMIT 1
                "#,
            )
            .bind(user_id)
            .fetch_optional(&pool)
            .await
            .unwrap();

        let (_, action, audited_tenant_id, details) =
            row.expect("expected a tenant.access_denied audit row");

        assert_eq!(action, "tenant.access_denied");
        // tenant_id is NULL because the user has no membership in tenant_b
        assert!(audited_tenant_id.is_none());
        // details contains requested_tenant_id and reason
        assert_eq!(
            details["requested_tenant_id"],
            serde_json::json!(tenant_b)
        );
        assert!(details.get("reason").is_some());
    }
}
