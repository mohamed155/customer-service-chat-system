use std::time::Duration;

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("skipping schema test: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
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
    let original_deleted = chrono::Utc::now() - chrono::Duration::hours(1);
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
    // Disable seqscan and check for index scan
    let plan: (serde_json::Value,) = sqlx::query_as(
        r#"
        SET LOCAL enable_seqscan = off;
        EXPLAIN (FORMAT JSON)
        SELECT * FROM tenant_memberships WHERE tenant_id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(tid)
    .fetch_one(&pool)
    .await
    .unwrap();
    let plan_str = plan.0.to_string();
    assert!(
        plan_str.contains("Index Scan") || plan_str.contains("Bitmap Index Scan"),
        "tenant-scoped membership query should use an index, got plan: {}",
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
    let plan: (serde_json::Value,) = sqlx::query_as(
        r#"
        SET LOCAL enable_seqscan = off;
        EXPLAIN (FORMAT JSON)
        SELECT * FROM audit_logs WHERE tenant_id = $1 ORDER BY created_at DESC
        "#,
    )
    .bind(tid)
    .fetch_one(&pool)
    .await
    .unwrap();
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
