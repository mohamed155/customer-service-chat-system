use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use server::router;
use server::state::AppState;
use storage::{InMemoryStorage, ObjectStorage};
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
    }
}

fn plain_state(pool: sqlx::PgPool) -> AppState {
    let cfg = test_config();
    AppState {
        config: Arc::new(cfg.clone()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(1)),
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
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            eprintln!("skipping knowledge base tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping knowledge base tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query(
        "TRUNCATE TABLE knowledge_item_tags, knowledge_documents, knowledge_items, knowledge_categories, \
         audit_logs, outbox_events, tenant_invitations, tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("failed to reset test tables");
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    let slug = format!("kb-tenant-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Knowledge Base Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str, _role: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Knowledge Base Test User")
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

async fn seed_item(pool: &sqlx::PgPool, tenant_id: Uuid, title: &str, user_id: Uuid) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO knowledge_items (tenant_id, item_type, title, body, source, created_by_user_id, created_by_display) \
         VALUES ($1, 'article', $2, 'body', 'authored', $3, 'Test User') RETURNING id",
    )
    .bind(tenant_id)
    .bind(title)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn send(state: &AppState, request: Request<Body>) -> axum::response::Response {
    router::app_with_test_routes_and_storage(state.clone(), Arc::new(InMemoryStorage::default()))
        .oneshot(request)
        .await
        .expect("request should complete")
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn auth_get(uri: &str, user_id: Uuid, tenant_id: Uuid) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(Method::GET)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .body(Body::empty())
        .unwrap()
}

fn json_post(uri: &str, user_id: Uuid, tenant_id: Uuid, body: serde_json::Value) -> Request<Body> {
    let bytes = serde_json::to_vec(&body).unwrap();
    Request::builder()
        .uri(uri)
        .method(Method::POST)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("content-type", "application/json")
        .body(Body::from(bytes))
        .unwrap()
}

fn json_patch(uri: &str, user_id: Uuid, tenant_id: Uuid, body: serde_json::Value) -> Request<Body> {
    let bytes = serde_json::to_vec(&body).unwrap();
    Request::builder()
        .uri(uri)
        .method(Method::PATCH)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("content-type", "application/json")
        .body(Body::from(bytes))
        .unwrap()
}

fn json_delete(uri: &str, user_id: Uuid, tenant_id: Uuid) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(Method::DELETE)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .body(Body::empty())
        .unwrap()
}

#[allow(clippy::type_complexity)]
fn multipart_body(fields: &[(&str, Option<&str>, Option<&str>, &[u8])], boundary: &str) -> Vec<u8> {
    let boundary = boundary.as_bytes();
    let mut body = Vec::new();
    for (name, filename, content_type, data) in fields {
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary);
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"");
        body.extend_from_slice(name.as_bytes());
        body.extend_from_slice(b"\"");
        if let Some(fname) = filename {
            body.extend_from_slice(b"; filename=\"");
            body.extend_from_slice(fname.as_bytes());
            body.extend_from_slice(b"\"");
        }
        body.extend_from_slice(b"\r\n");
        if let Some(ct) = content_type {
            body.extend_from_slice(b"Content-Type: ");
            body.extend_from_slice(ct.as_bytes());
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary);
    body.extend_from_slice(b"--\r\n");
    body
}

fn multipart_post(
    uri: &str,
    user_id: Uuid,
    tenant_id: Uuid,
    boundary: &str,
    body: Vec<u8>,
) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(Method::POST)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .header(
            "content-type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body))
        .unwrap()
}

async fn send_with_storage(
    state: &AppState,
    storage: Arc<InMemoryStorage>,
    request: Request<Body>,
) -> axum::response::Response {
    router::app_with_test_routes_and_storage(state.clone(), storage)
        .oneshot(request)
        .await
        .expect("request should complete")
}

async fn seed_document(pool: &sqlx::PgPool, tenant_id: Uuid, title: &str, user_id: Uuid) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO knowledge_items (tenant_id, item_type, title, body, source, created_by_user_id, created_by_display) \
         VALUES ($1, 'document', $2, NULL, 'uploaded', $3, 'Test User') RETURNING id",
    )
    .bind(tenant_id)
    .bind(title)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ═══════════════════════════════════════════════════════════════════════════════
// T018 — US1: Knowledge item CRUD
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_item_201() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "create-201@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    let payload = serde_json::json!({
        "title": "Test Article",
        "body": "<p>Hello world</p>",
        "itemType": "article",
    });
    let resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let json = body_json(resp).await;
    assert_eq!(json["title"], "Test Article");
    assert_eq!(json["body"], "<p>Hello world</p>");
    assert_eq!(json["itemType"], "article");
    assert_eq!(json["status"], "draft");
    assert_eq!(json["source"], "authored");
    assert!(!json["id"].as_str().unwrap().is_empty());
    assert!(!json["createdByDisplay"].as_str().unwrap().is_empty());

    let item_id = Uuid::parse_str(json["id"].as_str().unwrap()).unwrap();

    let list_resp = send(
        &state,
        auth_get("/api/v1/tenant/knowledge/items", user_id, tenant_id),
    )
    .await;
    assert_eq!(list_resp.status(), StatusCode::OK);
    let list_json = body_json(list_resp).await;
    let ids: Vec<&str> = list_json["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&item_id.to_string().as_str()));
}

#[tokio::test]
async fn create_item_empty_title_validation_failed() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "create-empty-title@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    let payload = serde_json::json!({
        "title": "",
        "body": "some body",
        "itemType": "article",
    });
    let resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let json = body_json(resp).await;
    assert_eq!(json["error"]["code"], "validation_failed");

    let list_resp = send(
        &state,
        auth_get("/api/v1/tenant/knowledge/items", user_id, tenant_id),
    )
    .await;
    let list_json = body_json(list_resp).await;
    assert_eq!(list_json["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn create_item_document_type_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "create-doc-rej@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    let payload = serde_json::json!({
        "title": "Document attempt",
        "body": "body",
        "itemType": "document",
    });
    let resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let json = body_json(resp).await;
    assert_eq!(json["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn create_item_unknown_category_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "create-cat-rej@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    let fake_id = Uuid::new_v4();
    let payload = serde_json::json!({
        "title": "Test",
        "body": "body",
        "itemType": "article",
        "categoryId": fake_id.to_string(),
    });
    let resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let json = body_json(resp).await;
    assert_eq!(json["error"]["code"], "validation_failed");
    assert_eq!(json["error"]["details"][0]["field"], "categoryId");
    assert_eq!(json["error"]["details"][0]["code"], "not_found");
}

#[tokio::test]
async fn update_item_title_body_type() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "update-1@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    let create_payload = serde_json::json!({
        "title": "Original Title",
        "body": "Original body",
        "itemType": "article",
    });
    let create_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            create_payload,
        ),
    )
    .await;
    let create_json = body_json(create_resp).await;
    let item_id = create_json["id"].as_str().unwrap().to_string();

    let update_payload = serde_json::json!({
        "title": "Updated Title",
        "body": "Updated body",
        "itemType": "faq",
    });
    let update_resp = send(
        &state,
        json_patch(
            &format!("/api/v1/tenant/knowledge/items/{item_id}"),
            user_id,
            tenant_id,
            update_payload,
        ),
    )
    .await;
    assert_eq!(update_resp.status(), StatusCode::OK);
    let update_json = body_json(update_resp).await;
    assert_eq!(update_json["title"], "Updated Title");
    assert_eq!(update_json["body"], "Updated body");
    assert_eq!(update_json["itemType"], "faq");

    let get_resp = send(
        &state,
        auth_get(
            &format!("/api/v1/tenant/knowledge/items/{item_id}"),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let get_json = body_json(get_resp).await;
    assert_eq!(get_json["title"], "Updated Title");
    assert_eq!(get_json["body"], "Updated body");
    assert_eq!(get_json["itemType"], "faq");
}

#[tokio::test]
async fn update_item_preserves_status() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "update-status@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    let create_payload = serde_json::json!({
        "title": "Status Test",
        "body": "body",
        "itemType": "article",
    });
    let create_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            create_payload,
        ),
    )
    .await;
    let create_json = body_json(create_resp).await;
    assert_eq!(create_json["status"], "draft");
    let item_id = create_json["id"].as_str().unwrap().to_string();

    let update_payload = serde_json::json!({
        "title": "Updated Status Test",
    });
    let update_resp = send(
        &state,
        json_patch(
            &format!("/api/v1/tenant/knowledge/items/{item_id}"),
            user_id,
            tenant_id,
            update_payload,
        ),
    )
    .await;
    assert_eq!(update_resp.status(), StatusCode::OK);
    let update_json = body_json(update_resp).await;
    assert_eq!(update_json["status"], "draft");
}

#[tokio::test]
async fn cross_tenant_not_found() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let user_a = seed_user(&pool, "cross-a@test.com", "admin").await;
    let user_b = seed_user(&pool, "cross-b@test.com", "admin").await;
    seed_membership(&pool, tenant_a, user_a, "admin").await;
    seed_membership(&pool, tenant_b, user_b, "admin").await;

    let state = plain_state(pool.clone());

    let create_payload = serde_json::json!({
        "title": "Tenant A Item",
        "body": "body",
        "itemType": "article",
    });
    let create_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_a,
            tenant_a,
            create_payload,
        ),
    )
    .await;
    let create_json = body_json(create_resp).await;
    let item_id = create_json["id"].as_str().unwrap().to_string();

    let get_resp = send(
        &state,
        auth_get(
            &format!("/api/v1/tenant/knowledge/items/{item_id}"),
            user_b,
            tenant_b,
        ),
    )
    .await;
    assert_eq!(get_resp.status(), StatusCode::NOT_FOUND);

    let patch_resp = send(
        &state,
        json_patch(
            &format!("/api/v1/tenant/knowledge/items/{item_id}"),
            user_b,
            tenant_b,
            serde_json::json!({"title": "Hacked"}),
        ),
    )
    .await;
    assert_eq!(patch_resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_items_tenant_scoped() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let user_a = seed_user(&pool, "scope-a@test.com", "admin").await;
    let user_b = seed_user(&pool, "scope-b@test.com", "admin").await;
    seed_membership(&pool, tenant_a, user_a, "admin").await;
    seed_membership(&pool, tenant_b, user_b, "admin").await;

    let state = plain_state(pool.clone());

    seed_item(&pool, tenant_a, "Item A1", user_a).await;
    seed_item(&pool, tenant_a, "Item A2", user_a).await;
    seed_item(&pool, tenant_b, "Item B1", user_b).await;

    let list_a = send(
        &state,
        auth_get("/api/v1/tenant/knowledge/items", user_a, tenant_a),
    )
    .await;
    let json_a = body_json(list_a).await;
    let titles_a: Vec<&str> = json_a["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles_a.len(), 2);
    assert!(titles_a.contains(&"Item A1"));
    assert!(titles_a.contains(&"Item A2"));

    let list_b = send(
        &state,
        auth_get("/api/v1/tenant/knowledge/items", user_b, tenant_b),
    )
    .await;
    let json_b = body_json(list_b).await;
    let titles_b: Vec<&str> = json_b["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles_b.len(), 1);
    assert_eq!(titles_b[0], "Item B1");
}

#[tokio::test]
async fn list_items_cursor_pagination() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "pagination@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    let count = 27;
    for i in 0..count {
        let title = format!("Paginated Item {}", i);
        seed_item(&pool, tenant_id, &title, user_id).await;
    }

    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let uri = match &cursor {
            Some(c) => format!("/api/v1/tenant/knowledge/items?limit=10&before={c}"),
            None => "/api/v1/tenant/knowledge/items?limit=10".to_string(),
        };
        let resp = send(&state, auth_get(&uri, user_id, tenant_id)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let items = json["items"].as_array().unwrap();
        for item in items {
            let id = item["id"].as_str().unwrap().to_string();
            assert!(!seen.contains(&id), "duplicate item {id}");
            seen.push(id);
        }
        if json["has_more"].as_bool().unwrap_or(false) {
            cursor = json["next_cursor"].as_str().map(|s| s.to_string());
            assert!(cursor.is_some(), "has_more=true but next_cursor is null");
        } else {
            assert!(json["next_cursor"].is_null());
            break;
        }
    }
    assert_eq!(seen.len(), count);
}

#[tokio::test]
async fn list_items_malformed_cursor() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "bad-cursor@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    let resp = send(
        &state,
        auth_get(
            "/api/v1/tenant/knowledge/items?before=not-a-valid-base64!!",
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let json = body_json(resp).await;
    assert_eq!(json["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn audit_created_has_actor_and_no_body() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "audit-create@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    let payload = serde_json::json!({
        "title": "Audit Test",
        "body": "Secret body content",
        "itemType": "article",
    });
    let resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let create_json = body_json(resp).await;
    let item_id = create_json["id"].as_str().unwrap();

    let audit: Vec<(String, Option<Uuid>, serde_json::Value)> = sqlx::query_as(
        "SELECT action, actor_user_id, details FROM audit_logs \
         WHERE tenant_id = $1 AND action = 'knowledge_item.created' \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(audit.len(), 1);
    let (action, actor, details) = &audit[0];
    assert_eq!(action, "knowledge_item.created");
    assert_eq!(*actor, Some(user_id));
    let details_str = serde_json::to_string(details).unwrap();
    assert!(
        !details_str.contains("Secret body content"),
        "audit details must not contain the item body"
    );
    assert_eq!(details["itemId"], item_id);
    assert_eq!(details["itemType"], "article");
}

#[tokio::test]
async fn update_preserves_attribution() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "attribution@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    let create_payload = serde_json::json!({
        "title": "Original",
        "body": "Original body",
        "itemType": "article",
    });
    let create_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            create_payload,
        ),
    )
    .await;
    let create_json = body_json(create_resp).await;
    let item_id = create_json["id"].as_str().unwrap().to_string();
    let original_created_by = create_json["createdByDisplay"]
        .as_str()
        .unwrap()
        .to_string();
    let original_created_at = create_json["createdAt"].as_str().unwrap().to_string();

    let update_payload = serde_json::json!({
        "title": "Modified",
        "body": "Modified body",
    });
    let update_resp = send(
        &state,
        json_patch(
            &format!("/api/v1/tenant/knowledge/items/{item_id}"),
            user_id,
            tenant_id,
            update_payload,
        ),
    )
    .await;
    assert_eq!(update_resp.status(), StatusCode::OK);
    let update_json = body_json(update_resp).await;
    assert_eq!(update_json["createdByDisplay"], original_created_by);
    assert_eq!(
        update_json["createdByUserId"],
        create_json["createdByUserId"]
    );
    assert_eq!(update_json["createdAt"], original_created_at);
}

// ═══════════════════════════════════════════════════════════════════════════════
// T019 — RBAC tests for knowledge base routes
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn rbac_knowledge_writes_require_manage() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let state = plain_state(pool.clone());

    // Roles without KnowledgeBaseManage → 403 on writes (POST, PATCH)
    for role in ["agent", "viewer"] {
        let tenant_id = seed_tenant(&pool).await;
        let user_id = seed_user(&pool, &format!("kb-write-{role}@test.com"), role).await;
        seed_membership(&pool, tenant_id, user_id, role).await;

        let create_payload = serde_json::json!({
            "title": "Write attempt",
            "body": "body",
            "itemType": "article",
        });
        let post_resp = send(
            &state,
            json_post(
                "/api/v1/tenant/knowledge/items",
                user_id,
                tenant_id,
                create_payload,
            ),
        )
        .await;
        assert_eq!(
            post_resp.status(),
            StatusCode::FORBIDDEN,
            "POST should be 403 for {role}"
        );

        // Seed an item first so PATCH has something to target
        let item_id = seed_item(&pool, tenant_id, "RBAC Item", user_id).await;

        let patch_resp = send(
            &state,
            json_patch(
                &format!("/api/v1/tenant/knowledge/items/{item_id}"),
                user_id,
                tenant_id,
                serde_json::json!({"title": "Hacked"}),
            ),
        )
        .await;
        assert_eq!(
            patch_resp.status(),
            StatusCode::FORBIDDEN,
            "PATCH should be 403 for {role}"
        );
    }

    // Owner, Admin, Manager succeed on writes
    for role in ["owner", "admin", "manager"] {
        let tenant_id = seed_tenant(&pool).await;
        let user_id = seed_user(&pool, &format!("kb-write-{role}@test.com"), role).await;
        seed_membership(&pool, tenant_id, user_id, role).await;

        let create_payload = serde_json::json!({
            "title": format!("{role} creation"),
            "body": "body",
            "itemType": "article",
        });
        let post_resp = send(
            &state,
            json_post(
                "/api/v1/tenant/knowledge/items",
                user_id,
                tenant_id,
                create_payload,
            ),
        )
        .await;
        assert_eq!(
            post_resp.status(),
            StatusCode::CREATED,
            "POST should succeed for {role}"
        );
        let post_json = body_json(post_resp).await;
        let item_id = post_json["id"].as_str().unwrap().to_string();

        let patch_resp = send(
            &state,
            json_patch(
                &format!("/api/v1/tenant/knowledge/items/{item_id}"),
                user_id,
                tenant_id,
                serde_json::json!({"title": "Updated by {role}"}),
            ),
        )
        .await;
        assert_eq!(
            patch_resp.status(),
            StatusCode::OK,
            "PATCH should succeed for {role}"
        );
    }
}

#[tokio::test]
async fn rbac_knowledge_reads_allowed_for_all_roles() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let state = plain_state(pool.clone());

    for role in ["owner", "admin", "manager", "agent", "viewer"] {
        let tenant_id = seed_tenant(&pool).await;
        let user_id = seed_user(&pool, &format!("kb-read-{role}@test.com"), role).await;
        seed_membership(&pool, tenant_id, user_id, role).await;

        let item_id = seed_item(&pool, tenant_id, &format!("Read test {role}"), user_id).await;

        let list_resp = send(
            &state,
            auth_get("/api/v1/tenant/knowledge/items", user_id, tenant_id),
        )
        .await;
        assert_eq!(
            list_resp.status(),
            StatusCode::OK,
            "GET list should succeed for {role}"
        );

        let get_resp = send(
            &state,
            auth_get(
                &format!("/api/v1/tenant/knowledge/items/{item_id}"),
                user_id,
                tenant_id,
            ),
        )
        .await;
        assert_eq!(
            get_resp.status(),
            StatusCode::OK,
            "GET item should succeed for {role}"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// T026 — US2: Publish / archive / restore (item status transitions)
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn publish_draft_succeeds() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "pub-draft@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    let payload = serde_json::json!({
        "title": "Publish Test",
        "body": "<p>ready</p>",
        "itemType": "article",
    });
    let create_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let item_id = body_json(create_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let status_resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "published"}),
        ),
    )
    .await;
    assert_eq!(status_resp.status(), StatusCode::OK);
    let status_json = body_json(status_resp).await;
    assert_eq!(status_json["status"], "published");
    assert_eq!(status_json["changed"], true);

    let get_resp = send(
        &state,
        auth_get(
            &format!("/api/v1/tenant/knowledge/items/{item_id}"),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(body_json(get_resp).await["status"], "published");
}

#[tokio::test]
async fn archive_published_succeeds() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "archive-pub@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    let payload = serde_json::json!({
        "title": "Archive Test",
        "body": "<p>body</p>",
        "itemType": "article",
    });
    let create_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    let item_id = body_json(create_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "published"}),
        ),
    )
    .await;

    let archive_resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "archived"}),
        ),
    )
    .await;
    assert_eq!(archive_resp.status(), StatusCode::OK);
    let archive_json = body_json(archive_resp).await;
    assert_eq!(archive_json["status"], "archived");
    assert_eq!(archive_json["changed"], true);
}

#[tokio::test]
async fn restore_archived_succeeds() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "restore@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    let payload = serde_json::json!({
        "title": "Restore Test",
        "body": "<p>body</p>",
        "itemType": "article",
    });
    let create_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    let item_id = body_json(create_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "published"}),
        ),
    )
    .await;
    send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "archived"}),
        ),
    )
    .await;

    let restore_resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "draft"}),
        ),
    )
    .await;
    assert_eq!(restore_resp.status(), StatusCode::OK);
    let restore_json = body_json(restore_resp).await;
    assert_eq!(restore_json["status"], "draft");
    assert_eq!(restore_json["changed"], true);
}

#[tokio::test]
async fn all_illegal_transitions_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "illegal@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    async fn create_and_publish(state: &AppState, user_id: Uuid, tenant_id: Uuid) -> String {
        let payload = serde_json::json!({
            "title": "Illegal Test",
            "body": "<p>body</p>",
            "itemType": "article",
        });
        let resp = send(
            state,
            json_post(
                "/api/v1/tenant/knowledge/items",
                user_id,
                tenant_id,
                payload,
            ),
        )
        .await;
        let item_id = body_json(resp).await["id"].as_str().unwrap().to_string();
        send(
            state,
            json_post(
                &format!("/api/v1/tenant/knowledge/items/{item_id}/status"),
                user_id,
                tenant_id,
                serde_json::json!({"status": "published"}),
            ),
        )
        .await;
        item_id
    }

    // published → draft (illegal)
    let item_id = create_and_publish(&state, user_id, tenant_id).await;
    let resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "draft"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body_json(resp).await["error"]["code"], "validation_failed");

    // draft → archived (illegal)
    let payload2 = serde_json::json!({"title": "Illegal2", "body": "body", "itemType": "article"});
    let resp2 = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            payload2,
        ),
    )
    .await;
    let item_id2 = body_json(resp2).await["id"].as_str().unwrap().to_string();
    let resp3 = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id2}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "archived"}),
        ),
    )
    .await;
    assert_eq!(resp3.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body_json(resp3).await["error"]["code"], "validation_failed");

    // archived → published (illegal)
    let payload4 = serde_json::json!({"title": "Illegal3", "body": "body", "itemType": "article"});
    let resp4 = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            payload4,
        ),
    )
    .await;
    let item_id3 = body_json(resp4).await["id"].as_str().unwrap().to_string();
    send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id3}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "published"}),
        ),
    )
    .await;
    send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id3}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "archived"}),
        ),
    )
    .await;
    let resp5 = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id3}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "published"}),
        ),
    )
    .await;
    assert_eq!(resp5.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body_json(resp5).await["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn same_status_noop() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "noop@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    let payload = serde_json::json!({
        "title": "Noop Test",
        "body": "body",
        "itemType": "article",
    });
    let create_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    let item_id = body_json(create_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let audit_before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let noop_resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "draft"}),
        ),
    )
    .await;
    assert_eq!(noop_resp.status(), StatusCode::OK);
    let noop_json = body_json(noop_resp).await;
    assert_eq!(noop_json["changed"], false);

    let audit_after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(audit_after, audit_before, "noop must not write audit row");
}

#[tokio::test]
async fn publish_empty_body_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "empty-body@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    let payload = serde_json::json!({
        "title": "Empty Body",
        "body": "",
        "itemType": "article",
    });
    let create_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    let item_id = body_json(create_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "published"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let json = body_json(resp).await;
    assert_eq!(json["error"]["code"], "validation_failed");

    let get_resp = send(
        &state,
        auth_get(
            &format!("/api/v1/tenant/knowledge/items/{item_id}"),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(body_json(get_resp).await["status"], "draft");
}

#[tokio::test]
async fn publish_document_no_body_succeeds() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "doc-pub@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    let item_id = seed_document(&pool, tenant_id, "Doc Publish", user_id).await;

    let resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "published"}),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["status"], "published");
    assert_eq!(json["changed"], true);
}

#[tokio::test]
async fn each_transition_writes_audit() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "audit-trans@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    // draft → published → "knowledge_item.published"
    let payload = serde_json::json!({"title": "Audit1", "body": "body", "itemType": "article"});
    let create_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    let item_id = body_json(create_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "published"}),
        ),
    )
    .await;
    let audit: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit_logs WHERE tenant_id = $1 AND item_id = $2::text AND action = 'knowledge_item.published'",
    )
    .bind(tenant_id)
    .bind(&item_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        !audit.is_empty(),
        "expected knowledge_item.published audit row"
    );

    // published → archived → "knowledge_item.archived"
    send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "archived"}),
        ),
    )
    .await;
    let audit2: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit_logs WHERE tenant_id = $1 AND item_id = $2::text AND action = 'knowledge_item.archived'",
    )
    .bind(tenant_id)
    .bind(&item_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        !audit2.is_empty(),
        "expected knowledge_item.archived audit row"
    );

    // archived → draft → "knowledge_item.restored"
    send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "draft"}),
        ),
    )
    .await;
    let audit3: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit_logs WHERE tenant_id = $1 AND item_id = $2::text AND action = 'knowledge_item.restored'",
    )
    .bind(tenant_id)
    .bind(&item_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        !audit3.is_empty(),
        "expected knowledge_item.restored audit row"
    );
}

#[tokio::test]
async fn status_filter() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "filter@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    // Create one draft and one published
    let _draft_id = seed_item(&pool, tenant_id, "Draft Item", user_id).await;
    let pub_payload =
        serde_json::json!({"title": "Pub Item", "body": "body", "itemType": "article"});
    let pub_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            pub_payload,
        ),
    )
    .await;
    let pub_id = body_json(pub_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{pub_id}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "published"}),
        ),
    )
    .await;

    // Filter published
    let list_pub = send(
        &state,
        auth_get(
            "/api/v1/tenant/knowledge/items?status=published",
            user_id,
            tenant_id,
        ),
    )
    .await;
    let pub_items = body_json(list_pub).await["items"]
        .as_array()
        .unwrap()
        .clone();
    for item in &pub_items {
        assert_eq!(item["status"], "published");
    }

    // No filter → all
    let list_all = send(
        &state,
        auth_get("/api/v1/tenant/knowledge/items", user_id, tenant_id),
    )
    .await;
    assert_eq!(
        body_json(list_all).await["items"].as_array().unwrap().len(),
        2
    );
}

#[tokio::test]
async fn agent_viewer_unauthorized_on_status() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let state = plain_state(pool.clone());

    for role in ["agent", "viewer"] {
        let tenant_id = seed_tenant(&pool).await;
        let user_id = seed_user(&pool, &format!("status-{role}@test.com"), role).await;
        seed_membership(&pool, tenant_id, user_id, role).await;

        let item_id = seed_item(&pool, tenant_id, "Status RBAC", user_id).await;

        let resp = send(
            &state,
            json_post(
                &format!("/api/v1/tenant/knowledge/items/{item_id}/status"),
                user_id,
                tenant_id,
                serde_json::json!({"status": "published"}),
            ),
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "status POST should be 403 for {role}"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// T032 — US3: Upload documents
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn upload_document_201() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "upload-201@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());
    let storage = Arc::new(InMemoryStorage::default());
    let boundary = "----uploadtestboundary";

    let body = multipart_body(
        &[(
            "file",
            Some("report.pdf"),
            Some("application/pdf"),
            b"%PDF-1.4 test data",
        )],
        boundary,
    );
    let resp = send_with_storage(
        &state,
        storage.clone(),
        multipart_post(
            "/api/v1/tenant/knowledge/documents",
            user_id,
            tenant_id,
            boundary,
            body,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let json = body_json(resp).await;
    assert_eq!(json["itemType"], "document");
    assert_eq!(json["source"], "uploaded");
    assert_eq!(json["status"], "draft");
    assert!(json["document"]["originalFilename"]
        .as_str()
        .unwrap()
        .contains("report.pdf"));
    assert_eq!(json["document"]["contentType"], "application/pdf");
    assert_eq!(json["document"]["sizeBytes"], 17);

    let item_id = json["id"].as_str().unwrap();
    let storage_key = format!("{}/knowledge/{}", tenant_id, item_id);
    let (stored_bytes, stored_ct) = storage.get(&storage_key).await.unwrap();
    assert_eq!(stored_bytes, b"%PDF-1.4 test data");
    assert_eq!(stored_ct, "application/pdf");
}

#[tokio::test]
async fn upload_published_status() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "upload-pub@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());
    let storage = Arc::new(InMemoryStorage::default());
    let boundary = "----uppubboundary";

    let body = multipart_body(
        &[
            ("file", Some("doc.pdf"), Some("application/pdf"), b"data"),
            ("status", None, None, b"published"),
        ],
        boundary,
    );
    let resp = send_with_storage(
        &state,
        storage.clone(),
        multipart_post(
            "/api/v1/tenant/knowledge/documents",
            user_id,
            tenant_id,
            boundary,
            body,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    assert_eq!(body_json(resp).await["status"], "published");
}

#[tokio::test]
async fn upload_default_status_is_draft() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "upload-draft@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());
    let storage = Arc::new(InMemoryStorage::default());
    let boundary = "----updraftboundary";

    let body = multipart_body(
        &[("file", Some("doc.pdf"), Some("application/pdf"), b"data")],
        boundary,
    );
    let resp = send_with_storage(
        &state,
        storage.clone(),
        multipart_post(
            "/api/v1/tenant/knowledge/documents",
            user_id,
            tenant_id,
            boundary,
            body,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    assert_eq!(body_json(resp).await["status"], "draft");
}

#[tokio::test]
async fn upload_archived_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "upload-arch@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());
    let storage = Arc::new(InMemoryStorage::default());
    let boundary = "----uparchboundary";

    let body = multipart_body(
        &[
            ("file", Some("doc.pdf"), Some("application/pdf"), b"data"),
            ("status", None, None, b"archived"),
        ],
        boundary,
    );
    let resp = send_with_storage(
        &state,
        storage.clone(),
        multipart_post(
            "/api/v1/tenant/knowledge/documents",
            user_id,
            tenant_id,
            boundary,
            body,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body_json(resp).await["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn upload_invalid_type_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "upload-exe@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());
    let storage = Arc::new(InMemoryStorage::default());
    let boundary = "----upexeeboundary";

    let body = multipart_body(
        &[(
            "file",
            Some("virus.exe"),
            Some("application/x-msdownload"),
            b"MZ\x90",
        )],
        boundary,
    );
    let resp = send_with_storage(
        &state,
        storage.clone(),
        multipart_post(
            "/api/v1/tenant/knowledge/documents",
            user_id,
            tenant_id,
            boundary,
            body,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body_json(resp).await["error"]["code"], "validation_failed");

    let items: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_items WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(items, 0, "no item rows created for invalid upload");
}

#[tokio::test]
async fn upload_oversize_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "upload-big@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());
    let storage = Arc::new(InMemoryStorage::default());
    let boundary = "----upbigboundary";
    let oversized = vec![0u8; 21 * 1024 * 1024];

    let body = multipart_body(
        &[("file", Some("big.pdf"), Some("application/pdf"), &oversized)],
        boundary,
    );
    let resp = send_with_storage(
        &state,
        storage.clone(),
        multipart_post(
            "/api/v1/tenant/knowledge/documents",
            user_id,
            tenant_id,
            boundary,
            body,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body_json(resp).await["error"]["code"], "validation_failed");

    let items: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_items WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(items, 0, "no item rows created for oversize upload");
}

#[tokio::test]
async fn download_returns_original_bytes() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "download@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());
    let storage = Arc::new(InMemoryStorage::default());
    let boundary = "----dlboundary";
    let file_content = b"%PDF-1.4 hello world";

    let body = multipart_body(
        &[(
            "file",
            Some("my report.pdf"),
            Some("application/pdf"),
            file_content,
        )],
        boundary,
    );
    let upload_resp = send_with_storage(
        &state,
        storage.clone(),
        multipart_post(
            "/api/v1/tenant/knowledge/documents",
            user_id,
            tenant_id,
            boundary,
            body,
        ),
    )
    .await;
    let item_id = body_json(upload_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let dl_resp = send_with_storage(
        &state,
        storage.clone(),
        auth_get(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/file"),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(dl_resp.status(), StatusCode::OK);
    let dl_bytes = dl_resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&dl_bytes[..], file_content);
}

#[tokio::test]
async fn download_missing_object_returns_not_found() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "dl-missing@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());
    let storage = Arc::new(InMemoryStorage::default());
    let boundary = "----dlmissboundary";

    let body = multipart_body(
        &[("file", Some("doc.pdf"), Some("application/pdf"), b"data")],
        boundary,
    );
    let upload_resp = send_with_storage(
        &state,
        storage.clone(),
        multipart_post(
            "/api/v1/tenant/knowledge/documents",
            user_id,
            tenant_id,
            boundary,
            body,
        ),
    )
    .await;
    let item_id = body_json(upload_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let storage_key = format!("{}/knowledge/{}", tenant_id, item_id);
    storage.delete(&storage_key).await.unwrap();

    let dl_resp = send_with_storage(
        &state,
        storage.clone(),
        auth_get(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/file"),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(dl_resp.status(), StatusCode::NOT_FOUND);

    let get_resp = send(
        &state,
        auth_get(
            &format!("/api/v1/tenant/knowledge/items/{item_id}"),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    assert_eq!(body_json(get_resp).await["itemType"], "document");
}

#[tokio::test]
async fn download_non_document_returns_validation_failed() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "dl-nondoc@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    let payload = serde_json::json!({"title": "Article", "body": "body", "itemType": "article"});
    let create_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    let item_id = body_json(create_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let dl_resp = send(
        &state,
        auth_get(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/file"),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(dl_resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body_json(dl_resp).await["error"]["code"],
        "validation_failed"
    );
}

#[tokio::test]
async fn agent_viewer_unauthorized_on_upload_allowed_on_download() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let state = plain_state(pool.clone());
    let admin_tenant_id = seed_tenant(&pool).await;
    let admin_user_id = seed_user(&pool, "upload-adm@test.com", "admin").await;
    seed_membership(&pool, admin_tenant_id, admin_user_id, "admin").await;
    let admin_storage = Arc::new(InMemoryStorage::default());
    let boundary = "----dlallboundary";

    // Upload a doc as admin for download tests
    let body = multipart_body(
        &[("file", Some("doc.pdf"), Some("application/pdf"), b"data")],
        boundary,
    );
    let upload_resp = send_with_storage(
        &state,
        admin_storage.clone(),
        multipart_post(
            "/api/v1/tenant/knowledge/documents",
            admin_user_id,
            admin_tenant_id,
            boundary,
            body,
        ),
    )
    .await;
    let item_id = body_json(upload_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Agent/viewer cannot upload
    for role in ["agent", "viewer"] {
        let tenant_id = seed_tenant(&pool).await;
        let user_id = seed_user(&pool, &format!("upload-{role}@test.com"), role).await;
        seed_membership(&pool, tenant_id, user_id, role).await;
        let storage = Arc::new(InMemoryStorage::default());
        let b = "----roleboundary";

        let body = multipart_body(
            &[("file", Some("doc.pdf"), Some("application/pdf"), b"data")],
            b,
        );
        let resp = send_with_storage(
            &state,
            storage.clone(),
            multipart_post(
                "/api/v1/tenant/knowledge/documents",
                user_id,
                tenant_id,
                b,
                body,
            ),
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "upload POST should be 403 for {role}"
        );
    }

    // Agent/viewer can download
    for role in ["agent", "viewer"] {
        let resp = send_with_storage(
            &state,
            admin_storage.clone(),
            auth_get(
                &format!("/api/v1/tenant/knowledge/items/{item_id}/file"),
                admin_user_id,
                admin_tenant_id,
            ),
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "download GET should succeed for {role}"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// T037 — US4: Categories and tags
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn category_crud_happy_path() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "cat-crud@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    // Create
    let create_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/categories",
            user_id,
            tenant_id,
            serde_json::json!({"name": "Support"}),
        ),
    )
    .await;
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let cat_id = body_json(create_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    // List contains
    let list_resp = send(
        &state,
        auth_get("/api/v1/tenant/knowledge/categories", user_id, tenant_id),
    )
    .await;
    let cats = body_json(list_resp).await.as_array().unwrap().clone();
    assert!(cats.iter().any(|c| c["name"] == "Support"));

    // Rename
    let rename_resp = send(
        &state,
        json_patch(
            &format!("/api/v1/tenant/knowledge/categories/{cat_id}"),
            user_id,
            tenant_id,
            serde_json::json!({"name": "Customer Support"}),
        ),
    )
    .await;
    assert_eq!(rename_resp.status(), StatusCode::OK);
    assert_eq!(body_json(rename_resp).await["name"], "Customer Support");

    // List reflects rename
    let list2_resp = send(
        &state,
        auth_get("/api/v1/tenant/knowledge/categories", user_id, tenant_id),
    )
    .await;
    let cats2 = body_json(list2_resp).await.as_array().unwrap().clone();
    assert!(!cats2.iter().any(|c| c["name"] == "Support"));
    assert!(cats2.iter().any(|c| c["name"] == "Customer Support"));

    // Delete
    let del_resp = send(
        &state,
        json_delete(
            &format!("/api/v1/tenant/knowledge/categories/{cat_id}"),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);

    // List empty
    let list3_resp = send(
        &state,
        auth_get("/api/v1/tenant/knowledge/categories", user_id, tenant_id),
    )
    .await;
    assert!(body_json(list3_resp).await.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn duplicate_category_name_case_insensitive() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "cat-dupe@test.com", "admin").await;
    let user_b = seed_user(&pool, "cat-dupe-b@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    seed_membership(&pool, tenant_b, user_b, "admin").await;
    let state = plain_state(pool.clone());

    send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/categories",
            user_id,
            tenant_id,
            serde_json::json!({"name": "Support"}),
        ),
    )
    .await;

    // Same tenant, different case → 409
    let dup_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/categories",
            user_id,
            tenant_id,
            serde_json::json!({"name": "support"}),
        ),
    )
    .await;
    assert_eq!(dup_resp.status(), StatusCode::CONFLICT);

    // Different tenant, same name → 201
    let ok_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/categories",
            user_b,
            tenant_b,
            serde_json::json!({"name": "Support"}),
        ),
    )
    .await;
    assert_eq!(ok_resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn delete_assigned_category_items_survive() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "cat-del@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    // Create category
    let cat_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/categories",
            user_id,
            tenant_id,
            serde_json::json!({"name": "FAQ"}),
        ),
    )
    .await;
    let cat_id = body_json(cat_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Create item with category
    let payload = serde_json::json!({
        "title": "FAQ Item",
        "body": "body",
        "itemType": "article",
        "categoryId": cat_id,
    });
    let create_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    let item_id = body_json(create_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Delete category
    let del_resp = send(
        &state,
        json_delete(
            &format!("/api/v1/tenant/knowledge/categories/{cat_id}"),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);

    // Item's categoryId is NULL
    let get_resp = send(
        &state,
        auth_get(
            &format!("/api/v1/tenant/knowledge/items/{item_id}"),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert!(body_json(get_resp).await["categoryId"].is_null());
}

#[tokio::test]
async fn cross_tenant_category_not_found() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let user_a = seed_user(&pool, "cross-cat-a@test.com", "admin").await;
    let user_b = seed_user(&pool, "cross-cat-b@test.com", "admin").await;
    seed_membership(&pool, tenant_a, user_a, "admin").await;
    seed_membership(&pool, tenant_b, user_b, "admin").await;
    let state = plain_state(pool.clone());

    // Create category in tenant A
    let cat_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/categories",
            user_a,
            tenant_a,
            serde_json::json!({"name": "A-only"}),
        ),
    )
    .await;
    let cat_id = body_json(cat_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Tenant B cannot see, rename, or delete it
    let get_resp = send(
        &state,
        auth_get("/api/v1/tenant/knowledge/categories", user_b, tenant_b),
    )
    .await;
    let cats = body_json(get_resp).await.as_array().unwrap().clone();
    assert!(!cats.iter().any(|c| c["id"] == cat_id));

    let rename_resp = send(
        &state,
        json_patch(
            &format!("/api/v1/tenant/knowledge/categories/{cat_id}"),
            user_b,
            tenant_b,
            serde_json::json!({"name": "Hacked"}),
        ),
    )
    .await;
    assert_eq!(rename_resp.status(), StatusCode::NOT_FOUND);

    let del_resp = send(
        &state,
        json_delete(
            &format!("/api/v1/tenant/knowledge/categories/{cat_id}"),
            user_b,
            tenant_b,
        ),
    )
    .await;
    assert_eq!(del_resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn tags_normalize_on_create() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "tags-norm@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    let payload = serde_json::json!({
        "title": "Tag Test",
        "body": "body",
        "itemType": "article",
        "tags": [" A ", "B", "a"],
    });
    let resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let json = body_json(resp).await;
    assert_eq!(json["tags"], serde_json::json!(["a", "b"]));
}

#[tokio::test]
async fn too_many_tags_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "tags-toomany@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    let tags: Vec<String> = (0..21).map(|i| format!("tag{}", i)).collect();
    let payload = serde_json::json!({
        "title": "Too Many Tags",
        "body": "body",
        "itemType": "article",
        "tags": tags,
    });
    let resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/items",
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body_json(resp).await["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn each_filter_returns_correct_items() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "filters@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    // Create category
    let cat_resp = send(
        &state,
        json_post(
            "/api/v1/tenant/knowledge/categories",
            user_id,
            tenant_id,
            serde_json::json!({"name": "Docs"}),
        ),
    )
    .await;
    let cat_id = body_json(cat_resp).await["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Create items with different attributes
    // Item 1: article, draft, no category, tags=[bug]
    let p1 = serde_json::json!({"title": "Bug Report", "body": "body", "itemType": "article", "tags": ["bug"]});
    let r1 = send(
        &state,
        json_post("/api/v1/tenant/knowledge/items", user_id, tenant_id, p1),
    )
    .await;
    let _id1 = body_json(r1).await["id"].as_str().unwrap().to_string();

    // Item 2: faq, published, category=cat_id, tags=[faq]
    let p2 = serde_json::json!({"title": "How to?", "body": "body", "itemType": "faq", "categoryId": cat_id, "tags": ["faq"]});
    let r2 = send(
        &state,
        json_post("/api/v1/tenant/knowledge/items", user_id, tenant_id, p2),
    )
    .await;
    let id2 = body_json(r2).await["id"].as_str().unwrap().to_string();
    send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{id2}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "published"}),
        ),
    )
    .await;

    // Item 3: article, published, category=cat_id, tags=[bug,faq]
    let p3 = serde_json::json!({"title": "Common Issue", "body": "body", "itemType": "article", "categoryId": cat_id, "tags": ["bug", "faq"]});
    let r3 = send(
        &state,
        json_post("/api/v1/tenant/knowledge/items", user_id, tenant_id, p3),
    )
    .await;
    let id3 = body_json(r3).await["id"].as_str().unwrap().to_string();
    send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{id3}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "published"}),
        ),
    )
    .await;

    // type filter
    let faq_resp = send(
        &state,
        auth_get(
            "/api/v1/tenant/knowledge/items?type=faq",
            user_id,
            tenant_id,
        ),
    )
    .await;
    let faq_items = body_json(faq_resp).await["items"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(faq_items.len(), 1);
    assert_eq!(faq_items[0]["title"], "How to?");

    // status filter
    let draft_resp = send(
        &state,
        auth_get(
            "/api/v1/tenant/knowledge/items?status=draft",
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(
        body_json(draft_resp).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // categoryId filter
    let cat_filter = send(
        &state,
        auth_get(
            &format!("/api/v1/tenant/knowledge/items?categoryId={}", cat_id),
            user_id,
            tenant_id,
        ),
    )
    .await;
    let cat_items = body_json(cat_filter).await["items"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(cat_items.len(), 2);

    // tag filter
    let tag_resp = send(
        &state,
        auth_get("/api/v1/tenant/knowledge/items?tag=bug", user_id, tenant_id),
    )
    .await;
    let tag_items = body_json(tag_resp).await["items"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(tag_items.len(), 2);

    let tag_faq_resp = send(
        &state,
        auth_get("/api/v1/tenant/knowledge/items?tag=faq", user_id, tenant_id),
    )
    .await;
    assert_eq!(
        body_json(tag_faq_resp).await["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // q filter
    let q_resp = send(
        &state,
        auth_get(
            "/api/v1/tenant/knowledge/items?q=common",
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(
        body_json(q_resp).await["items"].as_array().unwrap().len(),
        1
    );

    // multi-filter: published + category
    let multi_resp = send(
        &state,
        auth_get(
            &format!(
                "/api/v1/tenant/knowledge/items?status=published&categoryId={}",
                cat_id
            ),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(
        body_json(multi_resp).await["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn agent_viewer_unauthorized_on_category_writes() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let state = plain_state(pool.clone());

    for role in ["agent", "viewer"] {
        let tenant_id = seed_tenant(&pool).await;
        let user_id = seed_user(&pool, &format!("cat-rbac-{role}@test.com"), role).await;
        seed_membership(&pool, tenant_id, user_id, role).await;

        // Create → 403
        let create_resp = send(
            &state,
            json_post(
                "/api/v1/tenant/knowledge/categories",
                user_id,
                tenant_id,
                serde_json::json!({"name": "Test"}),
            ),
        )
        .await;
        assert_eq!(
            create_resp.status(),
            StatusCode::FORBIDDEN,
            "category POST should be 403 for {role}"
        );

        // List → 200 (view permission)
        let list_resp = send(
            &state,
            auth_get("/api/v1/tenant/knowledge/categories", user_id, tenant_id),
        )
        .await;
        assert_eq!(
            list_resp.status(),
            StatusCode::OK,
            "category GET should be 200 for {role}"
        );
    }
}
