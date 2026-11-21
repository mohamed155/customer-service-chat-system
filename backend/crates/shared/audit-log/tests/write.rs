#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: a failed unwrap is the failed test
//! Writing audit records (ADR-017, MOH-196), on real Postgres as the runtime role.
//!
//! These pin what every module relies on when it records a sensitive action: the record commits
//! and rolls back with the caller's transaction, a record that cannot be written is an error the
//! caller can roll the action back on, one tenant cannot write another's, and a row without a
//! tenant can only be one of the platform events.

use std::time::Duration;

use audit_log::{record, record_in_tx};
use serde_json::json;
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
            eprintln!("skipping audit-log tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping audit-log tests: DATABASE_URL is unreachable");
        return None;
    }
    db::testing::migrate().await;
    Some(pool)
}

async fn seed_tenant() -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Audit Log Tenant")
        .bind(format!("audit-log-{}", Uuid::new_v4().simple()))
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap()
}

async fn seed_user() -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name) VALUES ($1, 'Audit Actor') RETURNING id",
    )
    .bind(format!("audit-{}@example.com", Uuid::new_v4().simple()))
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap()
}

type Row = (
    Option<Uuid>,
    String,
    String,
    Option<Uuid>,
    serde_json::Value,
);

/// The audit rows with this resource id, read past Row-Level Security.
async fn rows(resource_id: &str) -> Vec<Row> {
    sqlx::query_as(
        "SELECT actor_user_id, action, resource_type, tenant_id, details \
         FROM audit_logs WHERE resource_id = $1 ORDER BY created_at, id",
    )
    .bind(resource_id)
    .fetch_all(&db::testing::seed_pool())
    .await
    .unwrap()
}

fn resource() -> String {
    format!("res-{}", Uuid::new_v4().simple())
}

/// The row commits with the caller's transaction and is gone with a rollback.
#[tokio::test]
async fn a_tenant_record_follows_the_callers_transaction() {
    let Some(pool) = get_pool().await else { return };
    let (tenant, actor, resource_id) = (seed_tenant().await, seed_user().await, resource());

    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, tenant)
        .await
        .unwrap();
    record_in_tx(
        uow.conn(),
        "member.invited",
        Some(actor),
        Some(tenant),
        "invitation",
        Some(&resource_id),
        &json!({"role": "agent"}),
    )
    .await
    .unwrap();
    assert!(
        rows(&resource_id).await.is_empty(),
        "nothing is visible before the commit"
    );
    uow.rollback().await.unwrap();
    assert!(
        rows(&resource_id).await.is_empty(),
        "a rollback takes the record"
    );

    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, tenant)
        .await
        .unwrap();
    record_in_tx(
        uow.conn(),
        "member.invited",
        Some(actor),
        Some(tenant),
        "invitation",
        Some(&resource_id),
        &json!({"role": "agent"}),
    )
    .await
    .unwrap();
    uow.commit().await.unwrap();
    assert_eq!(
        rows(&resource_id).await,
        [(
            Some(actor),
            "member.invited".to_string(),
            "invitation".to_string(),
            Some(tenant),
            json!({"role": "agent"}),
        )]
    );
}

/// A record that cannot be written is an error, not a log line, so the caller rolls the action back
/// with it: the action and its audit record are both there or both not.
#[tokio::test]
async fn a_record_that_cannot_be_written_rolls_the_action_back_with_it() {
    let Some(pool) = get_pool().await else { return };
    let tenant = seed_tenant().await;
    let name = format!("Rolled back {}", Uuid::new_v4().simple());

    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, tenant)
        .await
        .unwrap();
    // The action: a change the audit record describes.
    sqlx::query("INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2)")
        .bind(tenant)
        .bind(&name)
        .execute(uow.conn())
        .await
        .unwrap();
    // The record, with no resource id: the table refuses it (`audit_logs_resource_required`).
    let result = record_in_tx(
        uow.conn(),
        "customer.created",
        None,
        Some(tenant),
        "customer",
        None,
        &json!({}),
    )
    .await;
    let error = result.expect_err("a record without a resource id cannot be written");
    assert!(
        error.to_string().contains("audit_logs_resource_required"),
        "the caller sees why: {error}"
    );
    // The caller's answer to that error is to roll the action back.
    uow.rollback().await.unwrap();

    let customers: i64 =
        sqlx::query_scalar("SELECT count(*) FROM customers WHERE display_name = $1")
            .bind(&name)
            .fetch_one(&db::testing::seed_pool())
            .await
            .unwrap();
    assert_eq!(customers, 0, "the action did not happen without its record");
}

/// The nil UUID means "the system": the column references `users`, so it is stored as NULL, in
/// either function.
#[tokio::test]
async fn the_nil_actor_is_the_system_and_is_stored_as_null() {
    let Some(pool) = get_pool().await else { return };
    let tenant = seed_tenant().await;
    let (in_tx, best_effort) = (resource(), resource());

    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, tenant)
        .await
        .unwrap();
    record_in_tx(
        uow.conn(),
        "escalation.created",
        Some(Uuid::nil()),
        Some(tenant),
        "escalation",
        Some(&in_tx),
        &json!({}),
    )
    .await
    .unwrap();
    uow.commit().await.unwrap();
    record(
        &pool,
        "escalation.created",
        Some(Uuid::nil()),
        Some(tenant),
        "escalation",
        Some(&best_effort),
        &json!({}),
    )
    .await;

    for resource_id in [in_tx, best_effort] {
        let written = rows(&resource_id).await;
        assert_eq!(written.len(), 1, "{resource_id}");
        assert_eq!(written[0].0, None, "the actor is NULL for {resource_id}");
    }
}

/// One tenant's transaction cannot write another tenant's audit record.
#[tokio::test]
async fn a_tenants_transaction_cannot_write_another_tenants_record() {
    let Some(pool) = get_pool().await else { return };
    let (a, b) = (seed_tenant().await, seed_tenant().await);
    let resource_id = resource();

    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, a).await.unwrap();
    let result = record_in_tx(
        uow.conn(),
        "member.invited",
        None,
        Some(b),
        "invitation",
        Some(&resource_id),
        &json!({}),
    )
    .await;
    assert!(
        result.is_err(),
        "tenant A's transaction must not write tenant B's record"
    );
    uow.rollback().await.unwrap();
    assert!(rows(&resource_id).await.is_empty());
}

/// A row with no tenant is a platform event: only the listed actions are accepted.
#[tokio::test]
async fn a_record_without_a_tenant_is_accepted_only_as_a_platform_event() {
    let Some(pool) = get_pool().await else { return };
    let resource_id = resource();
    let mut conn = pool.acquire().await.unwrap();

    record_in_tx(
        &mut conn,
        "tenant.access_denied",
        None,
        None,
        "tenant",
        Some(&resource_id),
        &json!({"reason": "not_found"}),
    )
    .await
    .unwrap();
    assert_eq!(
        rows(&resource_id).await,
        [(
            None,
            "tenant.access_denied".to_string(),
            "tenant".to_string(),
            None,
            json!({"reason": "not_found"}),
        )]
    );

    // Any other action without a tenant is refused: the function cannot be used to write a
    // tenantless row of a kind it does not list.
    let other = resource();
    let refused = record_in_tx(
        &mut conn,
        "member.invited",
        None,
        None,
        "invitation",
        Some(&other),
        &json!({}),
    )
    .await;
    assert!(refused.is_err(), "member.invited is not a platform event");
    assert!(rows(&other).await.is_empty());
}

/// `record` writes in a transaction of its own and never fails the caller: a record that cannot be
/// written is logged, and nothing is stored.
#[tokio::test]
async fn record_writes_on_its_own_and_only_logs_a_failure() {
    let Some(pool) = get_pool().await else { return };
    let tenant = seed_tenant().await;
    let (written, platform, refused) = (resource(), resource(), resource());

    record(
        &pool,
        "security.rate_limit.refused",
        None,
        Some(tenant),
        "rate_limit",
        Some(&written),
        &json!({}),
    )
    .await;
    assert_eq!(rows(&written).await.len(), 1);

    record(
        &pool,
        "tenant.access_denied",
        None,
        None,
        "tenant",
        Some(&platform),
        &json!({}),
    )
    .await;
    assert_eq!(
        rows(&platform).await.len(),
        1,
        "a platform event needs no transaction of its own"
    );

    // Cannot be written (a tenant row without a resource id; a tenantless row of another kind):
    // the call returns normally and stores nothing.
    record(&pool, "x.y", None, Some(tenant), "x", None, &json!({})).await;
    record(
        &pool,
        "member.invited",
        None,
        None,
        "invitation",
        Some(&refused),
        &json!({}),
    )
    .await;
    assert!(rows(&refused).await.is_empty());
}
