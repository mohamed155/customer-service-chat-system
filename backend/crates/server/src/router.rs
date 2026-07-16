use authz::{platform_permission_middleware, require_permission, Permission};
use axum::http::{header, header::SET_COOKIE, HeaderName, Method, StatusCode};
use axum::middleware::from_fn_with_state;
use axum::response::{IntoResponse, Response};
use axum::routing::MethodRouter;
use axum::Extension;
use axum::{extract::Request, middleware, routing, Router};
use config::{AppConfig, Environment};
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
use utoipa::OpenApi;
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;
use utoipa_swagger_ui::SwaggerUi;

use crate::openapi::ApiDoc;
use crate::state::AppState;

#[utoipa::path(
    get,
    path = "/ready",
    tag = "ops",
    responses(
        (status = 200, description = "All readiness checks passed", body = observability::health::HealthReport),
        (status = 503, description = "One or more readiness checks failed", body = observability::health::HealthReport),
    )
)]
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
        Environment::Development | Environment::Test
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

async fn authentication_middleware(request: Request, next: middleware::Next) -> Response {
    if request.extensions().get::<identity::Principal>().is_none() {
        return ApiError::unauthenticated("Authentication required").into_response();
    }
    next.run(request).await
}

/// Authenticate a user and start a session.
///
/// Lives in `server::router` (rather than `identity::routes`) because it
/// composes identity authentication with tenancy-side `build_me_response` and
/// the shared `ApiError` envelope — moving it into the `identity` crate would
/// introduce a `tenancy → identity → tenancy` circular crate dependency.
/// The OpenAPI annotation stays here so the operation is documented under the
/// `auth` tag alongside `POST /auth/logout`.
#[utoipa::path(
    post,
    path = "/auth/login",
    tag = "auth",
    operation_id = "auth_login",
    summary = "Authenticate and start a session",
    description = "Accepts an email + password pair, verifies them against the `users` table, \
                  and on success issues a signed JWT session. The JWT is returned to the client via \
                  a `Set-Cookie: app_session=...; HttpOnly; SameSite=Lax; Path=/` response header. \
                  The response body is the same profile shape returned by `GET /me` (see \
                  `tenancy::routes::MeResponse`); the typed body schema is filled in by the \
                  tenancy-module annotation pass. This endpoint is public — no `app_session` cookie \
                  is required.",
    request_body = identity::routes::LoginRequest,
    responses(
        (status = 200, description = "Login successful. Returns the user profile and sets the `app_session` cookie.", body = serde_json::Value),
        (status = 400, description = "Validation failed (missing email or password).", body = kernel::ErrorEnvelope),
        (status = 401, description = "Invalid email or password.", body = kernel::ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = kernel::ErrorEnvelope),
    ),
    security(()),
)]
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
        config.environment == Environment::Production,
    )
    .await
    {
        Ok(response) => response,
        Err(error) => return error.into_response(),
    };
    identity::audit::login_succeeded(&pool, user_id).await;

    ([(SET_COOKIE, success.session_cookie)], axum::Json(response)).into_response()
}

/// Combine two `MethodRouter`s, each with its own per-method permission
/// layer applied. The result is a single `MethodRouter` whose `get` method
/// carries `get_permission` and whose `post` method carries `post_permission`.
fn merge_with_permissions<S>(
    get_router: MethodRouter<S>,
    get_permission: Permission,
    post_router: MethodRouter<S>,
    post_permission: Permission,
) -> MethodRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    let get_router = get_router.route_layer(require_permission(get_permission));
    let post_router = post_router.route_layer(require_permission(post_permission));
    get_router.merge(post_router)
}

fn public_routes() -> OpenApiRouter<sqlx::PgPool> {
    OpenApiRouter::new()
        .routes(routes!(login))
        .routes(routes!(identity::routes::logout))
        .routes(routes!(tenancy::invitations::preview_invitation))
        .routes(routes!(tenancy::invitations::accept_invitation))
}

fn authenticated_routes() -> OpenApiRouter<sqlx::PgPool> {
    OpenApiRouter::new().routes(routes!(tenancy::routes::me))
}

fn platform_routes(include_test_routes: bool) -> OpenApiRouter<sqlx::PgPool> {
    let mut router = OpenApiRouter::new()
        .routes(
            routes!(
                tenancy::routes::list_tenants,
                tenancy::routes::create_tenant
            )
            .map(|_| {
                merge_with_permissions(
                    routing::get(tenancy::routes::list_tenants),
                    Permission::PlatformTenantsList,
                    routing::post(tenancy::routes::create_tenant),
                    Permission::PlatformTenantsManage,
                )
            }),
        )
        .routes(
            routes!(
                tenancy::routes::get_tenant_detail,
                tenancy::routes::update_tenant
            )
            .map(|_| {
                merge_with_permissions(
                    routing::get(tenancy::routes::get_tenant_detail),
                    Permission::PlatformTenantsList,
                    routing::patch(tenancy::routes::update_tenant),
                    Permission::PlatformTenantsManage,
                )
            }),
        )
        .routes(
            routes!(tenancy::routes::switch_tenant)
                .layer(require_permission(Permission::PlatformTenantsSwitch)),
        )
        .routes(
            routes!(
                ai::routes::get_platform_config,
                ai::routes::put_platform_config
            )
            .map(|_| {
                merge_with_permissions(
                    routing::get(ai::routes::get_platform_config),
                    Permission::PlatformAdmin,
                    routing::put(ai::routes::put_platform_config),
                    Permission::PlatformAdmin,
                )
            }),
        )
        .routes(
            routes!(
                ai::routes::put_platform_credential,
                ai::routes::delete_platform_credential
            )
            .map(|_| {
                merge_with_permissions(
                    routing::put(ai::routes::put_platform_credential),
                    Permission::PlatformAdmin,
                    routing::delete(ai::routes::delete_platform_credential),
                    Permission::PlatformAdmin,
                )
            }),
        )
        .routes(
            routes!(ai::routes::test_platform_config)
                .layer(require_permission(Permission::PlatformAdmin)),
        );
    if include_test_routes {
        // Test routes are closures, not function paths, so they cannot use the
        // `routes!()` co-registration macro. They stay on the plain `.route()`
        // passthrough so they register in the live `Router` only — never in
        // the documented `OpenApi` (FR-004).
        router = router
            .route(
                "/test/platform/admin",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::PlatformAdmin)),
            )
            .route(
                "/test/platform/billing/view",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::PlatformBillingView)),
            )
            .route(
                "/test/platform/diagnostics/view",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::PlatformDiagnosticsView)),
            );
    }
    router
}

fn tenant_routes(include_test_routes: bool) -> OpenApiRouter<sqlx::PgPool> {
    let mut router = OpenApiRouter::new()
        .routes(
            routes!(tenancy::routes::get_tenant)
                .layer(require_permission(Permission::OverviewView)),
        )
        .routes(
            routes!(
                customers::routes::list_customers,
                customers::routes::create_customer
            )
            .map(|_| {
                merge_with_permissions(
                    routing::get(customers::routes::list_customers),
                    Permission::CustomersView,
                    routing::post(customers::routes::create_customer),
                    Permission::CustomersManage,
                )
            }),
        )
        .routes(
            routes!(
                customers::routes::get_customer,
                customers::routes::update_customer
            )
            .map(|_| {
                merge_with_permissions(
                    routing::get(customers::routes::get_customer),
                    Permission::CustomersView,
                    routing::patch(customers::routes::update_customer),
                    Permission::CustomersManage,
                )
            }),
        )
        .routes(
            routes!(
                conversations::routes::list_conversations,
                conversations::routes::create_conversation
            )
            .map(|_| {
                merge_with_permissions(
                    routing::get(conversations::routes::list_conversations),
                    Permission::ConversationsView,
                    routing::post(conversations::routes::create_conversation),
                    Permission::ConversationsManage,
                )
            }),
        )
        .routes(
            routes!(
                crate::handlers::get_conversation_with_escalation,
                conversations::routes::patch_conversation
            )
            .map(|_| {
                merge_with_permissions(
                    routing::get(crate::handlers::get_conversation_with_escalation),
                    Permission::ConversationsView,
                    routing::patch(conversations::routes::patch_conversation),
                    Permission::ConversationsManage,
                )
            }),
        )
        .routes(
            routes!(
                conversations::routes::get_timeline,
                conversations::routes::add_message
            )
            .map(|_| {
                merge_with_permissions(
                    routing::get(conversations::routes::get_timeline),
                    Permission::ConversationsView,
                    routing::post(conversations::routes::add_message),
                    Permission::ConversationsManage,
                )
            }),
        )
        .routes(
            routes!(conversations::get_conversation_history)
                .layer(require_permission(Permission::CustomersView)),
        )
        .routes(
            routes!(escalations::events::stream_events)
                .layer(require_permission(Permission::ConversationsView)),
        )
        .routes(
            routes!(escalations::routes::escalate)
                .layer(require_permission(Permission::ConversationsManage)),
        )
        .routes(
            routes!(ai::agent_routes::set_conversation_ai_handling)
                .layer(require_permission(Permission::ConversationsManage)),
        )
        .routes(
            routes!(escalations::routes::list_queue)
                .layer(require_permission(Permission::ConversationsView)),
        )
        .routes(
            routes!(escalations::routes::claim)
                .layer(require_permission(Permission::ConversationsManage)),
        )
        .routes(
            routes!(
                escalations::routes::get_my_availability,
                escalations::routes::set_my_availability
            )
            .map(|_| {
                merge_with_permissions(
                    routing::get(escalations::routes::get_my_availability),
                    Permission::ConversationsManage,
                    routing::put(escalations::routes::set_my_availability),
                    Permission::ConversationsManage,
                )
            }),
        )
        .routes(
            routes!(
                escalations::routes::list_skills,
                escalations::routes::create_skill
            )
            .map(|_| {
                merge_with_permissions(
                    routing::get(escalations::routes::list_skills),
                    Permission::MembersView,
                    routing::post(escalations::routes::create_skill),
                    Permission::MembersView,
                )
            }),
        )
        .routes(
            routes!(
                escalations::routes::rename_skill,
                escalations::routes::delete_skill
            )
            .map(|_| {
                merge_with_permissions(
                    routing::patch(escalations::routes::rename_skill),
                    Permission::MembersManage,
                    routing::delete(escalations::routes::delete_skill),
                    Permission::MembersManage,
                )
            }),
        )
        .routes(
            routes!(escalations::routes::set_member_skills)
                .layer(require_permission(Permission::MembersManage)),
        )
        .routes(
            routes!(crate::handlers::list_members_with_skills)
                .layer(require_permission(Permission::MembersView)),
        )
        .routes(
            routes!(tenancy::members::update_member)
                .layer(require_permission(Permission::MembersManage)),
        )
        .routes(
            routes!(
                tenancy::invitations::list_invitations,
                tenancy::invitations::create_invitation
            )
            .map(|_| {
                merge_with_permissions(
                    routing::get(tenancy::invitations::list_invitations),
                    Permission::MembersView,
                    routing::post(tenancy::invitations::create_invitation),
                    Permission::MembersManage,
                )
            }),
        )
        .routes(
            routes!(tenancy::invitations::get_invitation_delivery)
                .layer(require_permission(Permission::MembersView)),
        )
        .routes(
            routes!(tenancy::invitations::revoke_invitation)
                .layer(require_permission(Permission::MembersManage)),
        )
        .routes(
            routes!(
                ai::routes::get_tenant_config,
                ai::routes::put_tenant_config,
                ai::routes::delete_tenant_config
            )
            .map(|_| {
                let get = routing::get(ai::routes::get_tenant_config)
                    .route_layer(require_permission(Permission::AiAgentView));
                let put = routing::put(ai::routes::put_tenant_config)
                    .route_layer(require_permission(Permission::AiAgentManage));
                let delete = routing::delete(ai::routes::delete_tenant_config)
                    .route_layer(require_permission(Permission::AiAgentManage));
                get.merge(put).merge(delete)
            }),
        )
        .routes(
            routes!(
                ai::routes::put_tenant_credential,
                ai::routes::delete_tenant_credential
            )
            .map(|_| {
                merge_with_permissions(
                    routing::put(ai::routes::put_tenant_credential),
                    Permission::AiAgentManage,
                    routing::delete(ai::routes::delete_tenant_credential),
                    Permission::AiAgentManage,
                )
            }),
        )
        .routes(
            routes!(ai::routes::test_tenant_config)
                .layer(require_permission(Permission::AiAgentManage)),
        )
        .routes(
            routes!(ai::routes::list_tenant_usage)
                .layer(require_permission(Permission::AiAgentView)),
        )
        .routes(
            routes!(ai::routes::tenant_usage_summary)
                .layer(require_permission(Permission::AiAgentView)),
        )
        .routes(
            routes!(ai::routes::get_tenant_usage_detail)
                .layer(require_permission(Permission::AiAgentManage)),
        )
        .routes(
            routes!(
                ai::agent_routes::get_agent_config,
                ai::agent_routes::put_agent_config
            )
            .map(|_| {
                let get = routing::get(ai::agent_routes::get_agent_config)
                    .route_layer(require_permission(Permission::AiAgentView));
                let put = routing::put(ai::agent_routes::put_agent_config)
                    .route_layer(require_permission(Permission::AiAgentManage));
                get.merge(put)
            }),
        )
        .routes(
            routes!(ai::agent_routes::get_agent_options)
                .layer(require_permission(Permission::AiAgentView)),
        )
        .routes(
            routes!(
                ai::agent_routes::get_agent_avatar,
                ai::agent_routes::put_agent_avatar
            )
            .map(|_| {
                let get = routing::get(ai::agent_routes::get_agent_avatar)
                    .route_layer(require_permission(Permission::AiAgentView));
                let put = routing::put(ai::agent_routes::put_agent_avatar)
                    .route_layer(require_permission(Permission::AiAgentManage));
                get.merge(put)
            }),
        );
    if include_test_routes {
        // Test routes are closures, not function paths, so they cannot use the
        // `routes!()` co-registration macro. They stay on the plain `.route()`
        // passthrough so they register in the live `Router` only — never in
        // the documented `OpenApi` (FR-004).
        router = router
            .route(
                "/test/tenant/events",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::ConversationsView)),
            )
            .route(
                "/test/tenant/escalations/manage",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::ConversationsManage)),
            )
            .route(
                "/test/tenant/escalations/view",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::ConversationsView)),
            )
            .route(
                "/test/tenant/skills/manage",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::MembersManage)),
            )
            .route(
                "/test/tenant/members/{id}",
                routing::patch(|| async { StatusCode::OK })
                    .get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::MembersManage)),
            )
            .route(
                "/test/tenant/conversations/manage",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::ConversationsManage)),
            )
            .route(
                "/test/tenant/members/manage",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::MembersManage)),
            )
            .route(
                "/test/tenant/members/view",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::MembersView)),
            )
            .route(
                "/test/tenant/customers/view",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::CustomersView)),
            )
            .route(
                "/test/tenant/customers/manage",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::CustomersManage)),
            )
            .route(
                "/test/tenant/members/invitations/view",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::MembersView)),
            )
            .route(
                "/test/tenant/members/invitations/manage",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::MembersManage)),
            )
            .route(
                "/test/tenant/settings/manage",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::SettingsManage)),
            )
            .route(
                "/test/tenant/billing/view",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::BillingView)),
            )
            .route(
                "/test/tenant/billing/manage",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::BillingManage)),
            )
            .route(
                "/test/tenant/ai/manage",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::AiAgentManage)),
            )
            .route(
                "/test/tenant/ai/view",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::AiAgentView)),
            );
    }
    router
}

fn api_routes(
    state: &AppState,
    include_test_routes: bool,
    email_sender: Option<Arc<dyn EmailSender>>,
) -> (Router<sqlx::PgPool>, utoipa::openapi::OpenApi) {
    let identity_config = IdentityConfig {
        pool: state.db.clone(),
        environment: state.config.environment.clone(),
        auth_jwt_secret: state.config.auth_jwt_secret.clone(),
        auth_session_ttl_seconds: state.config.auth_session_ttl_seconds,
    };
    let tenancy_config = tenancy::TenancyConfig {
        pool: state.db.clone(),
        is_production: state.config.environment == Environment::Production,
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

    let router: OpenApiRouter<sqlx::PgPool> =
        OpenApiRouter::with_openapi(ApiDoc::openapi()).merge(public_routes());
    let router = router.merge(authenticated_routes());
    let platform = platform_routes(include_test_routes)
        .layer(middleware::from_fn(platform_permission_middleware))
        .layer(middleware::from_fn(authentication_middleware));
    let router = router.merge(platform);
    let tenant = tenant_routes(include_test_routes)
        .layer(from_fn_with_state(
            tenancy_config,
            tenancy::tenant_context_middleware,
        ))
        .layer(middleware::from_fn(authentication_middleware));
    let router = router.merge(tenant);

    let router = router
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
        .layer(Extension(state.ai.clone()))
        .layer(Extension(Arc::new(ai::agent_config::AiAgentStatusAdapter {
            pool: state.db.clone(),
        })
            as Arc<dyn conversations::AiAgentStatus>))
        .layer(from_fn_with_state(
            state.config.clone(),
            csrf_origin_middleware,
        ))
        .layer(from_fn_with_state(identity_config, principal_middleware));

    router.split_for_parts()
}

/// Build the OpenApiRouter that carries the documented paths (no state
/// needed) and return its accumulated `OpenApi`. This is the source of
/// truth for the coverage gate (T034, FR-015) — every route registered
/// through `routes!()` in `public_routes`/`authenticated_routes`/
/// `platform_routes`/`tenant_routes` is documented here by construction.
pub fn documented_openapi(include_test_routes: bool) -> utoipa::openapi::OpenApi {
    let router: OpenApiRouter<sqlx::PgPool> =
        OpenApiRouter::with_openapi(ApiDoc::openapi()).merge(public_routes());
    let router = router.merge(authenticated_routes());
    let router = router.merge(platform_routes(include_test_routes));
    let router = router.merge(tenant_routes(include_test_routes));
    router.into_openapi()
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

/// Whether the API documentation surface (`/swagger-ui` and
/// `/api-docs/openapi.json`) should be exposed for the running environment.
///
/// FR-014: dev/test always expose; production requires explicit opt-in via
/// `APP_DOCS_ENABLED=true`; staging is gated like production (safer default).
pub fn docs_surface_enabled(config: &AppConfig) -> bool {
    match config.environment {
        Environment::Development | Environment::Test => true,
        Environment::Production | Environment::Staging => config.docs_enabled,
    }
}

fn build_app(
    state: AppState,
    include_test_routes: bool,
    email_sender: Option<Arc<dyn EmailSender>>,
) -> Router {
    let config = state.config.clone();
    let (api_router, openapi_doc) = api_routes(&state, include_test_routes, email_sender);
    let expose_docs = docs_surface_enabled(&config);

    let mut router: Router<AppState> = Router::new()
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

    if expose_docs {
        router =
            router.merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", openapi_doc));
    } else {
        // Drop the openapi_doc to satisfy the unused-variable lint; the
        // entire documentation surface (both UI and JSON) is gated off
        // when this branch runs, so nothing must be served.
        drop(openapi_doc);
    }

    router
        .nest("/api/v1", api_router.with_state(state.db.clone()))
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
    use super::{docs_surface_enabled, platform_routes, OpenApiRouter};
    use config::{AppConfig, Environment, LogFormat};
    use sqlx::PgPool;

    #[test]
    fn platform_routes_construction_returns_openapi_router() {
        fn assert_openapi_router(_: OpenApiRouter<PgPool>) {}
        assert_openapi_router(platform_routes(false));
    }

    #[test]
    fn docs_surface_gating_matches_environment() {
        let mut config = AppConfig {
            database_url: String::new(),
            redis_url: String::new(),
            auth_jwt_secret: String::new(),
            auth_session_ttl_seconds: 0,
            port: 0,
            bind_address: String::new(),
            environment: Environment::Development,
            cors_allowed_origins: vec![],
            log_format: LogFormat::Pretty,
            smtp_url: None,
            smtp_from: None,
            public_dashboard_url: String::new(),
            ai_key_encryption_key: None,
            ai_openai_base_url: None,
            ai_anthropic_base_url: None,
            ai_gemini_base_url: None,
            db_max_connections: 0,
            db_acquire_timeout_ms: 0,
            ready_probe_timeout_ms: 0,
            shutdown_grace_seconds: 0,
            docs_enabled: false,
        };
        assert!(docs_surface_enabled(&config));
        config.environment = Environment::Test;
        assert!(docs_surface_enabled(&config));
        config.environment = Environment::Production;
        assert!(!docs_surface_enabled(&config));
        config.docs_enabled = true;
        assert!(docs_surface_enabled(&config));
        config.environment = Environment::Staging;
        assert!(docs_surface_enabled(&config));
        config.docs_enabled = false;
        assert!(!docs_surface_enabled(&config));
    }
}
