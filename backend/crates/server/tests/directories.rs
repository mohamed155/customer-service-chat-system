#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: a failed unwrap is the failed test
//! The `identity` and `tenancy` directories (MOH-193): what other modules ask instead of reading
//! `users`, `tenant_memberships` and `tenants`. They run as the application role, in a tenant Unit
//! of Work, so Row-Level Security is on: a question about another tenant's member must come back
//! empty even when the caller names that tenant.

use std::time::Duration;

use sqlx::PgPool;
use tenancy::directory::{self, ActiveMembers};
use uuid::Uuid;

/// `users.password_hash` must look like an argon2 hash; no one signs in with this one.
const ARGON2_SHAPED: &str = "$argon2id$v=19$m=19456,t=2,p=1$ZGlyZWN0b3JpZXM$ZGlyZWN0b3JpZXM";

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn runtime_pool() -> Option<PgPool> {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        assert!(
            !require_db_tests(),
            "REQUIRE_DB_TESTS=1 but DATABASE_URL is not set"
        );
        eprintln!("skipping directories tests: DATABASE_URL not set");
        return None;
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        assert!(
            !require_db_tests(),
            "REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable"
        );
        eprintln!("skipping directories tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

struct Member {
    user_id: Uuid,
    membership_id: Uuid,
    name: String,
}

struct World {
    tenant_a: Uuid,
    tenant_b: Uuid,
    owner_a: Member,
    agent_a: Member,
    disabled_a: Member,
    gone_a: Member,
    owner_b: Member,
}

async fn add_member(seed: &PgPool, tenant: Uuid, label: &str, role: &str, status: &str) -> Member {
    let tag = Uuid::new_v4().simple().to_string();
    let name = format!("{label} {}", &tag[..6]);
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name, password_hash) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(format!("{label}-{tag}@directories.test"))
    .bind(&name)
    .bind(ARGON2_SHAPED)
    .fetch_one(seed)
    .await
    .unwrap();
    let membership_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(tenant)
    .bind(user_id)
    .bind(role)
    .bind(status)
    .fetch_one(seed)
    .await
    .unwrap();
    Member {
        user_id,
        membership_id,
        name,
    }
}

async fn world() -> Option<(PgPool, World)> {
    let pool = runtime_pool().await?;
    db::testing::migrate().await;
    let seed = db::testing::seed_pool();
    let tag = Uuid::new_v4().simple().to_string();
    let tenant_a = db::testing::insert_tenant(&seed, "Directory A", &format!("dir-a-{tag}")).await;
    let tenant_b = db::testing::insert_tenant(&seed, "Directory B", &format!("dir-b-{tag}")).await;
    let owner_a = add_member(&seed, tenant_a, "owner-a", "owner", "active").await;
    let agent_a = add_member(&seed, tenant_a, "agent-a", "agent", "active").await;
    let disabled_a = add_member(&seed, tenant_a, "disabled-a", "agent", "disabled").await;
    let gone_a = add_member(&seed, tenant_a, "gone-a", "agent", "active").await;
    sqlx::query("UPDATE tenant_memberships SET deleted_at = now() WHERE id = $1")
        .bind(gone_a.membership_id)
        .execute(&seed)
        .await
        .unwrap();
    let owner_b = add_member(&seed, tenant_b, "owner-b", "owner", "active").await;
    Some((
        pool,
        World {
            tenant_a,
            tenant_b,
            owner_a,
            agent_a,
            disabled_a,
            gone_a,
            owner_b,
        },
    ))
}

#[tokio::test]
async fn a_member_is_found_in_their_own_tenant_and_not_in_another() {
    let Some((pool, w)) = world().await else {
        return;
    };
    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, w.tenant_a)
        .await
        .unwrap();
    let conn = uow.conn();

    assert_eq!(
        directory::membership_id_of_user(&mut *conn, w.tenant_a, w.owner_a.user_id)
            .await
            .unwrap(),
        Some(w.owner_a.membership_id)
    );
    // Tenant B's member, asked about by tenant A, and by tenant B's own ID from tenant A's Unit of
    // Work: neither answers. The Row-Level Security policy holds even when the caller names the
    // other tenant.
    for asked_tenant in [w.tenant_a, w.tenant_b] {
        assert_eq!(
            directory::membership_id_of_user(&mut *conn, asked_tenant, w.owner_b.user_id)
                .await
                .unwrap(),
            None,
            "tenant B's member leaked to tenant A (asked as {asked_tenant})"
        );
        assert_eq!(
            directory::membership_role(&mut *conn, asked_tenant, w.owner_b.membership_id)
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            directory::membership_user_id(&mut *conn, asked_tenant, w.owner_b.membership_id)
                .await
                .unwrap(),
            None
        );
        assert!(
            !directory::is_active_member(&mut *conn, asked_tenant, w.owner_b.membership_id)
                .await
                .unwrap()
        );
    }
    // And a tenant-A member asked about as tenant B is not found either.
    assert_eq!(
        directory::membership_id_of_user(&mut *conn, w.tenant_b, w.owner_a.user_id)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn status_and_deletion_decide_who_is_active() {
    let Some((pool, w)) = world().await else {
        return;
    };
    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, w.tenant_a)
        .await
        .unwrap();
    let conn = uow.conn();

    // A disabled member still has a membership, but is not an active one.
    assert_eq!(
        directory::membership_id_of_user(&mut *conn, w.tenant_a, w.disabled_a.user_id)
            .await
            .unwrap(),
        Some(w.disabled_a.membership_id)
    );
    assert_eq!(
        directory::active_membership_id_of_user(&mut *conn, w.tenant_a, w.disabled_a.user_id)
            .await
            .unwrap(),
        None
    );
    assert!(
        !directory::is_active_member(&mut *conn, w.tenant_a, w.disabled_a.membership_id)
            .await
            .unwrap()
    );
    // A deleted membership is nobody's.
    assert_eq!(
        directory::membership_id_of_user(&mut *conn, w.tenant_a, w.gone_a.user_id)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        directory::membership_role(&mut *conn, w.tenant_a, w.gone_a.membership_id)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn picking_active_members_stays_inside_the_tenant() {
    let Some((pool, w)) = world().await else {
        return;
    };
    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, w.tenant_a)
        .await
        .unwrap();
    let conn = uow.conn();

    let mut by_role = directory::active_member_ids(
        &mut *conn,
        w.tenant_a,
        ActiveMembers {
            roles: Some(&["owner", "agent"]),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    by_role.sort();
    let mut expected = vec![w.owner_a.membership_id, w.agent_a.membership_id];
    expected.sort();
    // Not the disabled agent, not the deleted one, and not tenant B's owner.
    assert_eq!(by_role, expected);

    // Naming other tenants' members (`or_id`, `among`) never adds them.
    let named = directory::active_member_ids(
        &mut *conn,
        w.tenant_a,
        ActiveMembers {
            or_id: Some(w.owner_b.membership_id),
            among: Some(&[w.owner_b.membership_id, w.agent_a.membership_id]),
            roles: Some(&["agent"]),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(named, vec![w.agent_a.membership_id]);
    // The same, asking as tenant B from tenant A's Unit of Work, finds nothing.
    let as_b = directory::active_member_ids(
        &mut *conn,
        w.tenant_b,
        ActiveMembers {
            roles: Some(&["owner"]),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(as_b.is_empty(), "tenant B's owner was visible to tenant A");

    // `exclude` leaves the member who caused the event out.
    let without_owner = directory::active_member_ids(
        &mut *conn,
        w.tenant_a,
        ActiveMembers {
            roles: Some(&["owner", "agent"]),
            exclude: Some(w.owner_a.membership_id),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(without_owner, vec![w.agent_a.membership_id]);
}

#[tokio::test]
async fn profiles_carry_names_for_this_tenants_members_only() {
    let Some((pool, w)) = world().await else {
        return;
    };
    let mut uow = db::UnitOfWork::begin_for_tenant(&pool, w.tenant_a)
        .await
        .unwrap();
    let conn = uow.conn();

    let profiles = directory::member_profiles(
        &mut *conn,
        w.tenant_a,
        &[
            w.owner_a.membership_id,
            w.disabled_a.membership_id,
            w.gone_a.membership_id,
            w.owner_b.membership_id,
            Uuid::new_v4(),
        ],
    )
    .await
    .unwrap();

    assert_eq!(profiles.len(), 2, "{profiles:?}");
    let owner = &profiles[&w.owner_a.membership_id];
    assert_eq!(owner.display_name, w.owner_a.name);
    assert_eq!(owner.user_id, w.owner_a.user_id);
    assert_eq!(owner.role, "owner");
    assert!(owner.active);
    let disabled = &profiles[&w.disabled_a.membership_id];
    assert!(!disabled.active, "a disabled member shows as inactive");
    assert_eq!(disabled.display_name, w.disabled_a.name);

    // Asked as tenant B from tenant A's Unit of Work: nobody.
    assert!(
        directory::member_profiles(&mut *conn, w.tenant_b, &[w.owner_b.membership_id])
            .await
            .unwrap()
            .is_empty()
    );
    assert!(directory::member_profiles(&mut *conn, w.tenant_a, &[])
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn the_identity_directory_answers_only_for_the_ids_it_is_given() {
    let Some((pool, w)) = world().await else {
        return;
    };
    let ids = [w.owner_a.user_id, w.agent_a.user_id];

    let names = identity::directory::display_names(&pool, &ids)
        .await
        .unwrap();
    assert_eq!(names.len(), 2);
    assert_eq!(names[&w.owner_a.user_id], w.owner_a.name);
    assert!(identity::directory::display_names(&pool, &[])
        .await
        .unwrap()
        .is_empty());

    // A search is limited to the candidates the caller passes: tenant A's roster never finds
    // tenant B's owner, even with a pattern that matches them.
    let found = identity::directory::matching_name_or_email(&pool, &ids, "%owner-%")
        .await
        .unwrap();
    assert_eq!(found, vec![w.owner_a.user_id]);

    // Registered emails, and a duplicate account refused on the unique email.
    let profile = identity::directory::profiles(&pool, &[w.owner_a.user_id])
        .await
        .unwrap()
        .remove(&w.owner_a.user_id)
        .unwrap();
    assert!(
        identity::directory::email_is_registered(&pool, &profile.email)
            .await
            .unwrap()
    );
    assert_eq!(
        identity::directory::user_id_by_email(&pool, &profile.email)
            .await
            .unwrap(),
        Some(w.owner_a.user_id)
    );
    assert!(
        !identity::directory::email_is_registered(&pool, "nobody@directories.test")
            .await
            .unwrap()
    );
    let duplicate =
        identity::directory::create_user(&pool, &profile.email, "Someone Else", ARGON2_SHAPED)
            .await;
    match duplicate {
        Err(sqlx::Error::Database(e)) => assert_eq!(e.code().as_deref(), Some("23505")),
        other => panic!("a duplicate email must be a unique violation, got {other:?}"),
    }
    assert!(identity::directory::user_exists(&pool, w.owner_a.user_id)
        .await
        .unwrap());
    assert!(!identity::directory::user_exists(&pool, Uuid::new_v4())
        .await
        .unwrap());
    assert!(
        !identity::directory::is_platform_staff(&pool, w.owner_a.user_id)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn tenant_questions_see_only_live_tenants() {
    let Some((pool, w)) = world().await else {
        return;
    };
    assert!(directory::tenant_exists(&pool, w.tenant_a).await.unwrap());
    assert!(!directory::tenant_exists(&pool, Uuid::new_v4())
        .await
        .unwrap());
    let live = directory::live_tenant_ids(&pool).await.unwrap();
    assert!(live.contains(&w.tenant_a) && live.contains(&w.tenant_b));
    let now = chrono::Utc::now();
    let (total, active, suspended, created) = directory::tenant_counts(
        &pool,
        now - chrono::Duration::hours(1),
        now + chrono::Duration::hours(1),
    )
    .await
    .unwrap();
    assert!(total >= 2 && active >= 2 && created >= 2 && suspended >= 0);
}
