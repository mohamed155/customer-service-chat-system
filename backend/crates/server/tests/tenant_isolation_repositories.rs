//! Repository functions called with the wrong tenant (ADR-004 section 7, gap T5; MOH-47).
//!
//! The database-level checks for each table are in `crates/shared/db/tests/tenant_isolation.rs`;
//! the endpoint-level ones are in the per-feature suites (see "The tenant-isolation harness" in
//! `docs/security/tenant-unit-of-work.md`). This file covers the repository layer: every function
//! below is given tenant B and the id of a row that belongs to tenant A, and must find nothing,
//! while the same call for tenant A finds it. Fixtures are created as the bypass role
//! (`db::testing::seed_pool`); the functions under test run on the runtime pool.
use sqlx::PgPool;
use uuid::Uuid;

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn runtime_pool() -> Option<PgPool> {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        assert!(
            !require_db_tests(),
            "REQUIRE_DB_TESTS=1 but DATABASE_URL is not set"
        );
        return None;
    };
    if !require_db_tests() && std::env::var("MIGRATION_DATABASE_URL").is_err() {
        eprintln!("skipping: MIGRATION_DATABASE_URL not set");
        return None;
    }
    Some(db::lazy_pool(&url, 3, std::time::Duration::from_secs(10)))
}

struct Rows {
    tenant: Uuid,
    customer: Uuid,
    conversation: Uuid,
    message: Uuid,
}

async fn seed(seed: &PgPool) -> Rows {
    let unique = Uuid::new_v4().simple().to_string();
    let tenant: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
            .bind(format!("Repo isolation {unique}"))
            .bind(format!("repo-iso-{unique}"))
            .fetch_one(seed)
            .await
            .unwrap();
    let customer: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, 'Customer') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(seed)
    .await
    .unwrap();
    let conversation: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'widget', 'open') RETURNING id",
    )
    .bind(tenant)
    .bind(customer)
    .fetch_one(seed)
    .await
    .unwrap();
    let message: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', 'a secret message') RETURNING id",
    )
    .bind(tenant)
    .bind(conversation)
    .fetch_one(seed)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'system', 'an automatic acknowledgement')",
    )
    .bind(tenant)
    .bind(conversation)
    .execute(seed)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO message_citations \
            (tenant_id, message_id, knowledge_item_id, item_title, passage_text, relevance_score, ordinal) \
         VALUES ($1, $2, gen_random_uuid(), 'Title', 'Passage', 0.5, 0)",
    )
    .bind(tenant)
    .bind(message)
    .execute(seed)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO message_attachments (tenant_id, message_id, kind) VALUES ($1, $2, 'image')",
    )
    .bind(tenant)
    .bind(message)
    .execute(seed)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO conversation_feedback (tenant_id, conversation_id, channel, rating) \
         VALUES ($1, $2, 'widget', 5)",
    )
    .bind(tenant)
    .bind(conversation)
    .execute(seed)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO escalations (tenant_id, conversation_id, reason, status) \
         VALUES ($1, $2, 'needs a human', 'queued')",
    )
    .bind(tenant)
    .bind(conversation)
    .execute(seed)
    .await
    .unwrap();
    Rows {
        tenant,
        customer,
        conversation,
        message,
    }
}

#[tokio::test]
async fn conversation_and_message_repositories_return_nothing_for_the_wrong_tenant() {
    let Some(pool) = runtime_pool().await else {
        return;
    };
    db::testing::migrate().await;
    let seed_pool = db::testing::seed_pool();
    let (a, b) = (seed(&seed_pool).await, seed(&seed_pool).await);

    // Control: the owning tenant finds its rows.
    assert!(
        conversations::queries::conversation_ai_state(&pool, a.tenant, a.conversation)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        conversations::queries::message_body(&pool, a.tenant, a.message)
            .await
            .unwrap()
            .as_deref(),
        Some("a secret message")
    );
    assert_eq!(
        conversations::queries::customer_id_for_conversation(&pool, a.tenant, a.conversation)
            .await
            .unwrap(),
        a.customer
    );
    assert!(
        conversations::queries::last_customer_message_at(&pool, a.tenant, a.conversation)
            .await
            .unwrap()
            .is_some()
    );
    // Controls for the checks that return "nothing": the owning tenant must find something, or a
    // denial for the wrong tenant would prove nothing.
    assert!(
        conversations::queries::has_system_message(&pool, a.tenant, a.conversation)
            .await
            .unwrap()
    );
    assert!(
        !conversations::queries::recent_history(&pool, a.tenant, a.conversation, 50)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        conversations::queries::load_citations_for_messages(&pool, a.tenant, &[a.message])
            .await
            .unwrap()
            .contains_key(&a.message)
    );
    assert!(
        conversations::queries::load_attachments_for_messages(&pool, a.tenant, &[a.message])
            .await
            .unwrap()
            .contains_key(&a.message)
    );

    // The wrong tenant, with the other tenant's real ids.
    assert!(
        conversations::queries::conversation_ai_state(&pool, b.tenant, a.conversation)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        conversations::queries::message_body(&pool, b.tenant, a.message)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        conversations::queries::customer_id_for_conversation(&pool, b.tenant, a.conversation)
            .await
            .is_err(),
        "a conversation of another tenant must not resolve to its customer"
    );
    assert!(
        conversations::queries::recent_history(&pool, b.tenant, a.conversation, 50)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        conversations::queries::last_customer_message_at(&pool, b.tenant, a.conversation)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !conversations::queries::has_system_message(&pool, b.tenant, a.conversation)
            .await
            .unwrap()
    );
    assert!(
        conversations::queries::load_citations_for_messages(&pool, b.tenant, &[a.message])
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        conversations::queries::load_attachments_for_messages(&pool, b.tenant, &[a.message])
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn customer_feedback_and_escalation_repositories_return_nothing_for_the_wrong_tenant() {
    let Some(pool) = runtime_pool().await else {
        return;
    };
    db::testing::migrate().await;
    let seed_pool = db::testing::seed_pool();
    let (a, b) = (seed(&seed_pool).await, seed(&seed_pool).await);

    assert!(
        customers::queries::fetch_profile_for_tool(&pool, a.tenant, a.customer)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        feedback::queries::find_feedback_by_conversation(&pool, a.tenant, a.conversation)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        escalations::routing::has_open_escalation(&pool, a.tenant, a.conversation)
            .await
            .unwrap()
    );

    assert!(
        customers::queries::fetch_profile_for_tool(&pool, b.tenant, a.customer)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        feedback::queries::find_feedback_by_conversation(&pool, b.tenant, a.conversation)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !escalations::routing::has_open_escalation(&pool, b.tenant, a.conversation)
            .await
            .unwrap()
    );
}
