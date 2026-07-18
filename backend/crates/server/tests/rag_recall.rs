use std::time::Duration;

use knowledge::retrieval::RetrievedChunk;
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
            eprintln!("skipping rag_recall tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping rag_recall tests: DATABASE_URL is unreachable");
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
    let slug = format!("recall-tenant-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Recall Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_published_item(pool: &PgPool, tenant_id: Uuid, title: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO knowledge_items (tenant_id, item_type, title, body, status, source, created_by_display) \
         VALUES ($1, 'article', $2, 'body', 'published', 'authored', 'Test User') RETURNING id",
    )
    .bind(tenant_id)
    .bind(title)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Format a 1536‑dim f32 vector as a comma‑separated string for pgvector literal.
fn fmt_vec(v: &[f32; 1536]) -> String {
    let mut s = String::with_capacity(1536 * 6);
    for (i, x) in v.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&x.to_string());
    }
    s
}

/// Query vector — points along the first axis.
fn query_vector() -> [f32; 1536] {
    let mut v = [0.0f32; 1536];
    v[0] = 1.0;
    v
}

/// Target tenant's best‑match chunk — direction very close to the query.
fn target_best_vector() -> [f32; 1536] {
    let mut v = [0.0f32; 1536];
    v[0] = 0.90;
    v[1] = 0.10;
    v[2] = 0.08;
    v
}

/// Noise chunk vector — a different direction still above the relevance
/// threshold. Each noise chunk gets a small perturbation so the cluster
/// has internal variety.
fn noise_vector(id: usize) -> [f32; 1536] {
    let mut v = [0.0f32; 1536];
    let a = (id as f64) * 0.017;
    v[0] = 0.65 + (a.sin() * 0.02) as f32;
    v[1] = 0.45 + (a.cos() * 0.03) as f32;
    v[2] = 0.05 + (a * 0.3).sin() as f32 * 0.01;
    v
}

/// Filler vector for the target tenant's other chunks — far from query.
fn filler_vector(id: usize) -> [f32; 1536] {
    let mut v = [0.0f32; 1536];
    v[id % 1536] = 1.0;
    v
}

const NOISE_TENANT_COUNT: usize = 20;
const TOP_K: i32 = 5;
const THRESHOLD: f32 = 0.70;

// ═══════════════════════════════════════════════════════════════════════════════
// Recall test — guards the filtered-ANN recall gap (research.md §1/§9)
//
// The target tenant owns < 5 % of the total knowledge_chunks. Without
// hnsw.iterative_scan and a raised ef_search, a naïve HNSW scan can miss
// the target's best passage because the global candidate list is dominated
// by noise tenants' chunks.
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn recall_target_tenant_chunk_among_noise() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    // ── Seed noise tenants ──────────────────────────────────────────────
    // Each gets one published item with 3 chunks whose vectors form a
    // dense cluster at moderate cosine similarity to the query.
    for i in 0..NOISE_TENANT_COUNT {
        let t = seed_tenant(&pool).await;
        let item = seed_published_item(&pool, t, &format!("Noise Article {}", i)).await;

        let v = fmt_vec(&noise_vector(i));
        sqlx::query(
            "INSERT INTO knowledge_chunks \
             (tenant_id, item_id, ordinal, content, embedding, content_hash) \
             VALUES ($1, $2, $3, $4, $5::vector, $6)",
        )
        .bind(t)
        .bind(item)
        .bind(0i32)
        .bind("Initial segment of noise content.")
        .bind(&v)
        .bind("noise-hash")
        .execute(&pool)
        .await
        .unwrap();

        let v = fmt_vec(&noise_vector(i + 50));
        sqlx::query(
            "INSERT INTO knowledge_chunks \
             (tenant_id, item_id, ordinal, content, embedding, content_hash) \
             VALUES ($1, $2, $3, $4, $5::vector, $6)",
        )
        .bind(t)
        .bind(item)
        .bind(1i32)
        .bind("Middle segment of noise content with additional context.")
        .bind(&v)
        .bind("noise-hash")
        .execute(&pool)
        .await
        .unwrap();

        let v = fmt_vec(&noise_vector(i + 100));
        sqlx::query(
            "INSERT INTO knowledge_chunks \
             (tenant_id, item_id, ordinal, content, embedding, content_hash) \
             VALUES ($1, $2, $3, $4, $5::vector, $6)",
        )
        .bind(t)
        .bind(item)
        .bind(2i32)
        .bind("Final segment of noise content concluding the passage.")
        .bind(&v)
        .bind("noise-hash")
        .execute(&pool)
        .await
        .unwrap();
    }

    // ── Seed target tenant ──────────────────────────────────────────────
    // One published item with three chunks:
    //   - chunk 0: the *distinctive* best-match passage
    //   - chunks 1‑2: filler with poor similarity (should not appear)
    let target_tenant = seed_tenant(&pool).await;
    let target_item =
        seed_published_item(&pool, target_tenant, "Target — Earth Science Article").await;

    let target_best = fmt_vec(&target_best_vector());
    sqlx::query(
        "INSERT INTO knowledge_chunks \
         (tenant_id, item_id, ordinal, content, embedding, content_hash) \
         VALUES ($1, $2, $3, $4, $5::vector, $6)",
    )
    .bind(target_tenant)
    .bind(target_item)
    .bind(0i32)
    .bind(
        "The Earth orbits the Sun at approximately 149.6 million kilometers, \
         completing one revolution every 365.25 days.",
    )
    .bind(&target_best)
    .bind("target-distinctive-hash")
    .execute(&pool)
    .await
    .unwrap();

    let filler_a = fmt_vec(&filler_vector(1));
    sqlx::query(
        "INSERT INTO knowledge_chunks \
         (tenant_id, item_id, ordinal, content, embedding, content_hash) \
         VALUES ($1, $2, $3, $4, $5::vector, $6)",
    )
    .bind(target_tenant)
    .bind(target_item)
    .bind(1i32)
    .bind("This is an unrelated passage about accounting standards.")
    .bind(&filler_a)
    .bind("target-filler-hash")
    .execute(&pool)
    .await
    .unwrap();

    let filler_b = fmt_vec(&filler_vector(2));
    sqlx::query(
        "INSERT INTO knowledge_chunks \
         (tenant_id, item_id, ordinal, content, embedding, content_hash) \
         VALUES ($1, $2, $3, $4, $5::vector, $6)",
    )
    .bind(target_tenant)
    .bind(target_item)
    .bind(2i32)
    .bind("This passage discusses corporate tax policy in detail.")
    .bind(&filler_b)
    .bind("target-filler-hash")
    .execute(&pool)
    .await
    .unwrap();

    // Verify the target tenant is < 5 % of total chunks
    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM knowledge_chunks")
        .fetch_one(&pool)
        .await
        .unwrap();
    let target_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM knowledge_chunks WHERE tenant_id = $1")
            .bind(target_tenant)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        (target_count.0 as f64) / (total.0 as f64) < 0.05,
        "target tenant must own < 5 % of chunks (got {}/{})",
        target_count.0,
        total.0
    );

    // ── Run retrieval ───────────────────────────────────────────────────
    // Mirror the settings used by knowledge::retrieval::search.
    sqlx::query("SET hnsw.iterative_scan = 'relaxed_order'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("SET hnsw.ef_search = 100")
        .execute(&pool)
        .await
        .unwrap();

    let q = fmt_vec(&query_vector());

    let results: Vec<RetrievedChunk> = sqlx::query_as(&format!(
        "SELECT kc.id AS chunk_id, \
                    kc.item_id, \
                    kc.tenant_id, \
                    kc.content, \
                    kc.content_hash, \
                    (1 - (kc.embedding <=> '[{}]'::vector)) AS similarity, \
                    ki.title AS item_title \
             FROM knowledge_chunks kc \
             JOIN knowledge_items ki \
               ON ki.id = kc.item_id \
              AND ki.tenant_id = kc.tenant_id \
             WHERE kc.tenant_id = $1 \
               AND ki.status = 'published' \
               AND (1 - (kc.embedding <=> '[{}]'::vector)) >= $2 \
             ORDER BY similarity DESC \
             LIMIT $3",
        q, q
    ))
    .bind(target_tenant)
    .bind(THRESHOLD)
    .bind(TOP_K)
    .fetch_all(&pool)
    .await
    .unwrap();

    // ── Assert ──────────────────────────────────────────────────────────
    assert!(
        !results.is_empty(),
        "expected at least 1 result for target tenant, got 0"
    );

    let found = results
        .iter()
        .any(|r| r.content.contains("149.6 million kilometers"));
    assert!(
        found,
        "target tenant's distinctive chunk must be in the top-{TOP_K} results \
         (got {} results, all: {:?})",
        results.len(),
        results.iter().map(|r| &r.content).collect::<Vec<_>>()
    );

    // Verify the distinctive chunk has the highest similarity score
    assert_eq!(
        results[0].chunk_id,
        results
            .iter()
            .find(|r| r.content.contains("149.6 million kilometers"))
            .map(|r| r.chunk_id)
            .unwrap(),
        "the distinctive chunk should be the top-ranked result"
    );
}
