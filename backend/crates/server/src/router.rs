use authz::{platform_permission_middleware, require_permission, Permission};
use axum::extract::{DefaultBodyLimit, Request};
use axum::http::{header, header::SET_COOKIE, HeaderName, Method, StatusCode};
use axum::middleware::from_fn_with_state;
use axum::response::{IntoResponse, Response};
use axum::routing::MethodRouter;
use axum::Extension;
use axum::{middleware, routing, Router};
use billing;
use cache::{limiter::RedisRateLimiter, Cache};
use config::{AppConfig, Environment};
use email::{noop::LogEmailSender, smtp::SmtpEmailSender, EmailSender};
use identity::{principal_middleware, IdentityConfig, Principal};
use kernel::{ApiError, RateLimiter, TenantRateLimit};
use observability::health::HealthReport;
use observability::request_id::{request_id_middleware, REQUEST_ID_HEADER};
use observability::trace::trace_middleware;
use observability::{liveness, metrics};
use std::sync::Arc;
use std::time::Duration;
use storage::{ObjectStorage, StorageBackend, StorageKind};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use utoipa::OpenApi;
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;
use utoipa_swagger_ui::SwaggerUi;

use crate::openapi::ApiDoc;
use crate::run_mode::RunMode;
use crate::security_headers;
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
    ApiError::internal_error_from("Handler panicked", payload).into_response()
}

async fn test_panic_handler() -> Response {
    panic!("intentional panic for testing");
}

/// The origin a browser says a request comes from: the `Origin` header, or the origin part of
/// `Referer` when `Origin` is absent. `None` when neither says anything usable.
fn request_origin(headers: &axum::http::HeaderMap) -> Option<String> {
    if let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    {
        return Some(origin.to_owned());
    }
    let referer = headers.get(header::REFERER)?.to_str().ok()?;
    let uri: axum::http::Uri = referer.parse().ok()?;
    Some(format!("{}://{}", uri.scheme_str()?, uri.authority()?))
}

/// A custom header a cross-site form or image can't set without a CORS preflight.
fn has_csrf_header(headers: &axum::http::HeaderMap) -> bool {
    ["x-requested-with", "x-tenant-id"].iter().any(|name| {
        headers
            .get(*name)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| !value.trim().is_empty())
    })
}

fn is_unsafe_method(method: &Method) -> bool {
    !matches!(method, &Method::GET | &Method::HEAD | &Method::OPTIONS)
}

/// The CSRF proof a state-changing request has to carry when its credential is the session cookie:
/// an allowed origin (from `Origin`, else `Referer`) and a custom header. Rejects with 403.
fn require_csrf_proof(config: &AppConfig, headers: &axum::http::HeaderMap) -> Result<(), ApiError> {
    let allowed =
        request_origin(headers).is_some_and(|origin| config.cors_allowed_origins.contains(&origin));
    if !allowed {
        return Err(ApiError::unauthorized("Origin not allowed"));
    }
    if !has_csrf_header(headers) {
        return Err(ApiError::unauthorized("Missing X-Requested-With header"));
    }
    Ok(())
}

/// Runs after the principal is known. A state-changing request authenticated by the session cookie
/// must prove where it came from ([`require_csrf_proof`]): a missing `Origin` and `Referer`, a
/// foreign origin and a missing custom header are all rejected. Other principals (the
/// development-only `X-Dev-User-Id` header carries no cookie to ride on) keep the old rule: an
/// `Origin`, when present, must be allowed.
async fn csrf_origin_middleware(
    axum::extract::State(config): axum::extract::State<Arc<AppConfig>>,
    request: Request,
    next: middleware::Next,
) -> Response {
    if is_unsafe_method(request.method()) && request.extensions().get::<Principal>().is_some() {
        let cookie_session = request
            .extensions()
            .get::<identity::session::SessionClaims>()
            .is_some();
        if cookie_session {
            if let Err(error) = require_csrf_proof(&config, request.headers()) {
                return error.into_response();
            }
        } else if let Some(origin) = request
            .headers()
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok())
        {
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

/// Login and signup start a session, so they run before any principal exists and the check above
/// never sees them (login CSRF: a foreign page signs the victim in as the attacker). They get the
/// same proof unconditionally.
async fn csrf_session_start_middleware(
    Extension(config): Extension<Arc<AppConfig>>,
    request: Request,
    next: middleware::Next,
) -> Response {
    if is_unsafe_method(request.method()) {
        if let Err(error) = require_csrf_proof(&config, request.headers()) {
            return error.into_response();
        }
    }
    next.run(request).await
}

fn widget_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::any())
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_credentials(false)
}

fn widget_routes() -> OpenApiRouter<sqlx::PgPool> {
    OpenApiRouter::new()
        .routes(routes!(widgets::public_routes::get_config))
        .routes(
            routes!(widgets::public_routes::create_session).layer(middleware::from_fn(
                crate::rate_limit::per_ip_creation_limit,
            )),
        )
        .routes(routes!(widgets::public_routes::get_conversation))
        .routes(routes!(widgets::public_routes::create_conversation))
        .routes(routes!(widgets::public_routes::send_message))
        .routes(routes!(feedback::public_routes::submit_feedback))
        .routes(routes!(feedback::public_routes::get_pending_feedback))
        .routes(routes!(widgets::public_events::stream_events))
}

/// Public inbound-webhook intake. Mounted outside the tenant middleware:
/// no auth, no tenant context — the per-connection intake token in the URL
/// is the credential. The 256 KB body cap (FR-014) is enforced here as the
/// outermost layer on this router.
fn hooks_routes() -> OpenApiRouter<sqlx::PgPool> {
    OpenApiRouter::new()
        .routes(routes!(integrations::webhook::receive_webhook))
        .layer(DefaultBodyLimit::max(256 * 1024))
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
        HeaderName::from_static("x-requested-with"),
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
        (status = 202, description = "The password was right and the user has two-factor authentication: no session yet. Sets a short-lived `app_mfa_pending` cookie; complete the sign-in with `POST /auth/mfa/verify`.", body = identity::mfa_routes::MfaRequiredResponse),
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
        Ok(identity::routes::LoginOutcome::Session(success)) => success,
        Ok(identity::routes::LoginOutcome::MfaRequired {
            pending_cookie,
            user_id,
        }) => {
            // The password was right but there is no session yet: the second factor comes next.
            let resource = user_id.to_string();
            identity::audit::record(
                &pool,
                "auth.mfa_challenged",
                Some(user_id),
                Some(&resource),
                &serde_json::json!({}),
            )
            .await;
            return (
                StatusCode::ACCEPTED,
                [(SET_COOKIE, pending_cookie)],
                axum::Json(identity::mfa_routes::MfaRequiredResponse { mfa_required: true }),
            )
                .into_response();
        }
        Err(identity::routes::LoginError::Validation) => {
            return ApiError::validation_failed("Email and password are required").into_response();
        }
        Err(identity::routes::LoginError::InvalidCredentials) => {
            return ApiError::unauthenticated("Invalid email or password").into_response();
        }
        Err(identity::routes::LoginError::Internal(message)) => {
            return ApiError::internal_error_from("Login failed", message).into_response();
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

/// The second sign-in step.
#[utoipa::path(
    post,
    path = "/auth/mfa/verify",
    tag = "auth",
    operation_id = "auth_mfa_verify",
    summary = "Complete sign-in with a second factor",
    description = "Takes an authenticator code or a recovery code, together with the \
                  `app_mfa_pending` cookie that `POST /auth/login` set. On success it starts the \
                  session (the `app_session` cookie records the second factor), clears the \
                  challenge and returns the same profile body as login. A code is accepted once; \
                  five wrong codes lock the account's second factor for fifteen minutes. The \
                  challenge lasts five minutes. Public: no `app_session` cookie is needed.",
    request_body = identity::mfa_routes::VerifyRequest,
    responses(
        (status = 200, description = "Signed in. Returns the user profile and sets the `app_session` cookie.", body = serde_json::Value),
        (status = 400, description = "Neither or both of `code` and `recoveryCode`.", body = kernel::ErrorEnvelope),
        (status = 401, description = "No valid challenge (missing, expired or already used): sign in again.", body = kernel::ErrorEnvelope),
        (status = 422, description = "The code is not valid.", body = kernel::ErrorEnvelope),
        (status = 429, description = "Too many attempts, or the second factor is locked.", body = kernel::ErrorEnvelope),
    ),
    security(()),
)]
async fn mfa_verify(
    axum::extract::State(pool): axum::extract::State<sqlx::PgPool>,
    Extension(config): Extension<Arc<AppConfig>>,
    headers: axum::http::HeaderMap,
    kernel::ApiJson(payload): kernel::ApiJson<identity::mfa_routes::VerifyRequest>,
) -> Response {
    let again = || ApiError::unauthenticated("Sign in again").into_response();
    let Some(token) = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(identity::session::pending_token_from_cookie_header)
    else {
        return again();
    };
    let Ok(challenge) = identity::session::validate_pending(&config.auth_jwt_secret, token) else {
        return again();
    };
    // A challenge that already completed a sign-in cannot start another.
    let used: Result<bool, sqlx::Error> =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM revoked_sessions WHERE jti = $1)")
            .bind(challenge.jti)
            .fetch_one(&pool)
            .await;
    match used {
        Ok(false) => {}
        Ok(true) => return again(),
        Err(e) => return ApiError::internal_error_from("Sign-in failed", e).into_response(),
    }

    let credential = match (payload.code, payload.recovery_code) {
        (Some(code), None) => identity::mfa::Credential::Code(code),
        (None, Some(code)) => identity::mfa::Credential::RecoveryCode(code),
        _ => {
            return ApiError::validation_failed("Send either a code or a recovery code")
                .into_response()
        }
    };
    let keys = match identity::mfa::MfaKeys::from_config(&pool, &config) {
        Ok(k) => k,
        Err(e) => {
            return ApiError::internal_error_from("Two-factor authentication is not configured", e)
                .into_response()
        }
    };
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let method = match identity::mfa::verify(&pool, &keys, challenge.sub, &credential, now).await {
        Ok(method) => method,
        Err(error) => return identity::mfa_routes::error_response(error),
    };

    type UserRow = (uuid::Uuid, String, String, Option<String>);
    let row: Result<Option<UserRow>, sqlx::Error> = sqlx::query_as(
        "SELECT id, email, display_name, platform_role FROM users \
             WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(challenge.sub)
    .fetch_optional(&pool)
    .await;
    let principal = match row {
        Ok(Some((id, email, display_name, role))) => {
            match identity::principal_from_row(id, email, display_name, role) {
                Some(p) => p,
                None => return again(),
            }
        }
        Ok(None) => return again(),
        Err(e) => return ApiError::internal_error_from("Sign-in failed", e).into_response(),
    };
    let (jwt, _, _) = match identity::session::issue_session(
        &config.auth_jwt_secret,
        config.auth_session_ttl_seconds,
        principal.user_id,
        identity::session::SessionAuth::second_factor(method.amr()),
    ) {
        Ok(issued) => issued,
        Err(e) => {
            return ApiError::internal_error_from("Session token issuance failed", e)
                .into_response()
        }
    };
    identity::mfa_routes::revoke(&pool, principal.user_id, challenge.jti, challenge.exp).await;
    let user_id = principal.user_id;
    let response = match tenancy::routes::build_me_response(
        &pool,
        principal,
        config.environment == Environment::Production,
    )
    .await
    {
        Ok(response) => response,
        Err(error) => return error.into_response(),
    };
    identity::audit::login_succeeded(&pool, user_id).await;

    let mut reply = axum::Json(response).into_response();
    let cookies = reply.headers_mut();
    if let Ok(value) =
        identity::session::build_session_cookie(&jwt, config.auth_session_ttl_seconds).parse()
    {
        cookies.append(SET_COOKIE, value);
    }
    if let Ok(value) = identity::session::clear_pending_cookie().parse() {
        cookies.append(SET_COOKIE, value);
    }
    reply
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
        .routes(
            routes!(login)
                .layer(middleware::from_fn(crate::rate_limit::login_rate_limit))
                .layer(middleware::from_fn(csrf_session_start_middleware)),
        )
        .routes(
            routes!(tenancy::signup::signup)
                .layer(middleware::from_fn(
                    crate::rate_limit::per_ip_creation_limit,
                ))
                .layer(middleware::from_fn(csrf_session_start_middleware)),
        )
        .routes(
            routes!(mfa_verify)
                .layer(middleware::from_fn(crate::rate_limit::mfa_rate_limit))
                .layer(middleware::from_fn(csrf_session_start_middleware)),
        )
        .routes(routes!(identity::routes::logout))
        .routes(routes!(tenancy::invitations::preview_invitation))
        .routes(routes!(tenancy::invitations::accept_invitation))
}

fn authenticated_routes() -> OpenApiRouter<sqlx::PgPool> {
    OpenApiRouter::new()
        .routes(routes!(tenancy::routes::me))
        .routes(
            routes!(identity::mfa_routes::begin_enrolment)
                .layer(middleware::from_fn(crate::rate_limit::mfa_rate_limit)),
        )
        .routes(routes!(identity::mfa_routes::confirm_enrolment))
        .routes(routes!(identity::mfa_routes::status))
        .routes(
            routes!(identity::mfa_routes::step_up)
                .layer(middleware::from_fn(crate::rate_limit::mfa_rate_limit)),
        )
        .routes(
            routes!(identity::mfa_routes::regenerate_recovery_codes)
                .layer(middleware::from_fn(identity::step_up::require_step_up)),
        )
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
                .layer(middleware::from_fn(identity::step_up::require_step_up))
                .layer(require_permission(Permission::PlatformTenantsSwitch)),
        )
        .routes(
            routes!(crate::mfa_admin::reset_user_mfa)
                .layer(middleware::from_fn(identity::step_up::require_step_up))
                .layer(require_permission(Permission::PlatformAdmin)),
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
                    routing::put(ai::routes::put_platform_credential)
                        .layer(middleware::from_fn(identity::step_up::require_step_up)),
                    Permission::PlatformAdmin,
                    routing::delete(ai::routes::delete_platform_credential)
                        .layer(middleware::from_fn(identity::step_up::require_step_up)),
                    Permission::PlatformAdmin,
                )
            }),
        )
        .routes(
            routes!(ai::routes::test_platform_config)
                .layer(require_permission(Permission::PlatformAdmin)),
        )
        // Platform audit logs (spec 026)
        .routes(
            routes!(audit::routes::list_platform_audit_logs)
                .layer(require_permission(Permission::PlatformAuditView)),
        )
        // Platform billing plan catalog — GET+POST share the same path (T043)
        .routes(
            routes!(billing::routes::list_plans, billing::routes::create_plan).map(|_| {
                merge_with_permissions(
                    routing::get(billing::routes::list_plans),
                    Permission::PlatformBillingView,
                    routing::post(billing::routes::create_plan),
                    Permission::PlatformBillingManage,
                )
            }),
        )
        // Platform billing plan update (T043)
        .routes(
            routes!(billing::routes::update_plan)
                .layer(require_permission(Permission::PlatformBillingManage)),
        )
        // Platform billing subscriptions (T043)
        .routes(
            routes!(billing::routes::list_subscriptions)
                .layer(require_permission(Permission::PlatformBillingView)),
        )
        // Platform billing tenant subscription set (T043)
        .routes(
            routes!(billing::routes::put_subscription)
                .layer(require_permission(Permission::PlatformBillingManage)),
        )
        // Platform analytics summary (032)
        .routes(
            routes!(platform::routes::get_platform_analytics_summary)
                .layer(require_permission(Permission::PlatformAnalyticsView)),
        )
        // Platform system health (032)
        .routes(
            routes!(platform::routes::get_platform_health)
                .layer(require_permission(Permission::PlatformDiagnosticsView)),
        )
        // Platform AI provider status (032)
        .routes(
            routes!(platform::routes::get_platform_ai_provider_status)
                .layer(require_permission(Permission::PlatformDiagnosticsView)),
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
                "/test/platform/billing/manage",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::PlatformBillingManage)),
            )
            .route(
                "/test/platform/diagnostics/view",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::PlatformDiagnosticsView)),
            )
            .route(
                "/test/platform/analytics/view",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::PlatformAnalyticsView)),
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
        // Mounted under /api/v1 by the parent router (the path previously
        // repeated the prefix, so the endpoint was unreachable). Authz is the
        // ConversationsView layer the handler's docs rely on.
        .route(
            "/tenant/conversations/{id}/summary",
            routing::post(ai::summary::handle_summary)
                .layer(require_permission(Permission::ConversationsView)),
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
                .layer(middleware::from_fn(identity::step_up::require_step_up))
                .layer(require_permission(Permission::MembersManage)),
        )
        .routes(
            routes!(crate::mfa_admin::reset_member_mfa)
                .layer(middleware::from_fn(identity::step_up::require_step_up))
                .layer(require_permission(Permission::OwnerAssign)),
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
                    // An invitation names a role, so it grants privilege like a role change does.
                    routing::post(tenancy::invitations::create_invitation)
                        .layer(middleware::from_fn(identity::step_up::require_step_up)),
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
                    routing::put(ai::routes::put_tenant_credential)
                        .layer(middleware::from_fn(identity::step_up::require_step_up)),
                    Permission::AiAgentManage,
                    routing::delete(ai::routes::delete_tenant_credential)
                        .layer(middleware::from_fn(identity::step_up::require_step_up)),
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
            routes!(tools::routes::tool_activity)
                .layer(require_permission(Permission::ConversationsView)),
        )
        .routes(
            routes!(crate::handlers::decide_tool_request)
                .layer(require_permission(Permission::ConversationsManage)),
        )
        .routes(
            routes!(tools::routes::list_pending_tool_requests)
                .layer(require_permission(Permission::ConversationsView)),
        )
        // T057: Tool settings
        .routes(
            // Read = Manager+ (spec 022); mutations below stay ai_agent.manage.
            routes!(tools::routes::list_tools).layer(require_permission(Permission::ToolsView)),
        )
        .routes(
            routes!(tools::routes::update_builtin_policy)
                .layer(require_permission(Permission::AiAgentManage)),
        )
        .routes(
            routes!(tools::routes::create_tenant_tool)
                .layer(middleware::from_fn(identity::step_up::require_step_up))
                .layer(require_permission(Permission::AiAgentManage)),
        )
        .routes(
            routes!(tools::routes::update_tenant_tool_route)
                .layer(middleware::from_fn(identity::step_up::require_step_up))
                .layer(require_permission(Permission::AiAgentManage)),
        )
        .routes(
            routes!(tools::routes::delete_tenant_tool_route)
                .layer(middleware::from_fn(identity::step_up::require_step_up))
                .layer(require_permission(Permission::AiAgentManage)),
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
            routes!(
                ai::prompt_routes::get_prompt_bootstrap,
                ai::prompt_routes::put_prompt
            )
            .map(|_| {
                let get = routing::get(ai::prompt_routes::get_prompt_bootstrap)
                    .route_layer(require_permission(Permission::AiAgentView));
                let put = routing::put(ai::prompt_routes::put_prompt)
                    .route_layer(require_permission(Permission::AiAgentManage));
                get.merge(put)
            }),
        )
        .routes(
            routes!(ai::prompt_routes::list_prompt_versions)
                .layer(require_permission(Permission::AiAgentView)),
        )
        .routes(
            routes!(ai::prompt_routes::get_prompt_version)
                .layer(require_permission(Permission::AiAgentView)),
        )
        .routes(
            routes!(ai::prompt_routes::restore_prompt_version)
                .layer(require_permission(Permission::AiAgentManage)),
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
        )
        .routes(
            routes!(
                knowledge::routes::list_items,
                knowledge::routes::create_item
            )
            .map(|_| {
                merge_with_permissions(
                    routing::get(knowledge::routes::list_items),
                    Permission::KnowledgeBaseView,
                    routing::post(knowledge::routes::create_item),
                    Permission::KnowledgeBaseManage,
                )
            }),
        )
        .routes(
            routes!(knowledge::routes::get_item, knowledge::routes::update_item).map(|_| {
                merge_with_permissions(
                    routing::get(knowledge::routes::get_item),
                    Permission::KnowledgeBaseView,
                    routing::patch(knowledge::routes::update_item),
                    Permission::KnowledgeBaseManage,
                )
            }),
        )
        // T025: Knowledge item status
        .routes(
            routes!(knowledge::routes::set_item_status)
                .layer(require_permission(Permission::KnowledgeBaseManage)),
        )
        // T042: Knowledge item reindex
        .routes(
            routes!(knowledge::routes::reindex_item)
                .layer(require_permission(Permission::KnowledgeBaseManage)),
        )
        // T031: Knowledge document upload and file download
        .routes(
            routes!(knowledge::routes::upload_document)
                .layer(DefaultBodyLimit::max(25 * 1024 * 1024))
                .layer(require_permission(Permission::KnowledgeBaseManage)),
        )
        .routes(
            routes!(knowledge::routes::download_file)
                .layer(require_permission(Permission::KnowledgeBaseView)),
        )
        // T035: Knowledge categories CRUD
        .routes(
            routes!(knowledge::routes::list_knowledge_categories)
                .layer(require_permission(Permission::KnowledgeBaseView)),
        )
        .routes(
            routes!(knowledge::routes::create_knowledge_category)
                .layer(require_permission(Permission::KnowledgeBaseManage)),
        )
        .routes(
            routes!(knowledge::routes::rename_knowledge_category)
                .layer(require_permission(Permission::KnowledgeBaseManage)),
        )
        .routes(
            routes!(knowledge::routes::delete_knowledge_category)
                .layer(require_permission(Permission::KnowledgeBaseManage)),
        )
        // T040: Feedback summary
        .routes(
            routes!(feedback::tenant_routes::get_feedback_summary)
                .layer(require_permission(Permission::ConversationsView)),
        )
        // Analytics summary (spec 025)
        .routes(
            routes!(analytics::routes::get_analytics_summary)
                .layer(require_permission(Permission::AnalyticsView)),
        )
        // Analytics timeseries (spec 025)
        .routes(
            routes!(analytics::routes::get_analytics_timeseries)
                .layer(require_permission(Permission::AnalyticsView)),
        )
        // Audit logs (spec 026)
        .routes(
            routes!(audit::routes::list_tenant_audit_logs)
                .layer(require_permission(Permission::AuditView)),
        )
        // Billing overview (spec 031)
        .routes(
            routes!(billing::routes::get_billing_overview)
                .layer(require_permission(Permission::BillingView)),
        )
        // Integrations (spec 028)
        .routes(
            routes!(integrations::routes::list_integrations)
                .layer(require_permission(Permission::IntegrationsView)),
        )
        .routes(
            routes!(integrations::routes::get_integration)
                .layer(require_permission(Permission::IntegrationsView)),
        )
        .routes(
            routes!(integrations::routes::connect_integration)
                .layer(middleware::from_fn(identity::step_up::require_step_up))
                .layer(require_permission(Permission::IntegrationsManage)),
        )
        .routes(
            routes!(integrations::routes::update_integration_config)
                .layer(middleware::from_fn(identity::step_up::require_step_up))
                .layer(require_permission(Permission::IntegrationsManage)),
        )
        .routes(
            routes!(integrations::routes::disconnect_integration)
                .layer(middleware::from_fn(identity::step_up::require_step_up))
                .layer(require_permission(Permission::IntegrationsManage)),
        )
        .routes(
            routes!(integrations::routes::list_integration_events)
                .layer(require_permission(Permission::IntegrationsView)),
        )
        // Notifications (spec 027) — no permission gate (FR-012a)
        .routes(routes!(notifications::routes::list_notifications))
        .routes(routes!(
            notifications::routes::get_unread_notification_count
        ))
        .routes(routes!(notifications::routes::mark_notification_read))
        .routes(routes!(notifications::routes::mark_all_notifications_read))
        // ── Widgets admin CRUD ─────────────────────────────────────────────
        .routes(
            routes!(
                widgets::admin_routes::list_instances,
                widgets::admin_routes::create_instance
            )
            .map(|_| {
                merge_with_permissions(
                    routing::get(widgets::admin_routes::list_instances),
                    Permission::WidgetsView,
                    routing::post(widgets::admin_routes::create_instance),
                    Permission::WidgetsManage,
                )
            }),
        )
        .routes(
            routes!(
                widgets::admin_routes::get_instance,
                widgets::admin_routes::update_instance
            )
            .map(|_| {
                merge_with_permissions(
                    routing::get(widgets::admin_routes::get_instance),
                    Permission::WidgetsView,
                    routing::put(widgets::admin_routes::update_instance),
                    Permission::WidgetsManage,
                )
            }),
        )
        .routes(
            routes!(widgets::admin_routes::delete_instance)
                .layer(require_permission(Permission::WidgetsManage)),
        )
        .routes(
            routes!(widgets::admin_routes::get_snippet)
                .layer(require_permission(Permission::WidgetsView)),
        )
        // WhatsApp attachment download (spec 029)
        .merge(whatsapp::routes::tenant_router().into());
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
            )
            .route(
                "/test/tenant/widgets/view",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::WidgetsView)),
            )
            .route(
                "/test/tenant/widgets/manage",
                routing::get(|| async { StatusCode::OK })
                    .route_layer(require_permission(Permission::WidgetsManage)),
            );
    }
    router
}

fn api_routes(
    state: &AppState,
    include_test_routes: bool,
    email_sender: Option<Arc<dyn EmailSender>>,
    storage: Option<Arc<dyn ObjectStorage>>,
    mode: RunMode,
) -> (
    Router<sqlx::PgPool>,
    utoipa::openapi::OpenApi,
    Arc<dyn RateLimiter>,
) {
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

    let (fallback_storage, storage_backend) = if let Some(ref s) = storage {
        (
            s.clone(),
            StorageBackend {
                kind: StorageKind::InMemory,
                s3_configured: false,
            },
        )
    } else {
        // `build_app` is sync but is called from inside `#[tokio::main]` (production) and
        // from `#[tokio::test]` (the suites), so `Handle::current().block_on(...)` panics
        // with "Cannot start a runtime from within a runtime". Run the async selection on
        // a dedicated thread with its own runtime instead.
        let s3 = state.config.s3.clone();
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .expect("failed to build storage-selection runtime")
                .block_on(storage::select_storage(s3.as_ref()))
        })
        .join()
        .expect("storage-selection thread panicked")
    };
    let storage: Arc<dyn ObjectStorage> = storage.unwrap_or(fallback_storage);

    // Widget public routes — no authentication, permissive CORS, rate-limited.
    // The same `rate_store` Arc is also injected onto the integration hooks
    // router (FR-014 per-connection rate limit), so the two share budget.
    let cache = Cache::new(&state.config.redis_url)
        .expect("Redis cache for rate limiter — configure APP_REDIS_URL");
    let rate_store: Arc<dyn RateLimiter> = Arc::new(RedisRateLimiter::new(
        cache,
        state.config.rate_limit_fail_mode.clone(),
    ));
    let login_store = rate_store.clone();
    let whatsapp_store = rate_store.clone();
    let tenant_limit: Arc<dyn TenantRateLimit> =
        Arc::new(crate::rate_limit::ServerTenantRateLimit {
            pool: state.db.clone(),
            resolver: state.budget_resolver.clone(),
            enabled: state.config.rate_limit_enabled,
        });
    let widget_router = widget_routes()
        .layer(Extension(rate_store.clone()))
        .layer(Extension(tenant_limit.clone()))
        .layer(widget_cors_layer());
    let hooks_router = hooks_routes()
        .layer(Extension(rate_store))
        .layer(Extension(tenant_limit));

    // Dashboard, platform and auth routes (everything except widget and hooks) get Scope B
    // headers. A `gateway` process mounts none of them, so a public request can't reach one by
    // path (ADR-010 section 2).
    let mut router: OpenApiRouter<sqlx::PgPool> = OpenApiRouter::with_openapi(ApiDoc::openapi());
    if mode.serves_api() {
        let mut protected: OpenApiRouter<sqlx::PgPool> =
            OpenApiRouter::with_openapi(ApiDoc::openapi())
                .merge(public_routes())
                .merge(authenticated_routes());
        let platform = platform_routes(include_test_routes)
            .layer(middleware::from_fn(platform_permission_middleware))
            .layer(middleware::from_fn(
                identity::step_up::enforce_second_factor,
            ))
            .layer(middleware::from_fn(authentication_middleware));
        protected = protected.merge(platform);
        let tenant = tenant_routes(include_test_routes)
            .layer(from_fn_with_state(
                tenancy_config,
                tenancy::tenant_context_middleware,
            ))
            .layer(middleware::from_fn(
                identity::step_up::enforce_second_factor,
            ))
            .layer(middleware::from_fn(authentication_middleware));
        protected = protected.merge(tenant);

        if state.config.security_headers_enabled {
            for layer in security_headers::scope_b_layers(&state.config) {
                protected = protected.layer(layer);
            }
        }
        router = router.merge(protected);
    }
    if mode.serves_gateway() {
        router = router.merge(widget_router).merge(hooks_router);
    }

    let mut router = router
        .fallback(|request: Request| async move {
            let request_id = request
                .headers()
                .get(&REQUEST_ID_HEADER)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("unknown");
            ApiError::not_found("Route not found").with_request_id(request_id)
        })
        .layer(Extension(login_store))
        .layer(Extension(email_sender))
        .layer(Extension(storage.clone()))
        .layer(Extension(state.config.clone()))
        .layer(Extension(state.escalations.clone()))
        .layer(Extension(state.ai.clone()))
        .layer(Extension(state.health_checks.clone()))
        .layer(Extension(state.ai.resilience()))
        .layer(Extension(storage_backend))
        .layer(Extension(Arc::new(ai::agent_config::AiAgentStatusAdapter {
            pool: state.db.clone(),
        })
            as Arc<dyn conversations::AiAgentStatus>));
    // Cookie sessions and their CSRF check belong to the dashboard side only. The gateway has no
    // session to resolve and holds no signing key for one.
    if mode.serves_api() {
        router = router
            .layer(from_fn_with_state(
                state.config.clone(),
                csrf_origin_middleware,
            ))
            .layer(from_fn_with_state(identity_config, principal_middleware));
    }

    let (router, doc) = router.split_for_parts();
    (router, doc, whatsapp_store)
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
    let router = router.merge(widget_routes());
    let router = router.merge(hooks_routes());
    router.into_openapi()
}

/// The documented paths a run mode mounts under `/api/v1`, built from the same route sets as
/// [`documented_openapi`]. The `api` and `gateway` sets must never overlap (ADR-010 section 2); a
/// test asserts it.
pub fn documented_paths(
    mode: RunMode,
    include_test_routes: bool,
) -> std::collections::BTreeSet<String> {
    let mut router: OpenApiRouter<sqlx::PgPool> = OpenApiRouter::with_openapi(ApiDoc::openapi());
    if mode.serves_api() {
        router = router
            .merge(public_routes())
            .merge(authenticated_routes())
            .merge(platform_routes(include_test_routes))
            .merge(tenant_routes(include_test_routes));
    }
    if mode.serves_gateway() {
        router = router.merge(widget_routes()).merge(hooks_routes());
    }
    router.into_openapi().paths.paths.keys().cloned().collect()
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
    storage: Option<Arc<dyn ObjectStorage>>,
    mode: RunMode,
) -> Router {
    let config = state.config.clone();
    // A `worker` process serves no API or gateway routes, so it builds none of them (and needs no
    // rate-limiter Redis client or object storage selection for them).
    let serves_http_routes = mode.serves_api() || mode.serves_gateway();
    let (api_router, openapi_doc, whatsapp_store) = if serves_http_routes {
        let (router, doc, store) =
            api_routes(&state, include_test_routes, email_sender, storage, mode);
        (Some(router), Some(doc), Some(store))
    } else {
        (None, None, None)
    };
    let expose_docs = mode.serves_api() && docs_surface_enabled(&config);

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

    if let (true, Some(openapi_doc)) = (expose_docs, openapi_doc) {
        router =
            router.merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", openapi_doc));
    }

    // WhatsApp webhook (spec 029) — mounted outside tenant auth, token-based. It is a public
    // channel webhook, so it belongs to the gateway.
    if let (true, Some(whatsapp_store)) = (mode.serves_gateway(), whatsapp_store) {
        let master_key = integrations::crypto::vault_from_config(&state.db, &state.config)
            .ok()
            .map(Arc::new);
        if let Some(ref mk) = master_key {
            let whatsapp_webhook = whatsapp::routes::public_router()
                .layer(Extension(mk.clone()))
                .layer(Extension(whatsapp_store))
                .layer(Extension(state.config.clone()))
                .layer(DefaultBodyLimit::max(256 * 1024))
                .with_state(state.db.clone());
            router = router.merge(whatsapp_webhook);
        } else {
            tracing::warn!("integration_secrets_key not configured — whatsapp webhook disabled");
        }
    }

    if let Some(api_router) = api_router {
        router = router.nest("/api/v1", api_router.with_state(state.db.clone()));
    }

    let mut app_router = router
        .fallback(|request: Request| async move {
            let request_id = request
                .headers()
                .get(&REQUEST_ID_HEADER)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("unknown");
            ApiError::not_found("Route not found").with_request_id(request_id)
        })
        .layer(CatchPanicLayer::custom(panic_handler))
        .layer(middleware::from_fn(
            crate::internal_error::internal_error_middleware,
        ))
        .layer(middleware::from_fn(trace_middleware))
        .layer(middleware::from_fn(request_id_middleware))
        .layer(cors_layer(&config))
        .layer(middleware::from_fn(
            observability::metrics::metrics_middleware,
        ));

    if config.security_headers_enabled {
        for layer in security_headers::scope_a_layers(&config) {
            app_router = app_router.layer(layer);
        }
    }

    app_router.with_state(state)
}

/// The router for one run mode, as `main` serves it. `app_with_email_sender` is the `all` mode.
pub fn app_for_mode(
    state: AppState,
    mode: RunMode,
    email_sender: Arc<dyn EmailSender>,
    storage: Arc<dyn ObjectStorage>,
) -> Router {
    build_app(state, false, Some(email_sender), Some(storage), mode)
}

/// Same as [`app_for_mode`] with the test-only routes, for the route-set tests.
pub fn app_for_mode_with_test_routes(state: AppState, mode: RunMode) -> Router {
    build_app(state, true, None, None, mode)
}

pub fn app(state: AppState) -> Router {
    build_app(state, false, None, None, RunMode::All)
}

pub fn app_with_email_sender(
    state: AppState,
    email_sender: Arc<dyn EmailSender>,
    storage: Option<Arc<dyn ObjectStorage>>,
) -> Router {
    build_app(state, false, Some(email_sender), storage, RunMode::All)
}

pub fn app_with_test_routes(state: AppState) -> Router {
    build_app(state, true, None, None, RunMode::All)
}

pub fn app_with_test_routes_and_email_sender(
    state: AppState,
    email_sender: Arc<dyn EmailSender>,
) -> Router {
    build_app(state, true, Some(email_sender), None, RunMode::All)
}

pub fn app_with_storage(state: AppState, storage: Arc<dyn ObjectStorage>) -> Router {
    build_app(state, false, None, Some(storage), RunMode::All)
}

pub fn app_with_test_routes_and_storage(
    state: AppState,
    storage: Arc<dyn ObjectStorage>,
) -> Router {
    build_app(state, true, None, Some(storage), RunMode::All)
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
            integration_secrets_key: None,
            secrets_root_key: None,
            secrets_root_key_previous: None,
            auth_mfa_required: false,
            ai_openai_base_url: None,
            ai_anthropic_base_url: None,
            ai_gemini_base_url: None,
            s3: None,
            db_max_connections: 0,
            db_acquire_timeout_ms: 0,
            ready_probe_timeout_ms: 0,
            shutdown_grace_seconds: 0,
            docs_enabled: false,
            rate_limit_enabled: true,
            rate_limit_login_max: 10,
            rate_limit_login_window_secs: 60,
            rate_limit_max_override: 10000,
            rate_limit_fail_mode: "auth_closed".into(),
            security_headers_enabled: true,
            csp_policy:
                "default-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'"
                    .into(),
            csp_report_only: true,
            hsts_max_age: 31536000,
            otlp_endpoint: None,
            trace_sample_ratio: 1.0,
            sentry_dsn: None,
            release_sha: None,
            trusted_proxy: false,
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
