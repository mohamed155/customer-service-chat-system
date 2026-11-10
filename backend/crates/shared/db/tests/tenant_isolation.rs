//! The shared tenant-isolation harness (ADR-004 section 7, gap T5; MOH-47).
//!
//! For every table under forced Row-Level Security this creates one row in each of two tenants
//! and runs `db::testing::isolation::assert_isolated` as `app_runtime`: read by guessed id, list,
//! update, delete and move to the other tenant as the wrong tenant, and read, update, delete and
//! insert with no tenant at all. Rows are created as the bypass role (`db::testing::seed_pool`),
//! which is test setup only; the checks run on the runtime pool.
//!
//! **Adding a table to Row-Level Security means adding it here.** The last test compares the list
//! below with the tables whose policy is forced and fails if they differ in either direction, so a
//! new group (MOH-48, MOH-49, ...) cannot be merged without being covered.
//!
//! Needs the role split (`MIGRATION_DATABASE_URL`); skips without it unless `REQUIRE_DB_TESTS=1`.
use db::testing::isolation::{assert_isolated, forced_rls_tables, Probe};
use sqlx::PgPool;
use uuid::Uuid;

fn can_run() -> bool {
    if std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
        || std::env::var("MIGRATION_DATABASE_URL").is_ok_and(|v| !v.is_empty())
    {
        return true;
    }
    eprintln!("skipping tenant isolation harness: MIGRATION_DATABASE_URL not set");
    false
}

async fn runtime_pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    db::lazy_pool(&url, 3, std::time::Duration::from_secs(10))
}

/// One tenant's rows, one per table in the conversations/customers group.
struct Graph {
    tenant: Uuid,
    customer: Uuid,
    identifier: Uuid,
    conversation: Uuid,
    message: Uuid,
    attachment: Uuid,
    citation: Uuid,
    feedback: Uuid,
    escalation: Uuid,
    whatsapp_meta: Uuid,
}

async fn scalar_uuid(seed: &PgPool, sql: &str, binds: &[Uuid]) -> Uuid {
    let mut q = sqlx::query_scalar::<_, Uuid>(sqlx::AssertSqlSafe(sql.to_string()));
    for b in binds {
        q = q.bind(*b);
    }
    q.fetch_one(seed)
        .await
        .unwrap_or_else(|e| panic!("seed failed: {sql}: {e}"))
}

async fn seed_graph(seed: &PgPool) -> Graph {
    let unique = Uuid::new_v4().simple().to_string();
    let tenant: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
            .bind(format!("Isolation {unique}"))
            .bind(format!("iso-{unique}"))
            .fetch_one(seed)
            .await
            .unwrap();
    let customer = scalar_uuid(
        seed,
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, 'C') RETURNING id",
        &[tenant],
    )
    .await;
    let identifier = scalar_uuid(
        seed,
        &format!(
            "INSERT INTO customer_channel_identifiers (tenant_id, customer_id, channel, identifier) \
             VALUES ($1, $2, 'email', 'c-{unique}@example.test') RETURNING id"
        ),
        &[tenant, customer],
    )
    .await;
    let conversation = scalar_uuid(
        seed,
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'widget', 'open') RETURNING id",
        &[tenant, customer],
    )
    .await;
    let message = scalar_uuid(
        seed,
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'hi') RETURNING id",
        &[tenant, conversation],
    )
    .await;
    let attachment = scalar_uuid(
        seed,
        "INSERT INTO message_attachments (tenant_id, message_id, kind) \
         VALUES ($1, $2, 'image') RETURNING id",
        &[tenant, message],
    )
    .await;
    let citation = scalar_uuid(
        seed,
        "INSERT INTO message_citations \
            (tenant_id, message_id, knowledge_item_id, item_title, passage_text, relevance_score, ordinal) \
         VALUES ($1, $2, gen_random_uuid(), 't', 'p', 0.5, 0) RETURNING id",
        &[tenant, message],
    )
    .await;
    let feedback = scalar_uuid(
        seed,
        "INSERT INTO conversation_feedback (tenant_id, conversation_id, channel, rating) \
         VALUES ($1, $2, 'widget', 5) RETURNING id",
        &[tenant, conversation],
    )
    .await;
    let escalation = scalar_uuid(
        seed,
        "INSERT INTO escalations (tenant_id, conversation_id, reason, status) \
         VALUES ($1, $2, 'needs a human', 'queued') RETURNING id",
        &[tenant, conversation],
    )
    .await;
    let whatsapp_meta = scalar_uuid(
        seed,
        "INSERT INTO whatsapp_message_meta (tenant_id, message_id, conversation_id, direction) \
         VALUES ($1, $2, $3, 'inbound') RETURNING id",
        &[tenant, message, conversation],
    )
    .await;
    Graph {
        tenant,
        customer,
        identifier,
        conversation,
        message,
        attachment,
        citation,
        feedback,
        escalation,
        whatsapp_meta,
    }
}

/// Every table in the harness: name, the row of a graph, an insert for a tenant, and the ids the
/// insert refers to. Add new tables to this list.
type Row = fn(&Graph) -> Uuid;
type Args = fn(&Graph) -> Vec<Uuid>;
struct Entry {
    table: &'static str,
    row: Row,
    insert_sql: &'static str,
    args: Args,
}

fn entries() -> Vec<Entry> {
    vec![
        Entry {
            table: "customers",
            row: |g| g.customer,
            insert_sql: "INSERT INTO customers (tenant_id, display_name) VALUES ($1, 'X')",
            args: |_| vec![],
        },
        Entry {
            table: "customer_channel_identifiers",
            row: |g| g.identifier,
            insert_sql: "INSERT INTO customer_channel_identifiers (tenant_id, customer_id, channel, identifier) VALUES ($1, $2, 'phone', 'x')",
            args: |g| vec![g.customer],
        },
        Entry {
            table: "conversations",
            row: |g| g.conversation,
            insert_sql: "INSERT INTO conversations (tenant_id, customer_id, channel, status) VALUES ($1, $2, 'widget', 'open')",
            args: |g| vec![g.customer],
        },
        Entry {
            table: "messages",
            row: |g| g.message,
            insert_sql: "INSERT INTO messages (tenant_id, conversation_id, kind, body) VALUES ($1, $2, 'customer', 'x')",
            args: |g| vec![g.conversation],
        },
        Entry {
            table: "message_attachments",
            row: |g| g.attachment,
            insert_sql: "INSERT INTO message_attachments (tenant_id, message_id, kind) VALUES ($1, $2, 'image')",
            args: |g| vec![g.message],
        },
        Entry {
            table: "message_citations",
            row: |g| g.citation,
            insert_sql: "INSERT INTO message_citations (tenant_id, message_id, knowledge_item_id, item_title, passage_text, relevance_score, ordinal) VALUES ($1, $2, gen_random_uuid(), 't', 'p', 0.5, 1)",
            args: |g| vec![g.message],
        },
        Entry {
            table: "conversation_feedback",
            row: |g| g.feedback,
            insert_sql: "INSERT INTO conversation_feedback (tenant_id, conversation_id, channel, rating) VALUES ($1, $2, 'widget', 4)",
            args: |g| vec![g.conversation],
        },
        Entry {
            table: "escalations",
            row: |g| g.escalation,
            insert_sql: "INSERT INTO escalations (tenant_id, conversation_id, reason, status) VALUES ($1, $2, 'x', 'queued')",
            args: |g| vec![g.conversation],
        },
        Entry {
            table: "whatsapp_message_meta",
            row: |g| g.whatsapp_meta,
            insert_sql: "INSERT INTO whatsapp_message_meta (tenant_id, message_id, conversation_id, direction) VALUES ($1, $2, $3, 'inbound')",
            args: |g| vec![g.message, g.conversation],
        },
    ]
}

#[tokio::test]
async fn every_table_in_the_conversations_and_customers_group_isolates_tenants() {
    if !can_run() {
        return;
    }
    db::testing::migrate().await;
    let seed = db::testing::seed_pool();
    let runtime = runtime_pool().await;
    let (a, b) = (seed_graph(&seed).await, seed_graph(&seed).await);

    for e in entries() {
        // Each probe inserts the same kind of row for the *other* tenant's ids, so the insert
        // arguments come from tenant A's graph while the tenant bound is also A: with no tenant
        // set the policy refuses it either way.
        let args = (e.args)(&a);
        assert_isolated(
            &runtime,
            &Probe {
                table: e.table,
                id_column: "id",
                tenant_a: a.tenant,
                tenant_b: b.tenant,
                row_a: (e.row)(&a),
                row_b: (e.row)(&b),
                insert_sql: e.insert_sql,
                insert_args: &args,
            },
        )
        .await;
    }
}

/// The harness must cover exactly the tables whose policy is forced: a table added to Row-Level
/// Security without a harness entry (or an entry for a table that is not protected) fails here.
#[tokio::test]
async fn the_harness_covers_exactly_the_tables_with_forced_row_level_security() {
    if !can_run() {
        return;
    }
    db::testing::migrate().await;
    let runtime = runtime_pool().await;
    let mut covered: Vec<String> = entries().iter().map(|e| e.table.to_string()).collect();
    covered.sort();
    let forced = forced_rls_tables(&runtime).await;
    assert_eq!(
        covered, forced,
        "tables with forced Row-Level Security and the harness entries must match: add the new \
         table to `entries()` in backend/crates/shared/db/tests/tenant_isolation.rs"
    );
}
