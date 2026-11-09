//! The backend connects as `app_runtime`, a role Row-Level Security applies to (ADR-004 §3 and
//! §7, MOH-45). A superuser, a role with BYPASSRLS or a table owner would ignore every policy,
//! so these tests fail if `DATABASE_URL` points at any of them.
use std::time::Duration;

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn runtime_pool() -> Option<sqlx::PgPool> {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        assert!(
            !require_db_tests(),
            "REQUIRE_DB_TESTS=1 but DATABASE_URL is not set; refusing to skip runtime role tests"
        );
        eprintln!("skipping runtime role test: DATABASE_URL not set");
        return None;
    };
    let pool = db::lazy_pool(&url, 1, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        assert!(
            !require_db_tests(),
            "REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable; refusing to skip runtime role tests"
        );
        eprintln!("skipping runtime role test: could not connect to DATABASE_URL");
        return None;
    }
    Some(pool)
}

/// Locally the suite may still run as a single superuser (no `MIGRATION_DATABASE_URL`); the role
/// checks only mean something once the two URLs are split, so they run when CI requires it or
/// when the owner URL is set.
fn role_split_configured() -> bool {
    require_db_tests() || std::env::var("MIGRATION_DATABASE_URL").is_ok_and(|v| !v.is_empty())
}

#[tokio::test]
async fn database_url_connects_as_app_runtime() {
    let Some(pool) = runtime_pool().await else {
        return;
    };
    if !role_split_configured() {
        eprintln!("skipping: MIGRATION_DATABASE_URL not set, so DATABASE_URL is not split yet");
        return;
    }
    let user: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        user, "app_runtime",
        "DATABASE_URL must use the app_runtime role"
    );
}

#[tokio::test]
async fn app_runtime_is_not_superuser_has_no_bypassrls_and_owns_nothing() {
    let Some(pool) = runtime_pool().await else {
        return;
    };
    if !role_split_configured() {
        eprintln!("skipping: MIGRATION_DATABASE_URL not set, so DATABASE_URL is not split yet");
        return;
    }
    db::testing::migrate().await;

    let (superuser, bypassrls, createdb, createrole): (bool, bool, bool, bool) = sqlx::query_as(
        "SELECT rolsuper, rolbypassrls, rolcreatedb, rolcreaterole \
         FROM pg_roles WHERE rolname = 'app_runtime'",
    )
    .fetch_one(&pool)
    .await
    .expect("app_runtime role must exist");
    assert!(!superuser, "app_runtime must not be a superuser");
    assert!(!bypassrls, "app_runtime must not have BYPASSRLS");
    assert!(!createdb, "app_runtime must not have CREATEDB");
    assert!(!createrole, "app_runtime must not have CREATEROLE");

    let owned: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname::text FROM pg_class c \
         WHERE c.relowner = 'app_runtime'::regrole ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        owned.is_empty(),
        "app_runtime must own no relations, owns {owned:?}"
    );

    // And it cannot change the schema, so it cannot make itself an owner later.
    let ddl = sqlx::query("CREATE TABLE runtime_role_probe (n int)")
        .execute(&pool)
        .await;
    assert!(
        ddl.is_err(),
        "app_runtime must not be able to create tables"
    );
}

#[tokio::test]
async fn app_owner_owns_every_application_table() {
    let Some(pool) = runtime_pool().await else {
        return;
    };
    if !role_split_configured() {
        eprintln!("skipping: MIGRATION_DATABASE_URL not set, so DATABASE_URL is not split yet");
        return;
    }
    db::testing::migrate().await;

    let foreign: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname::text FROM pg_class c \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = 'public' AND c.relkind IN ('r', 'p') \
           AND c.relowner <> 'app_owner'::regrole ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        foreign.is_empty(),
        "tables not owned by app_owner (migrations would fail on them): {foreign:?}"
    );

    // app_runtime can still read the tables it does not own (DML grants from migration 0066).
    let tables: i64 = sqlx::query_scalar("SELECT count(*) FROM tenants")
        .fetch_one(&pool)
        .await
        .expect("app_runtime must be able to read application tables");
    assert!(tables >= 0);
}
