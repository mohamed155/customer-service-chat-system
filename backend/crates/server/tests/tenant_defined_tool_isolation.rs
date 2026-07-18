use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use serde_json::json;
use server::router;
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_ENV: config::Environment = config::Environment::Test;

fn test_config() -> config::AppConfig {
    config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: "redis://127.0.0.1:6379".into(),
        auth_jwt_secret: "test-auth-jwt-secret-at-least-32-bytes".into(),
        auth_session_ttl_seconds: 28_800,
        port: 0,
        bind_address: "0.0.0.0".into(),
        environment: TEST_ENV,
        cors_allowed_origins: vec![],
        log_format: config::LogFormat::Pretty,
        smtp_url: None,
        smtp_from: None,
        public_dashboard_url: "http://localhost:4200".into(),
        db_max_connections: 2,
        db_acquire_timeout_ms: 5000,
        ready_probe_timeout_ms: 5000,
        shutdown_grace_seconds: 1,
        docs_enabled: false,
        ai_key_encryption_key: Some("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=".into()),
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
    }
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("skipping: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping: could not connect to DATABASE_URL");
        return None;
    }
    Some(pool)
}

fn test_app_state(pool: sqlx::PgPool) -> AppState {
    AppState {
        config: Arc::new(test_config()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool, &test_config()).unwrap(),
    }
}

async fn send_request(pool: sqlx::PgPool, req: Request<Body>) -> axum::response::Response {
    let state = test_app_state(pool);
    let app = router::app(state);
    app.oneshot(req).await.expect("request should succeed")
}

async fn seed_user(pool: &sqlx::PgPool, email: &str, platform_role: Option<&str>) -> Uuid {
    match platform_role {
        Some(role) => sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO users (email, display_name, platform_role) \
                 VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(email)
        .bind("Seed User")
        .bind(role)
        .fetch_one(pool)
        .await
        .expect("seed user with platform_role"),
        None => sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id",
        )
        .bind(email)
        .bind("Seed User")
        .fetch_one(pool)
        .await
        .expect("seed user"),
    }
}

async fn seed_tenant(pool: &sqlx::PgPool, label: &str) -> Uuid {
    let slug = format!(
        "tenant-{}-{}",
        label,
        Uuid::new_v4().to_string().split('-').next().unwrap()
    );
    sqlx::query_scalar::<_, Uuid>("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(format!("Tenant {label}"))
        .bind(&slug)
        .fetch_one(pool)
        .await
        .expect("seed tenant")
}

async fn seed_membership(pool: &sqlx::PgPool, tenant_id: Uuid, user_id: Uuid, role: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .fetch_one(pool)
    .await
    .expect("seed membership")
}

async fn seed_tenant_tool(pool: &sqlx::PgPool, tenant_id: Uuid, name: &str, membership_id: Uuid) {
    sqlx::query(
        "INSERT INTO tenant_tools \
         (tenant_id, name, description, input_schema, endpoint_url, classification, created_by_membership_id) \
         VALUES ($1, $2, $3, $4::jsonb, $5, $6, $7)",
    )
    .bind(tenant_id)
    .bind(name)
    .bind("Test tool")
    .bind(json!({"type": "object", "properties": {}}))
    .bind("https://api.example.com/tool")
    .bind("auto")
    .bind(membership_id)
    .execute(pool)
    .await
    .expect("seed tenant tool");
}

async fn body_bytes(res: &mut axum::response::Response) -> Vec<u8> {
    use http_body_util::BodyExt;
    BodyExt::collect(res.body_mut())
        .await
        .unwrap()
        .to_bytes()
        .to_vec()
}

async fn body_json(res: &mut axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&body_bytes(res).await).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn tenant_a_tool_not_visible_in_tenant_b() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        // Create two tenants each with a user
        let tenant_a = seed_tenant(&pool, "A").await;
        let tenant_b = seed_tenant(&pool, "B").await;

        let user_a = seed_user(&pool, "user_a@test.com", None).await;
        let user_b = seed_user(&pool, "user_b@test.com", None).await;

        let membership_a = seed_membership(&pool, tenant_a, user_a, "admin").await;
        let membership_b = seed_membership(&pool, tenant_b, user_b, "admin").await;

        // Seed a tool for tenant A only
        seed_tenant_tool(&pool, tenant_a, "tenant_a_special_tool", membership_a).await;

        // Tenant A should see the tool
        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant/tools")
                .header("X-Dev-User-Id", user_a.to_string())
                .header("X-Tenant-ID", tenant_a.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(res.status(), 200);
        let body_a = body_json(&mut res).await;
        let items_a = body_a["items"].as_array().unwrap();
        assert!(
            items_a.iter().any(|i| i["name"] == "tenant_a_special_tool"),
            "tenant A should see its tool"
        );

        // Tenant B should NOT see the tool
        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant/tools")
                .header("X-Dev-User-Id", user_b.to_string())
                .header("X-Tenant-ID", tenant_b.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(res.status(), 200);
        let body_b = body_json(&mut res).await;
        let items_b = body_b["items"].as_array().unwrap();
        assert!(
            !items_b.iter().any(|i| i["name"] == "tenant_a_special_tool"),
            "tenant B should NOT see tenant A's tool"
        );

        // Both tenants should see built-in tools
        assert!(
            items_a.iter().any(|i| i["source"] == "builtin"),
            "tenant A should see built-in tools"
        );
        assert!(
            items_b.iter().any(|i| i["source"] == "builtin"),
            "tenant B should see built-in tools"
        );
    }
}
