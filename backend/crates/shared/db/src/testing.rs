//! Test support: the schema-owner connection that test setup needs.
//!
//! Tests run the application code as `app_runtime` (`DATABASE_URL`), which can read and write
//! rows but cannot change the schema or `TRUNCATE`. Setup that needs those rights (applying
//! migrations, resetting tables between tests) goes through the owner connection,
//! `MIGRATION_DATABASE_URL`, the same variable `server migrate` uses.
//!
//! Only for tests: nothing in the server calls this module.

use std::time::Duration;

use sqlx::PgPool;

/// The owner connection string: `MIGRATION_DATABASE_URL`.
///
/// Without it, falls back to `DATABASE_URL` so a developer whose database still has a single
/// superuser login can run the suite. With `REQUIRE_DB_TESTS=1` (CI) the fallback is refused:
/// the suite must prove the runtime role works, so both URLs are required.
///
/// # Panics
/// When neither variable is set, or `REQUIRE_DB_TESTS=1` and `MIGRATION_DATABASE_URL` is unset.
pub fn owner_url() -> String {
    if let Ok(url) = std::env::var("MIGRATION_DATABASE_URL") {
        if !url.trim().is_empty() {
            return url;
        }
    }
    assert!(
        std::env::var("REQUIRE_DB_TESTS").as_deref() != Ok("1"),
        "REQUIRE_DB_TESTS=1 but MIGRATION_DATABASE_URL is not set; test setup needs the app_owner \
         connection (see docs/configuration.md)"
    );
    std::env::var("DATABASE_URL")
        .expect("neither MIGRATION_DATABASE_URL nor DATABASE_URL is set for test setup")
}

/// A small lazy pool on the owner connection, for migrations and table resets.
pub fn owner_pool() -> PgPool {
    crate::lazy_pool(&owner_url(), 2, Duration::from_secs(10))
}

/// Applies the migrations as the schema owner.
///
/// # Panics
/// When a migration fails.
pub async fn migrate() {
    let pool = owner_pool();
    crate::run_migrations(&pool)
        .await
        .expect("applying migrations over MIGRATION_DATABASE_URL failed");
    pool.close().await;
}

// ---------------------------------------------------------------------------------------------
// Proving the cross-tenant functions (MOH-46)
//
// A function in the `xtenant` schema must keep working once Row-Level Security is forced on the
// tables it reads, while `app_runtime` itself reads nothing from those tables without a tenant.
// The policies don't exist yet (MOH-47 to MOH-49), so the tests put them in place for a moment:
// inside one owner transaction that is never committed. DDL is transactional in Postgres, so
// nothing persists and other tests never see it. The transaction then drops to `app_runtime`
// with `SET LOCAL ROLE`, so every query that follows runs exactly as the application would.
// ---------------------------------------------------------------------------------------------

use sqlx::{PgConnection, Postgres, Transaction};

/// Applies the migrations, then opens a transaction on the owner connection. Seed fixtures on it
/// first (the owner is not restricted before [`enforce_rls`]), then call [`enforce_rls`] and
/// [`become_runtime`]. Drop the transaction at the end of the test: it is rolled back.
pub async fn owner_tx() -> Transaction<'static, Postgres> {
    migrate().await;
    owner_pool()
        .begin()
        .await
        .expect("could not open an owner transaction")
}

/// Enables and forces Row-Level Security on `tables` with a policy that matches no row, which is
/// what every tenant-owned table does without an `app.tenant_id`. Needs the owner role.
///
/// # Panics
/// When a name is not a plain identifier, or the statement fails.
pub async fn enforce_rls(conn: &mut PgConnection, tables: &[&str]) {
    // Locks are taken in a fixed order so two tests that share a table cannot deadlock.
    let mut tables = tables.to_vec();
    tables.sort_unstable();
    for table in tables {
        assert!(
            !table.is_empty()
                && table
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "not a plain table name: {table}"
        );
        for statement in [
            format!("ALTER TABLE {table} ENABLE ROW LEVEL SECURITY"),
            format!("ALTER TABLE {table} FORCE ROW LEVEL SECURITY"),
            format!("CREATE POLICY forced_rls_probe ON {table} USING (false) WITH CHECK (false)"),
        ] {
            // The table name was checked above to be a plain identifier.
            sqlx::query(sqlx::AssertSqlSafe(statement.clone()))
                .execute(&mut *conn)
                .await
                .unwrap_or_else(|e| panic!("`{statement}` failed: {e}"));
        }
    }
}

/// Runs the rest of the transaction as `app_runtime`.
pub async fn become_runtime(conn: &mut PgConnection) {
    set_role(conn, "app_runtime").await;
}

/// Runs the rest of the transaction as `app_platform`, which bypasses Row-Level Security. For test
/// assertions that must read rows (an audit row, say) after [`enforce_rls`] has hidden them.
pub async fn become_platform(conn: &mut PgConnection) {
    set_role(conn, "app_platform").await;
}

async fn set_role(conn: &mut PgConnection, role: &'static str) {
    sqlx::query("RESET ROLE")
        .execute(&mut *conn)
        .await
        .expect("RESET ROLE failed");
    // `role` is one of the two literals above, never caller input.
    sqlx::query(sqlx::AssertSqlSafe(format!("SET LOCAL ROLE {role}")))
        .execute(&mut *conn)
        .await
        .unwrap_or_else(|e| {
            panic!("SET LOCAL ROLE {role} failed (is the roles script current?): {e}")
        });
}
