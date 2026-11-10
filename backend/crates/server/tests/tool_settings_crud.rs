use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
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
        integration_secrets_key: None,
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
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
        trusted_proxy: false,
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
        budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
    }
}

async fn send_request(pool: sqlx::PgPool, req: Request<Body>) -> axum::response::Response {
    let state = test_app_state(pool);
    let app = router::app(state);
    app.oneshot(req).await.expect("request should succeed")
}

async fn seed_user(pool: &sqlx::PgPool, platform_role: Option<&str>) -> Uuid {
    let email = format!("test_{}@example.com", Uuid::new_v4());
    match platform_role {
        Some(role) => sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO users (email, display_name, platform_role) \
                 VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(&email)
        .bind("Seed User")
        .bind(role)
        .fetch_one(pool)
        .await
        .expect("seed user with platform_role"),
        None => sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id",
        )
        .bind(&email)
        .bind("Seed User")
        .fetch_one(pool)
        .await
        .expect("seed user"),
    }
}

async fn seed_tenant(pool: &sqlx::PgPool, status: Option<&str>) -> Uuid {
    let slug = format!(
        "tenant-{}",
        Uuid::new_v4().to_string().split('-').next().unwrap()
    );
    let status = status.unwrap_or("active");
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO tenants (name, slug, status) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind("Seed Tenant")
    .bind(&slug)
    .bind(status)
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
    .fetch_one(&db::testing::seed_pool())
    .await
    .expect("seed membership")
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
    async fn tenant_tool_full_crud_lifecycle() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let user_id = seed_user(&pool, None).await;
        let tenant_id = seed_tenant(&pool, None).await;
        // Tool settings are Owner/Admin in the shipped RBAC (route needs
        // ai_agent.view; dashboard needs settings.view).
        let _membership_id = seed_membership(&pool, tenant_id, user_id, "admin").await;

        // Step 1: GET /tenant/tools returns built-in tools
        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant/tools")
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(res.status(), 200);
        let body = body_json(&mut res).await;
        let initial_items = body["items"].as_array().unwrap();
        assert!(!initial_items.is_empty(), "should have built-in tools");

        // Step 2: POST /tenant/tools — create tenant-defined tool
        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant/tools")
                .method("POST")
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "custom_order_lookup",
                        "description": "Look up order status",
                        "inputSchema": {"type": "object", "properties": {"orderId": {"type": "string"}}},
                        "endpointUrl": "https://93.184.215.14/orders",
                        "credential": "sk-test-key-12345",
                        "classification": "auto",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;
        assert_eq!(
            res.status(),
            200,
            "create should succeed: {:?}",
            body_json(&mut res).await
        );
        let create_body = body_json(&mut res).await;
        let tool_id: Uuid = create_body["id"].as_str().unwrap().parse().unwrap();

        // Step 3: GET /tenant/tools includes the new tool with hasCredential:null and no credential value
        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant/tools")
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(res.status(), 200);
        let body = body_json(&mut res).await;
        let items = body["items"].as_array().unwrap();
        let created = items.iter().find(|i| i["name"] == "custom_order_lookup");
        assert!(created.is_some(), "new tool should appear in list");
        let created = created.unwrap();
        assert!(
            created["hasCredential"].is_null(),
            "credential should not be exposed"
        );
        // The credential value must NOT appear anywhere in the response
        let body_str = serde_json::to_string(&body).unwrap();
        assert!(
            !body_str.contains("sk-test-key-12345"),
            "credential must not appear in GET response"
        );

        // Step 4: PUT /tenant/tools/{id} — update
        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri(&format!("/api/v1/tenant/tools/{tool_id}"))
                .method("PUT")
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "description": "Updated description",
                        "enabled": false,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;
        assert_eq!(res.status(), 200, "update should succeed");

        // Step 5: DELETE /tenant/tools/{id} — soft delete
        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri(&format!("/api/v1/tenant/tools/{tool_id}"))
                .method("DELETE")
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(res.status(), 200, "delete should succeed");

        // Step 6: Verify it's gone from list
        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant/tools")
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(res.status(), 200);
        let body = body_json(&mut res).await;
        let items = body["items"].as_array().unwrap();
        assert!(
            !items.iter().any(|i| i["name"] == "custom_order_lookup"),
            "deleted tool should not appear in list"
        );
    }

    #[tokio::test]
    async fn manager_gets_403_on_mutations() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let user_id = seed_user(&pool, None).await;
        let tenant_id = seed_tenant(&pool, None).await;
        // "manager" role does NOT have AiAgentManage
        seed_membership(&pool, tenant_id, user_id, "manager").await;

        // POST should be 403
        let mut res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant/tools")
                .method("POST")
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "test_tool",
                        "description": "test",
                        "inputSchema": {"type": "object", "properties": {}},
                        "endpointUrl": "https://93.184.215.14/test",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;
        assert_eq!(res.status(), 403, "manager should get 403 on create");
    }

    async fn get_tools_as(pool: &sqlx::PgPool, role: &str) -> axum::response::Response {
        let user_id = seed_user(pool, None).await;
        let tenant_id = seed_tenant(pool, None).await;
        seed_membership(pool, tenant_id, user_id, role).await;
        send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant/tools")
                .method("GET")
                .header("X-Dev-User-Id", user_id.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    /// Spec 022 tool-settings-api: read = Manager+.
    #[tokio::test]
    async fn manager_can_read_tool_settings() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        let mut res = get_tools_as(&pool, "manager").await;
        assert_eq!(res.status(), 200, "manager should read tool settings");
        let json = body_json(&mut res).await;
        assert!(
            json["items"].is_array(),
            "tool list should carry items: {json}"
        );
    }

    #[tokio::test]
    async fn agent_and_viewer_cannot_read_tool_settings() {
        let Some(pool) = get_pool().await else { return };
        db::testing::migrate().await;

        for role in ["agent", "viewer"] {
            let res = get_tools_as(&pool, role).await;
            assert_eq!(res.status(), 403, "{role} must not read tool settings");
        }
    }
}
