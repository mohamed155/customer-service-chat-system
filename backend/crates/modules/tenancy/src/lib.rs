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
    State(pool): State<sqlx::PgPool>,
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

    let tenant = match authorize::fetch_tenant(&pool, tenant_id).await {
        Some(t) => t,
        None => {
            audit::access_denied(&pool, Some(principal.user_id), &tenant_id_str, "not_found").await;
            return forbidden_response();
        }
    };

    match principal.kind() {
        identity::PrincipalKind::Platform => {}
        identity::PrincipalKind::Tenant => {
            if !authorize::has_active_membership(&pool, tenant_id, principal.user_id).await {
                audit::access_denied(
                    &pool,
                    Some(principal.user_id),
                    &tenant_id_str,
                    "no_membership",
                )
                .await;
                return forbidden_response();
            }
            if tenant.status != "active" {
                audit::access_denied(&pool, Some(principal.user_id), &tenant_id_str, "suspended")
                    .await;
                return ApiError::unauthorized("Tenant is suspended").into_response();
            }
        }
    }

    let ctx = TenantContext {
        tenant_id,
        tenant_status: tenant.status.clone(),
        principal_kind: principal.kind(),
    };

    tracing::Span::current().record("tenant.id", field::display(&tenant_id));

    request.extensions_mut().insert(ctx);
    next.run(request).await
}
