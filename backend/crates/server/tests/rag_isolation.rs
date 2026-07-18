use std::time::Duration;

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
            eprintln!("skipping rag_isolation tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping rag_isolation tests: DATABASE_URL is unreachable");
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
    let slug = format!("rag-iso-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("RAG Isolation Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Build a 1536-dimension pgvector literal filled with a constant value.
fn make_vec(val: f32) -> String {
    let parts: Vec<String> = (0..1536).map(|_| val.to_string()).collect();
    format!("[{}]", parts.join(","))
}

/// Insert a knowledge_item with given status and one chunk, returning (item_id, chunk_id).
async fn seed_item_with_chunk(
    pool: &PgPool,
    tenant_id: Uuid,
    status: &str,
    title: &str,
    body: &str,
    chunk_content: &str,
    vec_val: f32,
) -> (Uuid, Uuid) {
    let item_id: Uuid = sqlx::query_scalar(
        "INSERT INTO knowledge_items (tenant_id, item_type, title, body, status, source, \
         created_by_display) \
         VALUES ($1, 'article', $2, $3, $4, 'authored', 'Test User') RETURNING id",
    )
    .bind(tenant_id)
    .bind(title)
    .bind(body)
    .bind(status)
    .fetch_one(pool)
    .await
    .unwrap();

    let chunk_id: Uuid = sqlx::query_scalar(
        "INSERT INTO knowledge_chunks (tenant_id, item_id, ordinal, content, embedding, content_hash) \
         VALUES ($1, $2, 0, $3, $4::vector, 'hash001') RETURNING id",
    )
    .bind(tenant_id)
    .bind(item_id)
    .bind(chunk_content)
    .bind(make_vec(vec_val))
    .fetch_one(pool)
    .await
    .unwrap();

    (item_id, chunk_id)
}

/// Execute the same retrieval query pattern that `knowledge::retrieval::search` uses,
/// returning (chunk_id, item_id, tenant_id, similarity).
async fn retrieve_for_tenant(
    pool: &PgPool,
    tenant_id: Uuid,
    query_vec_val: f32,
    threshold: f32,
    limit: i32,
) -> Vec<(Uuid, Uuid, Uuid, f64)> {
    let v = make_vec(query_vec_val);
    sqlx::query_as::<_, (Uuid, Uuid, Uuid, f64)>(&format!(
        "SELECT kc.id, kc.item_id, kc.tenant_id, \
             (1 - (kc.embedding <=> '[{v}]'::vector)) AS similarity \
             FROM knowledge_chunks kc \
             JOIN knowledge_items ki \
               ON ki.id = kc.item_id \
              AND ki.tenant_id = kc.tenant_id \
             WHERE kc.tenant_id = $1 \
               AND ki.status = 'published' \
               AND (1 - (kc.embedding <=> '[{v}]'::vector)) >= $2 \
             ORDER BY similarity DESC \
             LIMIT $3",
        v = v
    ))
    .bind(tenant_id)
    .bind(threshold)
    .bind(limit)
    .fetch_all(pool)
    .await
    .unwrap()
}

// ═══════════════════════════════════════════════════════════════════════════════
// FR-007 / SC-003: Tenant isolation — identical content across tenants
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cross_tenant_isolation_with_identical_content() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;

    // Seed both tenants with *identical* published content and identical embeddings.
    let (_item_a, chunk_a) = seed_item_with_chunk(
        &pool,
        tenant_a,
        "published",
        "Identical Article",
        "identical body",
        "identical chunk content for isolation test",
        0.5,
    )
    .await;
    let (_item_b, chunk_b) = seed_item_with_chunk(
        &pool,
        tenant_b,
        "published",
        "Identical Article",
        "identical body",
        "identical chunk content for isolation test",
        0.5,
    )
    .await;

    // Retrieval for tenant B — must return only tenant B's chunk.
    let results = retrieve_for_tenant(&pool, tenant_b, 0.5, 0.0, 10).await;

    let result_ids: Vec<Uuid> = results.iter().map(|r| r.0).collect();
    assert!(
        result_ids.contains(&chunk_b),
        "tenant B's own chunk must appear in the results"
    );
    assert!(
        !result_ids.contains(&chunk_a),
        "tenant A's chunk must NOT leak into tenant B's results — found chunk {} in tenant B's results",
        chunk_a
    );

    // Every returned row must carry tenant_b as tenant_id.
    for row in &results {
        assert_eq!(
            row.2, tenant_b,
            "returned chunk {} has tenant_id {} instead of expected {}",
            row.0, row.2, tenant_b
        );
    }

    // Reverse check: retrieval for tenant A returns only tenant A's chunk.
    let results_a = retrieve_for_tenant(&pool, tenant_a, 0.5, 0.0, 10).await;
    let result_a_ids: Vec<Uuid> = results_a.iter().map(|r| r.0).collect();
    assert!(result_a_ids.contains(&chunk_a));
    assert!(!result_a_ids.contains(&chunk_b));
}

// ═══════════════════════════════════════════════════════════════════════════════
// FR-005: Only published items are retrievable; draft/archived are excluded
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn draft_and_archived_items_excluded_from_retrieval() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;

    // Published item — should be retrievable.
    let (_pub_item, pub_chunk) = seed_item_with_chunk(
        &pool,
        tenant_id,
        "published",
        "Published Article",
        "published body",
        "published chunk content",
        0.3,
    )
    .await;

    // Draft item — must NOT be retrievable.
    let (_draft_item, draft_chunk) = seed_item_with_chunk(
        &pool,
        tenant_id,
        "draft",
        "Draft Article",
        "draft body",
        "draft chunk content",
        0.3,
    )
    .await;

    // Archived item — must NOT be retrievable.
    let (_archived_item, archived_chunk) = seed_item_with_chunk(
        &pool,
        tenant_id,
        "archived",
        "Archived Article",
        "archived body",
        "archived chunk content",
        0.3,
    )
    .await;

    // All three chunks have identical embeddings, so similarity cannot
    // be the discriminator — only the JOIN + status filter matters.
    let results = retrieve_for_tenant(&pool, tenant_id, 0.3, 0.0, 10).await;

    let result_ids: Vec<Uuid> = results.iter().map(|r| r.0).collect();

    assert!(
        result_ids.contains(&pub_chunk),
        "published chunk must be retrievable"
    );
    assert!(
        !result_ids.contains(&draft_chunk),
        "draft chunk must NOT be retrievable"
    );
    assert!(
        !result_ids.contains(&archived_chunk),
        "archived chunk must NOT be retrievable"
    );

    // Exactly one chunk returned.
    assert_eq!(
        results.len(),
        1,
        "expected exactly 1 result (published), got {}",
        results.len()
    );
}
