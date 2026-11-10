//! The audited cross-tenant path (ADR-004 section 2, MOH-46): one `SECURITY DEFINER` function
//! per job in the `xtenant` schema, owned by `app_platform`.
//!
//! Two things are checked:
//! - the catalog: every function is locked down the same way (owner, `search_path`, privileges);
//! - behaviour, per function: with Row-Level Security forced on the tables it reads, the function
//!   still returns what it should and writes its audit row, while `app_runtime` reading the same
//!   tables directly sees nothing. The policies don't exist yet, so each test puts them in place
//!   inside an owner transaction that is rolled back (see `db::testing`).
//!
//! They need the role split (`MIGRATION_DATABASE_URL`); like the rest of the DB tests they skip
//! when it is missing and fail instead when `REQUIRE_DB_TESTS=1`.
use sqlx::{PgConnection, Postgres, Transaction};
use uuid::Uuid;

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

/// True when the test can run: the owner URL is set (CI sets it, `REQUIRE_DB_TESTS=1` enforces it).
fn can_run() -> bool {
    if require_db_tests() || std::env::var("MIGRATION_DATABASE_URL").is_ok_and(|v| !v.is_empty()) {
        return true;
    }
    eprintln!("skipping xtenant test: MIGRATION_DATABASE_URL not set");
    false
}

type Tx = Transaction<'static, Postgres>;

async fn scalar_i64(conn: &mut PgConnection, sql: &str, bind: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.to_string()))
        .bind(bind.to_string())
        .fetch_one(conn)
        .await
        .unwrap()
}

/// Audit rows the functions wrote for `resource_id`, read as `app_platform` (the only role that
/// can see `audit_logs` once it is under Row-Level Security). Leaves the transaction as
/// `app_runtime` again.
async fn audit_rows(conn: &mut PgConnection, function: &str, resource_id: &str) -> i64 {
    db::testing::become_platform(conn).await;
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs \
         WHERE action = $1 AND resource_type = 'cross_tenant_access' AND resource_id = $2",
    )
    .bind(format!("xtenant.{function}"))
    .bind(resource_id)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    db::testing::become_runtime(conn).await;
    n
}

async fn seed_tenant(tx: &mut Tx) -> Uuid {
    let unique = Uuid::new_v4().simple().to_string();
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(format!("Tenant {unique}"))
        .bind(format!("t-{unique}"))
        .fetch_one(&mut **tx)
        .await
        .unwrap()
}

async fn seed_user(tx: &mut Tx) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name) VALUES ($1, 'Xtenant Test') RETURNING id",
    )
    .bind(format!("{}@example.test", Uuid::new_v4().simple()))
    .fetch_one(&mut **tx)
    .await
    .unwrap()
}

// ---------------------------------------------------------------------------------------------
// The catalog
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn every_xtenant_function_is_locked_down() {
    if !can_run() {
        return;
    }
    db::testing::migrate().await;
    let pool = db::testing::owner_pool();
    // proname, owner, security definer, search_path setting, PUBLIC may execute, app_runtime may execute
    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, String, bool, Option<String>, bool, bool)> = sqlx::query_as(
        "SELECT p.proname::text,
                pg_get_userbyid(p.proowner)::text,
                p.prosecdef,
                (SELECT c FROM unnest(p.proconfig) c WHERE c LIKE 'search_path=%'),
                -- a NULL ACL is the default, which includes PUBLIC
                (p.proacl IS NULL OR EXISTS (SELECT 1 FROM aclexplode(p.proacl) a
                                             WHERE a.grantee = 0 AND a.privilege_type = 'EXECUTE')),
                has_function_privilege('app_runtime', p.oid, 'EXECUTE')
         FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace
         WHERE n.nspname = 'xtenant'
         ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    pool.close().await;

    assert!(
        rows.len() >= 8,
        "expected the xtenant functions to exist, found {rows:?}"
    );
    for (name, owner, definer, search_path, public_exec, runtime_exec) in rows {
        assert_eq!(
            owner, "app_platform",
            "{name}: must be owned by app_platform"
        );
        assert!(definer, "{name}: must be SECURITY DEFINER");
        let search_path = search_path.unwrap_or_default();
        assert!(
            search_path.ends_with(", pg_temp") && !search_path.contains("\"$user\""),
            "{name}: search_path must be pinned with pg_temp last, got `{search_path}`"
        );
        assert!(
            !public_exec,
            "{name}: PUBLIC must not be able to execute it"
        );
        // `_`-prefixed functions are internal helpers called by the others, never by the app.
        if name.starts_with('_') {
            assert!(
                !runtime_exec,
                "{name}: an internal helper must not be executable by app_runtime"
            );
        } else {
            assert!(
                runtime_exec,
                "{name}: app_runtime must be able to execute it"
            );
        }
    }
}

#[tokio::test]
async fn app_platform_is_a_nologin_bypassrls_role_nobody_can_become_but_the_migration_role() {
    if !can_run() {
        return;
    }
    let pool = db::testing::owner_pool();
    let (login, bypass, superuser, createdb, createrole): (bool, bool, bool, bool, bool) =
        sqlx::query_as(
            "SELECT rolcanlogin, rolbypassrls, rolsuper, rolcreatedb, rolcreaterole \
             FROM pg_roles WHERE rolname = 'app_platform'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!login, "app_platform must not be able to log in");
    assert!(
        bypass,
        "app_platform needs BYPASSRLS to see rows under forced RLS"
    );
    assert!(
        !superuser && !createdb && !createrole,
        "app_platform must have no other elevated attributes"
    );

    let (runtime_can_set, owner_can_set, owner_inherits): (bool, bool, bool) = sqlx::query_as(
        "SELECT pg_has_role('app_runtime', 'app_platform', 'SET'),
                pg_has_role('app_owner', 'app_platform', 'SET'),
                pg_has_role('app_owner', 'app_platform', 'USAGE')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        !runtime_can_set,
        "app_runtime must not be able to become app_platform"
    );
    assert!(
        owner_can_set,
        "migrations (app_owner) create the functions as app_platform"
    );
    assert!(
        !owner_inherits,
        "app_owner must not inherit app_platform's BYPASSRLS"
    );
    let owns_tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_class c WHERE c.relowner = 'app_platform'::regrole AND c.relkind IN ('r', 'p')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(owns_tables, 0, "app_platform owns functions, never tables");
    pool.close().await;
}

// ---------------------------------------------------------------------------------------------
// Behaviour, per function
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn user_memberships_returns_a_users_memberships_across_tenants() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let user = seed_user(&mut tx).await;
    let other_user = seed_user(&mut tx).await;
    let (t1, t2) = (seed_tenant(&mut tx).await, seed_tenant(&mut tx).await);
    for (tenant, who, role) in [
        (t1, user, "owner"),
        (t2, user, "agent"),
        (t1, other_user, "viewer"),
    ] {
        sqlx::query(
            "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)",
        )
        .bind(tenant)
        .bind(who)
        .bind(role)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    db::testing::enforce_rls(&mut tx, &["tenant_memberships", "audit_logs"]).await;
    db::testing::become_runtime(&mut tx).await;

    assert_eq!(
        scalar_i64(
            &mut tx,
            "SELECT count(*) FROM tenant_memberships WHERE user_id::text = $1",
            &user.to_string()
        )
        .await,
        0,
        "app_runtime must not read memberships directly without a tenant"
    );
    let rows: Vec<(Uuid, String, String, String)> =
        sqlx::query_as("SELECT tenant_id, name, slug, role FROM xtenant.user_memberships($1)")
            .bind(user)
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    let mut tenants: Vec<Uuid> = rows.iter().map(|r| r.0).collect();
    tenants.sort();
    let mut expected = vec![t1, t2];
    expected.sort();
    assert_eq!(
        tenants, expected,
        "only this user's memberships, across both tenants"
    );
    assert_eq!(
        audit_rows(&mut tx, "user_memberships", &user.to_string()).await,
        1
    );

    // A user with no memberships gets nothing and no audit row.
    let nobody = Uuid::new_v4();
    let none: Vec<(Uuid,)> = sqlx::query_as("SELECT tenant_id FROM xtenant.user_memberships($1)")
        .bind(nobody)
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert!(none.is_empty());
    assert_eq!(
        audit_rows(&mut tx, "user_memberships", &nobody.to_string()).await,
        0
    );
}

#[tokio::test]
async fn record_auth_event_writes_only_auth_actions_and_app_runtime_cannot_write_audit_rows_directly(
) {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let user = seed_user(&mut tx).await;
    db::testing::enforce_rls(&mut tx, &["audit_logs"]).await;
    db::testing::become_runtime(&mut tx).await;

    let resource = user.to_string();
    sqlx::query("SELECT xtenant.record_auth_event($1, $2, $3, $4)")
        .bind("auth.login_succeeded")
        .bind(user)
        .bind(&resource)
        .bind(serde_json::json!({}))
        .execute(&mut *tx)
        .await
        .unwrap();
    db::testing::become_platform(&mut tx).await;
    let (action, resource_type, tenant): (String, String, Option<Uuid>) = sqlx::query_as(
        "SELECT action, resource_type, tenant_id FROM audit_logs WHERE resource_id = $1",
    )
    .bind(&resource)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        (action.as_str(), resource_type.as_str(), tenant),
        ("auth.login_succeeded", "user", None)
    );
    db::testing::become_runtime(&mut tx).await;

    // Not an auth event: refused. The failed statement aborts the transaction, so it goes last
    // inside a savepoint.
    let mut sp = sqlx::Acquire::begin(&mut *tx).await.unwrap();
    let refused = sqlx::query("SELECT xtenant.record_auth_event($1, $2, $3, $4)")
        .bind("tenant.deleted")
        .bind(user)
        .bind("x")
        .bind(serde_json::json!({}))
        .execute(&mut *sp)
        .await;
    assert!(refused.is_err(), "only auth.* actions may be recorded");
    sp.rollback().await.unwrap();

    let mut sp = sqlx::Acquire::begin(&mut *tx).await.unwrap();
    let direct = sqlx::query(
        "INSERT INTO audit_logs (action, resource_type, resource_id) VALUES ('x', 'user', 'x')",
    )
    .execute(&mut *sp)
    .await;
    assert!(
        direct.is_err(),
        "app_runtime must not insert into audit_logs without a tenant once it is under RLS"
    );
    sp.rollback().await.unwrap();
}

#[tokio::test]
async fn invitation_by_token_resolves_the_tenant_from_the_token() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let tenant = seed_tenant(&mut tx).await;
    let inviter = seed_user(&mut tx).await;
    let token_hash = Uuid::new_v4().simple().to_string();
    let invitation: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_invitations (tenant_id, email, role, token_hash, invited_by, expires_at) \
         VALUES ($1, 'invitee@example.test', 'agent', $2, $3, now() + interval '1 day') RETURNING id",
    )
    .bind(tenant)
    .bind(&token_hash)
    .bind(inviter)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    db::testing::enforce_rls(&mut tx, &["tenant_invitations", "audit_logs"]).await;
    db::testing::become_runtime(&mut tx).await;

    assert_eq!(
        scalar_i64(
            &mut tx,
            "SELECT count(*) FROM tenant_invitations WHERE token_hash = $1",
            &token_hash
        )
        .await,
        0,
        "app_runtime must not read invitations directly without a tenant"
    );
    let found: Option<(Uuid, Uuid, String, String, String, String)> = sqlx::query_as(
        "SELECT id, tenant_id, email, role, status, tenant_status FROM xtenant.invitation_by_token($1)",
    )
    .bind(&token_hash)
    .fetch_optional(&mut *tx)
    .await
    .unwrap();
    let (id, tenant_id, email, role, status, tenant_status) = found.expect("the invitation");
    assert_eq!(
        (
            id,
            tenant_id,
            email.as_str(),
            role.as_str(),
            status.as_str(),
            tenant_status.as_str()
        ),
        (
            invitation,
            tenant,
            "invitee@example.test",
            "agent",
            "pending",
            "active"
        )
    );
    assert_eq!(
        audit_rows(&mut tx, "invitation_by_token", &invitation.to_string()).await,
        1
    );

    let miss: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM xtenant.invitation_by_token($1)")
        .bind("no-such-token")
        .fetch_optional(&mut *tx)
        .await
        .unwrap();
    assert!(miss.is_none(), "an unknown token resolves to nothing");
    assert_eq!(
        audit_rows(&mut tx, "invitation_by_token", &invitation.to_string()).await,
        1,
        "a miss writes no audit row"
    );
}

#[tokio::test]
async fn invitation_by_token_treats_a_soft_deleted_tenant_as_a_miss() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let tenant = seed_tenant(&mut tx).await;
    let inviter = seed_user(&mut tx).await;
    let token_hash = Uuid::new_v4().simple().to_string();
    sqlx::query(
        "INSERT INTO tenant_invitations (tenant_id, email, role, token_hash, invited_by, expires_at) \
         VALUES ($1, 'invitee@example.test', 'agent', $2, $3, now() + interval '1 day')",
    )
    .bind(tenant)
    .bind(&token_hash)
    .bind(inviter)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("UPDATE tenants SET deleted_at = now() WHERE id = $1")
        .bind(tenant)
        .execute(&mut *tx)
        .await
        .unwrap();
    db::testing::enforce_rls(&mut tx, &["tenant_invitations", "audit_logs"]).await;
    db::testing::become_runtime(&mut tx).await;

    let miss: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM xtenant.invitation_by_token($1)")
        .bind(&token_hash)
        .fetch_optional(&mut *tx)
        .await
        .unwrap();
    assert!(miss.is_none());
}

#[tokio::test]
async fn widget_lookups_resolve_instance_and_session_without_a_tenant() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let tenant = seed_tenant(&mut tx).await;
    let public_id = format!("wgt_{}", Uuid::new_v4().simple());
    let instance: Uuid = sqlx::query_scalar(
        "INSERT INTO widget_instances (tenant_id, public_id, name) VALUES ($1, $2, 'Support') RETURNING id",
    )
    .bind(tenant)
    .bind(&public_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let live_hash = Uuid::new_v4().as_bytes().to_vec();
    let expired_hash = Uuid::new_v4().as_bytes().to_vec();
    let mut live_session = Uuid::nil();
    for (hash, interval) in [(&live_hash, "1 hour"), (&expired_hash, "-1 hour")] {
        let id: Uuid = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "INSERT INTO widget_sessions (tenant_id, widget_instance_id, token_hash, expires_at) \
             VALUES ($1, $2, $3, now() + interval '{interval}') RETURNING id"
        )))
        .bind(tenant)
        .bind(instance)
        .bind(hash)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        if hash == &live_hash {
            live_session = id;
        }
    }
    db::testing::enforce_rls(
        &mut tx,
        &["widget_instances", "widget_sessions", "audit_logs"],
    )
    .await;
    db::testing::become_runtime(&mut tx).await;

    assert_eq!(
        scalar_i64(
            &mut tx,
            "SELECT count(*) FROM widget_instances WHERE public_id = $1",
            &public_id
        )
        .await,
        0,
        "app_runtime must not read widget instances directly without a tenant"
    );
    let found: Option<(Uuid, Uuid, String, Vec<String>)> = sqlx::query_as(
        "SELECT id, tenant_id, name, allowed_domains FROM xtenant.widget_instance_by_public_id($1)",
    )
    .bind(&public_id)
    .fetch_optional(&mut *tx)
    .await
    .unwrap();
    let (id, tenant_id, name, _domains) = found.expect("the widget instance");
    assert_eq!(
        (id, tenant_id, name.as_str()),
        (instance, tenant, "Support")
    );
    assert_eq!(
        audit_rows(
            &mut tx,
            "widget_instance_by_public_id",
            &instance.to_string()
        )
        .await,
        1
    );
    let no_instance: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM xtenant.widget_instance_by_public_id('wgt_unknown')")
            .fetch_optional(&mut *tx)
            .await
            .unwrap();
    assert!(no_instance.is_none());

    let session: Option<(Uuid, Uuid, Uuid)> = sqlx::query_as(
        "SELECT id, tenant_id, widget_instance_id FROM xtenant.widget_session_by_token($1)",
    )
    .bind(&live_hash)
    .fetch_optional(&mut *tx)
    .await
    .unwrap();
    assert_eq!(session, Some((live_session, tenant, instance)));
    assert_eq!(
        audit_rows(
            &mut tx,
            "widget_session_by_token",
            &live_session.to_string()
        )
        .await,
        1
    );
    let expired: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM xtenant.widget_session_by_token($1)")
            .bind(&expired_hash)
            .fetch_optional(&mut *tx)
            .await
            .unwrap();
    assert!(expired.is_none(), "an expired session is a miss");
}

#[tokio::test]
async fn webhook_lookups_resolve_the_connection_and_tenant_from_the_token() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let tenant = seed_tenant(&mut tx).await;
    let (catalog, slug): (Uuid, String) =
        sqlx::query_as("SELECT id, slug::text FROM integration_catalog WHERE slug = 'whatsapp'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    let token_hash = Uuid::new_v4().as_bytes().to_vec();
    let connection: Uuid = sqlx::query_scalar(
        "INSERT INTO integration_connections \
            (tenant_id, catalog_id, webhook_token_hash, webhook_token_ciphertext, webhook_token_nonce) \
         VALUES ($1, $2, $3, '\\x00', '\\x00') RETURNING id",
    )
    .bind(tenant)
    .bind(catalog)
    .bind(&token_hash)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    db::testing::enforce_rls(&mut tx, &["integration_connections", "audit_logs"]).await;
    db::testing::become_runtime(&mut tx).await;

    let direct: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM integration_connections WHERE webhook_token_hash = $1",
    )
    .bind(&token_hash)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        direct, 0,
        "app_runtime must not read connections directly without a tenant"
    );

    let by_token: Option<(Uuid, Uuid, String, bool)> = sqlx::query_as(
        "SELECT connection_id, tenant_id, catalog_slug, is_active FROM xtenant.webhook_connection_by_token($1)",
    )
    .bind(&token_hash)
    .fetch_optional(&mut *tx)
    .await
    .unwrap();
    assert_eq!(by_token, Some((connection, tenant, slug.clone(), true)));
    assert_eq!(
        audit_rows(
            &mut tx,
            "webhook_connection_by_token",
            &connection.to_string()
        )
        .await,
        1
    );

    let by_slug: Option<(Uuid, Uuid, Uuid, bool)> = sqlx::query_as(
        "SELECT id, tenant_id, catalog_id, is_active FROM xtenant.webhook_connection_by_token_and_slug($1, $2)",
    )
    .bind(&token_hash)
    .bind(&slug)
    .fetch_optional(&mut *tx)
    .await
    .unwrap();
    assert_eq!(by_slug, Some((connection, tenant, catalog, true)));
    assert_eq!(
        audit_rows(
            &mut tx,
            "webhook_connection_by_token_and_slug",
            &connection.to_string()
        )
        .await,
        1
    );

    let wrong_slug: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM xtenant.webhook_connection_by_token_and_slug($1, 'slack')")
            .bind(&token_hash)
            .fetch_optional(&mut *tx)
            .await
            .unwrap();
    assert!(
        wrong_slug.is_none(),
        "the token only resolves for its own channel"
    );
    let unknown: Option<(Uuid,)> =
        sqlx::query_as("SELECT connection_id FROM xtenant.webhook_connection_by_token($1)")
            .bind(Uuid::new_v4().as_bytes().to_vec())
            .fetch_optional(&mut *tx)
            .await
            .unwrap();
    assert!(unknown.is_none());
}
