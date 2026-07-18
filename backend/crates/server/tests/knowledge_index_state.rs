use std::str::FromStr;
use std::time::Duration;

use knowledge::index_state::IndexStatus;
use sqlx::PgPool;
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
            eprintln!("skipping knowledge index_state tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping knowledge index_state tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query(
        "TRUNCATE TABLE knowledge_index_state, knowledge_chunks, knowledge_item_tags, \
         knowledge_documents, knowledge_items, knowledge_categories, \
         audit_logs, outbox_events, tenant_invitations, tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("failed to reset test tables");
}

async fn seed_tenant(pool: &PgPool) -> Uuid {
    let slug = format!("idx-tenant-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Index State Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_item(pool: &PgPool, tenant_id: Uuid) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO knowledge_items (tenant_id, item_type, title, body, source, created_by_display) \
         VALUES ($1, 'article', 'Index State Test', 'body', 'authored', 'Test User') RETURNING id",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ═══════════════════════════════════════════════════════════════════════════════
// IndexStatus conversions (no DB needed)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn index_status_display_lowercase() {
    assert_eq!(IndexStatus::NotIndexed.to_string(), "not_indexed");
    assert_eq!(IndexStatus::Pending.to_string(), "pending");
    assert_eq!(IndexStatus::Indexing.to_string(), "indexing");
    assert_eq!(IndexStatus::Indexed.to_string(), "indexed");
    assert_eq!(IndexStatus::Failed.to_string(), "failed");
    assert_eq!(IndexStatus::NotIndexable.to_string(), "not_indexable");
}

#[test]
fn index_status_from_str_valid() {
    assert_eq!(
        IndexStatus::from_str("not_indexed").unwrap(),
        IndexStatus::NotIndexed
    );
    assert_eq!(
        IndexStatus::from_str("pending").unwrap(),
        IndexStatus::Pending
    );
    assert_eq!(
        IndexStatus::from_str("indexing").unwrap(),
        IndexStatus::Indexing
    );
    assert_eq!(
        IndexStatus::from_str("indexed").unwrap(),
        IndexStatus::Indexed
    );
    assert_eq!(
        IndexStatus::from_str("failed").unwrap(),
        IndexStatus::Failed
    );
    assert_eq!(
        IndexStatus::from_str("not_indexable").unwrap(),
        IndexStatus::NotIndexable
    );
}

#[test]
fn index_status_from_str_invalid() {
    assert!(IndexStatus::from_str("unknown").is_err());
    assert!(IndexStatus::from_str("").is_err());
    assert!(IndexStatus::from_str("INDEXED").is_err());
}

// ═══════════════════════════════════════════════════════════════════════════════
// Database-backed tests
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn get_returns_none_when_no_row() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let _item_id = seed_item(&pool, tenant_id).await;
    let other_item_id = Uuid::new_v4();

    let result = knowledge::index_state::get(&pool, tenant_id, other_item_id)
        .await
        .unwrap();
    assert!(
        result.is_none(),
        "expected None for non-existent index state"
    );
}

#[tokio::test]
async fn get_returns_row_after_upsert() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let item_id = seed_item(&pool, tenant_id).await;

    // insert directly so get can find it
    sqlx::query(
        "INSERT INTO knowledge_index_state (item_id, tenant_id, status) VALUES ($1, $2, 'pending')",
    )
    .bind(item_id)
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    let result = knowledge::index_state::get(&pool, tenant_id, item_id)
        .await
        .unwrap();
    assert!(result.is_some(), "expected Some after upsert");
    let state = result.unwrap();
    assert_eq!(state.item_id, item_id);
    assert_eq!(state.tenant_id, tenant_id);
    assert_eq!(state.status, IndexStatus::Pending.to_string());
    assert_eq!(state.attempts, 0);
    assert_eq!(state.chunk_count, 0);
    assert!(state.failure_reason.is_none());
    assert!(state.indexed_content_hash.is_none());
    assert!(state.last_indexed_at.is_none());
}

#[tokio::test]
async fn get_tenant_scoped() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let item_a = seed_item(&pool, tenant_a).await;

    sqlx::query(
        "INSERT INTO knowledge_index_state (item_id, tenant_id, status) VALUES ($1, $2, 'indexed')",
    )
    .bind(item_a)
    .bind(tenant_a)
    .execute(&pool)
    .await
    .unwrap();

    // same item_id different tenant → should not be found
    let result = knowledge::index_state::get(&pool, tenant_b, item_a)
        .await
        .unwrap();
    assert!(result.is_none(), "must be tenant-scoped");
}

#[tokio::test]
async fn upsert_status_creates_new_row() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let item_id = seed_item(&pool, tenant_id).await;

    knowledge::index_state::upsert_status(&pool, tenant_id, item_id, &IndexStatus::Pending)
        .await
        .unwrap();

    let row: (String,) =
        sqlx::query_as("SELECT status FROM knowledge_index_state WHERE item_id = $1")
            .bind(item_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, "pending");
}

#[tokio::test]
async fn upsert_status_updates_existing_row() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let item_id = seed_item(&pool, tenant_id).await;

    knowledge::index_state::upsert_status(&pool, tenant_id, item_id, &IndexStatus::Pending)
        .await
        .unwrap();
    knowledge::index_state::upsert_status(&pool, tenant_id, item_id, &IndexStatus::Indexing)
        .await
        .unwrap();

    let row: (String,) =
        sqlx::query_as("SELECT status FROM knowledge_index_state WHERE item_id = $1")
            .bind(item_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, "indexing");
}

#[tokio::test]
async fn set_failed_sets_status_and_reason() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let item_id = seed_item(&pool, tenant_id).await;

    knowledge::index_state::upsert_status(&pool, tenant_id, item_id, &IndexStatus::Pending)
        .await
        .unwrap();
    knowledge::index_state::set_failed(&pool, item_id, "embedding failed: timeout")
        .await
        .unwrap();

    let result = knowledge::index_state::get(&pool, tenant_id, item_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.status, IndexStatus::Failed.to_string());
    assert_eq!(
        result.failure_reason.as_deref(),
        Some("embedding failed: timeout")
    );
}

#[tokio::test]
async fn set_indexed_sets_status_hash_and_chunk_count() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let item_id = seed_item(&pool, tenant_id).await;

    knowledge::index_state::upsert_status(&pool, tenant_id, item_id, &IndexStatus::Pending)
        .await
        .unwrap();
    knowledge::index_state::set_indexed(&pool, item_id, "abc123hash", 5)
        .await
        .unwrap();

    let result = knowledge::index_state::get(&pool, tenant_id, item_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.status, IndexStatus::Indexed.to_string());
    assert_eq!(result.indexed_content_hash.as_deref(), Some("abc123hash"));
    assert_eq!(result.chunk_count, 5);
    assert!(result.last_indexed_at.is_some());
}

#[tokio::test]
async fn increment_attempts_returns_new_count() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let item_id = seed_item(&pool, tenant_id).await;

    knowledge::index_state::upsert_status(&pool, tenant_id, item_id, &IndexStatus::Pending)
        .await
        .unwrap();

    let count1 = knowledge::index_state::increment_attempts(&pool, item_id)
        .await
        .unwrap();
    assert_eq!(count1, 1);

    let count2 = knowledge::index_state::increment_attempts(&pool, item_id)
        .await
        .unwrap();
    assert_eq!(count2, 2);
}

#[tokio::test]
async fn set_not_indexable_sets_status_and_reason() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let item_id = seed_item(&pool, tenant_id).await;

    knowledge::index_state::upsert_status(&pool, tenant_id, item_id, &IndexStatus::Pending)
        .await
        .unwrap();
    knowledge::index_state::set_not_indexable(&pool, item_id, "no extractable text")
        .await
        .unwrap();

    let result = knowledge::index_state::get(&pool, tenant_id, item_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.status, IndexStatus::NotIndexable.to_string());
    assert_eq!(
        result.failure_reason.as_deref(),
        Some("no extractable text")
    );
}
