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

/// Applies the migrations, then opens a transaction on the owner connection. Every tenant-owned
/// table has forced Row-Level Security, which applies to the owner too: call [`become_platform`]
/// to seed fixtures, then [`become_runtime`] to run as the application. Drop the transaction at the
/// end of the test: it is rolled back.
pub async fn owner_tx() -> Transaction<'static, Postgres> {
    migrate().await;
    owner_pool()
        .begin()
        .await
        .expect("could not open an owner transaction")
}

/// Goes back to the owner role (undoes [`become_runtime`] and [`become_platform`]).
///
/// The owner is **not** exempt from Row-Level Security on a table whose policy is forced, so seed
/// rows for such tables as [`become_platform`] and come back here only for DDL.
pub async fn become_owner(conn: &mut PgConnection) {
    sqlx::query("RESET ROLE")
        .execute(conn)
        .await
        .expect("RESET ROLE failed");
}

/// Runs the rest of the transaction as `app_runtime`.
pub async fn become_runtime(conn: &mut PgConnection) {
    set_role(conn, "app_runtime").await;
}

/// Runs the rest of the transaction as `app_platform`, which bypasses Row-Level Security. For test
/// seeding and for assertions that must read rows (an audit row, say) the application role cannot see.
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

// ---------------------------------------------------------------------------------------------
// Seeding and inspecting rows once Row-Level Security is on
// ---------------------------------------------------------------------------------------------

use std::cell::RefCell;

thread_local! {
    static SEED_POOL: RefCell<Option<PgPool>> = const { RefCell::new(None) };
}

/// A pool for test **fixtures and assertions** on tables under forced Row-Level Security.
///
/// `app_runtime`, the role the code under test runs as, sees only the tenant the transaction set,
/// so a test cannot create a customer, a conversation or a message for a tenant, nor read one back,
/// through the runtime pool. This pool connects as the schema owner and runs `SET ROLE
/// app_platform` (BYPASSRLS, DML only), so it can seed and read every tenant. It cannot change the
/// schema or `TRUNCATE`: use [`owner_pool`] for that.
///
/// One pool is kept per test thread, so a test reuses its connections.
///
/// **Never hand this pool to the application** (`AppState`, a router, a worker, a repository under
/// test). Code that runs on it bypasses Row-Level Security and hides the bugs the policies exist to
/// catch. Use it only in raw `sqlx::query` fixture and assertion statements.
pub fn seed_pool() -> PgPool {
    SEED_POOL.with(|cell| {
        cell.borrow_mut()
            .get_or_insert_with(|| {
                sqlx::postgres::PgPoolOptions::new()
                    .max_connections(3)
                    .acquire_timeout(Duration::from_secs(10))
                    .after_connect(|conn, _meta| {
                        Box::pin(async move {
                            sqlx::query("SET ROLE app_platform").execute(conn).await?;
                            Ok(())
                        })
                    })
                    .connect_lazy(&owner_url())
                    .expect("invalid MIGRATION_DATABASE_URL")
            })
            .clone()
    })
}

// ---------------------------------------------------------------------------------------------
// Tenant-isolation harness (ADR-004 section 7, gap T5)
// ---------------------------------------------------------------------------------------------

/// The wrong-tenant checks every table under forced Row-Level Security must pass. See
/// `docs/security/tenant-unit-of-work.md` ("The tenant-isolation harness") for how to use it.
pub mod isolation {
    use super::PgPool;
    use crate::UnitOfWork;
    use uuid::Uuid;

    /// A table under test and one row in each of two tenants. The rows must already exist; create
    /// them as the bypass role ([`super::seed_pool`]).
    pub struct Probe<'a> {
        pub table: &'a str,
        pub id_column: &'a str,
        pub tenant_a: Uuid,
        pub tenant_b: Uuid,
        pub row_a: Uuid,
        pub row_b: Uuid,
        /// A statement that inserts a valid row for the tenant bound as `$1`. Its other
        /// placeholders (`$2` onwards) are bound to `insert_args`.
        pub insert_sql: &'a str,
        pub insert_args: &'a [Uuid],
    }

    /// The statement must have failed because of the row-level policy (SQLSTATE 42501), not for
    /// some unrelated reason such as a typo in the fixture.
    fn assert_rls_violation<T>(result: Result<T, sqlx::Error>, what: &str) {
        match result {
            Ok(_) => panic!("{what} was accepted"),
            Err(sqlx::Error::Database(e)) => assert_eq!(
                e.code().as_deref(),
                Some("42501"),
                "{what} failed, but not on the row-level policy: {e}"
            ),
            Err(e) => panic!("{what} failed with a non-database error: {e}"),
        }
    }

    fn assert_plain(name: &str) {
        assert!(
            name.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "not a plain identifier: {name}"
        );
    }

    async fn count(uow: &mut UnitOfWork, sql: String, bind: Option<Uuid>) -> i64 {
        let q = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql));
        let q = if let Some(b) = bind { q.bind(b) } else { q };
        q.fetch_one(uow.conn()).await.expect("count failed")
    }

    async fn affected(uow: &mut UnitOfWork, sql: String, bind: Uuid) -> u64 {
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(bind)
            .execute(uow.conn())
            .await
            .expect("statement failed")
            .rows_affected()
    }

    /// Runs the whole battery on `runtime_pool` (the app's pool, `app_runtime`):
    ///
    /// - as the right tenant: its own row is visible (so the denials below are the policy, not an
    ///   empty table);
    /// - as the wrong tenant: reading by id, listing, a no-op update, a delete and moving its own
    ///   row to the other tenant all fail to reach the other tenant's row, and the row is intact;
    /// - with no tenant at all: nothing is visible, nothing is updated or deleted, and an insert
    ///   is refused.
    ///
    /// # Panics
    /// With a message naming the table and the check that leaked.
    pub async fn assert_isolated(runtime_pool: &PgPool, p: &Probe<'_>) {
        assert_plain(p.table);
        assert_plain(p.id_column);
        let (t, id) = (p.table, p.id_column);
        // `outbox_events.tenant_id` is TEXT, every other tenant column is uuid: bind the tenant as
        // text and cast it back to the column's own type where it is assigned.
        let tenant_type: String = sqlx::query_scalar(
            "SELECT format_type(a.atttypid, a.atttypmod) FROM pg_attribute a \
             WHERE a.attrelid = format('public.%I', $1::text)::regclass \
               AND a.attname = 'tenant_id' AND NOT a.attisdropped",
        )
        .bind(t)
        .fetch_one(runtime_pool)
        .await
        .expect("tenant_id column type");
        assert_plain(&tenant_type);

        // The right tenant sees its own row.
        let mut uow = UnitOfWork::begin_for_tenant(runtime_pool, p.tenant_a)
            .await
            .unwrap();
        assert_eq!(
            count(
                &mut uow,
                format!("SELECT count(*) FROM {t} WHERE {id} = $1"),
                Some(p.row_a)
            )
            .await,
            1,
            "{t}: the owning tenant must see its own row"
        );
        drop(uow);

        // The wrong tenant.
        let mut uow = UnitOfWork::begin_for_tenant(runtime_pool, p.tenant_b)
            .await
            .unwrap();
        assert_eq!(
            count(
                &mut uow,
                format!("SELECT count(*) FROM {t} WHERE {id} = $1"),
                Some(p.row_a)
            )
            .await,
            0,
            "{t}: read by guessed id leaked another tenant's row"
        );
        assert_eq!(
            count(
                &mut uow,
                format!("SELECT count(*) FROM {t} WHERE tenant_id::text = $1::uuid::text"),
                Some(p.tenant_a)
            )
            .await,
            0,
            "{t}: listing leaked another tenant's rows"
        );
        assert_eq!(
            affected(
                &mut uow,
                format!("UPDATE {t} SET tenant_id = tenant_id WHERE {id} = $1"),
                p.row_a
            )
            .await,
            0,
            "{t}: update reached another tenant's row"
        );
        assert_eq!(
            affected(
                &mut uow,
                format!("DELETE FROM {t} WHERE {id} = $1"),
                p.row_a
            )
            .await,
            0,
            "{t}: delete reached another tenant's row"
        );
        // Handing one's own row to the other tenant must be refused (WITH CHECK).
        let moved = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {t} SET tenant_id = $1::uuid::text::{tenant_type} WHERE {id} = $2"
        )))
        .bind(p.tenant_a)
        .bind(p.row_b)
        .execute(uow.conn())
        .await;
        // An append-only table (a trigger refuses every UPDATE, SQLSTATE P0001) never reaches the
        // policy here; the insert check below still proves its WITH CHECK.
        if !matches!(&moved, Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("P0001"))
        {
            assert_rls_violation(moved, &format!("{t}: moving a row to another tenant"));
        }
        drop(uow); // rolled back

        // No tenant at all (a plain pool transaction, as the application never does).
        let mut tx = runtime_pool.begin().await.unwrap();
        for (what, sql) in [("read", format!("SELECT count(*) FROM {t}"))] {
            let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
                .fetch_one(&mut *tx)
                .await
                .unwrap();
            assert_eq!(n, 0, "{t}: {what} without a tenant returned rows");
        }
        for (what, sql) in [
            (
                "update",
                format!("UPDATE {t} SET tenant_id = tenant_id WHERE {id} = $1"),
            ),
            ("delete", format!("DELETE FROM {t} WHERE {id} = $1")),
        ] {
            let n = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(p.row_a)
                .execute(&mut *tx)
                .await
                .unwrap()
                .rows_affected();
            assert_eq!(n, 0, "{t}: {what} without a tenant reached a row");
        }
        let mut insert =
            sqlx::query(sqlx::AssertSqlSafe(p.insert_sql.to_string())).bind(p.tenant_a);
        for arg in p.insert_args {
            insert = insert.bind(*arg);
        }
        assert_rls_violation(
            insert.execute(&mut *tx).await,
            &format!("{t}: an insert without a tenant"),
        );
        drop(tx);

        // Nothing above changed the rows (read as the bypass role).
        let seed = super::seed_pool();
        for row in [p.row_a, p.row_b] {
            let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "SELECT count(*) FROM {t} WHERE {id} = $1"
            )))
            .bind(row)
            .fetch_one(&seed)
            .await
            .unwrap();
            assert_eq!(n, 1, "{t}: a row changed during the isolation checks");
        }
    }

    /// Tables in `public` that deliberately have no `tenant_id` and no Row-Level Security: shared
    /// reference data and platform-wide records. **Adding a table here is a decision to keep it
    /// out of tenant isolation**; a table with a `tenant_id` column can never be listed (it must be
    /// protected), and a table with neither is reported until it is classified one way or the other.
    pub const GLOBAL_TABLES: &[&str] = &[
        "_sqlx_migrations",
        "billing_plans",
        "integration_catalog",
        "mfa_factors",
        "mfa_recovery_codes",
        "revoked_sessions",
        "secret_keks",
        "tenants",
        "users",
    ];

    /// Checks the tenant-isolation rule against the current schema (ADR-004 sections 2 and 7) and
    /// returns one message per violation; an empty list means the rule holds:
    ///
    /// - every table in `public` with a `tenant_id` column has Row-Level Security enabled **and**
    ///   forced, and a policy for all commands with both `USING` and `WITH CHECK`;
    /// - every other table is in [`GLOBAL_TABLES`], so a new table cannot skip the decision;
    /// - every entry of [`GLOBAL_TABLES`] still exists.
    pub async fn rls_rule_violations(conn: &mut sqlx::PgConnection) -> Vec<String> {
        let tables: Vec<(String, bool, bool, bool, bool)> = sqlx::query_as(
            "SELECT c.relname::text, \
                    EXISTS (SELECT 1 FROM pg_attribute a WHERE a.attrelid = c.oid \
                              AND a.attname = 'tenant_id' AND NOT a.attisdropped), \
                    c.relrowsecurity, c.relforcerowsecurity, \
                    EXISTS (SELECT 1 FROM pg_policy p WHERE p.polrelid = c.oid \
                              AND p.polcmd = '*' AND p.polqual IS NOT NULL \
                              AND p.polwithcheck IS NOT NULL) \
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = 'public' AND c.relkind IN ('r', 'p') AND NOT c.relispartition \
             ORDER BY c.relname::text COLLATE \"C\"",
        )
        .fetch_all(&mut *conn)
        .await
        .expect("could not read the table catalog");

        let mut violations = Vec::new();
        for (name, has_tenant_id, enabled, forced, has_policy) in &tables {
            if *has_tenant_id {
                if GLOBAL_TABLES.contains(&name.as_str()) {
                    violations.push(format!(
                        "{name}: has a tenant_id column, so it cannot be a global table; remove it from GLOBAL_TABLES and protect it"
                    ));
                }
                if !enabled {
                    violations.push(format!(
                        "{name}: has a tenant_id column but Row-Level Security is not enabled"
                    ));
                }
                if !forced {
                    violations.push(format!(
                        "{name}: has a tenant_id column but Row-Level Security is not forced (the owner would bypass it)"
                    ));
                }
                if !has_policy {
                    violations.push(format!(
                        "{name}: has a tenant_id column but no policy for all commands with USING and WITH CHECK"
                    ));
                }
            } else if !GLOBAL_TABLES.contains(&name.as_str()) {
                violations.push(format!(
                    "{name}: has no tenant_id column and is not in GLOBAL_TABLES; add a tenant_id with Row-Level Security, or list it as global"
                ));
            }
        }
        for global in GLOBAL_TABLES {
            if !tables.iter().any(|(name, ..)| name == global) {
                violations.push(format!(
                    "{global}: listed in GLOBAL_TABLES but the table does not exist"
                ));
            }
        }
        violations
    }

    /// The tables that currently have forced Row-Level Security (`relforcerowsecurity`), in byte order
    /// (`COLLATE "C"`, the order of a Rust `sort`), independent of the database collation.
    pub async fn forced_rls_tables(pool: &PgPool) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT c.relname::text FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE n.nspname = 'public' AND c.relkind = 'r' AND c.relforcerowsecurity \
             ORDER BY c.relname::text COLLATE \"C\"",
        )
        .fetch_all(pool)
        .await
        .expect("could not list forced-RLS tables")
    }
}
