use authz::{platform_permission_middleware, require_permission, Permission};
use axum::http::{header, header::SET_COOKIE, HeaderName, Method, StatusCode};
use axum::middleware::from_fn_with_state;
use axum::response::{IntoResponse, Response};
use axum::routing::MethodRouter;
use axum::Extension;
use axum::{extract::Request, middleware, routing, Router};
use config::AppConfig;
use identity::{principal_middleware, IdentityConfig, Principal};
use kernel::ApiError;
use notifications::{noop::LogEmailSender, smtp::SmtpEmailSender, EmailSender};
use observability::health::HealthReport;
use observability::request_id::{request_id_middleware, REQUEST_ID_HEADER};
use observability::trace::trace_middleware;
use observability::{liveness, metrics};
use std::sync::Arc;
use std::time::Duration;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::state::AppState;

async fn ready_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> HealthReport {
    observability::health::readiness(
        state.health_checks,
        Duration::from_millis(state.config.ready_probe_timeout_ms),
    )
    .await
}

fn panic_handler(panic_info: Box<dyn std::any::Any + Send + 'static>) -> axum::response::Response {
    let payload = panic_info
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic_info.downcast_ref::<String>().map(|s| s.as_str()))
        .unwrap_or("unknown");
    tracing::error!(panic_payload = %payload, "handler panicked");
    ApiError::internal_error("Internal server error").into_response()
}

async fn test_panic_handler() -> Response {
    panic!("intentional panic for testing");
}

async fn csrf_origin_middleware(
    axum::extract::State(config): axum::extract::State<Arc<AppConfig>>,
    request: Request,
    next: middleware::Next,
) -> Response {
    let method = request.method();
    let is_safe_method = matches!(method, &Method::GET | &Method::HEAD | &Method::OPTIONS);

    if !is_safe_method && request.extensions().get::<Principal>().is_some() {
        let origin = request
            .headers()
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok());
        if let Some(origin) = origin {
            let allowed = config
                .cors_allowed_origins
                .iter()
                .any(|allowed| allowed == origin);
            if !allowed {
                return ApiError::unauthorized("Origin not allowed").into_response();
            }
        }
    }

    next.run(request).await
}

fn cors_layer(config: &AppConfig) -> CorsLayer {
    let origins: Vec<_> = config
        .cors_allowed_origins
        .iter()
        .filter_map(|o| o.parse::<axum::http::HeaderValue>().ok())
        .collect();

    let mut headers = vec![
        axum::http::header::CONTENT_TYPE,
        axum::http::header::AUTHORIZATION,
        REQUEST_ID_HEADER.clone(),
        HeaderName::from_static("idempotency-key"),
        HeaderName::from_static("x-tenant-id"),
    ];
    if matches!(
        config.environment,
        config::Environment::Development | config::Environment::Test
    ) {
        headers.push(HeaderName::from_static("x-dev-user-id"));
    }

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_credentials(true)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PATCH,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers(headers)
        .expose_headers([REQUEST_ID_HEADER.clone()])
}

struct ProtectedRoutes {
    router: Router<sqlx::PgPool>,
}

impl ProtectedRoutes {
    fn new() -> Self {
        Self {
            router: Router::new(),
        }
    }

    fn guarded(
        mut self,
        path: &str,
        method_router: MethodRouter<sqlx::PgPool>,
        permission: Permission,
    ) -> Self {
        self.router = self.router.route(
            path,
            method_router.route_layer(require_permission(permission)),
        );
        self
    }

    fn guarded_with_methods(
        mut self,
        path: &str,
        get_router: MethodRouter<sqlx::PgPool>,
        get_permission: Permission,
        post_router: MethodRouter<sqlx::PgPool>,
        post_permission: Permission,
    ) -> Self {
        let merged = get_router
            .route_layer(require_permission(get_permission))
            .merge(post_router.route_layer(require_permission(post_permission)));
        self.router = self.router.route(path, merged);
        self
    }

    fn mount_platform(self, router: Router<sqlx::PgPool>) -> Router<sqlx::PgPool> {
        router.merge(
            self.router
                .layer(middleware::from_fn(platform_permission_middleware))
                .layer(middleware::from_fn(authentication_middleware)),
        )
    }

    fn mount_tenant(
        self,
        router: Router<sqlx::PgPool>,
        config: tenancy::TenancyConfig,
    ) -> Router<sqlx::PgPool> {
        router.merge(
            self.router
                .layer(from_fn_with_state(
                    config,
                    tenancy::tenant_context_middleware,
                ))
                .layer(middleware::from_fn(authentication_middleware)),
        )
    }
}

async fn authentication_middleware(request: Request, next: middleware::Next) -> Response {
    if request.extensions().get::<identity::Principal>().is_none() {
        return ApiError::unauthenticated("Authentication required").into_response();
    }
    next.run(request).await
}

async fn login(
    axum::extract::State(pool): axum::extract::State<sqlx::PgPool>,
    Extension(config): Extension<Arc<AppConfig>>,
    kernel::ApiJson(payload): kernel::ApiJson<identity::routes::LoginRequest>,
) -> Response {
    let success = match identity::routes::authenticate_login(&pool, &config, payload).await {
        Ok(success) => success,
        Err(identity::routes::LoginError::Validation) => {
            return ApiError::validation_failed("Email and password are required").into_response();
        }
        Err(identity::routes::LoginError::InvalidCredentials) => {
            return ApiError::unauthenticated("Invalid email or password").into_response();
        }
        Err(identity::routes::LoginError::Internal(message)) => {
            return ApiError::internal_error(message).into_response();
        }
    };
    let user_id = success.principal.user_id;
    let response = match tenancy::routes::build_me_response(
        &pool,
        success.principal,
        config.environment == config::Environment::Production,
    )
    .await
    {
        Ok(response) => response,
        Err(error) => return error.into_response(),
    };
    identity::audit::login_succeeded(&pool, user_id).await;

    ([(SET_COOKIE, success.session_cookie)], axum::Json(response)).into_response()
}

fn public_routes() -> Router<sqlx::PgPool> {
    Router::new()
        .route("/auth/login", routing::post(login))
        .route("/auth/logout", routing::post(identity::routes::logout))
        .route(
            "/invitations/{token}",
            routing::get(tenancy::invitations::preview_invitation),
        )
        .route(
            "/invitations/{token}/accept",
            routing::post(tenancy::invitations::accept_invitation),
        )
}

fn authenticated_routes() -> Router<sqlx::PgPool> {
    Router::new().route("/me", routing::get(tenancy::routes::me))
}

fn platform_routes(include_test_routes: bool) -> ProtectedRoutes {
    let routes = ProtectedRoutes::new()
        .guarded_with_methods(
            "/platform/tenants",
            routing::get(tenancy::routes::list_tenants),
            Permission::PlatformTenantsList,
            routing::post(tenancy::routes::create_tenant),
            Permission::PlatformTenantsManage,
        )
        .guarded_with_methods(
            "/platform/tenants/{id}",
            routing::get(tenancy::routes::get_tenant_detail),
            Permission::PlatformTenantsList,
            routing::patch(tenancy::routes::update_tenant),
            Permission::PlatformTenantsManage,
        )
        .guarded(
            "/platform/tenants/{id}/switch",
            routing::post(tenancy::routes::switch_tenant),
            Permission::PlatformTenantsSwitch,
        );
    if include_test_routes {
        routes
            .guarded(
                "/test/platform/admin",
                routing::get(|| async { StatusCode::OK }),
                Permission::PlatformAdmin,
            )
            .guarded(
                "/test/platform/billing/view",
                routing::get(|| async { StatusCode::OK }),
                Permission::PlatformBillingView,
            )
            .guarded(
                "/test/platform/diagnostics/view",
                routing::get(|| async { StatusCode::OK }),
                Permission::PlatformDiagnosticsView,
            )
    } else {
        routes
    }
}

fn tenant_routes(include_test_routes: bool) -> ProtectedRoutes {
    let routes = ProtectedRoutes::new()
        .guarded(
            "/tenant",
            routing::get(tenancy::routes::get_tenant),
            Permission::OverviewView,
        )
        .guarded_with_methods(
            "/tenant/customers",
            routing::get(customers::routes::list_customers),
            Permission::CustomersView,
            routing::post(customers::routes::create_customer),
            Permission::CustomersManage,
        )
        .guarded_with_methods(
            "/tenant/customers/{id}",
            routing::get(customers::routes::get_customer),
            Permission::CustomersView,
            routing::patch(customers::routes::update_customer),
            Permission::CustomersManage,
        )
        .guarded_with_methods(
            "/tenant/conversations",
            routing::get(conversations::routes::list_conversations),
            Permission::ConversationsView,
            routing::post(conversations::routes::create_conversation),
            Permission::ConversationsManage,
        )
        .guarded_with_methods(
            "/tenant/conversations/{id}",
            routing::get(crate::handlers::get_conversation_with_escalation),
            Permission::ConversationsView,
            routing::patch(conversations::routes::patch_conversation),
            Permission::ConversationsManage,
        )
        .guarded_with_methods(
            "/tenant/conversations/{id}/messages",
            routing::get(conversations::routes::get_timeline),
            Permission::ConversationsView,
            routing::post(conversations::routes::add_message),
            Permission::ConversationsManage,
        )
        .guarded(
            "/tenant/customers/{id}/conversations",
            routing::get(conversations::get_conversation_history),
            Permission::CustomersView,
        )
        .guarded(
            "/tenant/events",
            routing::get(escalations::events::stream_events),
            Permission::ConversationsView,
        )
        .guarded(
            "/tenant/conversations/{id}/escalate",
            routing::post(escalations::routes::escalate),
            Permission::ConversationsManage,
        )
        .guarded(
            "/tenant/escalations/queue",
            routing::get(escalations::routes::list_queue),
            Permission::ConversationsView,
        )
        .guarded(
            "/tenant/escalations/{id}/claim",
            routing::post(escalations::routes::claim),
            Permission::ConversationsManage,
        )
        .guarded(
            "/tenant/availability/me",
            routing::get(escalations::routes::get_my_availability)
                .put(escalations::routes::set_my_availability),
            Permission::ConversationsManage,
        )
        .guarded(
            "/tenant/skills",
            routing::get(escalations::routes::list_skills)
                .post(escalations::routes::create_skill),
            Permission::MembersView,
        )
        .guarded(
            "/tenant/skills/{id}",
            routing::patch(escalations::routes::rename_skill)
                .delete(escalations::routes::delete_skill),
            Permission::MembersManage,
        )
        .guarded(
            "/tenant/members/{membershipId}/skills",
            routing::put(escalations::routes::set_member_skills),
            Permission::MembersManage,
        )
        .guarded(
            "/tenant/members",
            routing::get(crate::handlers::list_members_with_skills),
            Permission::MembersView,
        )
        .guarded(
            "/tenant/members/{id}",
            routing::patch(tenancy::members::update_member),
            Permission::MembersManage,
        )
        .guarded_with_methods(
            "/tenant/members/invitations",
            routing::get(tenancy::invitations::list_invitations),
            Permission::MembersView,
            routing::post(tenancy::invitations::create_invitation),
            Permission::MembersManage,
        )
        .guarded(
            "/tenant/members/invitations/{id}/delivery",
            routing::get(tenancy::invitations::get_invitation_delivery),
            Permission::MembersView,
        )
        .guarded(
            "/tenant/members/invitations/{id}",
            routing::delete(tenancy::invitations::revoke_invitation),
            Permission::MembersManage,
        );
    if include_test_routes {
        routes
            .guarded(
                "/test/tenant/events",
                routing::get(|| async { StatusCode::OK }),
                Permission::ConversationsView,
            )
            .guarded(
                "/test/tenant/escalations/manage",
                routing::get(|| async { StatusCode::OK }),
                Permission::ConversationsManage,
            )
            .guarded(
                "/test/tenant/escalations/view",
                routing::get(|| async { StatusCode::OK }),
                Permission::ConversationsView,
            )
            .guarded(
                "/test/tenant/skills/manage",
                routing::get(|| async { StatusCode::OK }),
                Permission::MembersManage,
            )
            .guarded(
                "/test/tenant/members/{id}",
                // Also accept GET so the RBAC matrix test (rbac.rs
                // TENANT_OPERATIONS, which always probes with GET) can
                // exercise `members.manage` through this path-parameterized
                // synthetic route, in addition to mirroring the real PATCH
                // /tenant/members/{id} endpoint's method.
                routing::patch(|| async { StatusCode::OK }).get(|| async { StatusCode::OK }),
                Permission::MembersManage,
            )
            .guarded(
                "/test/tenant/conversations/manage",
                routing::get(|| async { StatusCode::OK }),
                Permission::ConversationsManage,
            )
            .guarded(
                "/test/tenant/members/manage",
                routing::get(|| async { StatusCode::OK }),
                Permission::MembersManage,
            )
            .guarded(
                "/test/tenant/members/view",
                routing::get(|| async { StatusCode::OK }),
                Permission::MembersView,
            )
            .guarded(
                "/test/tenant/customers/view",
                routing::get(|| async { StatusCode::OK }),
                Permission::CustomersView,
            )
            .guarded(
                "/test/tenant/customers/manage",
                routing::get(|| async { StatusCode::OK }),
                Permission::CustomersManage,
            )
            .guarded(
                "/test/tenant/members/invitations/view",
                routing::get(|| async { StatusCode::OK }),
                Permission::MembersView,
            )
            .guarded(
                "/test/tenant/members/invitations/manage",
                routing::get(|| async { StatusCode::OK }),
                Permission::MembersManage,
            )
            .guarded(
                "/test/tenant/settings/manage",
                routing::get(|| async { StatusCode::OK }),
                Permission::SettingsManage,
            )
            .guarded(
                "/test/tenant/billing/view",
                routing::get(|| async { StatusCode::OK }),
                Permission::BillingView,
            )
            .guarded(
                "/test/tenant/billing/manage",
                routing::get(|| async { StatusCode::OK }),
                Permission::BillingManage,
            )
    } else {
        routes
    }
}

fn api_routes(
    state: &AppState,
    include_test_routes: bool,
    email_sender: Option<Arc<dyn EmailSender>>,
) -> Router<sqlx::PgPool> {
    let identity_config = IdentityConfig {
        pool: state.db.clone(),
        environment: state.config.environment.clone(),
        auth_jwt_secret: state.config.auth_jwt_secret.clone(),
        auth_session_ttl_seconds: state.config.auth_session_ttl_seconds,
    };
    let tenancy_config = tenancy::TenancyConfig {
        pool: state.db.clone(),
        is_production: state.config.environment == config::Environment::Production,
    };

    let email_sender: Arc<dyn EmailSender> = email_sender.unwrap_or_else(|| {
        if let (Some(url), Some(from)) = (&state.config.smtp_url, &state.config.smtp_from) {
            match SmtpEmailSender::new(url, from) {
                Ok(s) => Arc::new(s),
                Err(e) => {
                    tracing::warn!(error = %e, "failed to create SMTP sender, falling back to log");
                    Arc::new(LogEmailSender)
                }
            }
        } else {
            Arc::new(LogEmailSender)
        }
    });

    let routes = Router::new()
        .merge(public_routes())
        .merge(authenticated_routes());
    let routes = platform_routes(include_test_routes).mount_platform(routes);
    let routes = tenant_routes(include_test_routes).mount_tenant(routes, tenancy_config);

    routes
        .fallback(|request: Request| async move {
            let request_id = request
                .headers()
                .get(&REQUEST_ID_HEADER)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("unknown");
            ApiError::not_found("Route not found").with_request_id(request_id)
        })
        .layer(Extension(email_sender))
        .layer(Extension(state.config.clone()))
        .layer(Extension(state.escalations.clone()))
        .layer(from_fn_with_state(
            state.config.clone(),
            csrf_origin_middleware,
        ))
        .layer(from_fn_with_state(identity_config, principal_middleware))
}

pub fn configured_email_sender(config: &AppConfig) -> Arc<dyn EmailSender> {
    if let (Some(url), Some(from)) = (&config.smtp_url, &config.smtp_from) {
        match SmtpEmailSender::new(url, from) {
            Ok(sender) => Arc::new(sender),
            Err(error) => {
                tracing::warn!(%error, "failed to create SMTP sender, falling back to log");
                Arc::new(LogEmailSender)
            }
        }
    } else {
        Arc::new(LogEmailSender)
    }
}

fn build_app(
    state: AppState,
    include_test_routes: bool,
    email_sender: Option<Arc<dyn EmailSender>>,
) -> Router {
    let config = state.config.clone();
    let mut router = Router::new()
        .route("/health", routing::get(liveness))
        .route("/ready", routing::get(ready_handler))
        .route("/metrics", routing::get(metrics));
    if include_test_routes {
        router = router
            .route(
                "/test-echo",
                routing::post(|body: kernel::ApiJson<serde_json::Value>| async move {
                    axum::Json(body.0)
                }),
            )
            .route("/test-panic", routing::get(test_panic_handler));
    }

    router
        .nest(
            "/api/v1",
            api_routes(&state, include_test_routes, email_sender).with_state(state.db.clone()),
        )
        .fallback(|request: Request| async move {
            let request_id = request
                .headers()
                .get(&REQUEST_ID_HEADER)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("unknown");
            ApiError::not_found("Route not found").with_request_id(request_id)
        })
        .layer(CatchPanicLayer::custom(panic_handler))
        .layer(middleware::from_fn(trace_middleware))
        .layer(middleware::from_fn(request_id_middleware))
        .layer(cors_layer(&config))
        .with_state(state)
}

pub fn app(state: AppState) -> Router {
    build_app(state, false, None)
}

pub fn app_with_email_sender(state: AppState, email_sender: Arc<dyn EmailSender>) -> Router {
    build_app(state, false, Some(email_sender))
}

pub fn app_with_test_routes(state: AppState) -> Router {
    build_app(state, true, None)
}

pub fn app_with_test_routes_and_email_sender(
    state: AppState,
    email_sender: Arc<dyn EmailSender>,
) -> Router {
    build_app(state, true, Some(email_sender))
}

#[cfg(test)]
mod tests {
    use super::{platform_routes, ProtectedRoutes};

    #[test]
    fn protected_scope_construction_returns_only_guarded_builder() {
        fn assert_protected_routes(_: ProtectedRoutes) {}

        assert_protected_routes(platform_routes(false));
    }
}
