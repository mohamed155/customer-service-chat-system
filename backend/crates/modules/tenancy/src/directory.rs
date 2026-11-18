//! What the other modules may ask `tenancy` about tenants and their members (MOH-193, ADR-017).
//!
//! `tenants` and `tenant_memberships` are owned by `tenancy`. A module that needs the acting
//! member's membership ID, a member's role, the members to notify or route to, or a member's name,
//! asks here instead of naming those tables in its own SQL.
//!
//! Every function that reads members takes the `tenant_id` and filters on it, whatever the caller's
//! Unit of Work already set: the tenant is part of the question, never inferred from the session
//! variable alone. Callers pass the tenant from their validated `TenantContext`. The functions run
//! on the executor or connection the caller holds, so they stay inside its transaction and under
//! its Row-Level Security settings. A deleted membership is never returned.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// One member
// ---------------------------------------------------------------------------

/// The membership ID of `user_id` in the tenant, whatever its status (active or disabled).
pub async fn membership_id_of_user<'e>(
    executor: impl PgExecutor<'e>,
    tenant_id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "SELECT id FROM tenant_memberships \
         WHERE tenant_id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_optional(executor)
    .await
}

/// The membership ID of `user_id` in the tenant when the membership is active.
pub async fn active_membership_id_of_user<'e>(
    executor: impl PgExecutor<'e>,
    tenant_id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "SELECT id FROM tenant_memberships \
         WHERE tenant_id = $1 AND user_id = $2 AND status = 'active' AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_optional(executor)
    .await
}

/// The stored role of a membership (`owner`, `admin`, ...), whatever its status.
pub async fn membership_role<'e>(
    executor: impl PgExecutor<'e>,
    tenant_id: Uuid,
    membership_id: Uuid,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(
        "SELECT role FROM tenant_memberships \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(membership_id)
    .fetch_optional(executor)
    .await
}

/// The user a membership belongs to.
pub async fn membership_user_id<'e>(
    executor: impl PgExecutor<'e>,
    tenant_id: Uuid,
    membership_id: Uuid,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "SELECT user_id FROM tenant_memberships \
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(membership_id)
    .bind(tenant_id)
    .fetch_optional(executor)
    .await
}

/// Whether the membership exists in this tenant and is active.
pub async fn is_active_member<'e>(
    executor: impl PgExecutor<'e>,
    tenant_id: Uuid,
    membership_id: Uuid,
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS( \
         SELECT 1 FROM tenant_memberships \
         WHERE id = $1 AND tenant_id = $2 AND status = 'active' AND deleted_at IS NULL \
         )",
    )
    .bind(membership_id)
    .bind(tenant_id)
    .fetch_one(executor)
    .await
}

/// The tenants a user has a live membership in.
pub async fn tenant_ids_of_user<'e>(
    executor: impl PgExecutor<'e>,
    user_id: Uuid,
) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar(
        "SELECT tenant_id FROM tenant_memberships WHERE user_id = $1 AND deleted_at IS NULL",
    )
    .bind(user_id)
    .fetch_all(executor)
    .await
}

// ---------------------------------------------------------------------------
// Several members
// ---------------------------------------------------------------------------

/// Which active members of a tenant to pick. A member is picked when it is active and matches
/// `roles` or `or_id` (a filter left as `None` does not match anything on its own), is one of
/// `among` when that is set, and is not `exclude`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ActiveMembers<'a> {
    /// Pick members with one of these stored roles.
    pub roles: Option<&'a [&'a str]>,
    /// Also pick this one membership (an assignee, a mentioned member).
    pub or_id: Option<Uuid>,
    /// Pick only among these memberships.
    pub among: Option<&'a [Uuid]>,
    /// Never pick this membership (the member who caused the event).
    pub exclude: Option<Uuid>,
}

/// The IDs of the tenant's active members that match `pick`, in ID order.
pub async fn active_member_ids<'e>(
    executor: impl PgExecutor<'e>,
    tenant_id: Uuid,
    pick: ActiveMembers<'_>,
) -> sqlx::Result<Vec<Uuid>> {
    let roles: Vec<String> = pick
        .roles
        .map(|roles| roles.iter().map(|r| (*r).to_owned()).collect())
        .unwrap_or_default();
    sqlx::query_scalar(
        "SELECT id FROM tenant_memberships \
         WHERE tenant_id = $1 AND status = 'active' AND deleted_at IS NULL \
           AND (role = ANY($2) OR id = $3) \
           AND ($4::uuid[] IS NULL OR id = ANY($4)) \
           AND ($5::uuid IS NULL OR id IS DISTINCT FROM $5) \
         ORDER BY id",
    )
    .bind(tenant_id)
    .bind(roles)
    .bind(pick.or_id)
    .bind(pick.among)
    .bind(pick.exclude)
    .fetch_all(executor)
    .await
}

/// A member as a screen shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberProfile {
    pub membership_id: Uuid,
    pub user_id: Uuid,
    pub display_name: String,
    pub role: String,
    /// `status = 'active'`: a disabled member keeps showing up on what they did.
    pub active: bool,
}

/// The profile of each of `membership_ids` that is a live membership of this tenant. IDs of another
/// tenant's memberships, deleted memberships and unknown IDs are left out. Names come from the
/// `identity` directory.
pub async fn member_profiles(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    membership_ids: &[Uuid],
) -> sqlx::Result<HashMap<Uuid, MemberProfile>> {
    if membership_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let members: Vec<(Uuid, Uuid, String, String)> = sqlx::query_as(
        "SELECT id, user_id, role, status FROM tenant_memberships \
         WHERE tenant_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(membership_ids)
    .fetch_all(&mut *conn)
    .await?;
    let user_ids: Vec<Uuid> = members.iter().map(|(_, user_id, _, _)| *user_id).collect();
    let names = identity::directory::display_names(&mut *conn, &user_ids).await?;
    Ok(members
        .into_iter()
        .filter_map(|(membership_id, user_id, role, status)| {
            let display_name = names.get(&user_id)?.clone();
            Some((
                membership_id,
                MemberProfile {
                    membership_id,
                    user_id,
                    display_name,
                    role,
                    active: status == "active",
                },
            ))
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Tenants
// ---------------------------------------------------------------------------

/// Whether a live (not deleted) tenant has this ID.
pub async fn tenant_exists<'e>(
    executor: impl PgExecutor<'e>,
    tenant_id: Uuid,
) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tenants WHERE id = $1 AND deleted_at IS NULL)")
        .bind(tenant_id)
        .fetch_one(executor)
        .await
}

/// The IDs of every live tenant, in ID order. `tenants` has no `tenant_id` and is not under
/// Row-Level Security, so this is for platform-wide jobs (key rotation sweeps) only.
pub async fn live_tenant_ids<'e>(executor: impl PgExecutor<'e>) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar("SELECT id FROM tenants WHERE deleted_at IS NULL ORDER BY id")
        .fetch_all(executor)
        .await
}

/// How many live tenants there are: all, active, suspended, and created in `[from, to)`.
pub async fn tenant_counts<'e>(
    executor: impl PgExecutor<'e>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> sqlx::Result<(i64, i64, i64, i64)> {
    sqlx::query_as(
        "SELECT COUNT(*)::bigint, \
                COUNT(*) FILTER (WHERE status = 'active')::bigint, \
                COUNT(*) FILTER (WHERE status = 'suspended')::bigint, \
                COUNT(*) FILTER (WHERE created_at >= $1 AND created_at < $2)::bigint \
         FROM tenants WHERE deleted_at IS NULL",
    )
    .bind(from)
    .bind(to)
    .fetch_one(executor)
    .await
}
