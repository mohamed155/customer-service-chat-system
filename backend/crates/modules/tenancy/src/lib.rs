pub mod api;
mod application;
pub mod audit;
pub mod authorize;
pub mod directory;
mod domain;
mod infrastructure;
pub mod invitations;
pub mod members;
pub mod routes;
pub mod signup;

use axum::{
    extract::{FromRequestParts, Request, State},
    http::request::Parts,
    middleware::Next,
    response::{IntoResponse, Response},
};
use kernel::ApiError;
use std::str::FromStr;
use tracing::field;
use uuid::Uuid;

pub use infrastructure::platform_access::sweep as sweep_platform_access_sessions;

/// Proof of the access session that authorized a request. Streaming endpoints revalidate
/// the same grant as they send events, even after the request's response headers were sent.
#[derive(Clone, Copy)]
pub struct PlatformAccessGrant {
    pub id: Uuid,
    expires_at: chrono::DateTime<chrono::Utc>,
    auth_session_id: Uuid,
}

#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant_id: Uuid,
    pub tenant_status: String,
    pub principal_kind: identity::PrincipalKind,
    pub tenant_role: Option<authz::TenantRole>,
    pub permissions: authz::PermissionSet,
    pub request_id: String,
}

/// Lets the Unit of Work (`db::UnitOfWork::begin(&pool, &ctx)`) bind a transaction to this tenant.
/// Only the tenant-context middleware builds a `TenantContext`, so a Unit of Work started from one
/// is always for a tenant the caller was authorised for.
impl db::TenantScoped for TenantContext {
    fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }
}

impl TenantContext {
    /// Opens this tenant's Unit of Work (`app.tenant_id` set for the transaction), or the 500
    /// response a handler should return when the database cannot start a transaction.
    pub async fn unit_of_work(&self, pool: &sqlx::PgPool) -> Result<db::UnitOfWork, Response> {
        db::UnitOfWork::begin(pool, self).await.map_err(|error| {
            ApiError::internal_error_from("Failed to begin tenant unit of work", error)
                .with_request_id(&self.request_id)
                .into_response()
        })
    }
}

impl TenantContext {
    /// Commits a Unit of Work, or returns the 500 response a handler should return. A Unit of Work
    /// that is dropped instead of committed is rolled back, so every handler path that wrote
    /// something must end here (or in `UnitOfWork::commit`).
    pub async fn commit(&self, uow: db::UnitOfWork) -> Result<(), Response> {
        uow.commit().await.map_err(|error| {
            ApiError::internal_error_from("Failed to commit tenant unit of work", error)
                .with_request_id(&self.request_id)
                .into_response()
        })
    }
}

#[derive(Clone)]
pub struct TenancyConfig {
    pub pool: sqlx::PgPool,
    pub is_production: bool,
}

impl<S: Send + Sync> FromRequestParts<S> for TenantContext {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<TenantContext>()
            .cloned()
            .ok_or_else(|| ApiError::internal_error("TenantContext not available"))
    }
}

fn forbidden_response_with_request_id(request_id: &str) -> Response {
    ApiError::unauthorized("Access denied")
        .with_request_id(request_id)
        .into_response()
}

pub async fn tenant_context_middleware(
    State(config): State<TenancyConfig>,
    mut request: Request,
    next: Next,
) -> Response {
    let tenant_id_str = match request.headers().get("X-Tenant-ID") {
        Some(v) => match v.to_str() {
            Ok(s) => s.to_owned(),
            Err(_) => {
                return ApiError::validation_failed("X-Tenant-ID header is invalid").into_response()
            }
        },
        None => {
            return ApiError::validation_failed("X-Tenant-ID header is missing").into_response()
        }
    };

    let tenant_id = match Uuid::from_str(&tenant_id_str) {
        Ok(id) => id,
        Err(_) => {
            return ApiError::validation_failed("X-Tenant-ID header is not a valid UUID")
                .into_response()
        }
    };

    let principal = match request.extensions().get::<identity::Principal>() {
        Some(p) => p.clone(),
        None => return ApiError::unauthenticated("Authentication required").into_response(),
    };

    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();

    let tenant = match authorize::fetch_tenant(&config.pool, tenant_id).await {
        Some(t) => t,
        None => {
            audit::access_denied(
                &config.pool,
                Some(principal.user_id),
                &tenant_id_str,
                "not_found",
            )
            .await;
            return forbidden_response_with_request_id(&request_id);
        }
    };

    let access_grant = if principal.kind() == identity::PrincipalKind::Platform {
        let claims = request
            .extensions()
            .get::<identity::session::SessionClaims>();
        match api::platform_access::require_access_session(
            &config.pool,
            principal.user_id,
            tenant_id,
            claims,
        )
        .await
        {
            Ok(grant) => Some(grant),
            Err(error) => return error.with_request_id(&request_id).into_response(),
        }
    } else {
        None
    };

    let (tenant_role, permissions) = match principal.kind() {
        identity::PrincipalKind::InvalidPlatformRole => {
            tracing::warn!(
                user.id = %principal.user_id,
                "user has unrecognized platform role; granting empty tenant permissions"
            );
            (None, authz::PermissionSet::default())
        }
        identity::PrincipalKind::Platform => match principal.platform_role {
            Some(role) => {
                let permissions = authz::staff_tenant_permissions(role, config.is_production);
                (None, authz::PermissionSet::new(permissions.iter().copied()))
            }
            // A platform principal always carries its role; if one does not, grant nothing.
            None => {
                tracing::error!(
                    user.id = %principal.user_id,
                    "platform principal without a platform role; granting empty tenant permissions"
                );
                (None, authz::PermissionSet::default())
            }
        },
        identity::PrincipalKind::Tenant => {
            let Some(stored_role) =
                authorize::fetch_membership_role(&config.pool, tenant_id, principal.user_id).await
            else {
                audit::access_denied(
                    &config.pool,
                    Some(principal.user_id),
                    &tenant_id_str,
                    "no_membership",
                )
                .await;
                return forbidden_response_with_request_id(&request_id);
            };
            let Some((tenant_role, permissions)) =
                permission_set_for_stored_tenant_role(&stored_role)
            else {
                tracing::error!(
                    tenant.id = %tenant_id,
                    user.id = %principal.user_id,
                    tenant.role = %stored_role,
                    "unrecognized stored tenant role"
                );
                audit::access_denied(
                    &config.pool,
                    Some(principal.user_id),
                    &tenant_id_str,
                    "unknown_role",
                )
                .await;
                return forbidden_response_with_request_id(&request_id);
            };
            if tenant.status != "active" {
                audit::access_denied(
                    &config.pool,
                    Some(principal.user_id),
                    &tenant_id_str,
                    "suspended",
                )
                .await;
                return ApiError::unauthorized("Tenant is suspended").into_response();
            }
            (Some(tenant_role), permissions)
        }
    };

    let ctx = TenantContext {
        tenant_id,
        tenant_status: tenant.status.clone(),
        principal_kind: principal.kind(),
        tenant_role,
        permissions: permissions.clone(),
        request_id,
    };

    tracing::Span::current().record("tenant.id", field::display(&tenant_id));

    request.extensions_mut().insert(ctx);
    request.extensions_mut().insert(permissions);
    let mut response = if let Some(grant) = access_grant {
        request.extensions_mut().insert(grant);
        audit_log::with_platform_access_session(
            principal.user_id,
            tenant_id,
            grant.id,
            next.run(request),
        )
        .await
    } else {
        next.run(request).await
    };
    // Lets the server-error log name the tenant (see the server's `internal_error` middleware).
    response
        .extensions_mut()
        .insert(kernel::ResponseTenant(tenant_id));
    if response.status() == axum::http::StatusCode::FORBIDDEN {
        audit::access_denied(
            &config.pool,
            Some(principal.user_id),
            &tenant_id_str,
            "permission_denied",
        )
        .await;
    }
    response
}

fn permission_set_for_stored_tenant_role(
    stored_role: &str,
) -> Option<(authz::TenantRole, authz::PermissionSet)> {
    let role = authz::TenantRole::from_str(stored_role).ok()?;
    let permissions = authz::tenant_role_permissions(role);
    Some((role, authz::PermissionSet::new(permissions.iter().copied())))
}

/// Permission required both when opening and revalidating tenant conversation streams.
pub const CONVERSATION_STREAM_PERMISSION: authz::Permission = authz::Permission::ConversationsView;

/// Reuse the tenant permission matrix for an existing stream's active membership.
pub async fn can_view_conversation_stream(
    pool: &sqlx::PgPool,
    tenant_id: uuid::Uuid,
    user_id: uuid::Uuid,
    membership_id: uuid::Uuid,
) -> sqlx::Result<bool> {
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant_id).await?;
    let role: Option<String> = sqlx::query_scalar(
        "SELECT m.role FROM tenant_memberships m JOIN tenants t ON t.id = m.tenant_id
         WHERE m.tenant_id = $1 AND m.user_id = $2 AND m.id = $3
           AND m.status = 'active' AND m.deleted_at IS NULL AND t.status = 'active'
           AND t.deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(membership_id)
    .fetch_optional(uow.conn())
    .await?;
    uow.commit().await?;
    Ok(role
        .as_deref()
        .and_then(permission_set_for_stored_tenant_role)
        .is_some_and(|(_, permissions)| permissions.contains(CONVERSATION_STREAM_PERMISSION)))
}

/// Check the current platform role against the same permission matrix used on subscribe.
pub async fn staff_can_view_conversation_stream(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    is_production: bool,
) -> bool {
    identity::directory::principal_row(pool, user_id)
        .await
        .ok()
        .flatten()
        .and_then(|(_, _, _, role)| role)
        .and_then(|role| identity::PlatformRole::from_str(&role).ok())
        .is_some_and(|role| {
            authz::staff_tenant_permissions(role, is_production)
                .contains(&CONVERSATION_STREAM_PERMISSION)
        })
}

#[cfg(test)]
mod tests {
    use super::permission_set_for_stored_tenant_role;

    #[test]
    fn unknown_stored_tenant_role_fails_closed() {
        assert!(permission_set_for_stored_tenant_role("legacy_role").is_none());
    }
}
