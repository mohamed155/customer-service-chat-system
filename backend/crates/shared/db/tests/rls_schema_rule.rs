//! The schema rule behind tenant isolation (ADR-004 sections 2 and 7; MOH-50), run by CI as part
//! of the backend suite: every tenant-owned table has Row-Level Security enabled and forced, has a
//! policy, and has no single-column foreign key to another tenant-owned table; every other table
//! is explicitly listed as global.
//!
//! The first test is the check itself, run against the migrated schema. The others prove it fails:
//! each plants a table or foreign key that breaks one part of the rule inside a transaction that is rolled back
//! (nothing is left in the schema) and expects the matching message.
//!
//! Needs the role split (`MIGRATION_DATABASE_URL`); skips without it unless `REQUIRE_DB_TESTS=1`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: a failed unwrap is the failed test
use db::testing::isolation::{rls_rule_violations, GLOBAL_TABLES};

fn can_run() -> bool {
    if std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
        || std::env::var("MIGRATION_DATABASE_URL").is_ok_and(|v| !v.is_empty())
    {
        return true;
    }
    eprintln!("skipping RLS schema rule: MIGRATION_DATABASE_URL not set");
    false
}

/// Plants `ddl` in a rolled-back owner transaction and returns the violations about `table`.
async fn violations_after(ddl: &[&str], table: &str) -> Vec<String> {
    let mut tx = db::testing::owner_tx().await;
    for statement in ddl {
        sqlx::query(sqlx::AssertSqlSafe(statement.to_string()))
            .execute(&mut *tx)
            .await
            .unwrap_or_else(|e| panic!("`{statement}` failed: {e}"));
    }
    let all = rls_rule_violations(&mut tx).await;
    // Dropping the transaction rolls the planted table back.
    drop(tx);
    all.into_iter()
        .filter(|v| v.starts_with(&format!("{table}:")))
        .collect()
}

#[tokio::test]
async fn the_migrated_schema_follows_the_rule() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let violations = rls_rule_violations(&mut tx).await;
    assert!(
        violations.is_empty(),
        "tenant isolation rule broken:\n{}",
        violations.join("\n")
    );
}

#[tokio::test]
async fn a_tenant_table_without_row_level_security_is_reported() {
    if !can_run() {
        return;
    }
    let found = violations_after(
        &["CREATE TABLE planted_no_rls (id uuid PRIMARY KEY, tenant_id uuid NOT NULL)"],
        "planted_no_rls",
    )
    .await;
    assert_eq!(
        found.len(),
        3,
        "enabled, forced and policy all missing: {found:?}"
    );
    assert!(found.iter().any(|v| v.contains("not enabled")));
    assert!(found.iter().any(|v| v.contains("not forced")));
    assert!(found.iter().any(|v| v.contains("no policy")));
}

#[tokio::test]
async fn enabled_but_not_forced_is_reported() {
    if !can_run() {
        return;
    }
    let found = violations_after(
        &[
            "CREATE TABLE planted_not_forced (id uuid PRIMARY KEY, tenant_id uuid NOT NULL)",
            "ALTER TABLE planted_not_forced ENABLE ROW LEVEL SECURITY",
            "CREATE POLICY tenant_isolation ON planted_not_forced \
             USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) \
             WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)",
        ],
        "planted_not_forced",
    )
    .await;
    assert_eq!(found.len(), 1, "only the missing FORCE: {found:?}");
    assert!(found[0].contains("not forced"));
}

#[tokio::test]
async fn forced_without_a_complete_policy_is_reported() {
    if !can_run() {
        return;
    }
    // No policy at all.
    let none = violations_after(
        &[
            "CREATE TABLE planted_no_policy (id uuid PRIMARY KEY, tenant_id uuid NOT NULL)",
            "ALTER TABLE planted_no_policy ENABLE ROW LEVEL SECURITY",
            "ALTER TABLE planted_no_policy FORCE ROW LEVEL SECURITY",
        ],
        "planted_no_policy",
    )
    .await;
    assert_eq!(none.len(), 1, "{none:?}");
    assert!(none[0].contains("no policy"));

    // A read-only policy (no WITH CHECK, one command) does not protect writes.
    let partial = violations_after(
        &[
            "CREATE TABLE planted_partial_policy (id uuid PRIMARY KEY, tenant_id uuid NOT NULL)",
            "ALTER TABLE planted_partial_policy ENABLE ROW LEVEL SECURITY",
            "ALTER TABLE planted_partial_policy FORCE ROW LEVEL SECURITY",
            "CREATE POLICY read_only ON planted_partial_policy FOR SELECT \
             USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)",
        ],
        "planted_partial_policy",
    )
    .await;
    assert_eq!(partial.len(), 1, "{partial:?}");
    assert!(partial[0].contains("no policy"));
}

#[tokio::test]
async fn a_table_that_is_neither_tenant_scoped_nor_listed_as_global_is_reported() {
    if !can_run() {
        return;
    }
    let found = violations_after(
        &["CREATE TABLE planted_unclassified (id uuid PRIMARY KEY)"],
        "planted_unclassified",
    )
    .await;
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("not in GLOBAL_TABLES"));
}

#[tokio::test]
async fn a_complete_tenant_table_passes() {
    if !can_run() {
        return;
    }
    let found = violations_after(
        &[
            "CREATE TABLE planted_ok (id uuid PRIMARY KEY, tenant_id uuid NOT NULL)",
            "ALTER TABLE planted_ok ENABLE ROW LEVEL SECURITY",
            "ALTER TABLE planted_ok FORCE ROW LEVEL SECURITY",
            "CREATE POLICY tenant_isolation ON planted_ok \
             USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) \
             WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)",
        ],
        "planted_ok",
    )
    .await;
    assert!(found.is_empty(), "a correct table must pass: {found:?}");
}

#[tokio::test]
async fn a_tenant_to_tenant_single_column_foreign_key_is_reported() {
    if !can_run() {
        return;
    }
    let found = violations_after(
        &[
            "CREATE TABLE planted_fk_parent (id uuid PRIMARY KEY, tenant_id uuid NOT NULL)",
            "CREATE TABLE planted_fk_child (id uuid PRIMARY KEY, tenant_id uuid NOT NULL, parent_id uuid NOT NULL REFERENCES planted_fk_parent(id))",
            "ALTER TABLE planted_fk_parent ENABLE ROW LEVEL SECURITY",
            "ALTER TABLE planted_fk_parent FORCE ROW LEVEL SECURITY",
            "CREATE POLICY tenant_isolation ON planted_fk_parent \
             USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) \
             WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)",
            "ALTER TABLE planted_fk_child ENABLE ROW LEVEL SECURITY",
            "ALTER TABLE planted_fk_child FORCE ROW LEVEL SECURITY",
            "CREATE POLICY tenant_isolation ON planted_fk_child \
             USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) \
             WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)",
        ],
        "planted_fk_child",
    )
    .await;
    assert_eq!(
        found,
        ["planted_fk_child: foreign key planted_fk_child_parent_id_fkey to planted_fk_parent must be composite (tenant_id, ...) -> (tenant_id, id)"]
    );
}

#[tokio::test]
async fn a_tenant_to_tenant_composite_foreign_key_passes() {
    if !can_run() {
        return;
    }
    let found = violations_after(
        &[
            "CREATE TABLE planted_fk_composite_parent (id uuid PRIMARY KEY, tenant_id uuid NOT NULL, UNIQUE (tenant_id, id))",
            "CREATE TABLE planted_fk_composite_child (id uuid PRIMARY KEY, tenant_id uuid NOT NULL, parent_id uuid NOT NULL, FOREIGN KEY (tenant_id, parent_id) REFERENCES planted_fk_composite_parent(tenant_id, id))",
            "ALTER TABLE planted_fk_composite_parent ENABLE ROW LEVEL SECURITY",
            "ALTER TABLE planted_fk_composite_parent FORCE ROW LEVEL SECURITY",
            "CREATE POLICY tenant_isolation ON planted_fk_composite_parent \
             USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) \
             WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)",
            "ALTER TABLE planted_fk_composite_child ENABLE ROW LEVEL SECURITY",
            "ALTER TABLE planted_fk_composite_child FORCE ROW LEVEL SECURITY",
            "CREATE POLICY tenant_isolation ON planted_fk_composite_child \
             USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) \
             WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)",
        ],
        "planted_fk_composite_child",
    )
    .await;
    assert!(
        found.is_empty(),
        "the composite reference must pass: {found:?}"
    );
}

#[tokio::test]
async fn a_single_column_foreign_key_to_a_tenant_partition_is_reported() {
    if !can_run() {
        return;
    }
    let found = violations_after(
        &[
            "CREATE TABLE planted_partition_parent (id uuid NOT NULL, tenant_id uuid NOT NULL, PRIMARY KEY (tenant_id, id)) PARTITION BY LIST (tenant_id)",
            "CREATE TABLE planted_partition_parent_a PARTITION OF planted_partition_parent FOR VALUES IN ('00000000-0000-0000-0000-000000000001')",
            "CREATE UNIQUE INDEX planted_partition_parent_a_id_uq ON planted_partition_parent_a (id)",
            "CREATE TABLE planted_partition_child (id uuid PRIMARY KEY, tenant_id uuid NOT NULL, parent_id uuid NOT NULL REFERENCES planted_partition_parent_a(id))",
            "ALTER TABLE planted_partition_parent ENABLE ROW LEVEL SECURITY",
            "ALTER TABLE planted_partition_parent FORCE ROW LEVEL SECURITY",
            "CREATE POLICY tenant_isolation ON planted_partition_parent \
             USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) \
             WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)",
            "ALTER TABLE planted_partition_child ENABLE ROW LEVEL SECURITY",
            "ALTER TABLE planted_partition_child FORCE ROW LEVEL SECURITY",
            "CREATE POLICY tenant_isolation ON planted_partition_child \
             USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) \
             WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)",
        ],
        "planted_partition_child",
    )
    .await;
    assert_eq!(
        found,
        ["planted_partition_child: foreign key planted_partition_child_parent_id_fkey to planted_partition_parent_a must be composite (tenant_id, ...) -> (tenant_id, id)"]
    );
}

#[tokio::test]
async fn a_single_column_foreign_key_defined_on_a_tenant_partition_is_reported() {
    if !can_run() {
        return;
    }
    let found = violations_after(
        &[
            "CREATE TABLE planted_childpart_parent (id uuid PRIMARY KEY, tenant_id uuid NOT NULL, UNIQUE (tenant_id, id))",
            "CREATE TABLE planted_childpart_child (id uuid NOT NULL, tenant_id uuid NOT NULL, parent_id uuid NOT NULL, PRIMARY KEY (tenant_id, id)) PARTITION BY LIST (tenant_id)",
            "CREATE TABLE planted_childpart_child_a PARTITION OF planted_childpart_child FOR VALUES IN ('00000000-0000-0000-0000-000000000001')",
            "ALTER TABLE planted_childpart_child_a ADD CONSTRAINT planted_childpart_direct_fkey FOREIGN KEY (parent_id) REFERENCES planted_childpart_parent(id)",
            "ALTER TABLE planted_childpart_parent ENABLE ROW LEVEL SECURITY",
            "ALTER TABLE planted_childpart_parent FORCE ROW LEVEL SECURITY",
            "CREATE POLICY tenant_isolation ON planted_childpart_parent \
             USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) \
             WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)",
            "ALTER TABLE planted_childpart_child ENABLE ROW LEVEL SECURITY",
            "ALTER TABLE planted_childpart_child FORCE ROW LEVEL SECURITY",
            "CREATE POLICY tenant_isolation ON planted_childpart_child \
             USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) \
             WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)",
        ],
        "planted_childpart_child_a",
    )
    .await;
    assert_eq!(
        found,
        ["planted_childpart_child_a: foreign key planted_childpart_direct_fkey to planted_childpart_parent must be composite (tenant_id, ...) -> (tenant_id, id)"]
    );
}

#[tokio::test]
async fn a_single_column_foreign_key_to_a_global_table_passes() {
    if !can_run() {
        return;
    }
    let found = violations_after(
        &[
            "CREATE TABLE planted_fk_global_child (id uuid PRIMARY KEY, tenant_id uuid NOT NULL, user_id uuid NOT NULL REFERENCES users(id))",
            "ALTER TABLE planted_fk_global_child ENABLE ROW LEVEL SECURITY",
            "ALTER TABLE planted_fk_global_child FORCE ROW LEVEL SECURITY",
            "CREATE POLICY tenant_isolation ON planted_fk_global_child \
             USING (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid) \
             WITH CHECK (tenant_id = NULLIF(current_setting('app.tenant_id', true), '')::uuid)",
        ],
        "planted_fk_global_child",
    )
    .await;
    assert!(
        found.is_empty(),
        "a reference to global users is allowed: {found:?}"
    );
}

#[tokio::test]
async fn postgres_rejects_a_composite_foreign_key_without_a_matching_unique_parent_key() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    sqlx::query(
        "CREATE TABLE planted_fk_nonunique_parent (id uuid PRIMARY KEY, tenant_id uuid NOT NULL)",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    let error = sqlx::query(
        "CREATE TABLE planted_fk_nonunique_child (id uuid PRIMARY KEY, tenant_id uuid NOT NULL, parent_id uuid NOT NULL, FOREIGN KEY (tenant_id, parent_id) REFERENCES planted_fk_nonunique_parent(tenant_id, id))",
    )
    .execute(&mut *tx)
    .await
    .expect_err("PostgreSQL must require a unique referenced (tenant_id, id) key");
    match error {
        sqlx::Error::Database(error) => assert_eq!(error.code().as_deref(), Some("42830")),
        error => panic!("unexpected non-database error: {error}"),
    }
}

/// The allow-list can only contain the known global platform authorization table and tables
/// without a tenant_id. The planted tables above never persist.
#[tokio::test]
async fn the_global_list_holds_only_the_known_exception_and_planted_tables_leave_nothing_behind() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let tenant_tables: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname::text FROM pg_class c JOIN pg_attribute a ON a.attrelid = c.oid \
         WHERE c.relnamespace = 'public'::regnamespace AND c.relkind = 'r' \
           AND a.attname = 'tenant_id' AND NOT a.attisdropped AND c.relname = ANY($1)",
    )
    .bind(GLOBAL_TABLES)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        tenant_tables,
        ["platform_access_sessions"],
        "only the platform authorization table may carry a tenant_id in GLOBAL_TABLES"
    );
    let leftovers: Vec<String> = sqlx::query_scalar(
        "SELECT relname::text FROM pg_class \
         WHERE relnamespace = 'public'::regnamespace AND relname LIKE 'planted\\_%'",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert!(
        leftovers.is_empty(),
        "planted tables persisted: {leftovers:?}"
    );
}
