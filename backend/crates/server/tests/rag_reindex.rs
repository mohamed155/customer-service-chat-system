use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use server::router;
use server::state::AppState;
use sqlx::PgPool;
use storage::{InMemoryStorage, ObjectStorage};
use tower::ServiceExt;
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers — same patterns as knowledge_base.rs / rag_indexing.rs
// ═══════════════════════════════════════════════════════════════════════════════

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            eprintln!("skipping rag_reindex tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping rag_reindex tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query(
        "TRUNCATE TABLE knowledge_chunks, knowledge_index_state, knowledge_item_tags, \
         knowledge_documents, knowledge_items, knowledge_categories, \
         audit_logs, outbox_events, tenant_invitations, tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("failed to reset test tables");
}

async fn seed_tenant(pool: &PgPool) -> Uuid {
    let slug = format!("reindex-tenant-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Reindex Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &PgPool, email: &str, _role: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Reindex Test User")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_membership(pool: &PgPool, tenant_id: Uuid, user_id: Uuid, role: &str) {
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(tenant_id)
        .bind(user_id)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
}

async fn seed_item(pool: &PgPool, tenant_id: Uuid, user_id: Uuid) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO knowledge_items (tenant_id, item_type, title, body, source, created_by_user_id, created_by_display) \
         VALUES ($1, 'article', 'Reindex Test Item', 'body content', 'authored', $2, 'Test User') RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn test_config() -> config::AppConfig {
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

fn plain_state(pool: PgPool) -> AppState {
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

/// Count unprocessed `knowledge.index_requested` outbox events for an item.
async fn count_pending_outbox_events(pool: &PgPool, item_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM outbox_events \
         WHERE aggregate_id = $1::text \
           AND event_type = 'knowledge.index_requested' \
           AND processed_at IS NULL",
    )
    .bind(item_id.to_string())
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Set an item's status directly in the database.
async fn set_item_status(pool: &PgPool, item_id: Uuid, status: &str) {
    sqlx::query("UPDATE knowledge_items SET status = $1 WHERE id = $2")
        .bind(status)
        .bind(item_id)
        .execute(pool)
        .await
        .unwrap();
}

/// Seed a document item + document record + storage content for T040.
async fn seed_document_with_storage(
    pool: &PgPool,
    storage: &InMemoryStorage,
    tenant_id: Uuid,
    title: &str,
    content_type: &str,
    file_content: &[u8],
) -> Uuid {
    let item_id = Uuid::new_v4();
    let storage_key = format!("{}/knowledge/{}", tenant_id, item_id);

    // Insert the knowledge item
    sqlx::query(
        "INSERT INTO knowledge_items (id, tenant_id, item_type, title, body, status, source, created_by_display) \
         VALUES ($1, $2, 'document', $3, NULL, 'published', 'uploaded', 'Test User')",
    )
    .bind(item_id)
    .bind(tenant_id)
    .bind(title)
    .execute(pool)
    .await
    .unwrap();

    // Insert the document record
    sqlx::query(
        "INSERT INTO knowledge_documents (item_id, tenant_id, storage_key, original_filename, content_type, size_bytes) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(item_id)
    .bind(tenant_id)
    .bind(&storage_key)
    .bind(title)
    .bind(content_type)
    .bind(file_content.len() as i64)
    .execute(pool)
    .await
    .unwrap();

    // Store the file content in in-memory storage
    storage
        .put(&storage_key, content_type, file_content.to_vec())
        .await
        .unwrap();

    item_id
}

// ═══════════════════════════════════════════════════════════════════════════════
// T038 — Reindex API test
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn reindex_published_item_returns_202() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "reindex-202@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    // Publish an item for the admin
    let payload = serde_json::json!({
        "title": "Reindex Test",
        "body": "<p>ready to reindex</p>",
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

    // Publish
    let publish_resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/status"),
            user_id,
            tenant_id,
            serde_json::json!({"status": "published"}),
        ),
    )
    .await;
    assert_eq!(publish_resp.status(), StatusCode::OK);

    // Reindex
    let reindex_resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/reindex"),
            user_id,
            tenant_id,
            serde_json::json!({}),
        ),
    )
    .await;

    assert_eq!(
        reindex_resp.status(),
        StatusCode::ACCEPTED,
        "reindex for published item should return 202"
    );
    let json = body_json(reindex_resp).await;
    assert_eq!(
        json["data"]["index_status"]["status"], "pending",
        "status should reset to pending"
    );
}

#[tokio::test]
async fn reindex_draft_returns_409() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "reindex-draft@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    // Create a draft item (default status is draft)
    let payload = serde_json::json!({
        "title": "Draft Item",
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

    let resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/reindex"),
            user_id,
            tenant_id,
            serde_json::json!({}),
        ),
    )
    .await;

    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "reindex for draft should return 409"
    );
    let json = body_json(resp).await;
    assert_eq!(
        json["error"]["code"], "not_publishable",
        "draft reindex should be rejected with not_publishable"
    );
}

#[tokio::test]
async fn reindex_archived_returns_409() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "reindex-archived@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    // Create, publish, then archive
    let payload = serde_json::json!({
        "title": "Archive Test",
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

    let resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/reindex"),
            user_id,
            tenant_id,
            serde_json::json!({}),
        ),
    )
    .await;

    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "reindex for archived should return 409"
    );
    let json = body_json(resp).await;
    assert_eq!(
        json["error"]["code"], "not_publishable",
        "archived reindex should be rejected with not_publishable"
    );
}

#[tokio::test]
async fn reindex_agent_viewer_returns_403() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let state = plain_state(pool.clone());

    for role in ["agent", "viewer"] {
        let tenant_id = seed_tenant(&pool).await;
        let user_id = seed_user(&pool, &format!("reindex-{role}@test.com"), role).await;
        seed_membership(&pool, tenant_id, user_id, role).await;

        // Admin publishes an item first
        let admin_id = seed_user(&pool, &format!("reindex-admin-{role}@test.com"), "admin").await;
        seed_membership(&pool, tenant_id, admin_id, "admin").await;
        let item_id = seed_item(&pool, tenant_id, admin_id).await;
        set_item_status(&pool, item_id, "published").await;

        let resp = send(
            &state,
            json_post(
                &format!("/api/v1/tenant/knowledge/items/{item_id}/reindex"),
                user_id,
                tenant_id,
                serde_json::json!({}),
            ),
        )
        .await;

        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "reindex should be 403 for {role}"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// T039 — Idempotent reindex test
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn reindex_while_pending_is_noop() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "reindex-noop@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    // Publish an item — this enqueues a single outbox event and sets status to pending
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

    // Record the baseline outbox event count (one from publishing)
    let events_before =
        count_pending_outbox_events(&pool, Uuid::parse_str(&item_id).unwrap()).await;
    assert_eq!(events_before, 1, "publishing should enqueue one event");

    // Reindex while already pending
    let resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/reindex"),
            user_id,
            tenant_id,
            serde_json::json!({}),
        ),
    )
    .await;

    assert_eq!(
        resp.status(),
        StatusCode::ACCEPTED,
        "reindex while pending returns 202"
    );

    // Verify no duplicate outbox event was enqueued
    let events_after = count_pending_outbox_events(&pool, Uuid::parse_str(&item_id).unwrap()).await;
    assert_eq!(
        events_after, events_before,
        "reindex while pending must not enqueue a duplicate outbox event"
    );
}

#[tokio::test]
async fn reindex_while_indexing_is_noop() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "reindex-indexing@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;
    let state = plain_state(pool.clone());

    // Create a published item
    let item_id = seed_item(&pool, tenant_id, user_id).await;
    set_item_status(&pool, item_id, "published").await;

    // Simulate indexing in progress: mark as indexing
    let item_uuid = item_id;
    knowledge::index_state::upsert_status(
        &pool,
        tenant_id,
        item_uuid,
        &knowledge::index_state::IndexStatus::Indexing,
    )
    .await
    .unwrap();

    // Manually enqueue an outbox event to simulate the one from publishing
    let mut tx = pool.begin().await.unwrap();
    knowledge::store::enqueue_index_requested_in_tx(&mut tx, tenant_id, item_uuid)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let events_before = count_pending_outbox_events(&pool, item_uuid).await;
    assert_eq!(events_before, 1, "expected one outbox event before reindex");

    // Reindex while indexing — should be a no-op
    let resp = send(
        &state,
        json_post(
            &format!("/api/v1/tenant/knowledge/items/{item_id}/reindex"),
            user_id,
            tenant_id,
            serde_json::json!({}),
        ),
    )
    .await;

    assert_eq!(
        resp.status(),
        StatusCode::ACCEPTED,
        "reindex while indexing returns 202"
    );

    // Verify no duplicate outbox event
    let events_after = count_pending_outbox_events(&pool, item_uuid).await;
    assert_eq!(
        events_after, events_before,
        "reindex while indexing must not enqueue a duplicate outbox event"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// T040 — Not-indexable test (document with no extractable text)
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn document_no_extractable_text_resolves_to_not_indexable() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let storage = Arc::new(InMemoryStorage::default());

    // Seed a published document item whose stored file has no extractable text
    // (empty bytes with text/plain content type)
    let item_id = seed_document_with_storage(
        &pool,
        &storage,
        tenant_id,
        "no-text-doc.pdf",
        "application/pdf",
        b"%PDF-1.4 empty file with no text layer",
    )
    .await;

    // Simulate the indexer's processing:

    // 1. Fetch the document record
    let doc = knowledge::store::get_document(&pool, tenant_id, item_id)
        .await
        .unwrap()
        .expect("document record should exist");

    // 2. Fetch the stored file
    let (body_bytes, content_type) = storage
        .get(&doc.storage_key)
        .await
        .expect("stored file should exist");

    // 3. Try to extract text — should return None for a PDF with no text
    let extracted = knowledge::chunking::extract_text(&content_type, &body_bytes);
    assert!(
        extracted.is_none(),
        "extract_text should return None for content with no extractable text"
    );

    // 4. The indexer calls set_not_indexable with a descriptive reason
    let reason = format!("No extractable text from content type: {content_type}");
    knowledge::index_state::set_not_indexable(&pool, item_id, &reason)
        .await
        .unwrap();

    // 5. Verify final state is not_indexable with a reason
    let result = knowledge::index_state::get(&pool, tenant_id, item_id)
        .await
        .unwrap()
        .expect("index state row should exist");

    assert_eq!(
        result.status, "not_indexable",
        "status should be not_indexable, not an error"
    );
    let reason_text = result.failure_reason.clone().unwrap();
    assert!(
        reason_text.contains("No extractable text"),
        "reason should explain why: got {reason_text}"
    );
    assert_eq!(
        result.chunk_count, 0,
        "chunk_count should be 0 for not_indexable"
    );
}

#[tokio::test]
async fn authored_article_empty_body_resolves_to_not_indexable() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;

    // Create a published article with only whitespace body
    let item_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO knowledge_items (id, tenant_id, item_type, title, body, status, source, created_by_display) \
         VALUES ($1, $2, 'article', 'Empty body', '   ', 'published', 'authored', 'Test User')",
    )
    .bind(item_id)
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    // Simulate the indexer chunking step:
    // authored articles combine title + body
    let source_text = format!("{}\n\n{}", "Empty body", "   ");
    let chunk_result = knowledge::chunking::chunk_text(&source_text);

    assert!(
        chunk_result.not_indexable,
        "chunk_text should signal not_indexable for whitespace-only content"
    );
    assert!(
        chunk_result.chunks.is_empty(),
        "no chunks should be produced"
    );

    // The indexer calls set_not_indexable
    knowledge::index_state::set_not_indexable(&pool, item_id, "No extractable text content")
        .await
        .unwrap();

    let result = knowledge::index_state::get(&pool, tenant_id, item_id)
        .await
        .unwrap()
        .expect("index state row should exist");

    assert_eq!(
        result.status, "not_indexable",
        "status should be not_indexable"
    );
    assert_eq!(
        result.failure_reason.as_deref(),
        Some("No extractable text content"),
        "reason should match the indexer's message"
    );
}
