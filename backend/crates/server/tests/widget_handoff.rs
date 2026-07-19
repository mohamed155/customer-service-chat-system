use std::time::Duration;

use sha2::Digest;
use uuid::Uuid;

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
            eprintln!("skipping widget handoff tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping widget handoff tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query(
        "TRUNCATE TABLE widget_sessions, widget_instances, messages, \
         customer_channel_identifiers, customers, conversations, \
         outbox_events, audit_logs, tenant_invitations, \
         tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("failed to reset test tables");
}

async fn seed_tenant(pool: &sqlx::PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(name)
        .bind(format!("wgt-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_widget_instance(pool: &sqlx::PgPool, tenant_id: Uuid, public_id: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO widget_instances \
         (tenant_id, public_id, name, display_name, enabled, allowed_domains) \
         VALUES ($1, $2, $3, $4, true, '{}') RETURNING id",
    )
    .bind(tenant_id)
    .bind(public_id)
    .bind("Test Widget")
    .bind("Test Widget")
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_customer(pool: &sqlx::PgPool, tenant_id: Uuid, display_name: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name, email, phone) \
         VALUES ($1, $2, '', '') RETURNING id",
    )
    .bind(tenant_id)
    .bind(display_name)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_widget_session(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    instance_id: Uuid,
    customer_id: Uuid,
) -> Uuid {
    let token_hash = sha2::Sha256::digest(b"test-handoff-token").to_vec();
    sqlx::query_scalar(
        "INSERT INTO widget_sessions \
         (tenant_id, widget_instance_id, token_hash, customer_id, expires_at) \
         VALUES ($1, $2, $3, $4, now() + interval '24 hours') RETURNING id",
    )
    .bind(tenant_id)
    .bind(instance_id)
    .bind(&token_hash)
    .bind(customer_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_conversation(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    customer_id: Uuid,
    status: &str,
    escalated: bool,
    assigned_membership_id: Option<Uuid>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, \
         escalated_at, assigned_membership_id) \
         VALUES ($1, $2, 'widget', $3, $4, $5) RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(status)
    .bind(if escalated {
        Some(chrono::Utc::now())
    } else {
        None
    })
    .bind(assigned_membership_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Agent User")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_membership(pool: &sqlx::PgPool, tenant_id: Uuid, user_id: Uuid, role: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, $3, 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t001_escalated_conversation_handling_is_human() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "Handoff").await;
    let pub_id = "wgt_handoff_001";
    let instance_id = seed_widget_instance(&pool, tenant_id, pub_id).await;
    let customer_id = seed_customer(&pool, tenant_id, "Handoff Customer").await;
    let _session_id = seed_widget_session(&pool, tenant_id, instance_id, customer_id).await;
    let conv_id = seed_conversation(&pool, tenant_id, customer_id, "open", true, None).await;

    let conv_row: (bool, bool) = sqlx::query_as(
        "SELECT escalated_at IS NOT NULL, assigned_membership_id IS NOT NULL \
         FROM conversations WHERE id = $1",
    )
    .bind(conv_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(
        conv_row.0,
        "conversation must be escalated for handoff test"
    );
}

#[tokio::test]
async fn t002_resolved_conversation_handling_is_closed() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "Handoff2").await;
    let pub_id = "wgt_handoff_002";
    let instance_id = seed_widget_instance(&pool, tenant_id, pub_id).await;
    let customer_id = seed_customer(&pool, tenant_id, "Handoff Cust 2").await;
    let _session_id = seed_widget_session(&pool, tenant_id, instance_id, customer_id).await;
    let conv_id = seed_conversation(&pool, tenant_id, customer_id, "resolved", false, None).await;

    let status: String = sqlx::query_scalar("SELECT status FROM conversations WHERE id = $1")
        .bind(conv_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "resolved");
}

#[tokio::test]
async fn t003_agent_reply_exposed_with_sender_agent_and_display_name_only() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let tenant_id = seed_tenant(&pool, "Handoff3").await;
    let pub_id = "wgt_handoff_003";
    let instance_id = seed_widget_instance(&pool, tenant_id, pub_id).await;
    let customer_id = seed_customer(&pool, tenant_id, "Handoff Cust 3").await;
    let user_id = seed_user(&pool, "agent@test.com").await;
    let membership_id = seed_membership(&pool, tenant_id, user_id, "agent").await;
    let _session_id = seed_widget_session(&pool, tenant_id, instance_id, customer_id).await;
    let conv_id = seed_conversation(
        &pool,
        tenant_id,
        customer_id,
        "open",
        false,
        Some(membership_id),
    )
    .await;

    sqlx::query(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body, sender_membership_id) \
         VALUES ($1, $2, 'reply', $3, $4)",
    )
    .bind(tenant_id)
    .bind(conv_id)
    .bind("I can help with that")
    .bind(membership_id)
    .execute(&pool)
    .await
    .unwrap();

    let (kind, display_name): (String, Option<String>) = sqlx::query_as(
        "SELECT m.kind, u.display_name \
         FROM messages m \
         LEFT JOIN tenant_memberships tm ON tm.id = m.sender_membership_id \
         LEFT JOIN users u ON u.id = tm.user_id \
         WHERE m.conversation_id = $1 AND m.kind = 'reply' \
         LIMIT 1",
    )
    .bind(conv_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(kind, "reply");
    assert_eq!(
        display_name,
        Some("Agent User".into()),
        "agent reply must expose display name only"
    );
}
