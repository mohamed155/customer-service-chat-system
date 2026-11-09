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
