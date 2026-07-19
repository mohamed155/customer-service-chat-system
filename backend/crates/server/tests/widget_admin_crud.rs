use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use http_body_util::BodyExt;
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

fn app_state(pool: sqlx::PgPool) -> AppState {
    AppState {
        config: Arc::new(test_config()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool, &test_config()).unwrap(),
    }
}

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            eprintln!("skipping widget admin CRUD tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping widget admin CRUD tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query(
        "TRUNCATE TABLE messages, customer_channel_identifiers, customers, conversations, \
         widget_sessions, widget_instances, outbox_events, audit_logs, \
         tenant_invitations, tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("failed to reset widget admin CRUD test tables");
}

async fn send(pool: sqlx::PgPool, request: Request<Body>) -> Response {
    router::app_with_test_routes(app_state(pool))
        .oneshot(request)
        .await
        .expect("request should complete")
}

fn request(uri: &str, method: Method, body: Body) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(method)
        .body(body)
        .unwrap()
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
    body: serde_json::Value,
) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(method)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

async fn body_json(response: Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Widget Test Tenant")
        .bind(format!("wgt-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Widget Test User")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[allow(dead_code)]
struct SeededActor {
    user_id: Uuid,
    membership_id: Uuid,
    tenant_id: Uuid,
}

async fn seed_admin(pool: &sqlx::PgPool, tenant_id: Uuid) -> SeededActor {
    let user_id = seed_user(pool, "admin@widget.test").await;
    let membership_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, 'admin', 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap();
    SeededActor {
        user_id,
        membership_id,
        tenant_id,
    }
}

async fn seed_viewer(pool: &sqlx::PgPool, tenant_id: Uuid) -> SeededActor {
    let user_id = seed_user(pool, "viewer@widget.test").await;
    let membership_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, 'viewer', 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap();
    SeededActor {
        user_id,
        membership_id,
        tenant_id,
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn full_create_list_get_update_delete_cycle() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let admin = seed_admin(&pool, tenant_id).await;

    // CREATE
    let create_body = serde_json::json!({
        "name": "Marketing Site Widget",
        "displayName": "Support",
        "primaryColor": "#4F46E5",
        "welcomeMessage": "Hi! How can we help?",
        "position": "bottom-right",
        "theme": "light",
        "enabled": true,
        "allowedDomains": ["example.com"]
    });
    let resp = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/widgets",
            Method::POST,
            admin.user_id,
            tenant_id,
            create_body,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    let instance_id = body["data"]["id"].as_str().unwrap().to_string();
    let public_id = body["data"]["publicId"].as_str().unwrap().to_string();
    assert!(
        public_id.starts_with("wgt_"),
        "publicId should start with wgt_"
    );
    assert_eq!(body["data"]["name"], "Marketing Site Widget");
    assert_eq!(body["data"]["displayName"], "Support");

    // LIST
    let resp = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/widgets",
            Method::GET,
            admin.user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let instances = body["data"].as_array().unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0]["id"], instance_id);
    assert_eq!(instances[0]["publicId"], public_id);

    // GET
    let resp = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/widgets/{}", instance_id),
            Method::GET,
            admin.user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["data"]["name"], "Marketing Site Widget");

    // UPDATE
    let update_body = serde_json::json!({
        "name": "Updated Widget",
        "displayName": "Updated Support",
        "primaryColor": "#000000",
        "welcomeMessage": "Updated welcome",
        "position": "bottom-left",
        "theme": "dark",
        "enabled": false,
        "allowedDomains": ["updated.com"]
    });
    let resp = send(
        pool.clone(),
        authenticated_json_request(
            &format!("/api/v1/tenant/widgets/{}", instance_id),
            Method::PUT,
            admin.user_id,
            tenant_id,
            update_body,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["data"]["name"], "Updated Widget");
    assert_eq!(body["data"]["displayName"], "Updated Support");
    // publicId is immutable
    assert_eq!(body["data"]["publicId"], public_id);

    // DELETE (soft)
    let resp = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/widgets/{}", instance_id),
            Method::DELETE,
            admin.user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // GET after delete = 404
    let resp = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/widgets/{}", instance_id),
            Method::GET,
            admin.user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // LIST after delete = empty
    let resp = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/widgets",
            Method::GET,
            admin.user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert!(body["data"].as_array().unwrap().is_empty());

    // Public config should also return 404 for deleted instance
    let resp = send(
        pool.clone(),
        request(
            &format!("/api/v1/widget/v1/config?widget_id={}", public_id),
            Method::GET,
            Body::empty(),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn validation_failures_return_422() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let admin = seed_admin(&pool, tenant_id).await;

    // Missing name
    let resp = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/widgets",
            Method::POST,
            admin.user_id,
            tenant_id,
            serde_json::json!({}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // Invalid color
    let resp = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/widgets",
            Method::POST,
            admin.user_id,
            tenant_id,
            serde_json::json!({"name": "Test", "primaryColor": "not-a-color"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // Invalid position
    let resp = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/widgets",
            Method::POST,
            admin.user_id,
            tenant_id,
            serde_json::json!({"name": "Test", "position": "top-center"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // Too many allowed domains
    let domains: Vec<String> = (0..25).map(|i| format!("domain{}.com", i)).collect();
    let resp = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/widgets",
            Method::POST,
            admin.user_id,
            tenant_id,
            serde_json::json!({"name": "Test", "allowedDomains": domains}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn viewer_cannot_write() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let viewer = seed_viewer(&pool, tenant_id).await;

    // POST = 403
    let resp = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/widgets",
            Method::POST,
            viewer.user_id,
            tenant_id,
            serde_json::json!({"name": "Test"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // GET = 200 (viewer can read)
    let resp = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/widgets",
            Method::GET,
            viewer.user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn manage_can_read_and_write() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let admin = seed_admin(&pool, tenant_id).await;

    // Admin can create
    let resp = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/widgets",
            Method::POST,
            admin.user_id,
            tenant_id,
            serde_json::json!({"name": "Admin Widget"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Admin can list
    let resp = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/widgets",
            Method::GET,
            admin.user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn widget_conversation_has_channel_and_attribution() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let admin = seed_admin(&pool, tenant_id).await;

    // Create a widget instance
    let resp = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/widgets",
            Method::POST,
            admin.user_id,
            tenant_id,
            serde_json::json!({"name": "Chat Widget"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    let widget_instance_id: Uuid = body["data"]["id"].as_str().unwrap().parse().unwrap();
    let widget_name = body["data"]["name"].as_str().unwrap().to_string();

    // Seed a customer
    let customer_id: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Widget Visitor")
    .fetch_one(&pool)
    .await
    .unwrap();

    // Create a conversation with widget_instance_id set
    let conv_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, widget_instance_id) \
         VALUES ($1, $2, 'widget', 'open', $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(widget_instance_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // Bump last_activity_at
    sqlx::query("UPDATE conversations SET last_activity_at = now() WHERE id = $1")
        .bind(conv_id)
        .execute(&pool)
        .await
        .unwrap();

    // Verify conversation appears in list with channel = "widget" and widgetInstance populated
    let resp = send(
        pool.clone(),
        authenticated_request(
            "/api/v1/tenant/conversations?channel=widget",
            Method::GET,
            admin.user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let conversations = body["data"].as_array().unwrap();
    let conv = conversations
        .iter()
        .find(|c| c["id"] == conv_id.to_string());
    assert!(conv.is_some(), "Widget conversation should be in the list");
    let conv = conv.unwrap();
    assert_eq!(conv["channel"], "widget");
    assert_eq!(conv["widgetInstance"]["id"], widget_instance_id.to_string());
    assert_eq!(conv["widgetInstance"]["name"], widget_name);
}

#[tokio::test]
async fn snippet_endpoint_returns_embed_code() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let admin = seed_admin(&pool, tenant_id).await;
    let viewer = seed_viewer(&pool, tenant_id).await;

    // Create an instance first
    let resp = send(
        pool.clone(),
        authenticated_json_request(
            "/api/v1/tenant/widgets",
            Method::POST,
            admin.user_id,
            tenant_id,
            serde_json::json!({"name": "Snippet Widget"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    let instance_id = body["data"]["id"].as_str().unwrap();
    let public_id = body["data"]["publicId"].as_str().unwrap();

    // Viewer can read snippet
    let resp = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/widgets/{}/snippet", instance_id),
            Method::GET,
            viewer.user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    let snippet = body["data"]["snippet"].as_str().unwrap();
    assert!(snippet.contains("<script"));
    assert!(snippet.contains("widget.js"));
    assert!(snippet.contains(public_id));

    // Snippet for non-existent instance = 404
    let fake_id = Uuid::new_v4().to_string();
    let resp = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/widgets/{}/snippet", fake_id),
            Method::GET,
            viewer.user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
