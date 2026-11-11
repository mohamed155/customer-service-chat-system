//! Resetting another user's second factor (MOH-34, ADR-005 phase M1).
//!
//! The recovery path for a user who has lost both their authenticator and their recovery codes.
//! A reset removes the factor and the recovery codes and ends every session the user has, so they
//! sign in again with their password and set a new factor up. It is audited (`auth.mfa_reset`,
//! naming who did it) and needs a fresh second factor from the person doing it.
//!
//! Users are global, not per tenant, so a reset is a write to something more than one tenant can
//! depend on. A tenant owner may reset a member only when that member belongs to no other
//! workspace and is not platform staff. Anyone else is for a platform administrator.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension,
};
use identity::{mfa, Principal};
use kernel::ApiError;
use sqlx::PgPool;
use tenancy::TenantContext;
use uuid::Uuid;

/// Reset a member's two-factor authentication (tenant owner).
#[utoipa::path(
    post,
    path = "/tenant/members/{id}/mfa/reset",
    tag = "tenant-members",
    operation_id = "reset_member_mfa",
    summary = "Reset a member's two-factor authentication",
    description = "Removes the member's second factor and recovery codes and ends all of their \
                  sessions; they sign in with their password and set up a new factor. Only for a \
                  member who belongs to this workspace alone and is not platform staff: for \
                  anyone else a platform administrator does it. You cannot reset your own. \
                  Needs a fresh second factor (`mfa_step_up_required`). Requires permission: \
                  owner.assign",
    params(("id" = Uuid, Path, description = "The member's membership id.")),
    responses(
        (status = 204, description = "Reset."),
        (status = 401, description = "Unauthenticated, or a fresh code is needed (`mfa_step_up_required`).", body = kernel::ErrorEnvelope),
        (status = 403, description = "Insufficient permissions, or the user belongs to other workspaces or is platform staff.", body = kernel::ErrorEnvelope),
        (status = 404, description = "No such member.", body = kernel::ErrorEnvelope),
        (status = 422, description = "You cannot reset your own.", body = kernel::ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn reset_member_mfa(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Path(member_id): Path<Uuid>,
) -> Response {
    let request_id = ctx.request_id.clone();
    let internal =
        |e: sqlx::Error| ApiError::internal_error_from("MFA reset failed", e).into_response();

    let mut tx = match db::UnitOfWork::begin(&pool, &ctx).await {
        Ok(tx) => tx,
        Err(e) => return internal(e),
    };
    let target: Option<Uuid> = match sqlx::query_scalar(
        "SELECT user_id FROM tenant_memberships \
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(member_id)
    .bind(ctx.tenant_id)
    .fetch_optional(tx.conn())
    .await
    {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    drop(tx);
    let Some(target) = target else {
        return ApiError::not_found("Member not found")
            .with_request_id(&request_id)
            .into_response();
    };
    if target == principal.user_id {
        return ApiError::unprocessable_entity(
            "You cannot reset your own two-factor authentication; use a recovery code",
        )
        .with_request_id(&request_id)
        .into_response();
    }

    // Other workspaces and platform staff depend on this user's second factor too.
    let others: Result<Vec<(Uuid,)>, sqlx::Error> =
        sqlx::query_as("SELECT tenant_id FROM xtenant.user_memberships($1)")
            .bind(target)
            .fetch_all(&pool)
            .await;
    let other_workspaces = match others {
        Ok(rows) => rows.iter().any(|(tenant_id,)| *tenant_id != ctx.tenant_id),
        Err(e) => return internal(e),
    };
    let platform_staff: Result<bool, sqlx::Error> =
        sqlx::query_scalar("SELECT platform_role IS NOT NULL FROM users WHERE id = $1")
            .bind(target)
            .fetch_one(&pool)
            .await;
    let platform_staff = match platform_staff {
        Ok(v) => v,
        Err(e) => return internal(e),
    };
    if other_workspaces || platform_staff {
        return ApiError::unauthorized(
            "This user belongs to other workspaces or is platform staff: ask a platform administrator",
        )
        .with_request_id(&request_id)
        .into_response();
    }

    match mfa::reset(
        &pool,
        target,
        principal.user_id,
        "tenant",
        Some(ctx.tenant_id),
    )
    .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => identity::mfa_routes::error_response(error),
    }
}

/// Reset any user's two-factor authentication (platform administrator).
#[utoipa::path(
    post,
    path = "/platform/users/{id}/mfa/reset",
    tag = "platform-tenants",
    operation_id = "reset_user_mfa",
    summary = "Reset a user's two-factor authentication",
    description = "Removes the user's second factor and recovery codes and ends all of their \
                  sessions; they sign in with their password and set up a new factor. You \
                  cannot reset your own. Needs a fresh second factor (`mfa_step_up_required`). \
                  Requires permission: platform.admin",
    params(("id" = Uuid, Path, description = "The user's id.")),
    responses(
        (status = 204, description = "Reset."),
        (status = 401, description = "Unauthenticated, or a fresh code is needed (`mfa_step_up_required`).", body = kernel::ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = kernel::ErrorEnvelope),
        (status = 404, description = "No such user.", body = kernel::ErrorEnvelope),
        (status = 422, description = "You cannot reset your own.", body = kernel::ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn reset_user_mfa(
    State(pool): State<PgPool>,
    Extension(principal): Extension<Principal>,
    Path(user_id): Path<Uuid>,
) -> Response {
    if user_id == principal.user_id {
        return ApiError::unprocessable_entity(
            "You cannot reset your own two-factor authentication; use a recovery code",
        )
        .into_response();
    }
    let exists: Result<bool, sqlx::Error> = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM users WHERE id = $1 AND deleted_at IS NULL)",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await;
    match exists {
        Ok(true) => {}
        Ok(false) => return ApiError::not_found("User not found").into_response(),
        Err(e) => return ApiError::internal_error_from("MFA reset failed", e).into_response(),
    }
    match mfa::reset(&pool, user_id, principal.user_id, "platform", None).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => identity::mfa_routes::error_response(error),
    }
}
