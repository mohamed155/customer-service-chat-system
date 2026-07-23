use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use config::Environment;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use server::router;
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_ENV: Environment = Environment::Test;

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
        integration_secrets_key: Some("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=".into()),
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
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
    }
}

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("skipping integrations_rbac live tests: DATABASE_URL not set");
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping integrations_rbac live tests: DATABASE_URL is unreachable");
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        return None;
    }
    Some(pool)
}

async fn send(pool: sqlx::PgPool, request: Request<Body>) -> Response {
    router::app_with_test_routes(app_state(pool))
        .oneshot(request)
        .await
        .expect("request should complete")
}

async fn body_json(response: Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn authenticated_request(
    uri: &str,
    method: Method,
    user_id: Uuid,
    tenant_id: Uuid,
) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(method)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .body(Body::empty())
        .unwrap()
}

fn authenticated_json_request(
    uri: &str,
    method: Method,
    user_id: Uuid,
    tenant_id: Uuid,
    body: Value,
) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(method)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name, platform_role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(format!("irbac_{}@example.com", Uuid::new_v4()))
    .bind("Integrations RBAC User")
    .bind(None::<String>)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Integrations RBAC Tenant")
        .bind(format!("irbac-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_membership(pool: &sqlx::PgPool, tenant_id: Uuid, user_id: Uuid, role: &str) {
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(tenant_id)
        .bind(user_id)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn admin_gets_200_on_list() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let user = seed_user(&pool).await;
    seed_membership(&pool, tenant, user, "admin").await;

    let response = send(
        pool.clone(),
        authenticated_request("/api/v1/tenant/integrations", Method::GET, user, tenant),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    assert!(json["data"].is_array());
}

#[tokio::test]
async fn manager_gets_200_on_list_and_detail() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let user = seed_user(&pool).await;
    seed_membership(&pool, tenant, user, "manager").await;

    let list = send(
        pool.clone(),
        authenticated_request("/api/v1/tenant/integrations", Method::GET, user, tenant),
    )
    .await;
    assert_eq!(list.status(), StatusCode::OK);
    let list_json = body_json(list).await;
    assert!(list_json["data"].is_array());

    let detail = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations/generic-webhook",
            Method::GET,
            user,
            tenant,
        ),
    )
    .await;
    assert_eq!(detail.status(), StatusCode::OK);
    let detail_json = body_json(detail).await;
    assert_eq!(detail_json["slug"], "generic-webhook");
}

#[tokio::test]
async fn viewer_gets_200_on_list_and_detail() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let user = seed_user(&pool).await;
    seed_membership(&pool, tenant, user, "viewer").await;

    let list = send(
        pool.clone(),
        authenticated_request("/api/v1/tenant/integrations", Method::GET, user, tenant),
    )
    .await;
    assert_eq!(list.status(), StatusCode::OK);
    let list_json = body_json(list).await;
    assert!(list_json["data"].is_array());

    let detail = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations/slack",
            Method::GET,
            user,
            tenant,
        ),
    )
    .await;
    assert_eq!(detail.status(), StatusCode::OK);
    let detail_json = body_json(detail).await;
    assert_eq!(detail_json["slug"], "slack");
}

#[tokio::test]
async fn agent_gets_403_on_list_and_detail() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let user = seed_user(&pool).await;
    seed_membership(&pool, tenant, user, "agent").await;

    let list = send(
        pool.clone(),
        authenticated_request("/api/v1/tenant/integrations", Method::GET, user, tenant),
    )
    .await;
    assert_eq!(list.status(), StatusCode::FORBIDDEN);

    let detail = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations/generic-webhook",
            Method::GET,
            user,
            tenant,
        ),
    )
    .await;
    assert_eq!(detail.status(), StatusCode::FORBIDDEN);
}

// ---------------------------------------------------------------------------
// T039 — RBAC matrix for mutating endpoints:
//   Viewer  → 403 on POST /connect, PUT /config, POST /disconnect
//   Manager → 2xx on all three
// ---------------------------------------------------------------------------

#[tokio::test]
async fn viewer_gets_403_on_connect_update_disconnect() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let viewer = seed_user(&pool).await;
    seed_membership(&pool, tenant, viewer, "viewer").await;

    // Connect — 403.
    let connect = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/connect",
            Method::POST,
            viewer,
            tenant,
            json!({
                "config": { "source_label": "X" },
                "secrets": { "signing_secret": "whsec_viewer_1111" },
            }),
        ),
    )
    .await;
    assert_eq!(connect.status(), StatusCode::FORBIDDEN);

    // Update config — 403.
    let update = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/config",
            Method::PUT,
            viewer,
            tenant,
            json!({
                "config": { "source_label": "Y" },
            }),
        ),
    )
    .await;
    assert_eq!(update.status(), StatusCode::FORBIDDEN);

    // Disconnect — 403.
    let disconnect = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations/generic-webhook/disconnect",
            Method::POST,
            viewer,
            tenant,
        ),
    )
    .await;
    assert_eq!(disconnect.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn manager_gets_2xx_on_connect_update_disconnect() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let manager = seed_user(&pool).await;
    seed_membership(&pool, tenant, manager, "manager").await;

    // (1) Connect — 201.
    let connect = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/connect",
            Method::POST,
            manager,
            tenant,
            json!({
                "config": { "source_label": "Billing" },
                "secrets": { "signing_secret": "whsec_mgr_1111" },
            }),
        ),
    )
    .await;
    assert_eq!(connect.status(), StatusCode::CREATED);

    // (2) Update config — 200.
    let update = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/config",
            Method::PUT,
            manager,
            tenant,
            json!({
                "config": { "source_label": "Billing 2" },
            }),
        ),
    )
    .await;
    assert_eq!(update.status(), StatusCode::OK);

    // (3) Disconnect — 200.
    let disconnect = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations/generic-webhook/disconnect",
            Method::POST,
            manager,
            tenant,
        ),
    )
    .await;
    assert_eq!(disconnect.status(), StatusCode::OK);
}
