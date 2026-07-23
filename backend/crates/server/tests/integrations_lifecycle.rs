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

// Same base64-encoded 32-byte key used by `integrations::crypto` unit tests
// (and accepted by `AppConfig::from_env` validation).
const TEST_INTEGRATION_SECRETS_KEY: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=";

fn test_config() -> config::AppConfig {
    config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: "redis://127.0.0.1:6379".into(),
        auth_jwt_secret: "test-auth-jwt-secret-at-least-32-bytes".into(),
        auth_session_ttl_seconds: 28_800,
        port: 0,
        bind_address: "0.0.0.0".into(),
        environment: Environment::Test,
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
        integration_secrets_key: Some(TEST_INTEGRATION_SECRETS_KEY.into()),
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
            eprintln!("skipping integrations_lifecycle live tests: DATABASE_URL not set");
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping integrations_lifecycle live tests: DATABASE_URL is unreachable");
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

#[allow(dead_code)]
async fn body_bytes(_response: Response) -> Vec<u8> {
    Vec::new()
}

async fn seed_user(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name, platform_role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(format!("ilife_{}@example.com", Uuid::new_v4()))
    .bind("Integrations Lifecycle User")
    .bind(None::<String>)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Integrations Lifecycle Tenant")
        .bind(format!("ilife-{}", Uuid::new_v4().simple()))
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

async fn count_connection_rows(pool: &sqlx::PgPool, tenant_id: Uuid, catalog_id: Uuid) -> i64 {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM integration_connections WHERE tenant_id = $1 AND catalog_id = $2",
    )
    .bind(tenant_id)
    .bind(catalog_id)
    .fetch_one(pool)
    .await
    .unwrap();
    row.0
}

async fn count_events_for_connection(
    pool: &sqlx::PgPool,
    connection_id: Uuid,
    event_type: &str,
) -> i64 {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM integration_events WHERE connection_id = $1 AND event_type = $2",
    )
    .bind(connection_id)
    .bind(event_type)
    .fetch_one(pool)
    .await
    .unwrap();
    row.0
}

async fn count_audit_for_action(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    action: &str,
    resource_id: &str,
) -> i64 {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM audit_logs WHERE tenant_id = $1 AND action = $2 AND resource_id = $3",
    )
    .bind(tenant_id)
    .bind(action)
    .bind(resource_id)
    .fetch_one(pool)
    .await
    .unwrap();
    row.0
}

// ---------------------------------------------------------------------------
// T037 — Lifecycle: connect / second connect / unavailable / missing field /
// config update / disconnect / reconnect / SC-004 audit+event coverage
// ---------------------------------------------------------------------------

#[tokio::test]
async fn connect_returns_201_and_status_connected() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    let response = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/connect",
            Method::POST,
            admin,
            tenant,
            json!({
                "config": { "source_label": "Billing system" },
                "secrets": { "signing_secret": "whsec_test_abc" },
            }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = body_json(response).await;
    assert_eq!(body["status"], "connected");
    assert_eq!(body["slug"], "generic-webhook");
    assert!(body["connection"].is_object());
    assert_eq!(
        body["connection"]["config"]["source_label"], "Billing system"
    );
    let secrets = body["connection"]["secrets"].as_array().unwrap();
    assert_eq!(secrets.len(), 1);
    assert_eq!(secrets[0]["field_key"], "signing_secret");
    assert_eq!(secrets[0]["hint"], "abc"); // last 4 chars of "whsec_test_abc"
    assert!(body["connection"]["webhook_url"].is_string());
    assert!(body["connection"]["webhook_url"]
        .as_str()
        .unwrap()
        .contains("/hooks/v1/"));
}

#[tokio::test]
async fn second_connect_returns_409() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    let first = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/connect",
            Method::POST,
            admin,
            tenant,
            json!({
                "config": { "source_label": "A" },
                "secrets": { "signing_secret": "whsec_aaaa1111" },
            }),
        ),
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);

    let second = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/connect",
            Method::POST,
            admin,
            tenant,
            json!({
                "config": { "source_label": "B" },
                "secrets": { "signing_secret": "whsec_bbbb2222" },
            }),
        ),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CONFLICT);
    let body = body_json(second).await;
    assert_eq!(body["error"]["code"], "integration_already_connected");
}

#[tokio::test]
async fn connecting_unavailable_slack_returns_422() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    let response = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/slack/connect",
            Method::POST,
            admin,
            tenant,
            json!({
                "config": {},
                "secrets": {},
            }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "unprocessable_entity");
}

#[tokio::test]
async fn connect_with_missing_required_field_returns_422() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    // source_label is required text, signing_secret is required secret.
    // Empty bodies for both required fields should be 422.
    let response = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/connect",
            Method::POST,
            admin,
            tenant,
            json!({
                "config": {},
                "secrets": {},
            }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "unprocessable_entity");
    let details = body["error"]["details"].as_array().unwrap();
    let fields: Vec<&str> = details
        .iter()
        .map(|d| d["field"].as_str().unwrap_or(""))
        .collect();
    assert!(
        fields.contains(&"source_label"),
        "expected source_label validation error, got: {fields:?}"
    );
    assert!(
        fields.contains(&"signing_secret"),
        "expected signing_secret validation error, got: {fields:?}"
    );
}

#[tokio::test]
async fn update_config_returns_200_and_changes_value() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    // Connect.
    let connect = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/connect",
            Method::POST,
            admin,
            tenant,
            json!({
                "config": { "source_label": "Billing" },
                "secrets": { "signing_secret": "whsec_orig_1111" },
            }),
        ),
    )
    .await;
    assert_eq!(connect.status(), StatusCode::CREATED);
    let connect_body = body_json(connect).await;
    let webhook_url_before = connect_body["connection"]["webhook_url"]
        .as_str()
        .unwrap()
        .to_string();

    // Update config only (no secrets).
    let update = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/config",
            Method::PUT,
            admin,
            tenant,
            json!({
                "config": { "source_label": "Billing 2" },
            }),
        ),
    )
    .await;
    assert_eq!(update.status(), StatusCode::OK);
    let update_body = body_json(update).await;
    assert_eq!(
        update_body["connection"]["config"]["source_label"], "Billing 2"
    );
    // Webhook URL is not rotated on a config-only update.
    let webhook_url_after = update_body["connection"]["webhook_url"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(webhook_url_after, webhook_url_before);
    // Existing secret hint is preserved on config-only update.
    let secrets = update_body["connection"]["secrets"].as_array().unwrap();
    assert_eq!(secrets.len(), 1);
    assert_eq!(secrets[0]["hint"], "1111");
}

#[tokio::test]
async fn disconnect_returns_200_with_disconnected_status_and_empty_secrets() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    // Connect.
    let connect = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/connect",
            Method::POST,
            admin,
            tenant,
            json!({
                "config": { "source_label": "X" },
                "secrets": { "signing_secret": "whsec_xxxx1111" },
            }),
        ),
    )
    .await;
    assert_eq!(connect.status(), StatusCode::CREATED);

    // Disconnect.
    let response = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations/generic-webhook/disconnect",
            Method::POST,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["status"], "disconnected");
    assert!(body["connection"].is_object());
    assert_eq!(
        body["connection"]["secrets"].as_array().unwrap().len(),
        0,
        "secrets must be empty after disconnect"
    );
    assert!(
        body["connection"]["webhook_url"].is_null(),
        "webhook_url must be null after disconnect"
    );
}

#[tokio::test]
async fn reconnect_keeps_exactly_one_connection_row() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    // Connect.
    let first = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/connect",
            Method::POST,
            admin,
            tenant,
            json!({
                "config": { "source_label": "One" },
                "secrets": { "signing_secret": "whsec_one_1111" },
            }),
        ),
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first_body = body_json(first).await;
    let connection_id_str = first_body["connection"]["config"]["source_label"]
        .as_str()
        .unwrap()
        .to_string();

    // Disconnect.
    let disconnect = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations/generic-webhook/disconnect",
            Method::POST,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(disconnect.status(), StatusCode::OK);

    // Reconnect.
    let second = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/connect",
            Method::POST,
            admin,
            tenant,
            json!({
                "config": { "source_label": "Two" },
                "secrets": { "signing_secret": "whsec_two_2222" },
            }),
        ),
    )
    .await;
    assert_eq!(second.status(), StatusCode::CREATED);

    // The integration_connections row for (tenant, generic-webhook) must
    // still be exactly 1 — reconnect reactivates the same row, never inserts
    // a duplicate (FR-004).
    let catalog_id = {
        let row: (Uuid,) =
            sqlx::query_as("SELECT id FROM integration_catalog WHERE slug = $1")
                .bind("generic-webhook")
                .fetch_one(&pool)
                .await
                .unwrap();
        row.0
    };
    let count = count_connection_rows(&pool, tenant, catalog_id).await;
    assert_eq!(
        count, 1,
        "reconnect must not create a new row; expected 1, got {count}"
    );

    // The reconnect updated the source_label; sanity check via detail.
    let detail = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations/generic-webhook",
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(detail.status(), StatusCode::OK);
    let detail_body = body_json(detail).await;
    assert_eq!(
        detail_body["connection"]["config"]["source_label"], "Two"
    );
    let _ = connection_id_str; // silence unused
}

#[tokio::test]
async fn sc_004_all_lifecycle_actions_land_in_both_logs() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    // (1) connect → connected event + integration.connected audit row.
    let connect = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/connect",
            Method::POST,
            admin,
            tenant,
            json!({
                "config": { "source_label": "Billing" },
                "secrets": { "signing_secret": "whsec_orig_1111" },
            }),
        ),
    )
    .await;
    assert_eq!(connect.status(), StatusCode::CREATED);

    let catalog_id = {
        let row: (Uuid,) =
            sqlx::query_as("SELECT id FROM integration_catalog WHERE slug = $1")
                .bind("generic-webhook")
                .fetch_one(&pool)
                .await
                .unwrap();
        row.0
    };
    let connection_id = {
        let row: (Uuid,) = sqlx::query_as(
            "SELECT id FROM integration_connections WHERE tenant_id = $1 AND catalog_id = $2",
        )
        .bind(tenant)
        .bind(catalog_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        row.0
    };

    assert_eq!(
        count_events_for_connection(&pool, connection_id, "connected").await,
        1,
        "expected exactly one connected event"
    );
    assert_eq!(
        count_audit_for_action(
            &pool,
            tenant,
            "integration.connected",
            &connection_id.to_string()
        )
        .await,
        1,
        "expected exactly one integration.connected audit row"
    );

    // (2) config-only update → config_updated event + integration.config_updated audit row.
    let cfg_update = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/config",
            Method::PUT,
            admin,
            tenant,
            json!({
                "config": { "source_label": "Billing 2" },
            }),
        ),
    )
    .await;
    assert_eq!(cfg_update.status(), StatusCode::OK);
    assert_eq!(
        count_events_for_connection(&pool, connection_id, "config_updated").await,
        1,
        "expected one config_updated event after config-only update"
    );
    assert_eq!(
        count_audit_for_action(
            &pool,
            tenant,
            "integration.config_updated",
            &connection_id.to_string()
        )
        .await,
        1,
        "expected one integration.config_updated audit row"
    );
    // No secret_rotated from a config-only update.
    assert_eq!(
        count_events_for_connection(&pool, connection_id, "secret_rotated").await,
        0
    );
    assert_eq!(
        count_audit_for_action(
            &pool,
            tenant,
            "integration.secret_rotated",
            &connection_id.to_string()
        )
        .await,
        0
    );

    // (3) secret rotation → secret_rotated event + integration.secret_rotated audit row.
    let rot = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/integrations/generic-webhook/config",
            Method::PUT,
            admin,
            tenant,
            json!({
                "config": { "source_label": "Billing 2" },
                "secrets": { "signing_secret": "whsec_new__2222" },
            }),
        ),
    )
    .await;
    assert_eq!(rot.status(), StatusCode::OK);
    assert_eq!(
        count_events_for_connection(&pool, connection_id, "secret_rotated").await,
        1,
        "expected one secret_rotated event after secret rotation"
    );
    assert_eq!(
        count_audit_for_action(
            &pool,
            tenant,
            "integration.secret_rotated",
            &connection_id.to_string()
        )
        .await,
        1,
        "expected one integration.secret_rotated audit row"
    );

    // (4) disconnect → disconnected event + integration.disconnected audit row.
    let disconnect = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/integrations/generic-webhook/disconnect",
            Method::POST,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(disconnect.status(), StatusCode::OK);
    assert_eq!(
        count_events_for_connection(&pool, connection_id, "disconnected").await,
        1,
        "expected one disconnected event after disconnect"
    );
    assert_eq!(
        count_audit_for_action(
            &pool,
            tenant,
            "integration.disconnected",
            &connection_id.to_string()
        )
        .await,
        1,
        "expected one integration.disconnected audit row"
    );

    // (5) FR-006: disconnect must NOT delete pre-existing event rows.
    // The connected + config_updated + secret_rotated + disconnected rows
    // must all still be present.
    for event_type in ["connected", "config_updated", "secret_rotated", "disconnected"] {
        let count = count_events_for_connection(&pool, connection_id, event_type).await;
        assert_eq!(
            count, 1,
            "event history for {event_type} must be preserved across disconnect (FR-006)"
        );
    }
}

// Silence unused-helper lint when the only references in this file are type
// annotations. (Kept for symmetry with the other integrations_* test files.)
#[allow(dead_code)]
fn _unused_helper_keep_in_sync(_x: Vec<u8>, _y: Value) -> Vec<u8> {
    Vec::new()
}
