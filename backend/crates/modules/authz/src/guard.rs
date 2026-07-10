use crate::Permission;
use axum::{
    extract::{Request, State},
    middleware::{self, FromFnLayer, Next},
    response::{IntoResponse, Response},
};
use identity::Principal;
use kernel::ApiError;
use std::{collections::HashSet, future::Future, pin::Pin};
use tracing::field;

/// Effective permissions attached to a request by authorization middleware.
#[derive(Debug, Clone, Default)]
pub struct PermissionSet(HashSet<Permission>);

impl PermissionSet {
    pub fn new(permissions: impl IntoIterator<Item = Permission>) -> Self {
        Self(permissions.into_iter().collect())
    }

    pub fn contains(&self, permission: Permission) -> bool {
        self.0.contains(&permission)
    }
}

/// Creates an Axum route layer that rejects requests lacking `required`.
pub fn require_permission(
    required: Permission,
) -> FromFnLayer<PermissionGuardFn, Permission, (State<Permission>, Request)> {
    middleware::from_fn_with_state(required, permission_guard as PermissionGuardFn)
}

/// Attaches platform-scope permissions derived from the current principal.
pub async fn platform_permission_middleware(mut request: Request, next: Next) -> Response {
    let permissions = request
        .extensions()
        .get::<Principal>()
        .and_then(|principal| principal.platform_role)
        .map(crate::platform_role_permissions)
        .unwrap_or_default();
    request
        .extensions_mut()
        .insert(PermissionSet::new(permissions.iter().copied()));
    next.run(request).await
}

type PermissionGuardFuture = Pin<Box<dyn Future<Output = Response> + Send>>;
type PermissionGuardFn = fn(State<Permission>, Request, Next) -> PermissionGuardFuture;

fn permission_guard(
    State(required): State<Permission>,
    request: Request,
    next: Next,
) -> PermissionGuardFuture {
    Box::pin(async move {
        if request
            .extensions()
            .get::<PermissionSet>()
            .is_some_and(|permissions| permissions.contains(required))
        {
            return next.run(request).await;
        }

        tracing::Span::current().record("authz.denied_permission", field::display(required));
        tracing::warn!(authz.denied_permission = %required, "permission denied");
        ApiError::unauthorized("Access denied").into_response()
    })
}

#[cfg(test)]
mod tests {
    use super::{PermissionSet, require_permission};
    use crate::Permission;
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::get,
    };
    use tower::ServiceExt;

    fn guarded_router(permission_set: Option<PermissionSet>) -> Router {
        let router = Router::new().route(
            "/",
            get(|| async { StatusCode::NO_CONTENT })
                .route_layer(require_permission(Permission::CustomersManage)),
        );

        match permission_set {
            Some(permission_set) => router.layer(axum::Extension(permission_set)),
            None => router,
        }
    }

    #[tokio::test]
    async fn allows_request_when_permission_is_present() {
        let response = guarded_router(Some(PermissionSet::new([Permission::CustomersManage])))
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn denies_request_when_permission_is_absent() {
        let response = guarded_router(Some(PermissionSet::new([Permission::CustomersView])))
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn denies_request_when_permission_set_extension_is_missing() {
        let response = guarded_router(None)
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
