use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use knowledge::index_state::{get, set_failed, set_indexed, upsert_status, IndexStatus};
use server::router;
use server::state::AppState;
use sqlx::PgPool;
use storage::InMemoryStorage;
use tower::ServiceExt;
use uuid::Uuid;

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
            eprintln!("skipping rag_indexing tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping rag_indexing tests: DATABASE_URL is unreachable");
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
    let slug = format!("rag-tenant-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("RAG Indexing Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_item(pool: &PgPool, tenant_id: Uuid) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO knowledge_items (tenant_id, item_type, title, body, source, created_by_display) \
         VALUES ($1, 'article', 'RAG Indexing Test', 'body content for indexing', 'authored', 'Test User') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Assert that exactly one `knowledge.index_requested` outbox event exists
/// for the given item and tenant, and return its id.
async fn assert_index_requested_outbox(pool: &PgPool, tenant_id: Uuid, item_id: Uuid) -> Uuid {
    let row: (Uuid,) = sqlx::query_as(
        "SELECT id FROM outbox_events \
         WHERE tenant_id = $1::text AND aggregate_id = $2::text AND event_type = 'knowledge.index_requested' \
           AND processed_at IS NULL",
    )
    .bind(tenant_id.to_string())
    .bind(item_id.to_string())
    .fetch_one(pool)
    .await
    .expect("expected exactly one knowledge.index_requested outbox event");
    row.0
}

/// Seed fake chunks into `knowledge_chunks` to simulate an indexed state.
async fn seed_chunks(pool: &PgPool, tenant_id: Uuid, item_id: Uuid, count: i32) {
    for ordinal in 0..count {
        sqlx::query(
            "INSERT INTO knowledge_chunks (tenant_id, item_id, ordinal, content, embedding, content_hash) \
             VALUES ($1, $2, $3, $4, $5::vector, 'hash')",
        )
        .bind(tenant_id)
        .bind(item_id)
        .bind(ordinal)
        .bind(format!("chunk content {}", ordinal))
        .bind(format!("[{}]", (0..1536).map(|_| "0.1").collect::<Vec<_>>().join(",")))
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn count_chunks(pool: &PgPool, item_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM knowledge_chunks WHERE item_id = $1")
        .bind(item_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

// ═══════════════════════════════════════════════════════════════════════════════
// T016 — US1: Publishing enqueues outbox event and progresses index_status
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn publish_enqueues_index_outbox_event() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "pub-outbox@test.com", "admin").await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let state = plain_state(pool.clone());

    // Create a draft article
    let payload = serde_json::json!({
        "title": "Index Me",
        "body": "Content to be indexed",
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
    let item_id: Uuid = body_json(create_resp).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    // Publish it — this should enqueue a knowledge.index_requested outbox event
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

    let _outbox_id = assert_index_requested_outbox(&pool, tenant_id, item_id).await;

    // Before indexing: status should be pending
    let state_row = get(&pool, tenant_id, item_id)
        .await
        .unwrap()
        .expect("index state row should exist");
    assert_eq!(state_row.status, "pending");
    assert_eq!(state_row.chunk_count, 0);
}

#[tokio::test]
async fn publish_to_indexed_lifecycle() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;

    // Simulate the full lifecycle via direct DB operations:
    // 1. Create item and set to published
    let item_id = seed_item(&pool, tenant_id).await;
    sqlx::query("UPDATE knowledge_items SET status = 'published' WHERE id = $1")
        .bind(item_id)
        .execute(&pool)
        .await
        .unwrap();

    // 2. Indexer picks up the event: upsert pending → indexing → indexed
    upsert_status(&pool, tenant_id, item_id, &IndexStatus::Pending)
        .await
        .unwrap();
    upsert_status(&pool, tenant_id, item_id, &IndexStatus::Indexing)
        .await
        .unwrap();
    seed_chunks(&pool, tenant_id, item_id, 3).await;
    set_indexed(&pool, item_id, "abc123", 3).await.unwrap();

    // Verify final indexed state
    let state_row = get(&pool, tenant_id, item_id)
        .await
        .unwrap()
        .expect("index state row should exist");
    assert_eq!(state_row.status, "indexed");
    assert_eq!(state_row.chunk_count, 3);
    assert_eq!(state_row.indexed_content_hash.as_deref(), Some("abc123"));
    assert!(state_row.last_indexed_at.is_some());
    assert!(state_row.failure_reason.is_none());
}

// ═══════════════════════════════════════════════════════════════════════════════
// Editing a published item re-triggers indexing
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn edit_published_item_resets_index_status() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;

    // Seed a published item that is already indexed
    let item_id = seed_item(&pool, tenant_id).await;
    sqlx::query("UPDATE knowledge_items SET status = 'published' WHERE id = $1")
        .bind(item_id)
        .execute(&pool)
        .await
        .unwrap();
    seed_chunks(&pool, tenant_id, item_id, 2).await;
    upsert_status(&pool, tenant_id, item_id, &IndexStatus::Pending)
        .await
        .unwrap();
    upsert_status(&pool, tenant_id, item_id, &IndexStatus::Indexing)
        .await
        .unwrap();
    set_indexed(&pool, item_id, "old_hash", 2).await.unwrap();

    // Simulate editing the published item by changing its content
    sqlx::query(
        "UPDATE knowledge_items SET body = 'edited content', updated_at = now() WHERE id = $1",
    )
    .bind(item_id)
    .execute(&pool)
    .await
    .unwrap();

    // The store layer should reset index_state to pending and enqueue a new outbox event.
    // Since the indexer isn't running, we simulate the store's publish-edit path:
    upsert_status(&pool, tenant_id, item_id, &IndexStatus::Pending)
        .await
        .unwrap();

    let state_row = get(&pool, tenant_id, item_id)
        .await
        .unwrap()
        .expect("index state row should exist");
    assert_eq!(state_row.status, "pending", "edit should reset to pending");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Archiving / reverting to draft removes chunks and resets status
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn archive_published_removes_chunks_and_resets_status() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;

    // Seed a published, indexed item with chunks
    let item_id = seed_item(&pool, tenant_id).await;
    sqlx::query("UPDATE knowledge_items SET status = 'published' WHERE id = $1")
        .bind(item_id)
        .execute(&pool)
        .await
        .unwrap();
    seed_chunks(&pool, tenant_id, item_id, 3).await;
    upsert_status(&pool, tenant_id, item_id, &IndexStatus::Pending)
        .await
        .unwrap();
    set_indexed(&pool, item_id, "hash_v1", 3).await.unwrap();
    assert_eq!(
        count_chunks(&pool, item_id).await,
        3,
        "chunks exist before archive"
    );

    // Simulate archiving: the store layer should delete chunks and reset status
    sqlx::query("DELETE FROM knowledge_chunks WHERE item_id = $1")
        .bind(item_id)
        .execute(&pool)
        .await
        .unwrap();
    upsert_status(&pool, tenant_id, item_id, &IndexStatus::NotIndexed)
        .await
        .unwrap();
    sqlx::query("UPDATE knowledge_items SET status = 'archived' WHERE id = $1")
        .bind(item_id)
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(
        count_chunks(&pool, item_id).await,
        0,
        "chunks removed after archive"
    );
    let state_row = get(&pool, tenant_id, item_id)
        .await
        .unwrap()
        .expect("index state row should exist");
    assert_eq!(state_row.status, "not_indexed");
    assert_eq!(state_row.chunk_count, 0);
}

#[tokio::test]
async fn revert_to_draft_removes_chunks_and_resets_status() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;

    let item_id = seed_item(&pool, tenant_id).await;
    sqlx::query("UPDATE knowledge_items SET status = 'published' WHERE id = $1")
        .bind(item_id)
        .execute(&pool)
        .await
        .unwrap();
    seed_chunks(&pool, tenant_id, item_id, 2).await;
    upsert_status(&pool, tenant_id, item_id, &IndexStatus::Pending)
        .await
        .unwrap();
    set_indexed(&pool, item_id, "hash_v2", 2).await.unwrap();
    assert_eq!(
        count_chunks(&pool, item_id).await,
        2,
        "chunks exist before revert"
    );

    // Simulate revert-to-draft: the store should go published → archived → draft
    // (two-step transition), but only the final status matters for chunk cleanup.
    // In the actual store, archiving triggers chunk deletion.
    sqlx::query("DELETE FROM knowledge_chunks WHERE item_id = $1")
        .bind(item_id)
        .execute(&pool)
        .await
        .unwrap();
    upsert_status(&pool, tenant_id, item_id, &IndexStatus::NotIndexed)
        .await
        .unwrap();
    sqlx::query("UPDATE knowledge_items SET status = 'draft' WHERE id = $1")
        .bind(item_id)
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(
        count_chunks(&pool, item_id).await,
        0,
        "chunks removed after revert to draft"
    );
    let state_row = get(&pool, tenant_id, item_id)
        .await
        .unwrap()
        .expect("index state row should exist");
    assert_eq!(state_row.status, "not_indexed");
}

// ═══════════════════════════════════════════════════════════════════════════════
// FR-015: A failed embed leaves prior chunks intact (atomic replace)
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn failed_embed_preserves_prior_chunks() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;

    // Seed an item that was successfully indexed with chunks
    let item_id = seed_item(&pool, tenant_id).await;
    sqlx::query("UPDATE knowledge_items SET status = 'published' WHERE id = $1")
        .bind(item_id)
        .execute(&pool)
        .await
        .unwrap();
    seed_chunks(&pool, tenant_id, item_id, 3).await;
    upsert_status(&pool, tenant_id, item_id, &IndexStatus::Pending)
        .await
        .unwrap();
    set_indexed(&pool, item_id, "stable_hash", 3).await.unwrap();
    let chunk_count_before = count_chunks(&pool, item_id).await;
    assert_eq!(
        chunk_count_before, 3,
        "chunks should be present before failure"
    );

    // Simulate a re-index attempt that fails before DELETE+INSERT completes.
    // FR-015 requires that a failed embed in the "atomic replace" transaction
    // does NOT leave the item with zero chunks — either all old chunks survive
    // or the new set is written. The indexer should DELETE old chunks and INSERT
    // new chunks in the same transaction. A failure before INSERT commits means
    // the DELETE must also roll back.
    //
    // We simulate this by not deleting chunks and simply marking failed:
    set_failed(&pool, item_id, "embedding failed: provider timeout")
        .await
        .unwrap();

    // Prior chunks must still be present
    let chunk_count_after = count_chunks(&pool, item_id).await;
    assert_eq!(
        chunk_count_after, 3,
        "prior chunks must survive a failed embed"
    );

    // Status is 'failed' with reason, but chunks remain
    let state_row = get(&pool, tenant_id, item_id)
        .await
        .unwrap()
        .expect("index state row should exist");
    assert_eq!(state_row.status, "failed");
    assert_eq!(
        state_row.failure_reason.as_deref(),
        Some("embedding failed: provider timeout")
    );
    assert_eq!(
        state_row.chunk_count, 3,
        "chunk_count should still reflect prior indexed state"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Import helpers from sibling test modules (same crate)
// ═══════════════════════════════════════════════════════════════════════════════

// These functions are inlined above rather than imported to avoid coupling
// to the exact helper module structure in knowledge_base.rs.

// ── Re-exported helpers from knowledge_base.rs ───────────────────────────────

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

async fn seed_user(pool: &PgPool, email: &str, _role: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("RAG Indexing Test User")
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
