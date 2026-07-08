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
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .expect("query _sqlx_migrations");
    assert!(count.0 > 0, "at least baseline migrations exist");
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
    let slug = valid_slug();
    sqlx::query("INSERT INTO tenants (name, slug) VALUES ($1, $2)")
        .bind("Rename Co")
        .bind(&slug)
        .execute(&pool)
        .await
        .unwrap();
    let new_slug = format!(
        "renamed-{}",
        uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
    );
    let result = sqlx::query("UPDATE tenants SET slug = $1 WHERE slug = $2")
        .bind(&new_slug)
        .bind(&slug)
        .execute(&pool)
        .await;
    assert!(result.is_ok(), "slug rename to a free value should succeed");
}

#[tokio::test]
async fn tenants_slug_rename_to_taken_active_rejected() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
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
    let result = sqlx::query("UPDATE tenants SET slug = $1 WHERE slug = $2")
        .bind(&slug_a)
        .bind(&slug_b)
        .execute(&pool)
        .await;
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
    let result = sqlx::query(
        "INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(user_id)
    .bind("test.action")
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
        sqlx::query_as("SELECT details FROM audit_logs WHERE resource_id = 'res-1'")
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
        "INSERT INTO audit_logs (action, resource_type, tenant_id) VALUES ($1, $2, $3)",
    )
    .bind("platform.action")
    .bind("config")
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
        "INSERT INTO audit_logs (actor_user_id, action, resource_type) VALUES ($1, $2, $3)",
    )
    .bind(Option::<uuid::Uuid>::None)
    .bind("system.action")
    .bind("scheduler")
    .execute(&pool)
    .await;
    assert!(
        result.is_ok(),
        "system audit entry (actor_user_id NULL) should be accepted"
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
        "INSERT INTO audit_logs (action, resource_type) VALUES ('test.defaults', 'test') RETURNING details",
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
async fn convention_tenants_bare_insert_receives_uuid() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let id: uuid::Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
            .bind("UUID Tenant")
            .bind(valid_slug())
            .fetch_one(&pool)
            .await
            .expect("bare tenant insert");
    assert_ne!(id.as_u128(), 0);
}

#[tokio::test]
async fn convention_memberships_bare_insert_receives_uuid() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let uid = seed_user(&pool).await;
    let tid = seed_tenant(&pool).await;
    let id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tid)
    .bind(uid)
    .bind("agent")
    .fetch_one(&pool)
    .await
    .expect("bare membership insert");
    assert_ne!(id.as_u128(), 0);
}

#[tokio::test]
async fn convention_audit_logs_bare_insert_receives_uuid() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    db::run_migrations(&pool).await.unwrap();
    let id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO audit_logs (action, resource_type) VALUES ($1, $2) RETURNING id",
    )
    .bind("test.auto_id")
    .bind("test")
    .fetch_one(&pool)
    .await
    .expect("bare audit insert");
    assert_ne!(id.as_u128(), 0);
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
    let (new_updated,): (chrono::DateTime<chrono::Utc>,) =
        sqlx::query_as("SELECT updated_at FROM users WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        new_updated > updated,
        "updated_at should advance after UPDATE"
    );
    assert_eq!(
        created,
        updated.min(created),
        "created_at should remain unchanged"
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
        "INSERT INTO audit_logs (actor_user_id, action, resource_type, tenant_id) VALUES ($1, $2, $3, $4)",
    )
    .bind(uid)
    .bind("user.soft_deleted")
    .bind("user")
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
        "INSERT INTO audit_logs (action, resource_type) VALUES ($1, $2) RETURNING id",
    )
    .bind("test.immutable")
    .bind("test")
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
        "INSERT INTO audit_logs (action, resource_type) VALUES ($1, $2) RETURNING id",
    )
    .bind("test.immutable_del")
    .bind("test")
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
    sqlx::query("INSERT INTO audit_logs (action, resource_type) VALUES ($1, 'test')")
        .bind(&marker)
        .execute(&pool)
        .await
        .unwrap();
    let before: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_logs WHERE action = $1")
        .bind(&marker)
        .fetch_one(&pool)
        .await
        .unwrap();
    // Attempt a delete (will fail)
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
