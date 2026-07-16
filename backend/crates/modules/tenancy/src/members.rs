//! Team member roster, role-change, and rank-model logic.
//!
//! Rank model (enforced in-handler, after the route permission gate):
//!
//! | Role   | Rank |
//! |--------|------|
//! | owner  | 5    |
//! | admin  | 4    |
//! | manager| 3    |
//! | agent  | 2    |
//! | viewer | 1    |
//!
//! Rules:
//! 1. Actor may act on a target only if `actor_rank > target_rank`;
//!    Owner-on-Owner is allowed (rank-equal exception).
//! 2. Assignable roles: `new_role_rank <= actor_rank`;
//!    `owner` additionally requires `owner.assign`.
//! 3. Self-guard: target must not be the actor.
//! 4. Last-owner guard: the last active Owner cannot be demoted or disabled.

use authz::TenantRole;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    Extension,
};
use identity::Principal;
use kernel::{ApiError, ErrorDetail, ErrorEnvelope, Page};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use std::str::FromStr;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{audit, TenantContext};

/// Numeric rank for hierarchy comparison.
pub const TENANT_ROLE_RANK: [(TenantRole, u8); 5] = [
    (TenantRole::Owner, 5),
    (TenantRole::Admin, 4),
    (TenantRole::Manager, 3),
    (TenantRole::Agent, 2),
    (TenantRole::Viewer, 1),
];

fn role_rank(role: &TenantRole) -> u8 {
    match role {
        TenantRole::Owner => 5,
        TenantRole::Admin => 4,
        TenantRole::Manager => 3,
        TenantRole::Agent => 2,
        TenantRole::Viewer => 1,
    }
}

/// Returns true if the actor is allowed to manage the target member.
///
/// Actor must be strictly above target in rank, except that an Owner actor
/// may also act on Owner targets (the Owner-on-Owner exception from FR-008).
pub fn can_manage(actor_role: &TenantRole, target_role: &TenantRole) -> bool {
    let actor = role_rank(actor_role);
    let target = role_rank(target_role);
    actor > target || (actor == 5 && target == 5)
}

/// Returns true if the actor may assign the given new role.
///
/// The new role must be at or below the actor's rank. Additionally,
/// assigning `owner` requires the `owner.assign` permission (checked
/// by the caller via the permission set).
pub fn can_assign(actor_role: &TenantRole, new_role: &TenantRole) -> bool {
    role_rank(actor_role) >= role_rank(new_role)
}

/// Parse a role string to a `TenantRole`.
pub fn parse_role(role: &str) -> Result<TenantRole, String> {
    TenantRole::from_str(role).map_err(|_| format!("unknown role: {role}"))
}

/// Derive the effective rank for a platform staff principal acting inside
/// a tenant. Returns `Some(5)` when the principal holds `owner.assign`,
/// `Some(4)` when holding `members.manage` (but not `owner.assign`), and
/// `None` when holding neither.
pub fn staff_effective_rank(permissions: &authz::PermissionSet) -> Option<u8> {
    if permissions.contains(authz::Permission::OwnerAssign) {
        Some(5)
    } else if permissions.contains(authz::Permission::MembersManage) {
        Some(4)
    } else {
        None
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(default, deny_unknown_fields)]
pub struct TeamMemberQuery {
    pub q: Option<String>,
    pub status: Option<String>,
    pub limit: u32,
    pub cursor: Option<String>,
}

impl Default for TeamMemberQuery {
    fn default() -> Self {
        Self {
            q: None,
            status: None,
            limit: 25,
            cursor: None,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TeamMemberResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub display_name: String,
    pub email: String,
    pub role: String,
    pub status: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct MemberRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub display_name: String,
    pub email: String,
    pub role: String,
    pub status: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
}

/// Execute the list-members query inside a transaction, returning rows
/// (over-fetched by one) and a `has_more` flag.
///
/// The caller is responsible for validation — cursor format, limit range,
/// etc. are assumed to have been checked before calling this function.
pub async fn list_members_rows_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    q: Option<&str>,
    status: Option<&str>,
    cursor: Option<&str>,
    limit: u32,
) -> sqlx::Result<(Vec<MemberRow>, bool)> {
    let mut where_clauses: Vec<String> =
        vec!["tm.tenant_id = $1".into(), "tm.deleted_at IS NULL".into()];
    let mut next_bind: usize = 2;

    if let Some(q) = q {
        if !q.is_empty() {
            where_clauses.push(format!(
                "(u.display_name ILIKE ${next_bind} OR u.email ILIKE ${next_bind})"
            ));
            next_bind += 1;
        }
    }

    if status.is_some() {
        where_clauses.push(format!("tm.status = ${next_bind}"));
        next_bind += 1;
    }

    if cursor.is_some() {
        where_clauses.push(format!(
            "(tm.created_at, tm.id) < (${next_bind}::timestamptz, ${bind2}::uuid)",
            next_bind = next_bind,
            bind2 = next_bind + 1
        ));
        next_bind += 2;
    }

    let where_sql = where_clauses.join(" AND ");
    let order_limit = format!(
        "ORDER BY tm.created_at DESC, tm.id DESC LIMIT ${next_bind}",
        next_bind = next_bind
    );

    let sql = format!(
        "SELECT tm.id, tm.user_id, u.display_name, u.email, tm.role, tm.status, \
                tm.created_at AS joined_at \
         FROM tenant_memberships tm \
         JOIN users u ON u.id = tm.user_id \
         WHERE {where_sql} {order_limit}"
    );

    let mut query = sqlx::query_as::<_, MemberRow>(&sql);
    query = query.bind(tenant_id);

    if let Some(q) = q {
        if !q.is_empty() {
            query = query.bind(format!("%{q}%"));
        }
    }

    if let Some(status) = status {
        query = query.bind(status);
    }

    if let Some(cursor) = cursor {
        if let Some((joined_str, id_str)) = cursor.split_once('_') {
            if let Ok(joined) = chrono::DateTime::parse_from_rfc3339(joined_str) {
                if let Ok(id) = Uuid::parse_str(id_str) {
                    query = query.bind(joined);
                    query = query.bind(id);
                }
            }
        }
    }

    query = query.bind(i64::from(limit + 1));

    let rows = query.fetch_all(&mut **tx).await?;

    let has_more = rows.len() > limit as usize;
    Ok((rows.into_iter().take(limit as usize).collect(), has_more))
}

#[utoipa::path(
    get,
    path = "/tenant/members",
    tag = "members",
    operation_id = "list_team_members",
    summary = "List tenant team members",
    description = "List the roster of active and disabled team members for the current tenant \
                  with cursor-based pagination and optional name/email `q` and `status` filters. \
                  Requires permission: members.view",
    params(TeamMemberQuery),
    responses(
        (status = 200, description = "Page of team members.", body = Page<TeamMemberResponse>),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (invalid `status` filter or `limit` range).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn list_members(
    State(pool): State<sqlx::PgPool>,
    ctx: TenantContext,
    Query(params): Query<TeamMemberQuery>,
) -> Response {
    let mut details: Vec<ErrorDetail> = Vec::new();

    if let Some(ref q) = params.q {
        if q.len() > 254 {
            details.push(ErrorDetail {
                field: "q".into(),
                code: "too_long".into(),
                message: "Search query must be 254 characters or fewer".into(),
            });
        }
    }

    if let Some(ref status) = params.status {
        if status != "active" && status != "disabled" {
            details.push(ErrorDetail {
                field: "status".into(),
                code: "invalid_value".into(),
                message: "Status must be active or disabled".into(),
            });
        }
    }

    if params.limit == 0 || params.limit > 100 {
        details.push(ErrorDetail {
            field: "limit".into(),
            code: "invalid_range".into(),
            message: "Limit must be between 1 and 100".into(),
        });
    }

    if let Some(ref cursor) = params.cursor {
        let valid_cursor = cursor
            .split_once('_')
            .and_then(|(joined_str, id_str)| {
                chrono::DateTime::parse_from_rfc3339(joined_str)
                    .ok()
                    .and_then(|_| Uuid::parse_str(id_str).ok())
            })
            .is_some();

        if !valid_cursor {
            details.push(ErrorDetail {
                field: "cursor".into(),
                code: "invalid_value".into(),
                message: "Cursor must be a valid roster cursor".into(),
            });
        }
    }

    if !details.is_empty() {
        return ApiError::unprocessable_entity("Validation failed")
            .with_details(details)
            .into_response();
    }

    let limit = params.limit;

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(error = %e, "failed to begin transaction");
            return ApiError::internal_error("Failed to fetch team members").into_response();
        }
    };

    let q = params.q.as_deref();
    let status = params.status.as_deref();
    let cursor = params.cursor.as_deref();

    let (rows, has_more) =
        match list_members_rows_in_tx(&mut tx, ctx.tenant_id, q, status, cursor, limit).await {
            Ok(result) => result,
            Err(e) => {
                tracing::error!(error = %e, "failed to fetch team members");
                return ApiError::internal_error("Failed to fetch team members").into_response();
            }
        };

    if let Err(e) = tx.commit().await {
        tracing::error!(error = %e, "failed to commit transaction");
        return ApiError::internal_error("Failed to commit transaction").into_response();
    }

    let items: Vec<TeamMemberResponse> = rows
        .into_iter()
        .map(|r| TeamMemberResponse {
            id: r.id,
            user_id: r.user_id,
            display_name: r.display_name,
            email: r.email,
            role: r.role,
            status: r.status,
            joined_at: r.joined_at,
        })
        .collect();

    let next_cursor = items
        .last()
        .map(|last| format!("{}_{}", last.joined_at.format("%+"), last.id));

    Json(Page {
        items,
        next_cursor,
        has_more,
    })
    .into_response()
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateMemberPayload {
    pub role: Option<String>,
    pub status: Option<String>,
}

#[utoipa::path(
    patch,
    path = "/tenant/members/{id}",
    tag = "members",
    operation_id = "update_team_member",
    summary = "Update a tenant team member",
    description = "Update the role or status of a single team membership. Exactly one of `role` or \
                  `status` must be supplied. Role rank rules (actor must outrank target) and the \
                  last-active-Owner guard apply. Requires permission: members.manage",
    params(("id" = Uuid, Path, description = "Membership identifier")),
    request_body = UpdateMemberPayload,
    responses(
        (status = 200, description = "Member updated.", body = TeamMemberResponse),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required or insufficient rank.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions or rank to manage the target.", body = ErrorEnvelope),
        (status = 404, description = "Membership not found.", body = ErrorEnvelope),
        (status = 409, description = "Conflict (role/status unchanged, last active Owner guard, or self-action).", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (per-field, e.g. unknown role/status, or both/neither of role and status supplied).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn update_member(
    State(pool): State<sqlx::PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    kernel::ApiJson(payload): kernel::ApiJson<UpdateMemberPayload>,
) -> Response {
    let has_role = payload.role.is_some();
    let has_status = payload.status.is_some();
    if has_role == has_status {
        let details = vec![ErrorDetail {
            field: "body".into(),
            code: "validation_failed".into(),
            message: "Exactly one of 'role' or 'status' must be provided".into(),
        }];
        return ApiError::unprocessable_entity("Validation failed")
            .with_details(details)
            .into_response();
    }

    let actor_role = match &ctx.tenant_role {
        Some(role) => *role,
        None => {
            let rank = staff_effective_rank(&ctx.permissions);
            match rank {
                Some(5) => TenantRole::Owner,
                Some(4) => TenantRole::Admin,
                _ => {
                    return ApiError::unauthorized("Insufficient permissions to manage members")
                        .into_response();
                }
            }
        }
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(error = %e, "failed to begin transaction");
            return ApiError::internal_error("Failed to begin transaction").into_response();
        }
    };

    let target = sqlx::query_as::<_, (Uuid, String, String, Uuid)>(
        "SELECT tm.id, tm.role, tm.status, tm.user_id \
         FROM tenant_memberships tm \
         WHERE tm.id = $1 AND tm.tenant_id = $2 AND tm.deleted_at IS NULL \
         FOR UPDATE",
    )
    .bind(id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *tx)
    .await;

    let (membership_id, current_role, current_status, target_user_id) = match target {
        Ok(Some(row)) => row,
        Ok(None) => {
            return ApiError::not_found("Membership not found").into_response();
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to fetch membership");
            return ApiError::internal_error("Failed to fetch membership").into_response();
        }
    };

    if target_user_id == principal.user_id {
        return ApiError::unauthorized("Cannot perform this action on yourself").into_response();
    }

    let target_role = match TenantRole::from_str(&current_role) {
        Ok(r) => r,
        Err(_) => {
            return ApiError::internal_error("Invalid target role").into_response();
        }
    };

    if !can_manage(&actor_role, &target_role) {
        return ApiError::unauthorized("Insufficient rank to manage this member").into_response();
    }

    if let Some(new_role) = &payload.role {
        let new_role_parsed = match TenantRole::from_str(new_role) {
            Ok(r) => r,
            Err(_) => {
                return ApiError::validation_failed(format!("Unknown role: {new_role}"))
                    .into_response();
            }
        };

        if !can_assign(&actor_role, &new_role_parsed) {
            return ApiError::unauthorized("Cannot assign this role").into_response();
        }

        if new_role == &current_role {
            return ApiError::conflict(format!("Member role is already {new_role}"))
                .into_response();
        }

        if new_role_parsed == TenantRole::Owner
            && !ctx.permissions.contains(authz::Permission::OwnerAssign)
        {
            return ApiError::unauthorized(
                "Assigning the owner role requires the owner.assign permission",
            )
            .into_response();
        }

        if current_role == "owner" && new_role != "owner" {
            let owner_rows = match sqlx::query(
                "SELECT id FROM tenant_memberships \
                  WHERE tenant_id = $1 AND role = 'owner' AND status = 'active' AND deleted_at IS NULL \
                  ORDER BY id \
                  FOR UPDATE",
            )
            .bind(ctx.tenant_id)
            .fetch_all(&mut *tx)
            .await
            {
                Ok(rows) => rows,
                Err(e) => {
                    tracing::error!(error = %e, "failed to lock owner memberships");
                    return ApiError::internal_error("Failed to lock owner memberships").into_response();
                }
            };

            if owner_rows.len() <= 1 {
                return ApiError::conflict("Cannot demote the last active Owner").into_response();
            }
        }

        if let Err(e) = sqlx::query(
            "UPDATE tenant_memberships SET role = $1 WHERE id = $2 AND tenant_id = $3 AND deleted_at IS NULL",
        )
            .bind(new_role)
            .bind(membership_id)
            .bind(ctx.tenant_id)
            .execute(&mut *tx)
            .await
        {
            tracing::error!(error = %e, "failed to update member role");
            return ApiError::internal_error("Failed to update role").into_response();
        }

        if let Err(e) = audit::record_member_role_changed(
            &mut tx,
            principal.user_id,
            ctx.tenant_id,
            membership_id,
            &current_role,
            new_role,
        )
        .await
        {
            tracing::error!(error = %e, "failed to record audit");
            return ApiError::internal_error("Failed to record audit").into_response();
        }
    } else if let Some(new_status) = &payload.status {
        if new_status != "active" && new_status != "disabled" {
            return ApiError::validation_failed(format!("Invalid status: {new_status}"))
                .into_response();
        }

        if new_status == &current_status {
            return ApiError::conflict(format!("Member status is already {new_status}"))
                .into_response();
        }

        if new_status == "disabled" && current_role == "owner" {
            let owner_rows = match sqlx::query(
                "SELECT id FROM tenant_memberships \
                  WHERE tenant_id = $1 AND role = 'owner' AND status = 'active' AND deleted_at IS NULL \
                  ORDER BY id \
                  FOR UPDATE",
            )
            .bind(ctx.tenant_id)
            .fetch_all(&mut *tx)
            .await
            {
                Ok(rows) => rows,
                Err(e) => {
                    tracing::error!(error = %e, "failed to lock owner memberships");
                    return ApiError::internal_error("Failed to lock owner memberships").into_response();
                }
            };

            if owner_rows.len() <= 1 {
                return ApiError::conflict("Cannot disable the last active Owner").into_response();
            }
        }

        if let Err(e) = sqlx::query(
            "UPDATE tenant_memberships SET status = $1 WHERE id = $2 AND tenant_id = $3 AND deleted_at IS NULL",
        )
            .bind(new_status)
            .bind(membership_id)
            .bind(ctx.tenant_id)
            .execute(&mut *tx)
            .await
        {
            tracing::error!(error = %e, "failed to update member status");
            return ApiError::internal_error("Failed to update status").into_response();
        }

        let audit_result = if new_status == "disabled" {
            audit::record_member_disabled(
                &mut tx,
                principal.user_id,
                ctx.tenant_id,
                membership_id,
                &current_role,
                &current_status,
                new_status,
            )
            .await
        } else {
            audit::record_member_enabled(
                &mut tx,
                principal.user_id,
                ctx.tenant_id,
                membership_id,
                &current_role,
                &current_status,
                new_status,
            )
            .await
        };

        if let Err(e) = audit_result {
            tracing::error!(error = %e, "failed to record audit");
            return ApiError::internal_error("Failed to record audit").into_response();
        }
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(error = %e, "failed to commit transaction");
        return ApiError::internal_error("Failed to commit transaction").into_response();
    }

    let updated = sqlx::query_as::<
        _,
        (
            Uuid,
            Uuid,
            String,
            String,
            String,
            String,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        "SELECT tm.id, tm.user_id, u.display_name, u.email, tm.role, tm.status, tm.created_at \
          FROM tenant_memberships tm \
          JOIN users u ON u.id = tm.user_id \
          WHERE tm.id = $1 AND tm.tenant_id = $2 AND tm.deleted_at IS NULL",
    )
    .bind(membership_id)
    .bind(ctx.tenant_id)
    .fetch_one(&pool)
    .await;

    match updated {
        Ok((id, user_id, display_name, email, role, status, joined_at)) => {
            let response = TeamMemberResponse {
                id,
                user_id,
                display_name,
                email,
                role,
                status,
                joined_at,
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to fetch updated member");
            ApiError::internal_error("Failed to fetch updated member").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use authz::TenantRole::*;

    #[test]
    fn owner_can_manage_admin() {
        assert!(can_manage(&Owner, &Admin));
    }

    #[test]
    fn owner_can_manage_another_owner() {
        assert!(can_manage(&Owner, &Owner));
    }

    #[test]
    fn admin_cannot_manage_owner() {
        assert!(!can_manage(&Admin, &Owner));
    }

    #[test]
    fn manager_cannot_manage_admin() {
        assert!(!can_manage(&Manager, &Admin));
    }

    #[test]
    fn viewer_cannot_manage_anyone() {
        assert!(!can_manage(&Viewer, &Viewer));
        assert!(!can_manage(&Viewer, &Agent));
    }

    #[test]
    fn admin_can_assign_below() {
        assert!(can_assign(&Admin, &Manager));
        assert!(can_assign(&Admin, &Viewer));
    }

    #[test]
    fn admin_cannot_assign_owner() {
        assert!(!can_assign(&Admin, &Owner));
    }

    #[test]
    fn owner_can_assign_any_role() {
        assert!(can_assign(&Owner, &Owner));
        assert!(can_assign(&Owner, &Viewer));
    }

    #[test]
    fn staff_rank_owner_assign() {
        let perms = authz::PermissionSet::new([authz::Permission::OwnerAssign]);
        assert_eq!(staff_effective_rank(&perms), Some(5));
    }

    #[test]
    fn staff_rank_members_manage_only() {
        let perms = authz::PermissionSet::new([authz::Permission::MembersManage]);
        assert_eq!(staff_effective_rank(&perms), Some(4));
    }

    #[test]
    fn staff_rank_neither() {
        let perms = authz::PermissionSet::new([]);
        assert_eq!(staff_effective_rank(&perms), None);
    }
}
