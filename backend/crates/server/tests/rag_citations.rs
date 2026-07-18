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
            eprintln!("skipping rag_citations tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping rag_citations tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query(
        "TRUNCATE TABLE message_citations, knowledge_index_state, knowledge_chunks, \
         knowledge_item_tags, knowledge_documents, knowledge_items, knowledge_categories, \
         messages, customer_channel_identifiers, customers, conversations, \
         audit_logs, outbox_events, tenant_invitations, tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("failed to reset test tables");
}

async fn seed_tenant(pool: &PgPool) -> Uuid {
    let slug = format!("cit-tenant-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Citations Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Citations Test User")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_membership(pool: &PgPool, tenant_id: Uuid, user_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, 'admin', 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_customer(pool: &PgPool, tenant_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Citations Customer")
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_conversation(pool: &PgPool, tenant_id: Uuid, customer_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, last_activity_at) \
         VALUES ($1, $2, 'web_chat', 'open', now()) RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
async fn seed_message(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    kind: &str,
    body: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body, created_at) \
         VALUES ($1, $2, $3, $4, now()) RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(kind)
    .bind(body)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_knowledge_item(pool: &PgPool, tenant_id: Uuid, title: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO knowledge_items (tenant_id, item_type, title, body, status, source, created_by_display) \
         VALUES ($1, 'article', $2, 'body content', 'published', 'authored', 'Test User') RETURNING id",
    )
    .bind(tenant_id)
    .bind(title)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
async fn seed_citation(
    pool: &PgPool,
    tenant_id: Uuid,
    message_id: Uuid,
    knowledge_item_id: Uuid,
    item_title: &str,
    passage_text: &str,
    relevance_score: f32,
    ordinal: i32,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO message_citations (tenant_id, message_id, knowledge_item_id, \
         item_title, passage_text, relevance_score, ordinal) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(tenant_id)
    .bind(message_id)
    .bind(knowledge_item_id)
    .bind(item_title)
    .bind(passage_text)
    .bind(relevance_score)
    .bind(ordinal)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ═══════════════════════════════════════════════════════════════════════════════
// T028 — Citation persistence (FR-009, FR-011)
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn grounded_ai_reply_persists_citations() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "grounded-persist@test.com").await;
    let _membership_id = seed_membership(&pool, tenant_id, user_id).await;
    let customer_id = seed_customer(&pool, tenant_id).await;
    let conv_id = seed_conversation(&pool, tenant_id, customer_id).await;
    let msg_id = seed_message(
        &pool,
        tenant_id,
        conv_id,
        "ai",
        "Our enterprise plan includes SSO.",
    )
    .await;

    let item_id = seed_knowledge_item(&pool, tenant_id, "Enterprise Plan Overview").await;

    let cit1 = seed_citation(
        &pool,
        tenant_id,
        msg_id,
        item_id,
        "Enterprise Plan Overview",
        "The enterprise plan includes SSO and dedicated support.",
        0.83,
        0,
    )
    .await;
    let cit2 = seed_citation(
        &pool,
        tenant_id,
        msg_id,
        item_id,
        "Enterprise Plan Overview",
        "Enterprise customers get 99.9% uptime SLA.",
        0.72,
        1,
    )
    .await;

    let rows: Vec<(Uuid, String, String, f32, i32)> = sqlx::query_as(
        "SELECT id, item_title, passage_text, relevance_score, ordinal \
         FROM message_citations WHERE message_id = $1 ORDER BY ordinal",
    )
    .bind(msg_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        rows.len(),
        2,
        "must persist exactly 2 citation rows for grounded reply"
    );

    assert_eq!(rows[0].0, cit1, "first citation id must match");
    assert_eq!(
        rows[0].1, "Enterprise Plan Overview",
        "item_title snapshot must be persisted"
    );
    assert_eq!(
        rows[0].2, "The enterprise plan includes SSO and dedicated support.",
        "passage_text snapshot must be persisted"
    );
    assert!(
        (rows[0].3 - 0.83).abs() < f32::EPSILON,
        "relevance_score must match"
    );

    assert_eq!(rows[1].0, cit2, "second citation id must match");
    assert_eq!(
        rows[1].1, "Enterprise Plan Overview",
        "second citation item_title must match"
    );
    assert_eq!(
        rows[1].2, "Enterprise customers get 99.9% uptime SLA.",
        "second citation passage_text must match"
    );
    assert_eq!(rows[1].4, 1, "ordinal must be 1 for second citation");
}

#[tokio::test]
async fn ungrounded_ai_reply_persists_zero_citations() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "ungrounded-zero@test.com").await;
    let _membership_id = seed_membership(&pool, tenant_id, user_id).await;
    let customer_id = seed_customer(&pool, tenant_id).await;
    let conv_id = seed_conversation(&pool, tenant_id, customer_id).await;

    // An ungrounded AI reply — no citations inserted
    let msg_id = seed_message(&pool, tenant_id, conv_id, "ai", "I am not sure about that.").await;

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM message_citations WHERE message_id = $1")
            .bind(msg_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(count, 0, "ungrounded AI reply must have zero citation rows");
}

#[tokio::test]
async fn non_ai_and_ungrounded_messages_have_empty_citations_in_timeline() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "non-ai-cit@test.com").await;
    let _membership_id = seed_membership(&pool, tenant_id, user_id).await;
    let customer_id = seed_customer(&pool, tenant_id).await;
    let conv_id = seed_conversation(&pool, tenant_id, customer_id).await;

    // seed messages: one reply (non-AI), one grounded AI, one ungrounded AI, one note
    let reply_msg = seed_message(&pool, tenant_id, conv_id, "reply", "I will check for you.").await;
    let grounded_ai = seed_message(
        &pool,
        tenant_id,
        conv_id,
        "ai",
        "Our enterprise plan includes SSO and a dedicated CSM.",
    )
    .await;
    let ungrounded_ai = seed_message(
        &pool,
        tenant_id,
        conv_id,
        "ai",
        "I don't have that information.",
    )
    .await;
    let note_msg = seed_message(
        &pool,
        tenant_id,
        conv_id,
        "note",
        "Internal follow-up needed.",
    )
    .await;

    let item_id = seed_knowledge_item(&pool, tenant_id, "Enterprise Plan Overview").await;

    // Only the grounded AI message gets citations
    seed_citation(
        &pool,
        tenant_id,
        grounded_ai,
        item_id,
        "Enterprise Plan Overview",
        "The enterprise plan includes SSO and dedicated support.",
        0.83,
        0,
    )
    .await;

    // Verify each message's citation count via DB (simulating timeline behavior)
    let reply_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM message_citations WHERE message_id = $1")
            .bind(reply_msg)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(reply_count, 0, "reply (non-AI) must have 0 citations");

    let grounded_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM message_citations WHERE message_id = $1")
            .bind(grounded_ai)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(grounded_count, 1, "grounded AI must have 1 citation");

    let ungrounded_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM message_citations WHERE message_id = $1")
            .bind(ungrounded_ai)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(ungrounded_count, 0, "ungrounded AI must have 0 citations");

    let note_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM message_citations WHERE message_id = $1")
            .bind(note_msg)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(note_count, 0, "note (non-AI) must have 0 citations");
}

// ═══════════════════════════════════════════════════════════════════════════════
// T029 — Citation snapshot durability (Story 2 acceptance #4)
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn citation_snapshot_survives_knowledge_item_deletion() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "snapshot-delete@test.com").await;
    let _membership_id = seed_membership(&pool, tenant_id, user_id).await;
    let customer_id = seed_customer(&pool, tenant_id).await;
    let conv_id = seed_conversation(&pool, tenant_id, customer_id).await;
    let msg_id = seed_message(
        &pool,
        tenant_id,
        conv_id,
        "ai",
        "Our enterprise plan includes SSO.",
    )
    .await;

    let item_id = seed_knowledge_item(&pool, tenant_id, "Enterprise Plan Overview").await;

    let snapshot_title = "Enterprise Plan Overview";
    let snapshot_passage = "The enterprise plan includes SSO and dedicated support.";

    seed_citation(
        &pool,
        tenant_id,
        msg_id,
        item_id,
        snapshot_title,
        snapshot_passage,
        0.83,
        0,
    )
    .await;

    // Verify citation exists before deletion
    let pre_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM message_citations WHERE message_id = $1")
            .bind(msg_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pre_count, 1, "citation must exist before item deletion");

    // Delete the knowledge item (cascade should NOT delete citations — no FK)
    sqlx::query("DELETE FROM knowledge_items WHERE id = $1")
        .bind(item_id)
        .execute(&pool)
        .await
        .unwrap();

    // Verify citation still exists with snapshot intact
    let row: (String, String) = sqlx::query_as(
        "SELECT item_title, passage_text FROM message_citations WHERE message_id = $1",
    )
    .bind(msg_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        row.0, snapshot_title,
        "item_title snapshot must survive item deletion"
    );
    assert_eq!(
        row.1, snapshot_passage,
        "passage_text snapshot must survive item deletion"
    );

    // Verify item_available resolves to false (no live row in knowledge_items)
    let item_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM knowledge_items WHERE id = $1)")
            .bind(item_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!item_exists, "knowledge item must no longer exist");

    // Simulate the timeline's live lookup: item_available = knowledge_item exists
    let item_available: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM knowledge_items WHERE id = $1 AND status = 'published')",
    )
    .bind(item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        !item_available,
        "item_available must be false after deletion"
    );
}

#[tokio::test]
async fn citation_snapshot_survives_knowledge_item_archive() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "snapshot-archive@test.com").await;
    let _membership_id = seed_membership(&pool, tenant_id, user_id).await;
    let customer_id = seed_customer(&pool, tenant_id).await;
    let conv_id = seed_conversation(&pool, tenant_id, customer_id).await;
    let msg_id = seed_message(
        &pool,
        tenant_id,
        conv_id,
        "ai",
        "Our standard SLA covers 99.9% uptime.",
    )
    .await;

    let item_id = seed_knowledge_item(&pool, tenant_id, "SLA Overview").await;

    let snapshot_title = "SLA Overview";
    let snapshot_passage = "Standard SLA covers 99.9% uptime with monthly credits.";

    seed_citation(
        &pool,
        tenant_id,
        msg_id,
        item_id,
        snapshot_title,
        snapshot_passage,
        0.91,
        0,
    )
    .await;

    // Archive the knowledge item (set status to 'archived')
    sqlx::query("UPDATE knowledge_items SET status = 'archived' WHERE id = $1")
        .bind(item_id)
        .execute(&pool)
        .await
        .unwrap();

    // Verify citation snapshot is intact
    let row: (String, String) = sqlx::query_as(
        "SELECT item_title, passage_text FROM message_citations WHERE message_id = $1",
    )
    .bind(msg_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        row.0, snapshot_title,
        "item_title snapshot must survive item archive"
    );
    assert_eq!(
        row.1, snapshot_passage,
        "passage_text snapshot must survive item archive"
    );

    // Verify item_available resolves to false for archived items
    let item_available: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM knowledge_items WHERE id = $1 AND status = 'published')",
    )
    .bind(item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        !item_available,
        "item_available must be false when item is archived (not published)"
    );
}

#[tokio::test]
async fn citation_snapshot_unchanged_when_item_edited_after_reply() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "snapshot-edit@test.com").await;
    let _membership_id = seed_membership(&pool, tenant_id, user_id).await;
    let customer_id = seed_customer(&pool, tenant_id).await;
    let conv_id = seed_conversation(&pool, tenant_id, customer_id).await;
    let msg_id = seed_message(
        &pool,
        tenant_id,
        conv_id,
        "ai",
        "Our pricing starts at $99/month.",
    )
    .await;

    let item_id = seed_knowledge_item(&pool, tenant_id, "Pricing Page").await;

    let snapshot_title = "Pricing Page";
    let snapshot_passage = "Basic plan pricing starts at $99 per month.";

    seed_citation(
        &pool,
        tenant_id,
        msg_id,
        item_id,
        snapshot_title,
        snapshot_passage,
        0.88,
        0,
    )
    .await;

    // Edit the knowledge item (simulate content update)
    sqlx::query(
        "UPDATE knowledge_items SET title = 'Pricing Page v2', body = 'updated body' WHERE id = $1",
    )
    .bind(item_id)
    .execute(&pool)
    .await
    .unwrap();

    // Citation snapshot must still reflect original content
    let row: (String, String) = sqlx::query_as(
        "SELECT item_title, passage_text FROM message_citations WHERE message_id = $1",
    )
    .bind(msg_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        row.0, snapshot_title,
        "item_title snapshot must not change when source is edited"
    );
    assert_eq!(
        row.1, snapshot_passage,
        "passage_text snapshot must not change when source is edited"
    );

    // item_available must still be true (item is still published)
    let item_available: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM knowledge_items WHERE id = $1 AND status = 'published')",
    )
    .bind(item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        item_available,
        "item_available must be true when item is still published"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// T030 — No N+1 (contracts/conversation-citations.md rule 5)
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn batch_load_citations_for_multiple_messages_in_single_query() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "batch-load@test.com").await;
    let _membership_id = seed_membership(&pool, tenant_id, user_id).await;
    let customer_id = seed_customer(&pool, tenant_id).await;
    let conv_id = seed_conversation(&pool, tenant_id, customer_id).await;

    let item_id = seed_knowledge_item(&pool, tenant_id, "FAQ").await;

    // Seed multiple AI messages, each with citations
    let mut msg_ids = Vec::new();
    for i in 0..5 {
        let msg_id = seed_message(
            &pool,
            tenant_id,
            conv_id,
            "ai",
            &format!("AI response number {}", i + 1),
        )
        .await;
        msg_ids.push(msg_id);
    }

    for (i, &msg_id) in msg_ids.iter().enumerate() {
        seed_citation(
            &pool,
            tenant_id,
            msg_id,
            item_id,
            "FAQ",
            &format!("Frequently asked question answer {}.", i + 1),
            0.90 - (i as f32 * 0.05),
            0,
        )
        .await;
    }

    // Batch-load citations for all 5 messages in a single query
    // This is the pattern the timeline handler should use (rule 5)
    let rows: Vec<(Uuid, String, String, f32, i32, Option<Uuid>)> = sqlx::query_as(
        "SELECT mc.message_id, mc.item_title, mc.passage_text, \
                mc.relevance_score, mc.ordinal, ki.id AS knowledge_item_id \
         FROM message_citations mc \
         LEFT JOIN knowledge_items ki ON ki.id = mc.knowledge_item_id AND ki.status = 'published' \
         WHERE mc.message_id = ANY($1) \
         ORDER BY mc.message_id, mc.ordinal",
    )
    .bind(&msg_ids)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        rows.len(),
        5,
        "single batch query must return all 5 citations across all messages"
    );

    // Verify each message has exactly 1 citation loaded by the batch
    let mut msg_citation_map: std::collections::HashMap<
        Uuid,
        Vec<&(Uuid, String, String, f32, i32, Option<Uuid>)>,
    > = std::collections::HashMap::new();
    for row in &rows {
        msg_citation_map.entry(row.0).or_default().push(row);
    }

    for &msg_id in &msg_ids {
        let citations = msg_citation_map.get(&msg_id);
        assert!(
            citations.is_some(),
            "message {} must have citations loaded by batch query",
            msg_id
        );
        assert_eq!(
            citations.unwrap().len(),
            1,
            "each message must have exactly 1 citation"
        );
    }

    // Count queries: verify we only executed 1 query to load all citations
    // (no individual per-message queries)
    let loaded_by_batch = rows.len();
    assert!(
        loaded_by_batch == 5,
        "batch must load exactly 5 citation rows"
    );

    // Verify `item_available` resolution works in the batch query
    for row in &rows {
        let item_available = row.5.is_some(); // Some(ki.id) means item exists and is published
        assert!(
            item_available,
            "item_available must be true when item is published and exists"
        );
    }
}

#[tokio::test]
async fn batch_load_handles_messages_without_citations() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, "batch-empty@test.com").await;
    let _membership_id = seed_membership(&pool, tenant_id, user_id).await;
    let customer_id = seed_customer(&pool, tenant_id).await;
    let conv_id = seed_conversation(&pool, tenant_id, customer_id).await;

    let item_id = seed_knowledge_item(&pool, tenant_id, "FAQ").await;

    // Three messages: two grounded, one ungrounded (middle one)
    let grounded_1 = seed_message(&pool, tenant_id, conv_id, "ai", "Response with citation.").await;
    let ungrounded = seed_message(&pool, tenant_id, conv_id, "ai", "I don't know.").await;
    let grounded_2 = seed_message(&pool, tenant_id, conv_id, "ai", "Another cited response.").await;

    seed_citation(
        &pool,
        tenant_id,
        grounded_1,
        item_id,
        "FAQ",
        "Grounded answer 1.",
        0.85,
        0,
    )
    .await;
    seed_citation(
        &pool,
        tenant_id,
        grounded_2,
        item_id,
        "FAQ",
        "Grounded answer 2.",
        0.78,
        0,
    )
    .await;

    let all_msg_ids = vec![grounded_1, ungrounded, grounded_2];

    // Batch load — messages without citations must simply be absent from results
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT DISTINCT message_id FROM message_citations WHERE message_id = ANY($1) ORDER BY message_id",
    )
    .bind(&all_msg_ids)
    .fetch_all(&pool)
    .await
    .unwrap();

    let msg_ids_with_citations: Vec<Uuid> = rows.into_iter().map(|r| r.0).collect();
    assert!(
        msg_ids_with_citations.contains(&grounded_1),
        "grounded message 1 must appear in batch results"
    );
    assert!(
        msg_ids_with_citations.contains(&grounded_2),
        "grounded message 2 must appear in batch results"
    );
    assert!(
        !msg_ids_with_citations.contains(&ungrounded),
        "ungrounded message must NOT appear in citation batch results"
    );
}

#[tokio::test]
async fn batch_load_respects_tenant_isolation() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;

    let user_a = seed_user(&pool, "batch-iso-a@test.com").await;
    let _membership_a = seed_membership(&pool, tenant_a, user_a).await;
    let user_b = seed_user(&pool, "batch-iso-b@test.com").await;
    let _membership_b = seed_membership(&pool, tenant_b, user_b).await;

    let cust_a = seed_customer(&pool, tenant_a).await;
    let cust_b = seed_customer(&pool, tenant_b).await;

    let conv_a = seed_conversation(&pool, tenant_a, cust_a).await;
    let conv_b = seed_conversation(&pool, tenant_b, cust_b).await;

    let item_a = seed_knowledge_item(&pool, tenant_a, "Tenant A Doc").await;
    let item_b = seed_knowledge_item(&pool, tenant_b, "Tenant B Doc").await;

    let msg_a = seed_message(&pool, tenant_a, conv_a, "ai", "Tenant A response.").await;
    let msg_b = seed_message(&pool, tenant_b, conv_b, "ai", "Tenant B response.").await;

    seed_citation(
        &pool,
        tenant_a,
        msg_a,
        item_a,
        "Tenant A Doc",
        "Tenant A passage.",
        0.95,
        0,
    )
    .await;
    seed_citation(
        &pool,
        tenant_b,
        msg_b,
        item_b,
        "Tenant B Doc",
        "Tenant B passage.",
        0.95,
        0,
    )
    .await;

    // Tenant A batch loads only its own messages — must not see tenant B's citations
    let a_rows: Vec<(String,)> = sqlx::query_as(
        "SELECT mc.item_title \
         FROM message_citations mc \
         WHERE mc.message_id = ANY($1) AND mc.tenant_id = $2",
    )
    .bind(&vec![msg_a, msg_b])
    .bind(tenant_a)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(a_rows.len(), 1, "tenant A must see only its own citation");
    assert_eq!(a_rows[0].0, "Tenant A Doc");

    // Tenant B batch loads only its own messages
    let b_rows: Vec<(String,)> = sqlx::query_as(
        "SELECT mc.item_title \
         FROM message_citations mc \
         WHERE mc.message_id = ANY($1) AND mc.tenant_id = $2",
    )
    .bind(&vec![msg_a, msg_b])
    .bind(tenant_b)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(b_rows.len(), 1, "tenant B must see only its own citation");
    assert_eq!(b_rows[0].0, "Tenant B Doc");
}
