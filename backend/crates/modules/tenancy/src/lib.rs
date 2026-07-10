pub mod audit;
pub mod authorize;
pub mod routes;

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

#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant_id: Uuid,
    pub tenant_status: String,
    pub principal_kind: identity::PrincipalKind,
    pub tenant_role: Option<authz::TenantRole>,
    pub permissions: authz::PermissionSet,
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

fn forbidden_response() -> Response {
    ApiError::unauthorized("Access denied").into_response()
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
            return forbidden_response();
        }
    };

    let (tenant_role, permissions) = match principal.kind() {
        identity::PrincipalKind::InvalidPlatformRole => {
            tracing::warn!(
                user.id = %principal.user_id,
                "user has unrecognized platform role; granting empty tenant permissions"
            );
            (None, authz::PermissionSet::default())
        }
        identity::PrincipalKind::Platform => {
            let permissions = authz::staff_tenant_permissions(
                principal
                    .platform_role
                    .expect("platform principal must carry a platform role"),
                config.is_production,
            );
            (None, authz::PermissionSet::new(permissions.iter().copied()))
        }
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
                return forbidden_response();
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
                return forbidden_response();
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
    };

    tracing::Span::current().record("tenant.id", field::display(&tenant_id));

    request.extensions_mut().insert(ctx);
    request.extensions_mut().insert(permissions);
    let response = next.run(request).await;
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

#[cfg(test)]
mod tests {
    use super::permission_set_for_stored_tenant_role;

    #[test]
    fn unknown_stored_tenant_role_fails_closed() {
        assert!(permission_set_for_stored_tenant_role("legacy_role").is_none());
    }
}
