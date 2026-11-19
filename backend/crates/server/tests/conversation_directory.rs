#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: a failed unwrap is the failed test
//! The `conversations` and `customers` interfaces other modules use (MOH-194): what the widget,
//! the escalation queue and the WhatsApp channel ask instead of reading `conversations`,
//! `messages`, `message_attachments`, `customers` and `customer_channel_identifiers` themselves.
//!
//! They run as the application role in a tenant Unit of Work, so Row-Level Security is on: a
//! question about another tenant's rows must come back empty even when the caller names that
//! tenant.

use std::time::Duration;

use conversations::{attachments, directory, inbound};
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
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        assert!(
            !require_db_tests(),
            "REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable"
        );
        return None;
    }
    Some(pool)
}

struct Side {
    tenant: Uuid,
    customer: Uuid,
    other_customer: Uuid,
    conversation: Uuid,
    /// A conversation that was deleted.
    gone: Uuid,
    /// An AI answer waiting to be sent, and a message the customer wrote.
    outbound: Uuid,
    inbound: Uuid,
}

struct World {
    a: Side,
    b: Side,
}

async fn customer(seed: &PgPool, tenant: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant)
    .bind(name)
    .fetch_one(seed)
    .await
    .unwrap()
}

async fn conversation(seed: &PgPool, tenant: Uuid, customer: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) \
         VALUES ($1, $2, 'whatsapp', 'open') RETURNING id",
    )
    .bind(tenant)
    .bind(customer)
    .fetch_one(seed)
    .await
    .unwrap()
}

async fn message(seed: &PgPool, tenant: Uuid, conversation: Uuid, kind: &str, body: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(tenant)
    .bind(conversation)
    .bind(kind)
    .bind(body)
    .fetch_one(seed)
    .await
    .unwrap()
}

async fn side(seed: &PgPool, label: &str) -> Side {
    let tag = Uuid::new_v4().simple().to_string();
    let tenant = db::testing::insert_tenant(seed, label, &format!("cd-{label}-{tag}")).await;
    let customer = customer(seed, tenant, &format!("{label} customer")).await;
    let other_customer = customer_other(seed, tenant, label).await;
    let conversation = conversation(seed, tenant, customer).await;
    let gone = self::conversation(seed, tenant, customer).await;
    sqlx::query("UPDATE conversations SET deleted_at = now() WHERE id = $1")
        .bind(gone)
        .execute(seed)
        .await
        .unwrap();
    let outbound = message(seed, tenant, conversation, "ai", "we are on it").await;
    let inbound = message(seed, tenant, conversation, "customer", "hello").await;
    Side {
        tenant,
        customer,
        other_customer,
        conversation,
        gone,
        outbound,
        inbound,
    }
}

async fn customer_other(seed: &PgPool, tenant: Uuid, label: &str) -> Uuid {
    customer(seed, tenant, &format!("{label} other")).await
}

async fn world() -> Option<(PgPool, World)> {
    let pool = runtime_pool().await?;
    db::testing::migrate().await;
    let seed = db::testing::seed_pool();
    let a = side(&seed, "a").await;
    let b = side(&seed, "b").await;
    Some((pool, World { a, b }))
}

#[tokio::test]
async fn a_visitors_open_conversation_is_found_only_in_their_own_tenant() {
    let Some((pool, w)) = world().await else {
        return;
    };
    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, w.a.tenant)
        .await
        .unwrap();
    let conn = uow.conn();

    let found = directory::open_conversation_of_customer(&mut *conn, w.a.tenant, w.a.customer)
        .await
        .unwrap()
        .expect("the customer's own open conversation");
    assert_eq!(found.id, w.a.conversation);
    assert_eq!(found.status, "open");
    assert!(!found.escalated && !found.assigned);

    // Another customer of the same tenant has none; tenant B's customer is invisible to tenant A,
    // whichever tenant the caller names.
    assert!(
        directory::open_conversation_of_customer(&mut *conn, w.a.tenant, w.a.other_customer)
            .await
            .unwrap()
            .is_none()
    );
    for asked in [w.a.tenant, w.b.tenant] {
        assert!(
            directory::open_conversation_of_customer(&mut *conn, asked, w.b.customer)
                .await
                .unwrap()
                .is_none(),
            "tenant B's conversation leaked to tenant A (asked as {asked})"
        );
    }
}

#[tokio::test]
async fn a_visitor_can_lock_only_their_own_conversation() {
    let Some((pool, w)) = world().await else {
        return;
    };
    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, w.a.tenant)
        .await
        .unwrap();
    let conn = uow.conn();

    assert_eq!(
        directory::lock_customers_conversation(
            &mut *conn,
            w.a.tenant,
            w.a.conversation,
            w.a.customer
        )
        .await
        .unwrap()
        .as_deref(),
        Some("open")
    );
    // Somebody else's conversation, a deleted one, and tenant B's: all absent.
    for (tenant, conversation, customer) in [
        (w.a.tenant, w.a.conversation, w.a.other_customer),
        (w.a.tenant, w.a.gone, w.a.customer),
        (w.a.tenant, w.b.conversation, w.b.customer),
        (w.b.tenant, w.b.conversation, w.b.customer),
    ] {
        assert!(
            directory::lock_customers_conversation(&mut *conn, tenant, conversation, customer)
                .await
                .unwrap()
                .is_none(),
            "{conversation} was lockable by {customer}"
        );
    }
}

#[tokio::test]
async fn who_a_conversation_belongs_to_stays_inside_the_tenant() {
    let Some((pool, w)) = world().await else {
        return;
    };
    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, w.a.tenant)
        .await
        .unwrap();
    let conn = uow.conn();

    assert_eq!(
        directory::customer_of_live_conversation(&mut *conn, w.a.tenant, w.a.conversation)
            .await
            .unwrap(),
        Some(w.a.customer)
    );
    // A deleted conversation is not live, but an outbound message still goes to its customer.
    assert_eq!(
        directory::customer_of_live_conversation(&mut *conn, w.a.tenant, w.a.gone)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        directory::customer_of_conversation(&mut *conn, w.a.tenant, w.a.gone)
            .await
            .unwrap(),
        Some(w.a.customer)
    );
    for asked in [w.a.tenant, w.b.tenant] {
        assert_eq!(
            directory::customer_of_live_conversation(&mut *conn, asked, w.b.conversation)
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            directory::customer_of_conversation(&mut *conn, asked, w.b.conversation)
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            directory::status_and_assignee(&mut *conn, asked, w.b.conversation)
                .await
                .unwrap(),
            None
        );
    }
    assert_eq!(
        directory::status_and_assignee(&mut *conn, w.a.tenant, w.a.conversation)
            .await
            .unwrap(),
        Some(("open".to_owned(), None))
    );
    assert_eq!(
        directory::status_and_assignee(&mut *conn, w.a.tenant, w.a.gone)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn queue_cards_name_the_channel_and_customer_of_this_tenants_conversations_only() {
    let Some((pool, w)) = world().await else {
        return;
    };
    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, w.a.tenant)
        .await
        .unwrap();
    let conn = uow.conn();

    let cards = directory::conversation_cards(
        &mut *conn,
        w.a.tenant,
        &[w.a.conversation, w.b.conversation, Uuid::new_v4()],
    )
    .await
    .unwrap();
    assert_eq!(cards.len(), 1, "{cards:?}");
    let card = &cards[&w.a.conversation];
    assert_eq!(card.channel, "whatsapp");
    assert_eq!(card.customer_id, w.a.customer);
    assert_eq!(card.customer_name, "a customer");

    assert!(
        directory::conversation_cards(&mut *conn, w.b.tenant, &[w.b.conversation])
            .await
            .unwrap()
            .is_empty()
    );
    assert!(directory::conversation_cards(&mut *conn, w.a.tenant, &[])
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn only_an_outbound_message_of_this_tenant_has_a_body_to_send() {
    let Some((pool, w)) = world().await else {
        return;
    };
    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, w.a.tenant)
        .await
        .unwrap();
    let conn = uow.conn();

    assert_eq!(
        directory::outbound_message_body(&mut *conn, w.a.tenant, w.a.outbound)
            .await
            .unwrap()
            .as_deref(),
        Some("we are on it")
    );
    // What the customer wrote is not outbound; tenant B's outbound message is not tenant A's.
    assert_eq!(
        directory::outbound_message_body(&mut *conn, w.a.tenant, w.a.inbound)
            .await
            .unwrap(),
        None
    );
    for asked in [w.a.tenant, w.b.tenant] {
        assert_eq!(
            directory::outbound_message_body(&mut *conn, asked, w.b.outbound)
                .await
                .unwrap(),
            None
        );
    }
}

#[tokio::test]
async fn how_busy_a_member_is_counts_only_open_and_pending_conversations_of_this_tenant() {
    let Some((pool, w)) = world().await else {
        return;
    };
    let seed = db::testing::seed_pool();
    let tag = Uuid::new_v4().simple().to_string();
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name, password_hash) VALUES ($1, 'Load', $2) \
         RETURNING id",
    )
    .bind(format!("load-{tag}@conversation-directory.test"))
    .bind("$argon2id$v=19$m=19456,t=2,p=1$ZGlyZWN0b3JpZXM$ZGlyZWN0b3JpZXM")
    .fetch_one(&seed)
    .await
    .unwrap();
    let member: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, 'agent') \
         RETURNING id",
    )
    .bind(w.a.tenant)
    .bind(user)
    .fetch_one(&seed)
    .await
    .unwrap();
    // Two open conversations assigned to the member, one of them deleted afterwards.
    let second = conversation(&seed, w.a.tenant, w.a.other_customer).await;
    for id in [w.a.conversation, second] {
        sqlx::query("UPDATE conversations SET assigned_membership_id = $1 WHERE id = $2")
            .bind(member)
            .bind(id)
            .execute(&seed)
            .await
            .unwrap();
    }
    sqlx::query("UPDATE conversations SET assigned_membership_id = $1 WHERE id = $2")
        .bind(member)
        .bind(w.a.gone)
        .execute(&seed)
        .await
        .unwrap();

    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, w.a.tenant)
        .await
        .unwrap();
    let loads = directory::open_load_by_member(uow.conn(), w.a.tenant, &[member])
        .await
        .unwrap();
    assert_eq!(
        loads.get(&member),
        Some(&2),
        "the deleted one does not count"
    );
    // Asked as tenant B: nothing, and a member nobody has assigned anything is simply absent.
    assert!(
        directory::open_load_by_member(uow.conn(), w.b.tenant, &[member])
            .await
            .unwrap()
            .is_empty()
    );
    assert!(directory::open_load_by_member(uow.conn(), w.a.tenant, &[])
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn a_channel_message_reuses_the_open_conversation_or_opens_one() {
    let Some((pool, w)) = world().await else {
        return;
    };
    let mut tx = db::UnitOfWork::begin_for_tenant(&pool, w.a.tenant)
        .await
        .unwrap();

    // The customer already has an open WhatsApp conversation: it is reused.
    let reused = inbound::open_or_create_in_tx(&mut tx, w.a.tenant, w.a.customer, "whatsapp")
        .await
        .unwrap();
    assert_eq!(reused, w.a.conversation);
    // Nothing open on another channel: a new conversation opens.
    let opened = inbound::open_or_create_in_tx(&mut tx, w.a.tenant, w.a.customer, "email")
        .await
        .unwrap();
    assert_ne!(opened, w.a.conversation);

    let message =
        inbound::append_customer_message_in_tx(&mut tx, w.a.tenant, w.a.conversation, "hi again")
            .await
            .unwrap();
    let (kind, body): (String, String) =
        sqlx::query_as("SELECT kind, body FROM messages WHERE id = $1")
            .bind(message)
            .fetch_one(tx.conn())
            .await
            .unwrap();
    assert_eq!((kind.as_str(), body.as_str()), ("customer", "hi again"));
    tx.commit().await.unwrap();

    // Tenant B's customer is invisible to tenant A's Unit of Work: a conversation is opened for
    // nobody, because the customer does not exist there (row-level security refuses the insert).
    let mut other = db::UnitOfWork::begin_for_tenant(&pool, w.a.tenant)
        .await
        .unwrap();
    let wrong =
        inbound::open_or_create_in_tx(&mut other, w.a.tenant, w.b.customer, "whatsapp").await;
    assert!(
        wrong.is_err(),
        "a conversation was opened for another tenant's customer"
    );
}

#[tokio::test]
async fn an_attachment_is_stored_failed_or_downloaded_only_through_its_own_conversation() {
    let Some((pool, w)) = world().await else {
        return;
    };
    let mut tx = db::UnitOfWork::begin_for_tenant(&pool, w.a.tenant)
        .await
        .unwrap();
    let id = attachments::insert_pending_in_tx(
        &mut tx,
        w.a.tenant,
        w.a.inbound,
        "image",
        "media-1",
        Some("photo.jpg"),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    // Still pending: not downloadable yet.
    assert!(
        attachments::stored_for_conversation(&pool, w.a.tenant, w.a.conversation, id)
            .await
            .unwrap()
            .is_none()
    );
    attachments::mark_stored(&pool, w.a.tenant, id, "k/photo.jpg", "image/jpeg", 12)
        .await
        .unwrap();
    let stored = attachments::stored_for_conversation(&pool, w.a.tenant, w.a.conversation, id)
        .await
        .unwrap()
        .expect("stored attachment of its own conversation");
    assert_eq!(stored.storage_key.as_deref(), Some("k/photo.jpg"));
    assert_eq!(stored.file_name.as_deref(), Some("photo.jpg"));

    // Through the wrong conversation, or as tenant B: nothing.
    assert!(
        attachments::stored_for_conversation(&pool, w.a.tenant, w.a.gone, id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        attachments::stored_for_conversation(&pool, w.b.tenant, w.a.conversation, id)
            .await
            .unwrap()
            .is_none()
    );

    // Failed fetches count up and the attachment fails on the fifth.
    let mut tx = db::UnitOfWork::begin_for_tenant(&pool, w.a.tenant)
        .await
        .unwrap();
    let failing = attachments::insert_pending_in_tx(
        &mut tx,
        w.a.tenant,
        w.a.inbound,
        "audio",
        "media-2",
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    for _ in 0..attachments::MAX_FETCH_ATTEMPTS {
        attachments::mark_failed_attempt(&pool, w.a.tenant, failing)
            .await
            .unwrap();
    }
    let (status, attempts): (String, i32) =
        sqlx::query_as("SELECT status, fetch_attempts FROM message_attachments WHERE id = $1")
            .bind(failing)
            .fetch_one(&db::testing::seed_pool())
            .await
            .unwrap();
    assert_eq!(
        (status.as_str(), attempts),
        ("failed", attachments::MAX_FETCH_ATTEMPTS)
    );
    // Another tenant cannot touch it.
    attachments::mark_failed_attempt(&pool, w.b.tenant, id)
        .await
        .unwrap();
    let attempts: i32 =
        sqlx::query_scalar("SELECT fetch_attempts FROM message_attachments WHERE id = $1")
            .bind(id)
            .fetch_one(&db::testing::seed_pool())
            .await
            .unwrap();
    assert_eq!(
        attempts, 0,
        "tenant B counted a failure on tenant A's attachment"
    );
}

#[tokio::test]
async fn a_customers_channel_identifier_and_name_are_this_tenants_only() {
    let Some((pool, w)) = world().await else {
        return;
    };
    let seed = db::testing::seed_pool();
    for (tenant, customer) in [(w.a.tenant, w.a.customer), (w.b.tenant, w.b.customer)] {
        sqlx::query(
            "INSERT INTO customer_channel_identifiers (tenant_id, customer_id, channel, identifier) \
             VALUES ($1, $2, 'whatsapp', $3)",
        )
        .bind(tenant)
        .bind(customer)
        .bind(format!("+2010{}", &customer.simple().to_string()[..7]))
        .execute(&seed)
        .await
        .unwrap();
    }
    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, w.a.tenant)
        .await
        .unwrap();
    let conn = uow.conn();

    let own =
        customers::directory::channel_identifier(&mut *conn, w.a.tenant, w.a.customer, "whatsapp")
            .await
            .unwrap();
    assert!(own.is_some_and(|i| i.starts_with("+2010")));
    assert_eq!(
        customers::directory::channel_identifier(&mut *conn, w.a.tenant, w.a.customer, "phone")
            .await
            .unwrap(),
        None
    );
    for asked in [w.a.tenant, w.b.tenant] {
        assert_eq!(
            customers::directory::channel_identifier(&mut *conn, asked, w.b.customer, "whatsapp")
                .await
                .unwrap(),
            None,
            "tenant B's identifier leaked to tenant A (asked as {asked})"
        );
    }

    let names = customers::directory::display_names(
        &mut *conn,
        w.a.tenant,
        &[w.a.customer, w.b.customer, Uuid::new_v4()],
    )
    .await
    .unwrap();
    assert_eq!(names.len(), 1);
    assert_eq!(names[&w.a.customer], "a customer");
}
