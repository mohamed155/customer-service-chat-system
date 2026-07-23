use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use config::Environment;
use http_body_util::BodyExt;
use rand::RngCore;
use serde_json::Value;
use server::router;
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_INTEGRATION_SECRETS_KEY: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=";
const TEST_SLUG: &str = "generic-webhook";

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
            eprintln!("skipping integrations_events live tests: DATABASE_URL not set");
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping integrations_events live tests: DATABASE_URL is unreachable");
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

async fn seed_user(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name, platform_role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(format!("ievt_{}@example.com", Uuid::new_v4()))
    .bind("Integrations Events User")
    .bind(None::<String>)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Integrations Events Tenant")
        .bind(format!("ievt-{}", Uuid::new_v4().simple()))
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

/// Seed a connection for a tenant directly via SQL. Returns its id. The
/// connection is `is_active = true` so the events endpoint has something to
/// page over (the endpoint's behaviour is identical for active and inactive
/// connections — it just filters by `connection_id`).
async fn seed_connection(pool: &sqlx::PgPool, tenant_id: Uuid) -> Uuid {
    let catalog_id: Uuid =
        sqlx::query_scalar("SELECT id FROM integration_catalog WHERE slug = $1")
            .bind(TEST_SLUG)
            .fetch_one(pool)
            .await
            .unwrap();

    let mut token_hash = [0u8; 32];
    let mut token_ciphertext = vec![0u8; 48];
    let mut token_nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut token_hash);
    rand::rngs::OsRng.fill_bytes(&mut token_ciphertext);
    rand::rngs::OsRng.fill_bytes(&mut token_nonce);

    sqlx::query_scalar(
        "INSERT INTO integration_connections \
         (tenant_id, catalog_id, is_active, config, \
          webhook_token_hash, webhook_token_ciphertext, webhook_token_nonce) \
         VALUES ($1, $2, true, '{}'::jsonb, $3, $4, $5) RETURNING id",
    )
    .bind(tenant_id)
    .bind(catalog_id)
    .bind(token_hash.to_vec())
    .bind(token_ciphertext)
    .bind(token_nonce.to_vec())
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Insert N `integration_events` rows for the given connection, each
/// one second apart starting from `base_ts` (so the ordering is fully
/// deterministic and the test can assert against it). Returns the
/// inserted ids in the same order as `base_ts..base_ts+N` (oldest first).
async fn seed_events(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    connection_id: Uuid,
    n: usize,
    base_ts: chrono::DateTime<chrono::Utc>,
) -> Vec<Uuid> {
    let mut ids = Vec::with_capacity(n);
    for i in 0..n {
        let ts = base_ts + chrono::Duration::seconds(i as i64);
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO integration_events \
             (tenant_id, connection_id, event_type, outcome, reason, created_at) \
             VALUES ($1, $2, 'delivery_accepted', 'success', NULL, $3) \
             RETURNING id",
        )
        .bind(tenant_id)
        .bind(connection_id)
        .bind(ts)
        .fetch_one(pool)
        .await
        .unwrap();
        ids.push(id);
    }
    ids
}

// ---------------------------------------------------------------------------
// T055 — Cursor pagination, limit clamping, cross-tenant isolation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn events_are_returned_newest_first() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;
    let connection_id = seed_connection(&pool, tenant).await;

    let base = chrono::Utc::now() - chrono::Duration::hours(1);
    let ids = seed_events(&pool, tenant, connection_id, 5, base).await;
    // Newest first → ids are reversed.
    let expected_newest_first: Vec<String> = ids
        .iter()
        .rev()
        .map(|id| id.to_string())
        .collect();

    let response = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/integrations/{TEST_SLUG}/events?limit=5"),
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 5);
    let actual: Vec<String> = entries
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        actual, expected_newest_first,
        "events must be ordered newest first"
    );
}

#[tokio::test]
async fn limit_is_honoured() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;
    let connection_id = seed_connection(&pool, tenant).await;

    let base = chrono::Utc::now() - chrono::Duration::hours(1);
    seed_events(&pool, tenant, connection_id, 5, base).await;

    let response = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/integrations/{TEST_SLUG}/events?limit=2"),
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 2, "limit=2 must return exactly 2 entries");
    assert_eq!(body["pagination"]["has_more"], true);
}

#[tokio::test]
async fn limit_is_clamped_to_one_to_one_hundred() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;
    let connection_id = seed_connection(&pool, tenant).await;

    let base = chrono::Utc::now() - chrono::Duration::hours(1);
    // 105 events so the upper-bound clamp is observable.
    seed_events(&pool, tenant, connection_id, 105, base).await;

    // limit=200 must clamp to 100.
    let response = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/integrations/{TEST_SLUG}/events?limit=200"),
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(
        body["data"].as_array().unwrap().len(),
        100,
        "limit must clamp to 100"
    );
    assert_eq!(body["pagination"]["has_more"], true);

    // limit=0 must clamp to 1.
    let response = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/integrations/{TEST_SLUG}/events?limit=0"),
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(
        body["data"].as_array().unwrap().len(),
        1,
        "limit=0 must clamp to 1"
    );

    // limit=-5 must also clamp to 1.
    let response = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/integrations/{TEST_SLUG}/events?limit=-5"),
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(
        body["data"].as_array().unwrap().len(),
        1,
        "limit=-5 must clamp to 1"
    );
}

#[tokio::test]
async fn cursor_pagination_walks_all_pages_without_overlap() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;
    let connection_id = seed_connection(&pool, tenant).await;

    let base = chrono::Utc::now() - chrono::Duration::hours(1);
    let ids = seed_events(&pool, tenant, connection_id, 5, base).await;
    let expected_newest_first: Vec<String> =
        ids.iter().rev().map(|id| id.to_string()).collect();

    let mut collected: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;
    let mut last_has_more = true;
    let mut page_count = 0;
    while last_has_more {
        let uri = match &cursor {
            Some(c) => format!("/api/v1/tenant/integrations/{TEST_SLUG}/events?limit=2&cursor={c}"),
            None => format!("/api/v1/tenant/integrations/{TEST_SLUG}/events?limit=2"),
        };
        let response = send(
            pool.clone(),
            authenticated_request(&uri, Method::GET, admin, tenant),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        let entries = body["data"].as_array().unwrap();
        for entry in entries {
            let id = entry["id"].as_str().unwrap().to_string();
            assert!(
                !collected.contains(&id),
                "duplicate id across pages: {id}"
            );
            collected.push(id);
        }
        last_has_more = body["pagination"]["has_more"].as_bool().unwrap();
        cursor = body["pagination"]["next_cursor"]
            .as_str()
            .map(|s| s.to_string());
        if !last_has_more {
            assert!(
                cursor.is_none(),
                "final page must return next_cursor: null when has_more is false"
            );
        }
        page_count += 1;
        assert!(page_count <= 10, "pagination loop must terminate");
    }

    assert_eq!(collected.len(), 5, "all 5 events must be reachable");
    assert_eq!(collected, expected_newest_first);
    assert_eq!(page_count, 3, "5 events / page 2 = 3 pages (2 + 2 + 1)");
}

#[tokio::test]
async fn invalid_cursor_returns_422() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    let response = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/integrations/{TEST_SLUG}/events?cursor=zzz"),
            Method::GET,
            admin,
            tenant,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn cross_tenant_tenant_b_admin_does_not_see_tenant_a_events() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    // Tenant A: connection + events.
    let tenant_a = seed_tenant(&pool).await;
    let admin_a = seed_user(&pool).await;
    seed_membership(&pool, tenant_a, admin_a, "admin").await;
    let conn_a = seed_connection(&pool, tenant_a).await;
    let base = chrono::Utc::now() - chrono::Duration::hours(1);
    let ids_a = seed_events(&pool, tenant_a, conn_a, 4, base).await;

    // Tenant B: admin user with no connection to the slug.
    let tenant_b = seed_tenant(&pool).await;
    let admin_b = seed_user(&pool).await;
    seed_membership(&pool, tenant_b, admin_b, "admin").await;

    // Request events for the slug as tenant B. The endpoint must return
    // either an empty list (no connection for tenant B) or only tenant B's
    // events — never any of tenant A's.
    let response = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/integrations/{TEST_SLUG}/events"),
            Method::GET,
            admin_b,
            tenant_b,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let entries = body["data"].as_array().unwrap();
    let returned_ids: Vec<String> = entries
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect();
    for id in &ids_a {
        assert!(
            !returned_ids.contains(&id.to_string()),
            "tenant B must not see tenant A's event {id}"
        );
    }
    assert_eq!(body["pagination"]["has_more"], false);
    assert!(body["pagination"]["next_cursor"].is_null());
}

#[tokio::test]
async fn cross_tenant_tenant_b_with_connection_does_not_see_tenant_a_events() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    // Tenant A: connection + events.
    let tenant_a = seed_tenant(&pool).await;
    let admin_a = seed_user(&pool).await;
    seed_membership(&pool, tenant_a, admin_a, "admin").await;
    let conn_a = seed_connection(&pool, tenant_a).await;
    let base = chrono::Utc::now() - chrono::Duration::hours(1);
    let ids_a = seed_events(&pool, tenant_a, conn_a, 4, base).await;

    // Tenant B: a separate connection to the same slug. The endpoint must
    // scope by `tenant_id` and return only tenant B's events (or none).
    let tenant_b = seed_tenant(&pool).await;
    let admin_b = seed_user(&pool).await;
    seed_membership(&pool, tenant_b, admin_b, "admin").await;
    let _conn_b = seed_connection(&pool, tenant_b).await;

    let response = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/integrations/{TEST_SLUG}/events"),
            Method::GET,
            admin_b,
            tenant_b,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let entries = body["data"].as_array().unwrap();
    let returned_ids: Vec<String> = entries
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect();
    for id in &ids_a {
        assert!(
            !returned_ids.contains(&id.to_string()),
            "tenant B must not see tenant A's event {id}"
        );
    }
}
