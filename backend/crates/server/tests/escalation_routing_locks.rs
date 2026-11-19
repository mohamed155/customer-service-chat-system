#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: a failed unwrap is the failed test
//! Routing an escalation picks a member in several statements (who is eligible, who is available,
//! who is least loaded) and then writes the assignment. The pick is locked so that the member
//! cannot be taken off the roster or go away in between (MOH-194, review of the load lookup).

use std::time::Duration;

use escalations::queries::lock_available_in_tx;
use sqlx::PgPool;
use uuid::Uuid;

/// `users.password_hash` must look like an argon2 hash; no one signs in with this one.
const ARGON2_SHAPED: &str = "$argon2id$v=19$m=19456,t=2,p=1$ZGlyZWN0b3JpZXM$ZGlyZWN0b3JpZXM";

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

/// A tenant with one agent whose availability is `state`: returns (tenant, membership).
async fn tenant_with_agent(seed: &PgPool, label: &str, state: &str) -> (Uuid, Uuid) {
    let tag = Uuid::new_v4().simple().to_string();
    let tenant = db::testing::insert_tenant(seed, label, &format!("lock-{label}-{tag}")).await;
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name, password_hash) VALUES ($1, 'Agent', $2) \
         RETURNING id",
    )
    .bind(format!("{label}-{tag}@routing-locks.test"))
    .bind(ARGON2_SHAPED)
    .fetch_one(seed)
    .await
    .unwrap();
    let membership: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, 'agent') \
         RETURNING id",
    )
    .bind(tenant)
    .bind(user)
    .fetch_one(seed)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_availability (tenant_id, membership_id, state, state_changed_at) \
         VALUES ($1, $2, $3, now())",
    )
    .bind(tenant)
    .bind(membership)
    .bind(state)
    .execute(seed)
    .await
    .unwrap();
    (tenant, membership)
}

#[tokio::test]
async fn only_an_available_member_of_this_tenant_passes_the_recheck() {
    let Some(pool) = runtime_pool().await else {
        return;
    };
    db::testing::migrate().await;
    let seed = db::testing::seed_pool();
    let (tenant_a, available) = tenant_with_agent(&seed, "a", "available").await;
    let (_, away) = {
        // Another member of tenant A who has gone away.
        let (_, m) = tenant_with_agent(&seed, "away", "away").await;
        (tenant_a, m)
    };
    let (_tenant_b, other_tenants) = tenant_with_agent(&seed, "b", "available").await;

    let mut tx = db::UnitOfWork::begin_for_tenant(&pool, tenant_a)
        .await
        .unwrap();
    assert!(lock_available_in_tx(&mut tx, tenant_a, available)
        .await
        .unwrap());
    // Away, another tenant's member, and an unknown one are all refused.
    for member in [away, other_tenants, Uuid::new_v4()] {
        assert!(
            !lock_available_in_tx(&mut tx, tenant_a, member)
                .await
                .unwrap(),
            "{member} passed the availability recheck"
        );
    }
}

/// Removing `FOR SHARE` would leave the predicate checks above green, so this uses two real
/// connections: while one transaction holds the pick, a presence change on it has to wait.
#[tokio::test]
async fn a_picked_member_cannot_go_away_until_the_assignment_transaction_ends() {
    let Some(pool) = runtime_pool().await else {
        return;
    };
    db::testing::migrate().await;
    let seed = db::testing::seed_pool();
    let (tenant, member) = tenant_with_agent(&seed, "picked", "available").await;

    let mut holder = db::UnitOfWork::begin_for_tenant(&pool, tenant)
        .await
        .unwrap();
    assert!(lock_available_in_tx(&mut holder, tenant, member)
        .await
        .unwrap());

    let go_away = |seed: PgPool| async move {
        let mut tx = seed.begin().await.unwrap();
        sqlx::query("SET LOCAL lock_timeout = '400ms'")
            .execute(&mut *tx)
            .await
            .unwrap();
        let result = sqlx::query(
            "UPDATE agent_availability SET state = 'away', state_changed_at = now() \
             WHERE tenant_id = $1 AND membership_id = $2",
        )
        .bind(tenant)
        .bind(member)
        .execute(&mut *tx)
        .await;
        (tx, result)
    };
    let (tx, blocked) = go_away(seed.clone()).await;
    match blocked {
        Err(sqlx::Error::Database(e)) => assert_eq!(
            e.code().as_deref(),
            Some("55P03"),
            "the update should have waited on the lock: {e}"
        ),
        other => panic!("a picked member must not be able to go away, got {other:?}"),
    }
    drop(tx);

    holder.commit().await.unwrap();
    let (tx, allowed) = go_away(seed.clone()).await;
    allowed.expect("the change must go through once the lock is released");
    tx.commit().await.unwrap();
    let state: String = sqlx::query_scalar(
        "SELECT state FROM agent_availability WHERE tenant_id = $1 AND membership_id = $2",
    )
    .bind(tenant)
    .bind(member)
    .fetch_one(&seed)
    .await
    .unwrap();
    assert_eq!(state, "away");
}
