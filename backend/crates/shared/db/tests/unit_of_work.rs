//! The Unit of Work sets `app.tenant_id` transaction-locally (ADR-004 §2, MOH-44).
//!
//! The pool has ONE connection in the tests that check for leaks, so the "next user of the pooled
//! connection" is guaranteed to be the same physical connection that just ran the transaction.
use std::time::Duration;

use db::{TenantScoped, UnitOfWork};
use uuid::Uuid;

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool(max_connections: u32) -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            assert!(
                !require_db_tests(),
                "REQUIRE_DB_TESTS=1 but DATABASE_URL is not set; refusing to skip unit of work tests"
            );
            eprintln!("skipping unit of work test: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, max_connections, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        assert!(
            !require_db_tests(),
            "REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable; refusing to skip unit of work tests"
        );
        eprintln!("skipping unit of work test: could not connect to DATABASE_URL");
        return None;
    }
    Some(pool)
}

/// What the ADR-004 policy expression sees: NULL if never set on this connection, '' after an
/// earlier transaction set it locally, the tenant id while inside the transaction.
async fn setting(conn: &mut sqlx::PgConnection) -> Option<String> {
    sqlx::query_scalar("SELECT current_setting('app.tenant_id', true)")
        .fetch_one(conn)
        .await
        .expect("read app.tenant_id")
}

fn is_unset(value: &Option<String>) -> bool {
    value.as_deref().is_none_or(str::is_empty)
}

struct Ctx(Uuid);
impl TenantScoped for Ctx {
    fn tenant_id(&self) -> Uuid {
        self.0
    }
}

#[tokio::test]
async fn setting_is_visible_inside_the_transaction() {
    let Some(pool) = get_pool(1).await else {
        return;
    };
    let tenant = Uuid::new_v4();

    let mut uow = UnitOfWork::begin(&pool, &Ctx(tenant)).await.unwrap();
    assert_eq!(setting(uow.conn()).await, Some(tenant.to_string()));
    assert_eq!(uow.tenant_id(), tenant);
    uow.rollback().await.unwrap();
}

#[tokio::test]
async fn setting_is_gone_after_commit() {
    let Some(pool) = get_pool(1).await else {
        return;
    };

    let uow = UnitOfWork::begin_for_tenant(&pool, Uuid::new_v4())
        .await
        .unwrap();
    uow.commit().await.unwrap();

    // The pool has one connection, so this is the connection the transaction just used.
    let mut conn = pool.acquire().await.unwrap();
    assert!(
        is_unset(&setting(&mut conn).await),
        "app.tenant_id must not survive the commit"
    );
}

#[tokio::test]
async fn setting_is_gone_after_rollback_and_after_drop() {
    let Some(pool) = get_pool(1).await else {
        return;
    };

    let uow = UnitOfWork::begin_for_tenant(&pool, Uuid::new_v4())
        .await
        .unwrap();
    uow.rollback().await.unwrap();
    {
        let mut conn = pool.acquire().await.unwrap();
        assert!(is_unset(&setting(&mut conn).await), "after rollback");
    }

    // Dropped without commit or rollback: sqlx rolls the transaction back when the connection
    // returns to the pool.
    drop(
        UnitOfWork::begin_for_tenant(&pool, Uuid::new_v4())
            .await
            .unwrap(),
    );
    let mut conn = pool.acquire().await.unwrap();
    assert!(is_unset(&setting(&mut conn).await), "after drop");
}

#[tokio::test]
async fn next_unit_of_work_on_the_same_connection_sees_only_its_own_tenant() {
    let Some(pool) = get_pool(1).await else {
        return;
    };
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());

    let uow = UnitOfWork::begin_for_tenant(&pool, a).await.unwrap();
    uow.commit().await.unwrap();

    let mut uow = UnitOfWork::begin_for_tenant(&pool, b).await.unwrap();
    assert_eq!(setting(uow.conn()).await, Some(b.to_string()));
    uow.rollback().await.unwrap();
}

#[tokio::test]
async fn concurrent_units_of_work_keep_separate_tenants() {
    let Some(pool) = get_pool(2).await else {
        return;
    };
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());

    let mut ua = UnitOfWork::begin_for_tenant(&pool, a).await.unwrap();
    let mut ub = UnitOfWork::begin_for_tenant(&pool, b).await.unwrap();
    assert_eq!(setting(ua.conn()).await, Some(a.to_string()));
    assert_eq!(setting(ub.conn()).await, Some(b.to_string()));
    ua.rollback().await.unwrap();
    ub.rollback().await.unwrap();
}

/// Creates a throwaway table as the schema owner: the tests connect as `app_runtime`, which cannot
/// run DDL (not even temporary tables). The default privileges give `app_runtime` DML on it.
async fn create_probe_table() -> String {
    db::testing::migrate().await;
    let table = format!("uow_probe_{}", Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE TABLE {table} (n int)")))
        .execute(&db::testing::owner_pool())
        .await
        .unwrap();
    table
}

async fn drop_probe_table(table: &str) {
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP TABLE {table}")))
        .execute(&db::testing::owner_pool())
        .await
        .unwrap();
}

#[tokio::test]
async fn work_done_through_the_unit_of_work_is_visible_to_itself() {
    let Some(pool) = get_pool(1).await else {
        return;
    };
    let table = create_probe_table().await;

    let mut uow = UnitOfWork::begin_for_tenant(&pool, Uuid::new_v4())
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO {table} VALUES (1)"
    )))
    .execute(uow.conn())
    .await
    .unwrap();
    let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(uow.conn())
        .await
        .unwrap();
    uow.rollback().await.unwrap();
    drop_probe_table(&table).await;

    assert_eq!(n, 1);
}

/// A write survives only if the Unit of Work was committed. A dropped Unit of Work rolls back, which
/// is the defect MOH-44 hit when auto-committing pool writes were moved onto it without a commit.
/// The table is a real one and is read on the pool afterwards, so it proves what other connections
/// see.
#[tokio::test]
async fn only_committed_work_survives_a_commit_a_rollback_and_a_drop() {
    let Some(pool) = get_pool(2).await else {
        return;
    };
    let table = create_probe_table().await;

    for (n, how) in [(1, "commit"), (2, "rollback"), (3, "drop")] {
        let mut uow = UnitOfWork::begin_for_tenant(&pool, Uuid::new_v4())
            .await
            .unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO {table} VALUES ({n})"
        )))
        .execute(uow.conn())
        .await
        .unwrap();
        match how {
            "commit" => uow.commit().await.unwrap(),
            "rollback" => uow.rollback().await.unwrap(),
            _ => drop(uow),
        }
    }

    let survivors: Vec<i32> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT n FROM {table} ORDER BY n"
    )))
    .fetch_all(&pool)
    .await
    .unwrap();
    drop_probe_table(&table).await;

    assert_eq!(survivors, vec![1]);
}
