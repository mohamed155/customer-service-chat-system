use std::time::Duration;

use chrono::SubsecRound;

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            assert!(
                !require_db_tests(),
                "REQUIRE_DB_TESTS=1 but DATABASE_URL is not set; refusing to skip schema tests"
            );
            eprintln!("skipping schema test: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        assert!(
            !require_db_tests(),
            "REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable; refusing to skip schema tests"
        );
        eprintln!("skipping schema test: could not connect to DATABASE_URL");
        return None;
    }
    Some(pool)
}

#[tokio::test]
async fn run_migrations_succeeds() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool)
        .await
        .expect("run_migrations should succeed");
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT version, description FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .expect("query _sqlx_migrations");
    let expected = vec![
        (1i64, "init"),
        (2, "outbox"),
        (3, "users"),
        (4, "tenants"),
        (5, "tenant memberships"),
        (6, "audit logs"),
        (7, "auth"),
        (8, "cascade fix"),
        (9, "membership guard and slug audit"),
        (10, "audit enhancements"),
        (11, "membership guard update"),
        (12, "slug audit actor"),
        (13, "audit resource required"),
        (14, "slug audit require actor"),
        (15, "slug audit transaction actor"),
        (16, "tenant business metadata"),
        (17, "tenant directory indexes"),
        (18, "membership status"),
        (19, "tenant invitations"),
        (20, "invitation email delivery status"),
        (21, "invitation delivery outbox"),
        (22, "outbox delivery claims"),
        (23, "invitation expired status"),
        (24, "invitation expired invariant"),
        (25, "customers"),
        (26, "conversations"),
        (27, "composite fk customer children"),
        (28, "customer search indexes"),
        (29, "identifier soft delete"),
        (30, "customer identifier cascade"),
        (31, "invitation delivery error"),
        (32, "normalize identifiers"),
        (33, "conversation core"),
        (34, "messages"),
        (35, "agent skills"),
        (36, "agent availability"),
        (37, "escalations"),
        (38, "ai configurations"),
        (39, "ai credentials"),
        (40, "ai usage records"),
    ];
    assert_eq!(
        rows.len(),
        expected.len(),
        "unexpected number of applied migrations"
    );
    for (i, (version, description)) in expected.iter().enumerate() {
        assert_eq!(rows[i].0, *version, "migration version {} mismatch", i + 1);
        assert_eq!(
            rows[i].1, *description,
            "migration description mismatch at version {}",
            version
        );
    }
}

#[tokio::test]
async fn run_migrations_idempotent() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool)
        .await
        .expect("first run should succeed");
    db::run_migrations(&pool)
        .await
        .expect("second run should succeed (no-op)");
}

fn valid_email() -> String {
    format!("test_{}@example.com", uuid::Uuid::new_v4())
}

/// Truncate a `DateTime<Utc>` to PostgreSQL `TIMESTAMPTZ` microsecond precision
/// so that values written via SQLx round-trip back identically. Without this,
/// `Utc::now()` carries nanoseconds that PG silently truncates on insert,
/// causing read-back comparisons to fail by a few hundred nanoseconds.
fn truncate_to_micros(ts: chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono::Utc> {
    ts.trunc_subsecs(6)
}

fn valid_slug() -> String {
    format!(
        "tenant-{}",
        uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
    )
}

// ---------------------------------------------------------------------------
// US2 — Users (T007)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn users_valid_insert_accepted() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let email = valid_email();
    let result = sqlx::query("INSERT INTO users (email, display_name) VALUES ($1, $2)")
        .bind(&email)
        .bind("Alice")
        .execute(&pool)
        .await;
    assert!(
        result.is_ok(),
        "valid user insert should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn users_duplicate_email_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let base = format!("dup_{}@example.com", uuid::Uuid::new_v4());
    sqlx::query("INSERT INTO users (email, display_name) VALUES ($1, $2)")
        .bind(&base)
        .bind("First")
        .execute(&pool)
        .await
        .unwrap();
    let variant = base.to_uppercase(); // e.g. DUP_...@EXAMPLE.COM
    let result = sqlx::query("INSERT INTO users (email, display_name) VALUES ($1, $2)")
        .bind(&variant)
        .bind("Second")
        .execute(&pool)
        .await;
    assert!(result.is_err(), "duplicate active email should be rejected");
}

#[tokio::test]
async fn users_email_without_at_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let result = sqlx::query("INSERT INTO users (email, display_name) VALUES ($1, $2)")
        .bind("notanemail")
        .bind("No At")
        .execute(&pool)
        .await;
    assert!(result.is_err(), "email without @ should be rejected");
}

#[tokio::test]
async fn users_unknown_platform_role_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let result =
        sqlx::query("INSERT INTO users (email, display_name, platform_role) VALUES ($1, $2, $3)")
            .bind(valid_email())
            .bind("Bad Role")
            .bind("nonexistent_role")
            .execute(&pool)
            .await;
    assert!(result.is_err(), "unknown platform_role should be rejected");
}

// ---------------------------------------------------------------------------
// US2 — Tenants (T008)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tenants_valid_insert_accepted() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let result = sqlx::query("INSERT INTO tenants (name, slug) VALUES ($1, $2)")
        .bind("Test Corp")
        .bind(valid_slug())
        .execute(&pool)
        .await;
    assert!(
        result.is_ok(),
        "valid tenant insert should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn tenants_duplicate_slug_rejected_case_insensitive() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let slug = format!(
        "corp-{}",
        uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
    );
    sqlx::query("INSERT INTO tenants (name, slug) VALUES ($1, $2)")
        .bind("First Corp")
        .bind(&slug)
        .execute(&pool)
        .await
        .unwrap();
    let upper_slug = slug.to_uppercase();
    let result = sqlx::query("INSERT INTO tenants (name, slug) VALUES ($1, $2)")
        .bind("Second Corp")
        .bind(&upper_slug)
        .execute(&pool)
        .await;
    assert!(result.is_err(), "duplicate active slug should be rejected");
}

#[tokio::test]
async fn tenants_malformed_slug_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let result = sqlx::query("INSERT INTO tenants (name, slug) VALUES ($1, $2)")
        .bind("Bad Slug Co")
        .bind("Bad_Slug!") // underscores and exclamation marks are invalid
        .execute(&pool)
        .await;
    assert!(result.is_err(), "malformed slug should be rejected");
}

#[tokio::test]
async fn tenants_archived_status_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let result = sqlx::query("INSERT INTO tenants (name, slug, status) VALUES ($1, $2, $3)")
        .bind("Archived Co")
        .bind(valid_slug())
        .bind("archived")
        .execute(&pool)
        .await;
    assert!(result.is_err(), "status 'archived' should be rejected");
}

#[tokio::test]
async fn tenants_slug_rename_to_free_succeeds() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let actor_id = seed_user(&pool).await;
    let slug = valid_slug();
    let mut conn = pool.acquire().await.expect("acquire conn");
    sqlx::query("INSERT INTO tenants (name, slug) VALUES ($1, $2)")
        .bind("Rename Co")
        .bind(&slug)
        .execute(&mut *conn)
        .await
        .unwrap();
    let new_slug = format!(
        "renamed-{}",
        uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
    );
    sqlx::query("BEGIN")
        .execute(&mut *conn)
        .await
        .expect("begin");
    sqlx::query("SELECT set_audit_actor($1)")
        .bind(actor_id)
        .execute(&mut *conn)
        .await
        .expect("set actor");
    let result = sqlx::query("UPDATE tenants SET slug = $1 WHERE slug = $2")
        .bind(&new_slug)
        .bind(&slug)
        .execute(&mut *conn)
        .await;
    sqlx::query("COMMIT")
        .execute(&mut *conn)
        .await
        .expect("commit");
    assert!(result.is_ok(), "slug rename to a free value should succeed");
}

#[tokio::test]
async fn tenants_slug_rename_to_taken_active_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let actor_id = seed_user(&pool).await;
    let slug_a = valid_slug();
    let slug_b = valid_slug();
    sqlx::query("INSERT INTO tenants (name, slug) VALUES ($1, $2)")
        .bind("Tenant A")
        .bind(&slug_a)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tenants (name, slug) VALUES ($1, $2)")
        .bind("Tenant B")
        .bind(&slug_b)
        .execute(&pool)
        .await
        .unwrap();
    let mut conn = pool.acquire().await.expect("acquire conn");
    sqlx::query("BEGIN")
        .execute(&mut *conn)
        .await
        .expect("begin");
    sqlx::query("SELECT set_audit_actor($1)")
        .bind(actor_id)
        .execute(&mut *conn)
        .await
        .expect("set actor");
    let result = sqlx::query("UPDATE tenants SET slug = $1 WHERE slug = $2")
        .bind(&slug_a)
        .bind(&slug_b)
        .execute(&mut *conn)
        .await;
    sqlx::query("COMMIT")
        .execute(&mut *conn)
        .await
        .expect("commit");
    assert!(
        result.is_err(),
        "slug rename to a taken active slug should be rejected"
    );
}

// ---------------------------------------------------------------------------
// US2 — Tenant Memberships (T009)
// ---------------------------------------------------------------------------

/// Insert a minimal user and return its id.
async fn seed_user(pool: &sqlx::PgPool) -> uuid::Uuid {
    sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(valid_email())
    .bind("Seed User")
    .fetch_one(pool)
    .await
    .expect("seed user")
}

/// Insert a minimal tenant and return its id.
async fn seed_tenant(pool: &sqlx::PgPool) -> uuid::Uuid {
    sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id",
    )
    .bind("Seed Tenant")
    .bind(valid_slug())
    .fetch_one(pool)
    .await
    .expect("seed tenant")
}

/// Insert a minimal customer for a tenant and return its id.
async fn seed_customer(pool: &sqlx::PgPool, tenant_id: uuid::Uuid, name: &str) -> uuid::Uuid {
    sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO customers (tenant_id, display_name, email) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(name)
    .bind(format!("{}@example.com", name))
    .fetch_one(pool)
    .await
    .expect("seed customer")
}

#[tokio::test]
async fn memberships_fk_missing_user_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let fake_id = uuid::Uuid::nil();
    let result = sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)",
    )
    .bind(tenant_id)
    .bind(fake_id)
    .bind("agent")
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "membership with missing user FK should be rejected"
    );
}

#[tokio::test]
async fn memberships_fk_missing_tenant_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let user_id = seed_user(&pool).await;
    let fake_id = uuid::Uuid::nil();
    let result = sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)",
    )
    .bind(fake_id)
    .bind(user_id)
    .bind("agent")
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "membership with missing tenant FK should be rejected"
    );
}

#[tokio::test]
async fn memberships_valid_insert_accepted() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let user_id = seed_user(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let result = sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind("agent")
    .execute(&pool)
    .await;
    assert!(
        result.is_ok(),
        "valid membership insert should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn memberships_unknown_role_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let user_id = seed_user(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let result = sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind("superadmin")
    .execute(&pool)
    .await;
    assert!(result.is_err(), "unknown role should be rejected");
}

#[tokio::test]
async fn memberships_duplicate_active_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let user_id = seed_user(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(tenant_id)
        .bind(user_id)
        .bind("agent")
        .execute(&pool)
        .await
        .unwrap();
    let result = sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind("admin")
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "duplicate active membership should be rejected"
    );
}

// ---------------------------------------------------------------------------
// US2 — Audit Logs (T010)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn audit_logs_full_insert_accepted() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let user_id = seed_user(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let details = serde_json::json!({"reason": "test"});
    let unique_action = format!("test.action.{}", uuid::Uuid::new_v4());
    let result = sqlx::query(
        "INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(user_id)
    .bind(&unique_action)
    .bind("test_resource")
    .bind("res-1")
    .bind(tenant_id)
    .bind(&details)
    .execute(&pool)
    .await;
    assert!(
        result.is_ok(),
        "full audit insert should succeed: {:?}",
        result.err()
    );
    let rows: Vec<(serde_json::Value,)> =
        sqlx::query_as("SELECT details FROM audit_logs WHERE action = $1")
            .bind(&unique_action)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, details);
}

#[tokio::test]
async fn audit_logs_platform_entry_tenant_null() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let result = sqlx::query(
        "INSERT INTO audit_logs (action, resource_type, resource_id, tenant_id) VALUES ($1, $2, $3, $4)",
    )
    .bind("platform.action")
    .bind("config")
    .bind("cfg-1")
    .bind(Option::<uuid::Uuid>::None)
    .execute(&pool)
    .await;
    assert!(
        result.is_ok(),
        "platform-level audit entry (tenant_id NULL) should be accepted"
    );
}

#[tokio::test]
async fn audit_logs_system_entry_actor_null() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let result = sqlx::query(
        "INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(Option::<uuid::Uuid>::None)
    .bind("system.action")
    .bind("scheduler")
    .bind("sys-1")
    .execute(&pool)
    .await;
    assert!(
        result.is_ok(),
        "system audit entry (actor_user_id NULL, resource_id set) should be accepted"
    );
}

#[tokio::test]
async fn audit_logs_details_defaults_to_empty_jsonb() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let result: (serde_json::Value,) = sqlx::query_as(
        "INSERT INTO audit_logs (action, resource_type, resource_id) VALUES ('test.defaults', 'test', 'res-default') RETURNING details",
    )
    .fetch_one(&pool)
    .await
    .expect("audit insert with default details");
    assert_eq!(
        result.0,
        serde_json::json!({}),
        "details should default to '{{}}'"
    );
}

// ---------------------------------------------------------------------------
// US3 — Auto-populated UUID PK and timestamps (T016)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn convention_users_bare_insert_receives_uuid_and_timestamps() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let row: (uuid::Uuid, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id, created_at, updated_at",
    )
    .bind(valid_email())
    .bind("UUID Test")
    .fetch_one(&pool)
    .await
    .expect("bare insert");
    assert_ne!(row.0.as_u128(), 0, "UUID should be auto-generated");
    assert!(
        row.1 <= chrono::Utc::now() + chrono::Duration::seconds(1),
        "created_at should be ~now"
    );
    assert!(
        row.2 <= chrono::Utc::now() + chrono::Duration::seconds(1),
        "updated_at should be ~now"
    );
}

#[tokio::test]
async fn convention_tenants_bare_insert_receives_uuid_and_timestamps() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let row: (
        uuid::Uuid,
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
    ) = sqlx::query_as(
        "INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id, created_at, updated_at",
    )
    .bind("UUID Tenant")
    .bind(valid_slug())
    .fetch_one(&pool)
    .await
    .expect("bare tenant insert");
    assert_ne!(row.0.as_u128(), 0);
    assert!(
        row.1 <= chrono::Utc::now() + chrono::Duration::seconds(1),
        "created_at should be ~now"
    );
    assert!(
        row.2 <= chrono::Utc::now() + chrono::Duration::seconds(1),
        "updated_at should be ~now"
    );
}

#[tokio::test]
async fn convention_memberships_bare_insert_receives_uuid_and_timestamps() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let uid = seed_user(&pool).await;
    let tid = seed_tenant(&pool).await;
    let row: (uuid::Uuid, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3) RETURNING id, created_at, updated_at",
    )
    .bind(tid)
    .bind(uid)
    .bind("agent")
    .fetch_one(&pool)
    .await
    .expect("bare membership insert");
    assert_ne!(row.0.as_u128(), 0);
    assert!(
        row.1 <= chrono::Utc::now() + chrono::Duration::seconds(1),
        "created_at should be ~now"
    );
    assert!(
        row.2 <= chrono::Utc::now() + chrono::Duration::seconds(1),
        "updated_at should be ~now"
    );
}

#[tokio::test]
async fn convention_audit_logs_bare_insert_receives_uuid_and_timestamps() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let row: (uuid::Uuid, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "INSERT INTO audit_logs (action, resource_type, resource_id) VALUES ($1, $2, $3) RETURNING id, created_at, updated_at",
    )
    .bind("test.auto_id")
    .bind("test")
    .bind("res-auto")
    .fetch_one(&pool)
    .await
    .expect("bare audit insert");
    assert_ne!(row.0.as_u128(), 0);
    assert!(
        row.1 <= chrono::Utc::now() + chrono::Duration::seconds(1),
        "created_at should be ~now"
    );
    assert!(
        row.2 <= chrono::Utc::now() + chrono::Duration::seconds(1),
        "updated_at should be ~now"
    );
}

#[tokio::test]
async fn convention_update_advances_updated_at() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let (id, created, updated): (uuid::Uuid, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id, created_at, updated_at",
    )
    .bind(valid_email())
    .bind("Advance Test")
    .fetch_one(&pool)
    .await
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    sqlx::query("UPDATE users SET display_name = $1 WHERE id = $2")
        .bind("Updated Name")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (new_created, new_updated): (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as("SELECT created_at, updated_at FROM users WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        new_updated > updated,
        "updated_at should advance after UPDATE"
    );
    assert_eq!(
        created, new_created,
        "created_at should remain unchanged after user UPDATE"
    );
}

// ---------------------------------------------------------------------------
// US3 — Soft-delete semantics (T017)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn soft_delete_user_same_email_reused() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let email = valid_email();
    let id: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
            .bind(&email)
            .bind("Original")
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let result = sqlx::query("INSERT INTO users (email, display_name) VALUES ($1, $2)")
        .bind(&email)
        .bind("Replacement")
        .execute(&pool)
        .await;
    assert!(
        result.is_ok(),
        "same email after soft-delete should be accepted"
    );
}

#[tokio::test]
async fn soft_delete_tenant_same_slug_reused() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let slug = valid_slug();
    let id: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
            .bind("Original Tenant")
            .bind(&slug)
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE tenants SET deleted_at = now() WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let result = sqlx::query("INSERT INTO tenants (name, slug) VALUES ($1, $2)")
        .bind("Replacement Tenant")
        .bind(&slug)
        .execute(&pool)
        .await;
    assert!(
        result.is_ok(),
        "same slug after soft-delete should be accepted"
    );
}

#[tokio::test]
async fn soft_delete_membership_recreate_pair() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let uid = seed_user(&pool).await;
    let tid = seed_tenant(&pool).await;
    let mid: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tid)
    .bind(uid)
    .bind("agent")
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE tenant_memberships SET deleted_at = now() WHERE id = $1")
        .bind(mid)
        .execute(&pool)
        .await
        .unwrap();
    let result = sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)",
    )
    .bind(tid)
    .bind(uid)
    .bind("admin")
    .execute(&pool)
    .await;
    assert!(
        result.is_ok(),
        "same (tenant, user) after soft-delete should be accepted"
    );
}

// ---------------------------------------------------------------------------
// US3 — Cascade rules (T018)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cascade_soft_delete_tenant_stamps_memberships() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let uid = seed_user(&pool).await;
    let tid = seed_tenant(&pool).await;
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(tid)
        .bind(uid)
        .bind("agent")
        .execute(&pool)
        .await
        .unwrap();
    let deleted_at = chrono::Utc::now();
    sqlx::query("UPDATE tenants SET deleted_at = $1 WHERE id = $2")
        .bind(deleted_at)
        .bind(tid)
        .execute(&pool)
        .await
        .unwrap();
    let mem_deleted: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT deleted_at FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tid)
    .bind(uid)
    .fetch_one(&pool)
    .await
    .expect("membership should exist");
    assert!(
        mem_deleted.is_some(),
        "membership should be cascade-soft-deleted"
    );
}

#[tokio::test]
async fn cascade_soft_delete_user_stamps_memberships() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let uid = seed_user(&pool).await;
    let tid = seed_tenant(&pool).await;
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(tid)
        .bind(uid)
        .bind("agent")
        .execute(&pool)
        .await
        .unwrap();
    let deleted_at = chrono::Utc::now();
    sqlx::query("UPDATE users SET deleted_at = $1 WHERE id = $2")
        .bind(deleted_at)
        .bind(uid)
        .execute(&pool)
        .await
        .unwrap();
    let mem_deleted: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT deleted_at FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tid)
    .bind(uid)
    .fetch_one(&pool)
    .await
    .expect("membership should exist");
    assert!(
        mem_deleted.is_some(),
        "membership should be cascade-soft-deleted from user"
    );
}

#[tokio::test]
async fn cascade_already_deleted_membership_unchanged() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let uid = seed_user(&pool).await;
    let tid = seed_tenant(&pool).await;
    let mid: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tid)
    .bind(uid)
    .bind("viewer")
    .fetch_one(&pool)
    .await
    .unwrap();
    let original_deleted = truncate_to_micros(chrono::Utc::now() - chrono::Duration::hours(1));
    sqlx::query("UPDATE tenant_memberships SET deleted_at = $1 WHERE id = $2")
        .bind(original_deleted)
        .bind(mid)
        .execute(&pool)
        .await
        .unwrap();
    // Now delete the tenant — should NOT overwrite already-deleted membership
    sqlx::query("UPDATE tenants SET deleted_at = now() WHERE id = $1")
        .bind(tid)
        .execute(&pool)
        .await
        .unwrap();
    let mem_deleted: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT deleted_at FROM tenant_memberships WHERE id = $1")
            .bind(mid)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        mem_deleted,
        Some(original_deleted),
        "already-deleted membership should retain its original deleted_at"
    );
}

#[tokio::test]
async fn cascade_audit_entries_survive_soft_delete() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let uid = seed_user(&pool).await;
    let tid = seed_tenant(&pool).await;
    sqlx::query(
        "INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(uid)
    .bind("user.soft_deleted")
    .bind("user")
    .bind("audit-cascade-test")
    .bind(tid)
    .execute(&pool)
    .await
    .unwrap();
    // Soft-delete the referenced user/tenant
    sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
        .bind(uid)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE tenants SET deleted_at = now() WHERE id = $1")
        .bind(tid)
        .execute(&pool)
        .await
        .unwrap();
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_logs WHERE actor_user_id = $1")
        .bind(uid)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count.0, 1,
        "audit entry should remain readable after actor/tenant soft-delete"
    );
}

// ---------------------------------------------------------------------------
// US3 — Append-only audit (T019)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn append_only_audit_update_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO audit_logs (action, resource_type, resource_id) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind("test.immutable")
    .bind("test")
    .bind("res-immut")
    .fetch_one(&pool)
    .await
    .unwrap();
    let result = sqlx::query("UPDATE audit_logs SET action = 'changed' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(result.is_err(), "UPDATE on audit_logs should be rejected");
}

#[tokio::test]
async fn append_only_audit_delete_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO audit_logs (action, resource_type, resource_id) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind("test.immutable_del")
    .bind("test")
    .bind("res-immut-del")
    .fetch_one(&pool)
    .await
    .unwrap();
    let result = sqlx::query("DELETE FROM audit_logs WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await;
    assert!(result.is_err(), "DELETE on audit_logs should be rejected");
}

#[tokio::test]
async fn append_only_audit_row_count_unchanged_after_failed_update() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let marker = format!("test.count.{}", uuid::Uuid::new_v4());
    sqlx::query("INSERT INTO audit_logs (action, resource_type, resource_id) VALUES ($1, 'test', 'res-count')")
        .bind(&marker)
        .execute(&pool)
        .await
        .unwrap();
    let before: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_logs WHERE action = $1")
        .bind(&marker)
        .fetch_one(&pool)
        .await
        .unwrap();
    let _ = sqlx::query("DELETE FROM audit_logs").execute(&pool).await;
    let after: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_logs WHERE action = $1")
        .bind(&marker)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        before.0, after.0,
        "row count should be unchanged after failed DELETE"
    );
}

// ---------------------------------------------------------------------------
// US3 — Index coverage (T020)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn index_coverage_all_indexes_exist() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let expected_indexes = [
        "users_email_active_uniq",
        "tenants_slug_active_uniq",
        "tenant_memberships_tenant_user_active_uniq",
        "tenant_memberships_user_idx",
        "audit_logs_tenant_created_idx",
        "audit_logs_created_idx",
        "idx_tenants_directory_filter",
        "idx_tenants_directory_search",
        "tenant_invitations_token_hash_uniq",
        "tenant_invitations_pending_email_uniq",
        "customers_tenant_cursor_idx",
        "customers_display_name_trgm_idx",
        "customers_email_trgm_idx",
        "customers_phone_trgm_idx",
        "customer_channel_identifiers_customer_idx",
        "customer_channel_identifiers_identifier_trgm_idx",
        "customer_channel_identifiers_live_unique_idx",
        "conversations_customer_recent_idx",
    ];
    for idx_name in &expected_indexes {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT indexname FROM pg_indexes WHERE indexname = $1")
                .bind(idx_name)
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert!(
            row.is_some(),
            "expected index '{}' not found in pg_indexes",
            idx_name,
        );
    }
}

#[tokio::test]
async fn index_coverage_membership_tenant_query_uses_index() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let tid = seed_tenant(&pool).await;
    // Insert a user + membership to have data
    let uid = seed_user(&pool).await;
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(tid)
        .bind(uid)
        .bind("agent")
        .execute(&pool)
        .await
        .unwrap();
    // Disable seqscan and check for index scan. We use a transaction so that
    // SET LOCAL actually persists for the EXPLAIN below; the SET must run as
    // its own simple query (Postgres rejects multiple commands in a prepared
    // statement).
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .unwrap();
    let plan: (serde_json::Value,) = sqlx::query_as(
        "EXPLAIN (FORMAT JSON) \
         SELECT * FROM tenant_memberships WHERE tenant_id = $1 AND deleted_at IS NULL",
    )
    .bind(tid)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let plan_str = plan.0.to_string();
    assert!(
        plan_str.contains("Index Scan") || plan_str.contains("Bitmap Index Scan"),
        "tenant-scoped membership query should use an index, got plan: {}",
        plan_str,
    );
}

// ---------------------------------------------------------------------------
// T102 — EXPLAIN-based index-usage assertions for escalations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn explain_routing_candidate_selection_uses_index() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let tid = seed_tenant(&pool).await;
    let uid = seed_user(&pool).await;
    let mid: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, 'agent', 'active') RETURNING id",
    )
    .bind(tid)
    .bind(uid)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_availability (tenant_id, membership_id, state) VALUES ($1, $2, 'available')",
    )
    .bind(tid)
    .bind(mid)
    .execute(&pool)
    .await
    .unwrap();

    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .unwrap();
    let plan: (serde_json::Value,) = sqlx::query_as(
        "EXPLAIN (FORMAT JSON) \
         SELECT tm.id FROM tenant_memberships tm \
         JOIN agent_availability aa ON aa.tenant_id = tm.tenant_id AND aa.membership_id = tm.id \
         LEFT JOIN LATERAL ( \
             SELECT COUNT(*) AS match_count, array_agg(ask.skill_id) AS matched_ids \
             FROM agent_skills ask \
             WHERE ask.tenant_id = tm.tenant_id AND ask.membership_id = tm.id \
               AND ask.skill_id = ANY($2) \
         ) m ON true \
         LEFT JOIN LATERAL ( \
             SELECT COUNT(*) AS load_count FROM conversations c \
             WHERE c.tenant_id = tm.tenant_id AND c.assigned_membership_id = tm.id \
               AND c.status IN ('open','pending') AND c.deleted_at IS NULL \
         ) l ON true \
         WHERE tm.tenant_id = $1 AND tm.status = 'active' AND tm.deleted_at IS NULL \
           AND tm.role IN ('owner','admin','manager','agent') \
           AND aa.state = 'available' \
           AND tm.id = ANY($3) \
         ORDER BY m.match_count DESC, l.load_count ASC, tm.id ASC \
         LIMIT 1",
    )
    .bind(tid)
    .bind(&[] as &[uuid::Uuid])
    .bind(&[mid])
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let plan_str = plan.0.to_string();
    assert_index_scan(&plan_str);
    assert_no_seq_scan(&plan_str);
    assert!(
        !plan_str.contains("escalations")
            || plan_str.contains("Index Scan")
            || plan_str.contains("Bitmap Index Scan"),
        "routing candidate query must not seq-scan escalations"
    );
}

#[tokio::test]
async fn explain_queue_list_uses_index() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let tid = seed_tenant(&pool).await;
    let cid = seed_customer(&pool, tid, "QueueIdx").await;
    let conv_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, last_activity_at) \
         VALUES ($1, $2, 'web_chat', 'open', now()) RETURNING id",
    )
    .bind(tid)
    .bind(cid)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO escalations (tenant_id, conversation_id, reason, status) \
         VALUES ($1, $2, 'test reason', 'queued')",
    )
    .bind(tid)
    .bind(conv_id)
    .execute(&pool)
    .await
    .unwrap();

    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .unwrap();
    let plan: (serde_json::Value,) = sqlx::query_as(
        "EXPLAIN (FORMAT JSON) \
         SELECT e.id FROM escalations e \
         JOIN conversations c ON c.id = e.conversation_id AND c.tenant_id = e.tenant_id \
         JOIN customers cu ON cu.id = c.customer_id AND cu.tenant_id = c.tenant_id \
         WHERE e.tenant_id = $1 AND e.status = 'queued' \
           AND (e.escalated_at, e.id) > ($2, $3) \
         ORDER BY e.escalated_at ASC, e.id ASC \
         LIMIT $4",
    )
    .bind(tid)
    .bind(chrono::Utc::now())
    .bind(uuid::Uuid::nil())
    .bind(10i64)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let plan_str = plan.0.to_string();
    assert_index_scan(&plan_str);
    assert_no_seq_scan(&plan_str);
    assert!(
        !plan_str.contains("Seq Scan")
            || plan_str.contains("escalations")
            || plan_str.contains("Index Scan"),
        "queue list query must not seq-scan escalations"
    );
}

#[tokio::test]
async fn explain_escalated_inbox_filter_uses_index() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let tid = seed_tenant(&pool).await;
    let cid = seed_customer(&pool, tid, "EscInbox").await;
    sqlx::query(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, escalated_at, last_activity_at) \
         VALUES ($1, $2, 'web_chat', 'open', now(), now()) RETURNING id",
    )
    .bind(tid)
    .bind(cid)
    .execute(&pool)
    .await
    .unwrap();

    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .unwrap();
    let plan: (serde_json::Value,) = sqlx::query_as(
        "EXPLAIN (FORMAT JSON) \
         SELECT c.id FROM conversations c \
         JOIN customers cu ON cu.id = c.customer_id AND cu.tenant_id = c.tenant_id AND cu.deleted_at IS NULL \
         WHERE c.tenant_id = $1 AND c.deleted_at IS NULL \
           AND c.status = 'open' \
           AND c.escalated_at IS NOT NULL \
         ORDER BY c.last_activity_at DESC, c.id DESC \
         LIMIT 10",
    )
    .bind(tid)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let plan_str = plan.0.to_string();
    assert_index_scan(&plan_str);
    assert_no_seq_scan(&plan_str);
    assert!(
        plan_str.contains("escalated_at") || plan_str.contains("Index Scan"),
        "escalated inbox filter must use an index, got plan: {}",
        plan_str,
    );
}

// ---------------------------------------------------------------------------
// T035 — Timestamp advancement for tenants and memberships
// ---------------------------------------------------------------------------

#[tokio::test]
async fn convention_tenant_update_advances_updated_at() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let (id, created, updated): (
        uuid::Uuid,
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
    ) = sqlx::query_as(
        "INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id, created_at, updated_at",
    )
    .bind("Advance Tenant")
    .bind(valid_slug())
    .fetch_one(&pool)
    .await
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    sqlx::query("UPDATE tenants SET name = $1 WHERE id = $2")
        .bind("Updated Tenant")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (new_created, new_updated): (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as("SELECT created_at, updated_at FROM tenants WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        new_updated > updated,
        "tenant updated_at should advance after UPDATE"
    );
    assert_eq!(
        created, new_created,
        "created_at should remain unchanged after tenant UPDATE"
    );
}

#[tokio::test]
async fn convention_membership_update_advances_updated_at() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let uid = seed_user(&pool).await;
    let tid = seed_tenant(&pool).await;
    let (id, created, updated): (uuid::Uuid, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3) RETURNING id, created_at, updated_at",
    )
    .bind(tid)
    .bind(uid)
    .bind("agent")
    .fetch_one(&pool)
    .await
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    sqlx::query("UPDATE tenant_memberships SET role = 'admin' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (new_created, new_updated): (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as("SELECT created_at, updated_at FROM tenant_memberships WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        new_updated > updated,
        "membership updated_at should advance after UPDATE"
    );
    assert_eq!(
        created, new_created,
        "created_at should remain unchanged after membership UPDATE"
    );
}

// ---------------------------------------------------------------------------
// T028 — UUID collision cascade regression (scoped cascade fix)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cascade_user_delete_does_not_affect_tenant_scoped_memberships() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let shared_uuid = uuid::Uuid::new_v4();
    // Insert a tenant and user with the SAME UUID (different tables)
    sqlx::query("INSERT INTO tenants (id, name, slug) VALUES ($1, $2, $3)")
        .bind(shared_uuid)
        .bind("Shared UUID Tenant")
        .bind(valid_slug())
        .execute(&pool)
        .await
        .expect("insert shared-uuid tenant");
    sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, $3)")
        .bind(shared_uuid)
        .bind(valid_email())
        .bind("Shared UUID User")
        .execute(&pool)
        .await
        .expect("insert shared-uuid user");
    // Create a second user (different UUID) and a membership with the tenant
    let uid2 = seed_user(&pool).await;
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(shared_uuid) // tenant_id matches the shared uuid
        .bind(uid2)
        .bind("agent")
        .execute(&pool)
        .await
        .expect("insert membership with shared-uuid tenant");
    // Soft-delete the shared-uuid user — should NOT cascade to the membership
    // because the user cascade only matches on user_id, not tenant_id
    sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
        .bind(shared_uuid)
        .execute(&pool)
        .await
        .unwrap();
    let mem_deleted: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT deleted_at FROM tenant_memberships WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(shared_uuid)
    .bind(uid2)
    .fetch_one(&pool)
    .await
    .expect("membership should exist");
    assert!(
        mem_deleted.is_none(),
        "membership should NOT be cascade-deleted when a different user (even with same UUID) is deleted"
    );
}

// ---------------------------------------------------------------------------
// T027 — Membership guard: reject active memberships with soft-deleted parents
// ---------------------------------------------------------------------------

#[tokio::test]
async fn memberships_rejected_when_user_soft_deleted() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let uid = seed_user(&pool).await;
    let tid = seed_tenant(&pool).await;
    sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
        .bind(uid)
        .execute(&pool)
        .await
        .unwrap();
    let result = sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)",
    )
    .bind(tid)
    .bind(uid)
    .bind("agent")
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "membership with soft-deleted user should be rejected"
    );
}

#[tokio::test]
async fn memberships_rejected_when_tenant_soft_deleted() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let uid = seed_user(&pool).await;
    let tid = seed_tenant(&pool).await;
    sqlx::query("UPDATE tenants SET deleted_at = now() WHERE id = $1")
        .bind(tid)
        .execute(&pool)
        .await
        .unwrap();
    let result = sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)",
    )
    .bind(tid)
    .bind(uid)
    .bind("agent")
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "membership with soft-deleted tenant should be rejected"
    );
}

// ---------------------------------------------------------------------------
// T029 — Slug change audit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn slug_change_writes_audit_entry() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let old_slug = valid_slug();
    let new_slug = valid_slug();
    let tenant_id: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
            .bind("Audit Slug Co")
            .bind(&old_slug)
            .fetch_one(&pool)
            .await
            .unwrap();
    let actor_id = seed_user(&pool).await;
    let mut conn = pool.acquire().await.expect("acquire conn");
    sqlx::query("BEGIN")
        .execute(&mut *conn)
        .await
        .expect("begin");
    sqlx::query("SELECT set_audit_actor($1)")
        .bind(actor_id)
        .execute(&mut *conn)
        .await
        .expect("set actor");
    sqlx::query("UPDATE tenants SET slug = $1 WHERE id = $2")
        .bind(&new_slug)
        .bind(tenant_id)
        .execute(&mut *conn)
        .await
        .expect("rename should succeed");
    sqlx::query("COMMIT")
        .execute(&mut *conn)
        .await
        .expect("commit");
    let row: Option<(uuid::Uuid, serde_json::Value)> = sqlx::query_as(
        "SELECT actor_user_id, details FROM audit_logs WHERE action = 'tenant.slug_changed' AND tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(row.is_some(), "slug change should write an audit log entry");
    let (recorded_actor, details) = row.unwrap();
    assert_eq!(
        recorded_actor, actor_id,
        "audit entry should record the actor who changed the slug"
    );
    assert_eq!(
        details["old_slug"], old_slug,
        "audit details should contain old_slug"
    );
    assert_eq!(
        details["new_slug"], new_slug,
        "audit details should contain new_slug"
    );
}

// ---------------------------------------------------------------------------
// T031 — Audit resource required constraint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn audit_entry_without_resource_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let result = sqlx::query("INSERT INTO audit_logs (action, resource_type) VALUES ($1, $2)")
        .bind("test.anonymous")
        .bind("test")
        .execute(&pool)
        .await;
    assert!(
        result.is_err(),
        "audit entry without resource_id should be rejected"
    );
}

// ---------------------------------------------------------------------------
// T034 — Audit tenant+time index EXPLAIN
// ---------------------------------------------------------------------------

#[tokio::test]
async fn index_coverage_audit_tenant_time_query_uses_index() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let tid = seed_tenant(&pool).await;
    sqlx::query(
        "INSERT INTO audit_logs (action, resource_type, resource_id, tenant_id) VALUES ($1, $2, $3, $4)",
    )
    .bind("test.index")
    .bind("test")
    .bind("res-idx")
    .bind(tid)
    .execute(&pool)
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .unwrap();
    let plan: (serde_json::Value,) = sqlx::query_as(
        "EXPLAIN (FORMAT JSON) \
         SELECT * FROM audit_logs WHERE tenant_id = $1 ORDER BY created_at DESC",
    )
    .bind(tid)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let plan_str = plan.0.to_string();
    assert!(
        plan_str.contains("Index Scan") || plan_str.contains("Bitmap Index Scan"),
        "audit tenant+time query should use an index, got plan: {}",
        plan_str,
    );
    assert!(
        plan_str.contains("audit_logs_tenant_created_idx"),
        "audit tenant+time query should use audit_logs_tenant_created_idx, got plan: {}",
        plan_str,
    );
}

// ---------------------------------------------------------------------------
// T108 — Customer list query index coverage
// ---------------------------------------------------------------------------

#[tokio::test]
async fn customer_index_coverage_uses_intended_indexes() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let tid = seed_tenant(&pool).await;
    sqlx::query("INSERT INTO customers (tenant_id, display_name, email) VALUES ($1, $2, $3)")
        .bind(tid)
        .bind("Test Customer")
        .bind("test@example.com")
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .unwrap();
    let plan: (serde_json::Value,) = sqlx::query_as(
        "EXPLAIN (FORMAT JSON) \
         SELECT id, display_name, email, phone, metadata, created_at, updated_at \
         FROM customers \
         WHERE tenant_id = $1 AND deleted_at IS NULL \
         ORDER BY created_at DESC, id DESC",
    )
    .bind(tid)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let plan_str = plan.0.to_string();
    assert!(
        plan_str.contains("Index Scan") || plan_str.contains("Bitmap Index Scan"),
        "tenant+cursor customer query should use an index, got plan: {}",
        plan_str,
    );

    // --- Structural index definition verification ---
    // 1. Verify customers_tenant_cursor_idx via pg_indexes.indexdef
    let idxdef: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'customers_tenant_cursor_idx'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        !idxdef.contains("UNIQUE"),
        "customers_tenant_cursor_idx must not be unique"
    );
    assert!(
        idxdef.contains("USING btree"),
        "customers_tenant_cursor_idx must use btree"
    );
    assert!(idxdef.contains("tenant_id"), "must cover tenant_id");
    assert!(
        idxdef.contains("created_at DESC"),
        "created_at must be DESC"
    );
    assert!(idxdef.contains("id DESC"), "id must be DESC");
    assert!(
        idxdef.contains("deleted_at IS NULL"),
        "must have WHERE deleted_at IS NULL"
    );

    // 2. Direct pg_catalog structural query (access method, uniqueness, predicate)
    let (am_name, is_unique, pred): (String, bool, Option<String>) = sqlx::query_as(
        "SELECT a.amname, i.indisunique, pg_get_expr(i.indpred, i.indrelid)
         FROM pg_index i
         JOIN pg_class c ON c.oid = i.indexrelid
         JOIN pg_am a ON a.oid = c.relam
         WHERE c.relname = 'customers_tenant_cursor_idx'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(am_name, "btree", "access method must be btree");
    assert!(!is_unique, "must not be unique");
    let pred_str = pred.as_deref().unwrap_or("");
    assert!(
        pred_str.contains("deleted_at IS NULL"),
        "predicate must include deleted_at IS NULL, got: {pred_str}"
    );

    // 3. Verify key column order via pg_index.indkey + pg_attribute
    let index_attrs: Vec<(i16, String)> = sqlx::query_as(
        "SELECT a.attnum, a.attname
         FROM pg_index i
         JOIN pg_class c ON c.oid = i.indexrelid
         JOIN pg_attribute a ON a.attrelid = i.indrelid
            AND a.attnum = ANY(i.indkey)
         WHERE c.relname = 'customers_tenant_cursor_idx'
         ORDER BY array_position(i.indkey, a.attnum)",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let col_names: Vec<&str> = index_attrs.iter().map(|(_, n)| n.as_str()).collect();
    assert!(
        col_names.contains(&"tenant_id"),
        "index must include tenant_id, got {col_names:?}"
    );
    assert!(
        col_names.contains(&"created_at"),
        "index must include created_at, got {col_names:?}"
    );
    assert!(
        col_names.contains(&"id"),
        "index must include id, got {col_names:?}"
    );

    // 4. Verify DESC ordering via pg_index.indnkeyatts and indoption
    let ind_options: Vec<i16> = sqlx::query_scalar(
        "SELECT unnest(i.indoption)
         FROM pg_index i
         JOIN pg_class c ON c.oid = i.indexrelid
         WHERE c.relname = 'customers_tenant_cursor_idx'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    // indoption bit 0 = DESC. For a 3-column index, we expect the 2nd and 3rd
    // columns (created_at, id) to have DESC set, the 1st (tenant_id) to be ASC.
    assert_eq!(ind_options.len(), 3, "expected 3 key columns");
    assert_eq!(
        ind_options[0] & 1,
        0,
        "tenant_id must be ASC (bit 0 not set), got {:#06b}",
        ind_options[0]
    );
    assert_eq!(
        ind_options[1] & 1,
        1,
        "created_at must be DESC (bit 0 set), got {:#06b}",
        ind_options[1]
    );
    assert_eq!(
        ind_options[2] & 1,
        1,
        "id must be DESC (bit 0 set), got {:#06b}",
        ind_options[2]
    );
}

// ---------------------------------------------------------------------------
// T128 — Tenant directory EXPLAIN plan regressions
//
// Verifies the production index strategy for the four list_tenants query
// shapes uses index scans rather than sequential scans. The specific index
// chosen by the planner depends on data volume — with test-scale data the
// planner may prefer tenants_pkey over the composite/GIN indexes, which is
// acceptable for the index strategy (both are index scans). Under production
// data volume (500+ active tenants) the composite and GIN indexes will be
// preferred for filtered and search queries respectively.
// ---------------------------------------------------------------------------

fn assert_index_scan(plan_str: &str) {
    assert!(
        plan_str.contains("\"Index Scan\"")
            || plan_str.contains("\"Bitmap Index Scan\"")
            || plan_str.contains("\"Bitmap Heap Scan\""),
        "query plan must use an index scan (not sequential scan), got plan: {}",
        plan_str,
    );
}

fn assert_no_seq_scan(plan_str: &str) {
    assert!(
        !plan_str.contains("\"Seq Scan\""),
        "query plan must NOT use a sequential scan, got plan: {}",
        plan_str,
    );
}

#[tokio::test]
async fn tenant_directory_explain_cursor_only() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .unwrap();
    let plan: (serde_json::Value,) = sqlx::query_as(
        "EXPLAIN (FORMAT JSON) \
         SELECT id, name, slug, status, plan FROM tenants \
         WHERE deleted_at IS NULL AND id > $1 \
         ORDER BY id ASC LIMIT $2",
    )
    .bind(uuid::Uuid::nil())
    .bind(10i64)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let plan_str = plan.0.to_string();
    assert_index_scan(&plan_str);
    assert_no_seq_scan(&plan_str);
}

#[tokio::test]
async fn tenant_directory_explain_status_and_cursor() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .unwrap();
    let plan: (serde_json::Value,) = sqlx::query_as(
        "EXPLAIN (FORMAT JSON) \
         SELECT id, name, slug, status, plan FROM tenants \
         WHERE deleted_at IS NULL AND status = $1 AND id > $2 \
         ORDER BY id ASC LIMIT $3",
    )
    .bind("active")
    .bind(uuid::Uuid::nil())
    .bind(10i64)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let plan_str = plan.0.to_string();
    assert_index_scan(&plan_str);
    assert_no_seq_scan(&plan_str);
}

#[tokio::test]
async fn tenant_directory_explain_search() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .unwrap();
    let plan: (serde_json::Value,) = sqlx::query_as(
        "EXPLAIN (FORMAT JSON) \
         SELECT id, name, slug, status, plan FROM tenants \
         WHERE deleted_at IS NULL AND (name ILIKE $1 OR slug ILIKE $1) \
         ORDER BY id ASC LIMIT $2",
    )
    .bind("%acme%")
    .bind(10i64)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let plan_str = plan.0.to_string();
    assert_index_scan(&plan_str);
    assert_no_seq_scan(&plan_str);
}

#[tokio::test]
async fn tenant_directory_explain_search_and_status() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .unwrap();
    let plan: (serde_json::Value,) = sqlx::query_as(
        "EXPLAIN (FORMAT JSON) \
         SELECT id, name, slug, status, plan FROM tenants \
         WHERE deleted_at IS NULL \
           AND (name ILIKE $1 OR slug ILIKE $1) \
           AND status = $2 \
         ORDER BY id ASC LIMIT $3",
    )
    .bind("%acme%")
    .bind("active")
    .bind(10i64)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let plan_str = plan.0.to_string();
    assert_index_scan(&plan_str);
    assert_no_seq_scan(&plan_str);
}

// ---------------------------------------------------------------------------
// T036 — Membership guard: UPDATE reactivation and reparenting
// ---------------------------------------------------------------------------

#[tokio::test]
async fn memberships_reactivation_with_soft_deleted_user_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let uid = seed_user(&pool).await;
    let tid = seed_tenant(&pool).await;
    let mid: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tid)
    .bind(uid)
    .bind("agent")
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
        .bind(uid)
        .execute(&pool)
        .await
        .unwrap();
    // Cascades should have soft-deleted the membership; try to reactivate
    let result = sqlx::query("UPDATE tenant_memberships SET deleted_at = NULL WHERE id = $1")
        .bind(mid)
        .execute(&pool)
        .await;
    assert!(
        result.is_err(),
        "reactivating membership with soft-deleted user should be rejected"
    );
}

#[tokio::test]
async fn memberships_reparent_to_soft_deleted_user_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let uid1 = seed_user(&pool).await;
    let uid2 = seed_user(&pool).await;
    let tid = seed_tenant(&pool).await;
    let mid: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tid)
    .bind(uid1)
    .bind("agent")
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
        .bind(uid2)
        .execute(&pool)
        .await
        .unwrap();
    let result = sqlx::query("UPDATE tenant_memberships SET user_id = $1 WHERE id = $2")
        .bind(uid2)
        .bind(mid)
        .execute(&pool)
        .await;
    assert!(
        result.is_err(),
        "reparenting membership to soft-deleted user should be rejected"
    );
}

#[tokio::test]
async fn memberships_reparent_to_soft_deleted_tenant_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let uid = seed_user(&pool).await;
    let tid1 = seed_tenant(&pool).await;
    let tid2 = seed_tenant(&pool).await;
    let mid: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tid1)
    .bind(uid)
    .bind("agent")
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE tenants SET deleted_at = now() WHERE id = $1")
        .bind(tid2)
        .execute(&pool)
        .await
        .unwrap();
    let result = sqlx::query("UPDATE tenant_memberships SET tenant_id = $1 WHERE id = $2")
        .bind(tid2)
        .bind(mid)
        .execute(&pool)
        .await;
    assert!(
        result.is_err(),
        "reparenting membership to soft-deleted tenant should be rejected"
    );
}

// ---------------------------------------------------------------------------
// T044 — Slug change requires caller identity
// ---------------------------------------------------------------------------

#[tokio::test]
async fn slug_change_without_actor_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let mut conn = pool.acquire().await.expect("acquire connection");
    sqlx::query("SELECT clear_audit_actor()")
        .execute(&mut *conn)
        .await
        .unwrap();
    let slug = valid_slug();
    sqlx::query("INSERT INTO tenants (name, slug) VALUES ($1, $2)")
        .bind("Anon Rename Co")
        .bind(&slug)
        .execute(&mut *conn)
        .await
        .unwrap();
    let new_slug = format!(
        "anon-renamed-{}",
        uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
    );
    let result = sqlx::query("UPDATE tenants SET slug = $1 WHERE slug = $2")
        .bind(&new_slug)
        .bind(&slug)
        .execute(&mut *conn)
        .await;
    assert!(
        result.is_err(),
        "slug change without an audit actor must be rejected"
    );
}

// ---------------------------------------------------------------------------
// T045/T047 — Concurrency: membership insert races parent soft-delete
// ---------------------------------------------------------------------------

#[tokio::test]
async fn membership_insert_races_parent_soft_delete() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    // Seed parent user and tenant once.
    let uid: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
            .bind(valid_email())
            .bind("Race User")
            .fetch_one(&pool)
            .await
            .expect("seed user");
    let tid: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
            .bind("Race Tenant")
            .bind(valid_slug())
            .fetch_one(&pool)
            .await
            .expect("seed tenant");

    // Two separate pooled connections running independent transactions.
    let mut conn_a = pool.acquire().await.expect("acquire conn_a");
    let conn_b = pool.acquire().await.expect("acquire conn_b");

    // A dedicated observer pool so lock-wait polling cannot compete with conn_a/conn_b for
    // the (size-2) main pool.
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let observer_pool = db::lazy_pool(&url, 2, std::time::Duration::from_secs(5));

    // Connection A: begin tx, insert membership (acquires user + tenant FOR UPDATE locks).
    sqlx::query("BEGIN")
        .execute(&mut *conn_a)
        .await
        .expect("begin A");
    let (mid,): (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tid)
    .bind(uid)
    .bind("agent")
    .fetch_one(&mut *conn_a)
    .await
    .expect("insert membership on A");

    // Connection B: BEGINS, sets a 5s lock_timeout, captures its backend PID, then issues
    // UPDATE on the user. The UPDATE will block on A's row lock; we observe that block via
    // pg_stat_activity before committing A.
    let (b_pid_tx, b_pid_rx) = tokio::sync::oneshot::channel::<i32>();
    let uid_for_b = uid;
    let mut conn_for_b = conn_b;
    let delete_handle = tokio::spawn(async move {
        sqlx::query("BEGIN")
            .execute(&mut *conn_for_b)
            .await
            .expect("begin B");
        sqlx::query("SET lock_timeout = '5s'")
            .execute(&mut *conn_for_b)
            .await
            .expect("set B lock_timeout");
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *conn_for_b)
            .await
            .expect("get B pid");
        b_pid_tx.send(pid).expect("signal B pid");
        let result = sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
            .bind(uid_for_b)
            .execute(&mut *conn_for_b)
            .await
            .expect("B's UPDATE must succeed within lock_timeout after A commits");
        sqlx::query("COMMIT")
            .execute(&mut *conn_for_b)
            .await
            .expect("commit B");
        result.rows_affected()
    });

    // Wait for B to publish its backend PID (sent just before the blocking UPDATE).
    let b_pid = b_pid_rx.await.expect("B should signal pid");

    // Poll pg_stat_activity until it confirms B is waiting on a lock. The query is null-safe
    // because wait_event_type is NULL while a backend is running normally; we only assert the
    // existence of a Lock wait for B's PID, never decode the nullable column itself.
    let mut observed_lock_wait = false;
    for _ in 0..200u32 {
        let on_lock: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid = $1 AND wait_event_type = 'Lock')",
        )
        .bind(b_pid)
        .fetch_one(&observer_pool)
        .await
        .expect("pg_stat_activity query");
        if on_lock.0 {
            observed_lock_wait = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        observed_lock_wait,
        "connection B (pid {b_pid}) must be reported as waiting on a lock in pg_stat_activity"
    );

    // Connection A: commit, releasing the row lock and unblocking B.
    sqlx::query("COMMIT")
        .execute(&mut *conn_a)
        .await
        .expect("commit A");

    let rows = delete_handle.await.expect("join delete task");
    assert_eq!(rows, 1, "B should soft-delete exactly one user row");

    // The cascade trigger must have stamped the membership as soft-deleted.
    let mem_deleted: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT deleted_at FROM tenant_memberships WHERE id = $1")
            .bind(mid)
            .fetch_one(&pool)
            .await
            .expect("membership must exist");
    assert!(
        mem_deleted.is_some(),
        "membership must be cascade-soft-deleted after concurrent parent delete"
    );
}

// ---------------------------------------------------------------------------
// T046/T049 — Actor identity must not leak across pooled transactions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn actor_does_not_leak_across_pooled_transactions() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    // Use a dedicated one-connection pool so both transactions are guaranteed to reuse the
    // same PostgreSQL backend session; this makes the test a true transaction-local check.
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let single_pool = db::lazy_pool(&url, 1, std::time::Duration::from_secs(5));

    // Seed a tenant and an actor user.
    let first_actor: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
            .bind(valid_email())
            .bind("Leak Actor")
            .fetch_one(&single_pool)
            .await
            .expect("seed actor");
    let tenant_id: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
            .bind("Leak Tenant")
            .bind(valid_slug())
            .fetch_one(&single_pool)
            .await
            .expect("seed tenant");

    // First transaction: capture backend PID, set actor, rename, commit.
    let first_pid: i32;
    {
        let mut conn = single_pool.acquire().await.expect("first conn");
        first_pid = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *conn)
            .await
            .expect("first pid");
        sqlx::query("BEGIN")
            .execute(&mut *conn)
            .await
            .expect("begin first");
        sqlx::query("SELECT set_audit_actor($1)")
            .bind(first_actor)
            .execute(&mut *conn)
            .await
            .expect("set first actor");
        let new_slug = format!(
            "leak-{}",
            uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
        );
        sqlx::query("UPDATE tenants SET slug = $1 WHERE id = $2")
            .bind(&new_slug)
            .bind(tenant_id)
            .execute(&mut *conn)
            .await
            .expect("first rename");
        sqlx::query("COMMIT")
            .execute(&mut *conn)
            .await
            .expect("commit first");
    }

    // Second transaction on the same pool must reuse the same backend (proving we test true
    // transaction-local isolation rather than session-wide leakage to a fresh backend).
    let new_slug_2 = format!(
        "leak2-{}",
        uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
    );
    {
        let mut conn = single_pool.acquire().await.expect("second conn");
        let second_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *conn)
            .await
            .expect("second pid");
        assert_eq!(
            second_pid, first_pid,
            "one-connection pool must reuse the same backend across transactions"
        );
        sqlx::query("BEGIN")
            .execute(&mut *conn)
            .await
            .expect("begin second");
        let result = sqlx::query("UPDATE tenants SET slug = $1 WHERE id = $2")
            .bind(&new_slug_2)
            .bind(tenant_id)
            .execute(&mut *conn)
            .await;
        sqlx::query("ROLLBACK")
            .execute(&mut *conn)
            .await
            .expect("rollback second");
        assert!(
            result.is_err(),
            "second transaction on the same backend must NOT inherit the first transaction's actor"
        );
    }
}

// ---------------------------------------------------------------------------
// T006 — Migrations 0025/0026: customer profiles schema
// ---------------------------------------------------------------------------

#[tokio::test]
async fn migration_0025_customer_profile_columns_exist() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    for (table, columns) in [
        (
            "customers",
            &[
                "id",
                "tenant_id",
                "display_name",
                "email",
                "phone",
                "metadata",
                "created_at",
                "updated_at",
                "deleted_at",
            ][..],
        ),
        (
            "customer_channel_identifiers",
            &[
                "id",
                "tenant_id",
                "customer_id",
                "channel",
                "identifier",
                "created_at",
            ][..],
        ),
        (
            "conversations",
            &[
                "id",
                "tenant_id",
                "customer_id",
                "channel",
                "status",
                "last_activity_at",
                "created_at",
                "updated_at",
                "deleted_at",
            ][..],
        ),
    ] {
        for column in columns {
            let exists: (bool,) = sqlx::query_as(
                "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
                 WHERE table_name = $1 AND column_name = $2)",
            )
            .bind(table)
            .bind(column)
            .fetch_one(&pool)
            .await
            .expect("query column existence");
            assert!(exists.0, "{table}.{column} column should exist");
        }
    }
}

#[tokio::test]
async fn migration_0025_0026_check_constraints_exist() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    for (table, constraint, expected_definition) in [
        (
            "customers",
            "customers_display_name_length",
            "CHECK (((length(display_name) >= 1) AND (length(display_name) <= 200)))",
        ),
        (
            "customers",
            "customers_metadata_object",
            "CHECK ((jsonb_typeof(metadata) = 'object'::text))",
        ),
        (
            "customer_channel_identifiers",
            "customer_channel_identifiers_channel_check",
            "CHECK ((channel = ANY (ARRAY['email'::text, 'phone'::text, 'web_chat'::text, 'whatsapp'::text, 'telegram'::text])))",
        ),
        (
            "customer_channel_identifiers",
            "customer_channel_identifiers_identifier_length",
            "CHECK (((length(identifier) >= 1) AND (length(identifier) <= 320)))",
        ),
        (
            "conversations",
            "conversations_channel_check",
            "CHECK ((channel = ANY (ARRAY['email'::text, 'phone'::text, 'web_chat'::text, 'whatsapp'::text, 'telegram'::text])))",
        ),
        (
            "conversations",
            "conversations_status_check",
            "CHECK ((status = ANY (ARRAY['open'::text, 'escalated'::text, 'closed'::text])))",
        ),
    ] {
        let definition: Option<String> = sqlx::query_scalar(
            "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
             WHERE conrelid = $1::regclass AND conname = $2 AND contype = 'c'",
        )
        .bind(table)
        .bind(constraint)
        .fetch_one(&pool)
        .await
        .expect("query check constraint definition");
        let definition = definition.expect("{constraint} CHECK constraint should exist");
        let normalized_definition = definition.split_whitespace().collect::<String>();
        let normalized_expected = expected_definition.split_whitespace().collect::<String>();
        assert_eq!(
            normalized_definition, normalized_expected,
            "{constraint} CHECK definition mismatch; actual: {definition}"
        );
    }
}

#[tokio::test]
async fn migration_0025_identifier_unique_index_exists() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    let definition: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE schemaname = 'public' \
         AND indexname = 'customer_channel_identifiers_live_unique_idx'",
    )
    .fetch_one(&pool)
    .await
    .expect("customer channel identifier live unique index");
    assert!(definition.starts_with("CREATE UNIQUE INDEX"));
    assert!(definition.contains("tenant_id, channel, identifier"));
    assert!(definition.contains("deleted_at IS NULL"));
}

#[tokio::test]
async fn migration_0025_0026_foreign_keys_exist() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    for (table, column, referenced_table) in [
        ("customers", "tenant_id", "tenants"),
        ("customer_channel_identifiers", "tenant_id", "tenants"),
        ("customer_channel_identifiers", "customer_id", "customers"),
        ("conversations", "tenant_id", "tenants"),
        ("conversations", "customer_id", "customers"),
    ] {
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS( \
                 SELECT 1 FROM information_schema.key_column_usage AS kcu \
                 JOIN information_schema.table_constraints AS tc \
                   ON tc.constraint_name = kcu.constraint_name \
                  AND tc.table_schema = kcu.table_schema \
                 JOIN information_schema.constraint_column_usage AS ccu \
                   ON ccu.constraint_name = tc.constraint_name \
                  AND ccu.table_schema = tc.table_schema \
                 WHERE tc.constraint_type = 'FOREIGN KEY' \
                   AND kcu.table_schema = 'public' \
                   AND kcu.table_name = $1 \
                   AND kcu.column_name = $2 \
                   AND ccu.table_name = $3 \
                   AND ccu.column_name = 'id')",
        )
        .bind(table)
        .bind(column)
        .bind(referenced_table)
        .fetch_one(&pool)
        .await
        .expect("query foreign key");
        assert!(
            exists.0,
            "{table}.{column} should reference {referenced_table}.id"
        );
    }
}

// ---------------------------------------------------------------------------
// T054 — Migration 0027: composite FK constraints
// ---------------------------------------------------------------------------

#[tokio::test]
async fn migration_0027_composite_fk_constraints_exist() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    for (table, constraint_name) in [
        (
            "customer_channel_identifiers",
            "customer_channel_identifiers_parent_tenant_fkey",
        ),
        ("conversations", "conversations_parent_tenant_fkey"),
    ] {
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS( \
             SELECT 1 FROM pg_constraint \
             WHERE conrelid = $1::regclass AND conname = $2 AND contype = 'f')",
        )
        .bind(table)
        .bind(constraint_name)
        .fetch_one(&pool)
        .await
        .expect("query constraint existence");
        assert!(
            exists.0,
            "{table} should have composite FK {constraint_name}"
        );
    }

    // Verify the supporting unique index exists
    let idx_exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM pg_indexes WHERE indexname = 'customers_tenant_id_id_uq')",
    )
    .fetch_one(&pool)
    .await
    .expect("query index existence");
    assert!(
        idx_exists.0,
        "customers_tenant_id_id_uq unique index should exist"
    );
}

#[tokio::test]
async fn migration_0027_identifier_parent_tenant_fk_rejects_mismatch() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    let tid: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
            .bind("FK Reject Tenant A")
            .bind(format!("fk-reject-a-{}", uuid::Uuid::new_v4().simple()))
            .fetch_one(&pool)
            .await
            .unwrap();

    let cid: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tid)
    .bind("FK Reject Customer")
    .fetch_one(&pool)
    .await
    .unwrap();

    // Try inserting a channel identifier with a different tenant_id
    let other_tid: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
            .bind("FK Reject Tenant B")
            .bind(format!("fk-reject-b-{}", uuid::Uuid::new_v4().simple()))
            .fetch_one(&pool)
            .await
            .unwrap();

    let result = sqlx::query(
        "INSERT INTO customer_channel_identifiers (tenant_id, customer_id, channel, identifier) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(other_tid)
    .bind(cid)
    .bind("email")
    .bind("cross-tenant@example.com")
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "inserting identifier with mismatched tenant_id should be rejected by composite FK"
    );
}

#[tokio::test]
async fn migration_0027_conversation_parent_tenant_fk_rejects_mismatch() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    let tid: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
            .bind("FK Reject Tenant C")
            .bind(format!("fk-reject-c-{}", uuid::Uuid::new_v4().simple()))
            .fetch_one(&pool)
            .await
            .unwrap();

    let cid: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tid)
    .bind("FK Reject Customer 2")
    .fetch_one(&pool)
    .await
    .unwrap();

    let other_tid: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
            .bind("FK Reject Tenant D")
            .bind(format!("fk-reject-d-{}", uuid::Uuid::new_v4().simple()))
            .fetch_one(&pool)
            .await
            .unwrap();

    let result = sqlx::query(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, last_activity_at) \
         VALUES ($1, $2, $3, $4, now())",
    )
    .bind(other_tid)
    .bind(cid)
    .bind("web_chat")
    .bind("open")
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "inserting conversation with mismatched tenant_id should be rejected by composite FK"
    );
}

// ---------------------------------------------------------------------------
// T006 — Migration 0018: tenant_memberships status column
// ---------------------------------------------------------------------------

#[tokio::test]
async fn migration_0018_status_column_exists() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS( \
         SELECT 1 FROM information_schema.columns \
         WHERE table_name = 'tenant_memberships' AND column_name = 'status')",
    )
    .fetch_one(&pool)
    .await
    .expect("query information_schema.columns");
    assert!(exists.0, "tenant_memberships.status column should exist");
}

#[tokio::test]
async fn migration_0018_status_check_constraint() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS( \
         SELECT 1 FROM information_schema.table_constraints \
         WHERE table_name = 'tenant_memberships' \
           AND constraint_name = 'tenant_memberships_status_check' \
           AND constraint_type = 'CHECK')",
    )
    .fetch_one(&pool)
    .await
    .expect("query check constraint");
    assert!(
        exists.0,
        "tenant_memberships_status_check CHECK constraint should exist"
    );
}

// ---------------------------------------------------------------------------
// T007 — Migration 0019: tenant_invitations table
// ---------------------------------------------------------------------------

#[tokio::test]
async fn migration_0019_tenant_invitations_table_exists() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS( \
         SELECT 1 FROM information_schema.tables \
         WHERE table_name = 'tenant_invitations')",
    )
    .fetch_one(&pool)
    .await
    .expect("query information_schema.tables");
    assert!(exists.0, "tenant_invitations table should exist");
}

#[tokio::test]
async fn migration_0019_tenant_invitations_columns_exist() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    for col in &[
        "id",
        "tenant_id",
        "email",
        "role",
        "token_hash",
        "status",
        "invited_by",
        "expires_at",
        "accepted_at",
        "accepted_user_id",
        "revoked_at",
        "revoked_by",
        "created_at",
        "updated_at",
    ] {
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
             WHERE table_name = 'tenant_invitations' AND column_name = $1)",
        )
        .bind(col)
        .fetch_one(&pool)
        .await
        .expect("query column existence");
        assert!(exists.0, "tenant_invitations.{} column should exist", col);
    }
}

#[tokio::test]
async fn migration_0020_invitation_email_delivery_status_exists() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.expect("run migrations");

    let column: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
         WHERE table_name = 'tenant_invitations' \
         AND column_name = 'email_delivery_status' \
         AND column_default = '''unconfigured''::text' \
         AND is_nullable = 'NO')",
    )
    .fetch_one(&pool)
    .await
    .expect("query email delivery status column");
    assert!(column.0, "email delivery status column should exist");

    let constraint: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM pg_constraint \
         WHERE conrelid = 'tenant_invitations'::regclass \
         AND conname = 'tenant_invitations_email_delivery_status_check')",
    )
    .fetch_one(&pool)
    .await
    .expect("query email delivery status constraint");
    assert!(constraint.0, "email delivery status check should exist");
}

#[tokio::test]
async fn invitation_email_delivery_status_constraint_rejects_invalid_value() {
    let pool = match get_pool().await {
        Some(pool) => pool,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let suffix = uuid::Uuid::new_v4();
    let user_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name) VALUES ($1, 'Constraint User') RETURNING id",
    )
    .bind(format!("constraint-{suffix}@example.com"))
    .fetch_one(&pool)
    .await
    .unwrap();
    let tenant_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO tenants (name, slug) VALUES ('Constraint Tenant', $1) RETURNING id",
    )
    .bind(format!("constraint-{suffix}"))
    .fetch_one(&pool)
    .await
    .unwrap();
    let invitation_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_invitations (tenant_id,email,role,token_hash,invited_by,expires_at) \
         VALUES ($1,$2,'agent',$3,$4,now() + interval '1 day') RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("invite-{suffix}@example.com"))
    .bind(suffix.simple().to_string().repeat(2))
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let result =
        sqlx::query("UPDATE tenant_invitations SET email_delivery_status = 'lost' WHERE id = $1")
            .bind(invitation_id)
            .execute(&pool)
            .await;
    if let Err(sqlx::Error::Database(error)) = result {
        assert_eq!(error.code().as_deref(), Some("23514"));
    } else {
        panic!("invalid delivery status must violate the database constraint");
    }
}

#[tokio::test]
async fn invitation_delivery_outbox_claim_index_has_production_predicate() {
    let pool = match get_pool().await {
        Some(pool) => pool,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let definition: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'outbox_invitation_delivery_claimable_idx'",
    ).fetch_one(&pool).await.expect("claimable outbox index");
    assert!(definition.contains("available_at"));
    assert!(definition.contains("processed_at IS NULL"));
    assert!(definition.contains("dead_lettered_at IS NULL"));
    assert!(definition.contains("event_type = 'invitation.email_delivery'"));
}

#[tokio::test]
async fn migration_0019_tenant_invitations_unique_indexes_exist() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    for idx_name in &[
        "tenant_invitations_token_hash_uniq",
        "tenant_invitations_pending_email_uniq",
    ] {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT indexname FROM pg_indexes WHERE indexname = $1")
                .bind(idx_name)
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert!(
            row.is_some(),
            "expected index '{}' not found in pg_indexes",
            idx_name,
        );
    }
}

#[tokio::test]
async fn migration_0023_allows_expired_and_rejects_unknown_invitation_status() {
    let pool = match get_pool().await {
        Some(pool) => pool,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let suffix = uuid::Uuid::new_v4();
    let user_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name) VALUES ($1, 'Expiry Constraint User') RETURNING id",
    )
    .bind(format!("expiry-status-{suffix}@example.com"))
    .fetch_one(&pool)
    .await
    .unwrap();
    let tenant_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO tenants (name, slug) VALUES ('Expiry Constraint Tenant', $1) RETURNING id",
    )
    .bind(format!("expiry-status-{suffix}"))
    .fetch_one(&pool)
    .await
    .unwrap();
    let invitation_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_invitations (tenant_id,email,role,token_hash,invited_by,expires_at) \
         VALUES ($1,$2,'agent',$3,$4,now() - interval '1 hour') RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("expiry-invite-{suffix}@example.com"))
    .bind(format!("{suffix}expired"))
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query("UPDATE tenant_invitations SET status = 'expired' WHERE id = $1")
        .bind(invitation_id)
        .execute(&pool)
        .await
        .expect("expired must be an allowed lifecycle status");
    let unknown = sqlx::query("UPDATE tenant_invitations SET status = 'unknown' WHERE id = $1")
        .bind(invitation_id)
        .execute(&pool)
        .await;
    assert!(
        matches!(unknown, Err(sqlx::Error::Database(ref error)) if error.code().as_deref() == Some("23514"))
    );
}

#[tokio::test]
async fn invitation_expired_status_rejects_future_expiry() {
    let pool = match get_pool().await {
        Some(pool) => pool,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let suffix = uuid::Uuid::new_v4();
    let user_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name) VALUES ($1, 'Future Expiry User') RETURNING id",
    )
    .bind(format!("future-expiry-{suffix}@example.com"))
    .fetch_one(&pool)
    .await
    .unwrap();
    let tenant_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO tenants (name, slug) VALUES ('Future Expiry Tenant', $1) RETURNING id",
    )
    .bind(format!("future-expiry-{suffix}"))
    .fetch_one(&pool)
    .await
    .unwrap();
    let invitation_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_invitations (tenant_id,email,role,token_hash,invited_by,expires_at) \
         VALUES ($1,$2,'agent',$3,$4,now() + interval '1 day') RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!("future-invite-{suffix}@example.com"))
    .bind(format!("{suffix}future"))
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let result = sqlx::query("UPDATE tenant_invitations SET status = 'expired' WHERE id = $1")
        .bind(invitation_id)
        .execute(&pool)
        .await;
    assert!(
        matches!(result, Err(sqlx::Error::Database(ref error)) if error.code().as_deref() == Some("23514"))
    );
}

// ---------------------------------------------------------------------------
// T090 — Post-0029 index definitions (detailed definition checks)
//
// These verify the exact index definitions supplementing the existence-only
// checks in migration_0027/0028/0029 tests above.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn migration_0027_customers_tenant_id_id_uq_definition() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    // customers_tenant_id_id_uq is the UNIQUE constraint backing the
    // composite FK target from migration 0027.
    let indexdef: Result<String, sqlx::Error> = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
         WHERE conrelid = 'customers'::regclass \
         AND conname = 'customers_tenant_id_id_uq'",
    )
    .fetch_one(&pool)
    .await;
    match indexdef {
        Ok(def) => {
            assert!(
                def.contains("UNIQUE"),
                "customers_tenant_id_id_uq must be a UNIQUE constraint"
            );
            assert!(
                def.contains("tenant_id") && def.contains("id"),
                "customers_tenant_id_id_uq must cover (tenant_id, id)"
            );
        }
        Err(_) => {
            // Fallback: check the index definition directly.
            let idxdef: String = sqlx::query_scalar(
                "SELECT indexdef FROM pg_indexes \
                 WHERE indexname = 'customers_tenant_id_id_uq'",
            )
            .fetch_one(&pool)
            .await
            .expect("customers_tenant_id_id_uq indexdef");
            assert!(
                idxdef.contains("UNIQUE"),
                "customers_tenant_id_id_uq must be a UNIQUE index"
            );
            assert!(
                idxdef.contains("tenant_id") && idxdef.contains("id"),
                "customers_tenant_id_id_uq must cover (tenant_id, id)"
            );
        }
    }
}

#[tokio::test]
async fn migration_0028_trigram_index_definitions() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    // customers_phone_trgm_idx — GIN trigram index on phone with partial filter.
    let phone_def: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes \
         WHERE indexname = 'customers_phone_trgm_idx'",
    )
    .fetch_one(&pool)
    .await
    .expect("customers_phone_trgm_idx indexdef");
    assert!(
        phone_def.to_lowercase().contains("gin"),
        "customers_phone_trgm_idx must use GIN, got: {phone_def}"
    );
    assert!(
        phone_def.contains("gin_trgm_ops"),
        "customers_phone_trgm_idx must use gin_trgm_ops"
    );
    assert!(
        phone_def.contains("phone IS NOT NULL"),
        "customers_phone_trgm_idx must have WHERE phone IS NOT NULL"
    );

    // customer_channel_identifiers_identifier_trgm_idx — GIN trigram index on identifier.
    let ident_def: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes \
         WHERE indexname = 'customer_channel_identifiers_identifier_trgm_idx'",
    )
    .fetch_one(&pool)
    .await
    .expect("customer_channel_identifiers_identifier_trgm_idx indexdef");
    assert!(
        ident_def.to_lowercase().contains("gin"),
        "customer_channel_identifiers_identifier_trgm_idx must use GIN, got: {ident_def}"
    );
    assert!(
        ident_def.contains("gin_trgm_ops"),
        "customer_channel_identifiers_identifier_trgm_idx must use gin_trgm_ops"
    );
}

// ---------------------------------------------------------------------------
// T075 — pg_trgm extension and trigram index definitions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn migration_0025_trgm_extension_is_installed() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let exists: (bool,) =
        sqlx::query_as("SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'pg_trgm')")
            .fetch_one(&pool)
            .await
            .expect("query pg_extension");
    assert!(exists.0, "pg_trgm extension must be installed");
}

#[tokio::test]
async fn migration_0028_supplementary_indexes_exist() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let expected_indexes = [
        "customers_phone_trgm_idx",
        "customer_channel_identifiers_identifier_trgm_idx",
    ];
    for idx_name in &expected_indexes {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT indexname FROM pg_indexes WHERE indexname = $1")
                .bind(idx_name)
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert!(
            row.is_some(),
            "expected trigram index '{}' not found in pg_indexes",
            idx_name,
        );
    }
}

#[tokio::test]
async fn migration_0025_customer_trigram_index_names_exist() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    for idx_name in &[
        "customers_display_name_trgm_idx",
        "customers_email_trgm_idx",
    ] {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT indexname FROM pg_indexes WHERE indexname = $1")
                .bind(idx_name)
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert!(
            row.is_some(),
            "expected trigram index '{}' not found in pg_indexes",
            idx_name,
        );
    }
}

#[tokio::test]
async fn migration_0029_identifier_soft_delete_index_exists() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    let old_idx: Option<(String,)> = sqlx::query_as(
        "SELECT indexname FROM pg_indexes WHERE indexname = 'customer_channel_identifiers_unique_idx'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(old_idx.is_none(), "old unique index must be dropped");

    let definition: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE schemaname = 'public' \
         AND indexname = 'customer_channel_identifiers_live_unique_idx'",
    )
    .fetch_one(&pool)
    .await
    .expect("live unique index definition");
    assert!(definition.starts_with("CREATE UNIQUE INDEX"));
    assert!(definition.contains("tenant_id, channel, identifier"));
    assert!(definition.contains("deleted_at IS NULL"));
}

#[tokio::test]
async fn migration_0029_identifier_soft_delete_column_exists() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
         WHERE table_name = 'customer_channel_identifiers' AND column_name = 'deleted_at')",
    )
    .fetch_one(&pool)
    .await
    .expect("query column existence");
    assert!(
        exists.0,
        "customer_channel_identifiers.deleted_at column should exist"
    );
}

// ---------------------------------------------------------------------------
// T125 — Migration 0030: cascade trigger and exact index definitions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn migration_0030_cascade_and_all_index_definitions() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    // Assert the cascade trigger exists.
    let trigger_exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM pg_trigger \
         WHERE tgname = 'trg_cascade_identifier_soft_delete' \
           AND tgrelid = 'customers'::regclass)",
    )
    .fetch_one(&pool)
    .await
    .expect("query pg_trigger");
    assert!(
        trigger_exists.0,
        "trg_cascade_identifier_soft_delete must exist (migration 0030)"
    );

    // Helper closures to reduce boilerplate.
    let def = |name: &str| {
        let name = name.to_owned();
        let pool = &pool;
        async move {
            sqlx::query_scalar::<_, String>("SELECT indexdef FROM pg_indexes WHERE indexname = $1")
                .bind(&name)
                .fetch_one(pool)
                .await
                .unwrap_or_else(|_| panic!("index {name} not found"))
        }
    };

    // ---- customers_tenant_cursor_idx ----
    let d = def("customers_tenant_cursor_idx").await;
    assert!(!d.contains("UNIQUE"), "must not be unique");
    assert!(d.contains("USING btree"), "must use btree");
    assert!(d.contains("tenant_id"), "must cover tenant_id");
    assert!(d.contains("created_at DESC"), "created_at must be DESC");
    assert!(d.contains("id DESC"), "id must be DESC");
    assert!(
        d.contains("deleted_at IS NULL"),
        "partial filter on deleted_at"
    );

    // ---- customers_display_name_trgm_idx ----
    let d = def("customers_display_name_trgm_idx").await;
    assert!(!d.contains("UNIQUE"), "must not be unique");
    assert!(d.contains("USING gin"), "must use GIN");
    assert!(d.contains("display_name"), "must cover display_name");
    assert!(d.contains("gin_trgm_ops"), "must use gin_trgm_ops");

    // ---- customers_email_trgm_idx ----
    let d = def("customers_email_trgm_idx").await;
    assert!(!d.contains("UNIQUE"), "must not be unique");
    assert!(d.contains("USING gin"), "must use GIN");
    assert!(d.contains("email"), "must cover email");
    assert!(d.contains("gin_trgm_ops"), "must use gin_trgm_ops");

    // ---- customers_phone_trgm_idx ----
    let d = def("customers_phone_trgm_idx").await;
    assert!(!d.contains("UNIQUE"), "must not be unique");
    assert!(d.contains("USING gin"), "must use GIN");
    assert!(d.contains("phone"), "must cover phone");
    assert!(d.contains("gin_trgm_ops"), "must use gin_trgm_ops");
    assert!(
        d.contains("phone IS NOT NULL"),
        "partial filter on phone IS NOT NULL"
    );

    // ---- customer_channel_identifiers_customer_idx ----
    let d = def("customer_channel_identifiers_customer_idx").await;
    assert!(!d.contains("UNIQUE"), "must not be unique");
    assert!(d.contains("USING btree"), "must use btree");
    assert!(d.contains("customer_id"), "must cover customer_id");

    // ---- customer_channel_identifiers_identifier_trgm_idx ----
    let d = def("customer_channel_identifiers_identifier_trgm_idx").await;
    assert!(!d.contains("UNIQUE"), "must not be unique");
    assert!(d.contains("USING gin"), "must use GIN");
    assert!(d.contains("identifier"), "must cover identifier");
    assert!(d.contains("gin_trgm_ops"), "must use gin_trgm_ops");

    // ---- customer_channel_identifiers_live_unique_idx ----
    let d = def("customer_channel_identifiers_live_unique_idx").await;
    assert!(d.starts_with("CREATE UNIQUE INDEX"), "must be UNIQUE");
    assert!(d.contains("tenant_id"), "must cover tenant_id");
    assert!(d.contains("channel"), "must cover channel");
    assert!(d.contains("identifier"), "must cover identifier");
    assert!(
        d.contains("deleted_at IS NULL"),
        "partial filter on deleted_at IS NULL"
    );

    // ---- conversations_customer_recent_idx ----
    let d = def("conversations_customer_recent_idx").await;
    assert!(!d.contains("UNIQUE"), "must not be unique");
    assert!(d.contains("USING btree"), "must use btree");
    assert!(d.contains("tenant_id"), "must cover tenant_id");
    assert!(d.contains("customer_id"), "must cover customer_id");
    assert!(
        d.contains("last_activity_at DESC"),
        "last_activity_at must be DESC"
    );
    assert!(
        d.contains("deleted_at IS NULL"),
        "partial filter on deleted_at"
    );
}

// ---------------------------------------------------------------------------
// T004 — Migration 0033: conversation core — status, assignee, indexes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn migration_0033_conversation_core_changes() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    // 1. Status CHECK accepts open|pending|resolved|closed
    let definition: Option<String> = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
         WHERE conrelid = 'conversations'::regclass \
         AND conname = 'conversations_status_check' AND contype = 'c'",
    )
    .fetch_one(&pool)
    .await
    .expect("query check constraint definition");
    let definition = definition.expect("conversations_status_check CHECK constraint should exist");
    let normalized = definition.split_whitespace().collect::<String>();
    let expected = "CHECK ((status = ANY (ARRAY['open'::text, 'pending'::text, 'resolved'::text, 'closed'::text])))"
        .split_whitespace()
        .collect::<String>();
    assert_eq!(
        normalized, expected,
        "conversations_status_check definition mismatch"
    );

    // 2. assigned_membership_id column exists and is nullable
    let col: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
         WHERE table_name = 'conversations' AND column_name = 'assigned_membership_id' \
         AND is_nullable = 'YES')",
    )
    .fetch_one(&pool)
    .await
    .expect("query column");
    assert!(
        col.0,
        "conversations.assigned_membership_id should exist and be nullable"
    );

    // 2b. Composite FK to tenant_memberships exists
    let fk: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM pg_constraint \
         WHERE conrelid = 'conversations'::regclass \
         AND conname = 'conversations_assignee_tenant_fkey' \
         AND contype = 'f')",
    )
    .fetch_one(&pool)
    .await
    .expect("query FK");
    assert!(fk.0, "conversations_assignee_tenant_fkey FK should exist");

    // 3. conversations_inbox_idx
    let d: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'conversations_inbox_idx'",
    )
    .fetch_one(&pool)
    .await
    .expect("conversations_inbox_idx");
    assert!(
        d.contains("tenant_id, status, last_activity_at DESC, id DESC"),
        "inbox index must cover tenant_id, status, last_activity_at DESC, id DESC"
    );
    assert!(
        d.contains("deleted_at IS NULL"),
        "inbox index must be partial on deleted_at IS NULL"
    );

    // 3b. conversations_assignee_idx
    let d: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'conversations_assignee_idx'",
    )
    .fetch_one(&pool)
    .await
    .expect("conversations_assignee_idx");
    assert!(
        d.contains("tenant_id, assigned_membership_id, last_activity_at DESC"),
        "assignee index must cover tenant_id, assigned_membership_id, last_activity_at DESC"
    );
    assert!(
        d.contains("deleted_at IS NULL"),
        "assignee index must be partial on deleted_at IS NULL"
    );
}

// ---------------------------------------------------------------------------
// T004 — Migration 0034: messages table
// ---------------------------------------------------------------------------

#[tokio::test]
async fn migration_0034_messages_table() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    // 4. Messages columns with correct types and nullability
    for (col, expected_udt, nullable) in [
        ("id", "uuid", false),
        ("tenant_id", "uuid", false),
        ("conversation_id", "uuid", false),
        ("kind", "text", false),
        ("sender_membership_id", "uuid", true),
        ("logged_by_membership_id", "uuid", true),
        ("body", "text", false),
        ("seq", "int8", false),
        ("created_at", "timestamptz", false),
    ] {
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
             WHERE table_name = 'messages' AND column_name = $1 \
             AND udt_name = $2 AND is_nullable = $3)",
        )
        .bind(col)
        .bind(expected_udt)
        .bind(if nullable { "YES" } else { "NO" })
        .fetch_one(&pool)
        .await
        .expect("query column");
        assert!(
            exists.0,
            "messages.{col} should exist with type {expected_udt} nullable={nullable}"
        );
    }

    // 5. kind CHECK accepts customer|reply|note
    let def: Option<String> = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
         WHERE conrelid = 'messages'::regclass AND conname = 'messages_kind_check' AND contype = 'c'",
    )
    .fetch_one(&pool)
    .await
    .expect("query kind check");
    let def = def.expect("messages_kind_check should exist");
    let normalized = def.split_whitespace().collect::<String>();
    let expected = "CHECK ((kind = ANY (ARRAY['customer'::text, 'reply'::text, 'note'::text])))"
        .split_whitespace()
        .collect::<String>();
    assert_eq!(normalized, expected, "messages_kind_check definition");

    // 6. body length CHECK (1-10000)
    let def: Option<String> = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
         WHERE conrelid = 'messages'::regclass AND conname = 'messages_body_length' AND contype = 'c'",
    )
    .fetch_one(&pool)
    .await
    .expect("query body length check");
    let def = def.expect("messages_body_length should exist");
    let normalized = def.split_whitespace().collect::<String>();
    let expected = "CHECK (((char_length(body) >= 1) AND (char_length(body) <= 10000)))"
        .split_whitespace()
        .collect::<String>();
    assert_eq!(normalized, expected, "messages_body_length definition");

    // 7. kind-consistency CHECK exists
    let exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM pg_constraint \
         WHERE conrelid = 'messages'::regclass \
         AND conname = 'messages_kind_consistency' AND contype = 'c')",
    )
    .fetch_one(&pool)
    .await
    .expect("query kind-consistency check");
    assert!(exists.0, "messages_kind_consistency CHECK should exist");

    // 8. Composite FKs exist
    for fk_name in [
        "messages_conversation_fkey",
        "messages_sender_fkey",
        "messages_logged_by_fkey",
    ] {
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM pg_constraint \
             WHERE conrelid = 'messages'::regclass AND conname = $1 AND contype = 'f')",
        )
        .bind(fk_name)
        .fetch_one(&pool)
        .await
        .expect("query FK");
        assert!(exists.0, "{fk_name} FK should exist");
    }

    // 9. seq is GENERATED ALWAYS AS IDENTITY (bigint)
    let identity: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
         WHERE table_name = 'messages' AND column_name = 'seq' \
         AND is_identity = 'YES' AND identity_generation = 'ALWAYS' \
         AND udt_name = 'int8')",
    )
    .fetch_one(&pool)
    .await
    .expect("query seq identity");
    assert!(
        identity.0,
        "messages.seq should be GENERATED ALWAYS AS IDENTITY (bigint)"
    );

    // 10. messages_timeline_idx exists
    let idx: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'messages_timeline_idx'",
    )
    .fetch_one(&pool)
    .await
    .expect("messages_timeline_idx");
    assert!(
        idx.contains("tenant_id, conversation_id, created_at DESC, seq DESC"),
        "timeline index must cover tenant_id, conversation_id, created_at DESC, seq DESC"
    );
}

// ---------------------------------------------------------------------------
// T005 — Migration 0035: agent skills (skills + agent_skills)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn migration_0035_agent_skills() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    // 1. skills columns exist with correct types
    for (col, udt, nullable) in [
        ("id", "uuid", false),
        ("tenant_id", "uuid", false),
        ("name", "text", false),
        ("created_at", "timestamptz", false),
        ("updated_at", "timestamptz", false),
    ] {
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
             WHERE table_name = 'skills' AND column_name = $1 \
             AND udt_name = $2 AND is_nullable = $3)",
        )
        .bind(col)
        .bind(udt)
        .bind(if nullable { "YES" } else { "NO" })
        .fetch_one(&pool)
        .await
        .expect("query column");
        assert!(exists.0, "skills.{col} should exist with type {udt}");
    }

    // 2. skills name CHECK (1-50)
    let def: Option<String> = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
         WHERE conrelid = 'skills'::regclass AND conname = 'skills_name_check' AND contype = 'c'",
    )
    .fetch_one(&pool)
    .await
    .expect("query skills name check");
    let def = def.expect("skills_name_check should exist");
    assert!(
        def.contains("char_length(trim(name)) BETWEEN 1 AND 50"),
        "skills name CHECK should enforce 1-50 chars after trim"
    );

    // 3. Case-insensitive unique index
    let idx: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'skills_tenant_lower_name_uniq'",
    )
    .fetch_one(&pool)
    .await
    .expect("skills_tenant_lower_name_uniq");
    assert!(
        idx.contains("lower(name)"),
        "case-insensitive unique index must use lower(name)"
    );

    // 4. agent_skills columns
    for (col, udt, nullable) in [
        ("tenant_id", "uuid", false),
        ("membership_id", "uuid", false),
        ("skill_id", "uuid", false),
        ("created_at", "timestamptz", false),
    ] {
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
             WHERE table_name = 'agent_skills' AND column_name = $1 \
             AND udt_name = $2 AND is_nullable = $3)",
        )
        .bind(col)
        .bind(udt)
        .bind(if nullable { "YES" } else { "NO" })
        .fetch_one(&pool)
        .await
        .expect("query column");
        assert!(exists.0, "agent_skills.{col} should exist");
    }

    // 5. Composite FKs on agent_skills
    for fk_name in [
        "agent_skills_tenant_id_membership_id_fkey",
        "agent_skills_tenant_id_skill_id_fkey",
    ] {
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM pg_constraint \
             WHERE conrelid = 'agent_skills'::regclass AND conname = $1 AND contype = 'f')",
        )
        .bind(fk_name)
        .fetch_one(&pool)
        .await
        .expect("query FK");
        assert!(exists.0, "{fk_name} should exist");
    }

    // 6. agent_skills_tenant_skill_idx
    let idx: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'agent_skills_tenant_skill_idx'",
    )
    .fetch_one(&pool)
    .await
    .expect("agent_skills_tenant_skill_idx");
    assert!(
        idx.contains("tenant_id, skill_id"),
        "agent_skills index must cover tenant_id, skill_id"
    );
}

// ---------------------------------------------------------------------------
// T005 — Migration 0036: agent availability
// ---------------------------------------------------------------------------

#[tokio::test]
async fn migration_0036_agent_availability() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    // 1. Columns
    for (col, udt, nullable) in [
        ("tenant_id", "uuid", false),
        ("membership_id", "uuid", false),
        ("state", "text", false),
        ("state_changed_at", "timestamptz", false),
        ("created_at", "timestamptz", false),
        ("updated_at", "timestamptz", false),
    ] {
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
             WHERE table_name = 'agent_availability' AND column_name = $1 \
             AND udt_name = $2 AND is_nullable = $3)",
        )
        .bind(col)
        .bind(udt)
        .bind(if nullable { "YES" } else { "NO" })
        .fetch_one(&pool)
        .await
        .expect("query column");
        assert!(exists.0, "agent_availability.{col} should exist");
    }

    // 2. State CHECK (available|away)
    let def: Option<String> = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
         WHERE conrelid = 'agent_availability'::regclass \
         AND conname = 'agent_availability_state_check' AND contype = 'c'",
    )
    .fetch_one(&pool)
    .await
    .expect("query state check");
    let def = def.expect("agent_availability_state_check should exist");
    assert!(
        def.contains("available") && def.contains("away"),
        "state CHECK should allow available and away"
    );

    // 3. Composite FK
    let fk: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM pg_constraint \
         WHERE conrelid = 'agent_availability'::regclass \
         AND conname = 'agent_availability_tenant_id_membership_id_fkey' AND contype = 'f')",
    )
    .fetch_one(&pool)
    .await
    .expect("query FK");
    assert!(fk.0, "agent_availability composite FK should exist");
}

// ---------------------------------------------------------------------------
// T005 — Migration 0037: escalations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn migration_0037_escalations() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    // 1. conversations.escalated_at
    let col: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
         WHERE table_name = 'conversations' AND column_name = 'escalated_at' \
         AND is_nullable = 'YES')",
    )
    .fetch_one(&pool)
    .await
    .expect("query escalated_at");
    assert!(
        col.0,
        "conversations.escalated_at should exist and be nullable"
    );

    // 2. conversations_tenant_id_id_uq
    let uq: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM pg_constraint \
         WHERE conrelid = 'conversations'::regclass \
         AND conname = 'conversations_tenant_id_id_uq' AND contype = 'u')",
    )
    .fetch_one(&pool)
    .await
    .expect("query unique constraint");
    assert!(uq.0, "conversations_tenant_id_id_uq should exist");

    // 3. Load-count index
    let idx: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'conversations_open_pending_load_idx'",
    )
    .fetch_one(&pool)
    .await
    .expect("conversations_open_pending_load_idx");
    assert!(
        idx.contains("tenant_id, assigned_membership_id"),
        "load-count index must cover tenant_id, assigned_membership_id"
    );

    // 4. Escalated inbox index
    let idx: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'conversations_escalated_inbox_idx'",
    )
    .fetch_one(&pool)
    .await
    .expect("conversations_escalated_inbox_idx");
    assert!(
        idx.contains("escalated_at IS NOT NULL"),
        "escalated inbox index must be partial on escalated_at IS NOT NULL"
    );

    // 5. escalations columns
    for (col, udt, nullable) in [
        ("id", "uuid", false),
        ("tenant_id", "uuid", false),
        ("conversation_id", "uuid", false),
        ("reason", "text", false),
        ("required_skill_ids", "_uuid", false),
        ("required_skill_names", "_text", false),
        ("status", "text", false),
        ("routing_reason", "text", true),
        ("matched_skill_ids", "_uuid", false),
        ("matched_skill_names", "_text", false),
        ("assigned_membership_id", "uuid", true),
        ("escalated_at", "timestamptz", false),
        ("assigned_at", "timestamptz", true),
        ("closed_at", "timestamptz", true),
    ] {
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
             WHERE table_name = 'escalations' AND column_name = $1 \
             AND udt_name = $2 AND is_nullable = $3)",
        )
        .bind(col)
        .bind(udt)
        .bind(if nullable { "YES" } else { "NO" })
        .fetch_one(&pool)
        .await
        .expect("query column");
        assert!(exists.0, "escalations.{col} should exist");
    }

    // 6. Status CHECK
    let def: Option<String> = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
         WHERE conrelid = 'escalations'::regclass \
         AND conname = 'escalations_status_check' AND contype = 'c'",
    )
    .fetch_one(&pool)
    .await
    .expect("query status check");
    let def = def.expect("escalations_status_check should exist");
    assert!(
        def.contains("queued") && def.contains("assigned") && def.contains("closed"),
        "status CHECK should allow queued, assigned, closed"
    );

    // 7. Consistency CHECK
    let check: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM pg_constraint \
         WHERE conrelid = 'escalations'::regclass \
         AND conname = 'escalations_consistency_check' AND contype = 'c')",
    )
    .fetch_one(&pool)
    .await
    .expect("query consistency check");
    assert!(check.0, "escalations_consistency_check should exist");

    // 8. Partial unique index (one active per conversation)
    let idx: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'escalations_one_active_uniq'",
    )
    .fetch_one(&pool)
    .await
    .expect("escalations_one_active_uniq");
    assert!(
        idx.contains("WHERE ((status = 'queued'::text) OR (status = 'assigned'::text))"),
        "escalations_one_active_uniq must be partial on queued/assigned"
    );
    assert!(
        idx.contains("UNIQUE"),
        "escalations_one_active_uniq must be unique"
    );

    // 9. Queue index
    let idx: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'escalations_queue_idx'",
    )
    .fetch_one(&pool)
    .await
    .expect("escalations_queue_idx");
    assert!(
        idx.contains("tenant_id, escalated_at"),
        "queue index must cover tenant_id, escalated_at"
    );

    // 10. outbox_escalations_claimable_idx
    let idx: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'outbox_escalations_claimable_idx'",
    )
    .fetch_one(&pool)
    .await
    .expect("outbox_escalations_claimable_idx");
    assert!(
        idx.contains("claimed_at IS NULL"),
        "outbox claimable index must be partial on claimed_at IS NULL"
    );
}

// ---------------------------------------------------------------------------
// T007 — Migration 0038: ai_configurations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn migration_0038_ai_configurations() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    // 1. Columns
    for (col, udt, nullable) in [
        ("id", "uuid", false),
        ("tenant_id", "uuid", true),
        ("provider", "text", false),
        ("model", "text", false),
        ("max_output_tokens", "int4", true),
        ("temperature", "float4", true),
        ("fallbacks", "jsonb", false),
        ("capture_content", "bool", false),
        ("created_at", "timestamptz", false),
        ("updated_at", "timestamptz", false),
        ("deleted_at", "timestamptz", true),
    ] {
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
             WHERE table_name = 'ai_configurations' AND column_name = $1 \
             AND udt_name = $2 AND is_nullable = $3)",
        )
        .bind(col)
        .bind(udt)
        .bind(if nullable { "YES" } else { "NO" })
        .fetch_one(&pool)
        .await
        .expect("query column");
        assert!(exists.0, "ai_configurations.{col} should exist");
    }

    // 2. Provider CHECK
    let def: Option<String> = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
         WHERE conrelid = 'ai_configurations'::regclass \
         AND conname = 'ai_configurations_provider_check' AND contype = 'c'",
    )
    .fetch_one(&pool)
    .await
    .expect("query provider check");
    let def = def.expect("ai_configurations_provider_check should exist");
    assert!(
        def.contains("openai") && def.contains("anthropic") && def.contains("gemini"),
        "provider CHECK should allow openai, anthropic, gemini"
    );

    // 3. Temperature CHECK
    let def: Option<String> = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
         WHERE conrelid = 'ai_configurations'::regclass \
         AND conname = 'ai_configurations_temperature_check' AND contype = 'c'",
    )
    .fetch_one(&pool)
    .await
    .expect("query temperature check");
    let def = def.expect("ai_configurations_temperature_check should exist");
    assert!(
        def.contains(">= 0") && def.contains("<= 2"),
        "temperature CHECK should constrain 0..2"
    );

    // 4. Platform partial unique index
    let idx: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'ai_configurations_platform_live_uq'",
    )
    .fetch_one(&pool)
    .await
    .expect("ai_configurations_platform_live_uq");
    assert!(idx.contains("UNIQUE"), "platform_live_uq must be unique");
    assert!(
        idx.contains("WHERE tenant_id IS NULL"),
        "platform_live_uq must be partial on tenant_id IS NULL"
    );

    // 5. Tenant partial unique index (enforce one live row per tenant)
    let idx: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'ai_configurations_tenant_live_uq'",
    )
    .fetch_one(&pool)
    .await
    .expect("ai_configurations_tenant_live_uq");
    assert!(idx.contains("UNIQUE"), "tenant_live_uq must be unique");

    // 5b. Insert a tenant row, second same tenant fails (unique), soft-delete then insert ok
    sqlx::query(
        "INSERT INTO ai_configurations (tenant_id, provider, model) VALUES ($1, 'openai', 'gpt-4')",
    )
    .bind(uuid::Uuid::nil())
    .execute(&pool)
    .await
    .expect("insert first tenant config");
    let err = sqlx::query(
        "INSERT INTO ai_configurations (tenant_id, provider, model) VALUES ($1, 'anthropic', 'claude-3')",
    )
    .bind(uuid::Uuid::nil())
    .execute(&pool)
    .await
    .expect_err("second tenant row should fail");
    assert!(
        err.to_string().contains("duplicate"),
        "second tenant row should be rejected: {err}"
    );
    sqlx::query("UPDATE ai_configurations SET deleted_at = now() WHERE tenant_id = $1")
        .bind(uuid::Uuid::nil())
        .execute(&pool)
        .await
        .expect("soft-delete first row");
    sqlx::query(
        "INSERT INTO ai_configurations (tenant_id, provider, model) VALUES ($1, 'anthropic', 'claude-3')",
    )
    .bind(uuid::Uuid::nil())
    .execute(&pool)
    .await
    .expect("insert after soft-delete should succeed");

    // Cleanup
    sqlx::query("DELETE FROM ai_configurations WHERE tenant_id = $1")
        .bind(uuid::Uuid::nil())
        .execute(&pool)
        .await
        .expect("cleanup ai_configurations");
}

// ---------------------------------------------------------------------------
// T008 — Migration 0039: ai_credentials
// ---------------------------------------------------------------------------

#[tokio::test]
async fn migration_0039_ai_credentials() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    // 1. Columns
    for (col, udt, nullable) in [
        ("id", "uuid", false),
        ("tenant_id", "uuid", true),
        ("provider", "text", false),
        ("ciphertext", "bytea", false),
        ("nonce", "bytea", false),
        ("key_hint", "text", false),
        ("created_at", "timestamptz", false),
        ("updated_at", "timestamptz", false),
        ("deleted_at", "timestamptz", true),
    ] {
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
             WHERE table_name = 'ai_credentials' AND column_name = $1 \
             AND udt_name = $2 AND is_nullable = $3)",
        )
        .bind(col)
        .bind(udt)
        .bind(if nullable { "YES" } else { "NO" })
        .fetch_one(&pool)
        .await
        .expect("query column");
        assert!(exists.0, "ai_credentials.{col} should exist");
    }

    // 2. Provider CHECK
    let def: Option<String> = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
         WHERE conrelid = 'ai_credentials'::regclass \
         AND conname = 'ai_credentials_provider_check' AND contype = 'c'",
    )
    .fetch_one(&pool)
    .await
    .expect("query provider check");
    let def = def.expect("ai_credentials_provider_check should exist");
    assert!(
        def.contains("openai") && def.contains("anthropic") && def.contains("gemini"),
        "provider CHECK should allow openai, anthropic, gemini"
    );

    // 3. Platform-unique per provider (tenant_id IS NULL)
    sqlx::query(
        "INSERT INTO ai_credentials (provider, ciphertext, nonce, key_hint) \
         VALUES ('openai', '\\x01'::bytea, '\\x01'::bytea, 'sk-...')",
    )
    .execute(&pool)
    .await
    .expect("insert first platform credential");
    let err = sqlx::query(
        "INSERT INTO ai_credentials (provider, ciphertext, nonce, key_hint) \
         VALUES ('openai', '\\x02'::bytea, '\\x02'::bytea, 'sk-...')",
    )
    .execute(&pool)
    .await
    .expect_err("second platform row for same provider should fail");
    assert!(
        err.to_string().contains("duplicate"),
        "second platform row should be rejected: {err}"
    );

    // 4. Per-scope partial unique indexes exist
    for idxname in [
        "ai_credentials_platform_provider_live_uq",
        "ai_credentials_tenant_provider_live_uq",
    ] {
        let idx: String =
            sqlx::query_scalar("SELECT indexdef FROM pg_indexes WHERE indexname = $1")
                .bind(idxname)
                .fetch_one(&pool)
                .await
                .unwrap_or_else(|_| panic!("{idxname} should exist"));
        assert!(idx.contains("UNIQUE"), "{idxname} must be unique");
    }

    // Cleanup
    sqlx::query("DELETE FROM ai_credentials")
        .execute(&pool)
        .await
        .expect("cleanup ai_credentials");
}

// ---------------------------------------------------------------------------
// T009 — Migration 0040: ai_usage_records
// ---------------------------------------------------------------------------

#[tokio::test]
async fn migration_0040_ai_usage_records() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();

    // 1. Columns
    for (col, udt, nullable) in [
        ("id", "uuid", false),
        ("tenant_id", "uuid", false),
        ("provider", "text", false),
        ("model", "text", false),
        ("input_tokens", "int4", true),
        ("output_tokens", "int4", true),
        ("status", "text", false),
        ("error_category", "text", true),
        ("streamed", "bool", false),
        ("latency_ms", "int4", false),
        ("request_id", "text", true),
        ("request_content", "jsonb", true),
        ("response_content", "text", true),
        ("created_at", "timestamptz", false),
    ] {
        let exists: (bool,) = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
             WHERE table_name = 'ai_usage_records' AND column_name = $1 \
             AND udt_name = $2 AND is_nullable = $3)",
        )
        .bind(col)
        .bind(udt)
        .bind(if nullable { "YES" } else { "NO" })
        .fetch_one(&pool)
        .await
        .expect("query column");
        assert!(exists.0, "ai_usage_records.{col} should exist");
    }

    // 2. No updated_at or deleted_at
    let has_updated_at: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
         WHERE table_name = 'ai_usage_records' AND column_name = 'updated_at')",
    )
    .fetch_one(&pool)
    .await
    .expect("query updated_at");
    assert!(
        !has_updated_at.0,
        "ai_usage_records must NOT have updated_at"
    );

    let has_deleted_at: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
         WHERE table_name = 'ai_usage_records' AND column_name = 'deleted_at')",
    )
    .fetch_one(&pool)
    .await
    .expect("query deleted_at");
    assert!(
        !has_deleted_at.0,
        "ai_usage_records must NOT have deleted_at"
    );

    // 3. Status CHECK
    let def: Option<String> = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
         WHERE conrelid = 'ai_usage_records'::regclass \
         AND conname = 'ai_usage_records_status_check' AND contype = 'c'",
    )
    .fetch_one(&pool)
    .await
    .expect("query status check");
    let def = def.expect("ai_usage_records_status_check should exist");
    assert!(
        def.contains("success") && def.contains("failure"),
        "status CHECK should allow success, failure"
    );

    // 4. Error_category CHECK
    let def: Option<String> = sqlx::query_scalar(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint \
         WHERE conrelid = 'ai_usage_records'::regclass \
         AND conname = 'ai_usage_records_error_category_check' AND contype = 'c'",
    )
    .fetch_one(&pool)
    .await
    .expect("query error_category check");
    let def = def.expect("ai_usage_records_error_category_check should exist");
    assert!(
        def.contains("authentication") && def.contains("rate_limited"),
        "error_category CHECK should list categories"
    );

    // 5. ai_usage_records_tenant_created_idx exists
    let idx: String = sqlx::query_scalar(
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'ai_usage_records_tenant_created_idx'",
    )
    .fetch_one(&pool)
    .await
    .expect("ai_usage_records_tenant_created_idx");
    assert!(
        idx.contains("tenant_id, created_at"),
        "index must cover tenant_id, created_at DESC"
    );
}
