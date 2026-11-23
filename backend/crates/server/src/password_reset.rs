use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use axum::{Extension, Json};
use config::AppConfig;
use identity::password_reset::{self, ResetError};
use kernel::{ApiError, ApiJson, Budget, Decision, ErrorDetail, RateLimiter, Scope};
use observability::request_id::REQUEST_ID_HEADER;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ForgotPasswordRequest {
    #[schema(format = Email)]
    email: String,
}

#[derive(Serialize, ToSchema)]
pub struct ForgotPasswordResponse {
    message: &'static str,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ResetPasswordRequest {
    #[schema(write_only)]
    token: String,
    #[schema(write_only)]
    password: String,
}

#[utoipa::path(
    post,
    path = "/auth/password/forgot",
    tag = "auth",
    operation_id = "auth_forgot_password",
    summary = "Request a password reset link",
    request_body = ForgotPasswordRequest,
    responses(
        (status = 202, description = "Request accepted", body = ForgotPasswordResponse),
        (status = 400, description = "Invalid request", body = kernel::ErrorEnvelope),
        (status = 403, description = "Missing or invalid request origin proof", body = kernel::ErrorEnvelope),
        (status = 422, description = "Invalid email", body = kernel::ErrorEnvelope),
        (status = 429, description = "Too many requests", body = kernel::ErrorEnvelope),
        (status = 500, description = "Request could not be queued", body = kernel::ErrorEnvelope),
        (status = 503, description = "Rate limiter unavailable", body = kernel::ErrorEnvelope),
    ),
    security(()),
)]
pub async fn forgot(
    State(pool): State<PgPool>,
    Extension(config): Extension<Arc<AppConfig>>,
    Extension(store): Extension<Arc<dyn RateLimiter>>,
    Extension(client): Extension<ClientAddress>,
    headers: HeaderMap,
    ApiJson(payload): ApiJson<ForgotPasswordRequest>,
) -> Result<(StatusCode, Json<ForgotPasswordResponse>), ApiError> {
    let email = password_reset::normalize_email(&payload.email).ok_or_else(|| {
        ApiError::unprocessable_entity("Validation failed").with_details(vec![ErrorDetail {
            field: "email".into(),
            code: "invalid_email".into(),
            message: "Enter a valid email address".into(),
        }])
    })?;
    if config.rate_limit_enabled {
        check_limit(
            store.as_ref(),
            Scope::PasswordForgotEmail,
            &password_reset::email_rate_limit_key(&email),
            config.password_reset_limits.forgot_email_max,
            config.password_reset_limits.window_secs,
        )
        .await?;
    }
    let trace_id = headers
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .unwrap_or_else(observability::request_id::generate);

    password_reset::enqueue(&pool, &email, client.0, &trace_id)
        .await
        .map_err(|error| {
            ApiError::internal_error_from("Password reset queue write failed", error)
        })?;
    Ok((
        StatusCode::ACCEPTED,
        Json(ForgotPasswordResponse {
            message: "If an account exists for this email, we will send a password reset link.",
        }),
    ))
}

#[utoipa::path(
    post,
    path = "/auth/password/reset",
    tag = "auth",
    operation_id = "auth_reset_password",
    summary = "Reset a password using a one-time link",
    request_body = ResetPasswordRequest,
    responses(
        (status = 204, description = "Password changed and sessions revoked"),
        (status = 400, description = "Invalid, expired, or already used token", body = kernel::ErrorEnvelope),
        (status = 403, description = "Missing or invalid request origin proof", body = kernel::ErrorEnvelope),
        (status = 422, description = "Invalid password", body = kernel::ErrorEnvelope),
        (status = 429, description = "Too many requests", body = kernel::ErrorEnvelope),
        (status = 500, description = "Password could not be reset", body = kernel::ErrorEnvelope),
        (status = 503, description = "Rate limiter unavailable", body = kernel::ErrorEnvelope),
    ),
    security(()),
)]
pub async fn reset(
    State(pool): State<PgPool>,
    ApiJson(payload): ApiJson<ResetPasswordRequest>,
) -> Result<StatusCode, ApiError> {
    password_reset::reset(&pool, &payload.token, payload.password)
        .await
        .map_err(|error| match error {
            ResetError::InvalidToken => ApiError::new_with_code(
                StatusCode::BAD_REQUEST,
                "reset_token_invalid",
                "The password reset link is invalid or expired.",
            ),
            ResetError::InvalidPassword(error) => {
                ApiError::unprocessable_entity("Validation failed").with_details(vec![error])
            }
            ResetError::Database(error) => {
                ApiError::internal_error_from("Password reset transaction failed", error)
            }
            ResetError::Hash(error) => {
                ApiError::internal_error_from("Password hashing failed", error)
            }
            ResetError::HashTask(error) => {
                ApiError::internal_error_from("Password hashing task failed", error)
            }
        })?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Clone, Copy)]
pub struct ClientAddress(Option<IpAddr>);

pub async fn forgot_ip_limit(
    Extension(store): Extension<Arc<dyn RateLimiter>>,
    Extension(config): Extension<Arc<AppConfig>>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    limit_by_ip(
        Scope::PasswordForgotIp,
        config.password_reset_limits.forgot_ip_max,
        &config,
        store.as_ref(),
        request,
        next,
    )
    .await
}

pub async fn reset_ip_limit(
    Extension(store): Extension<Arc<dyn RateLimiter>>,
    Extension(config): Extension<Arc<AppConfig>>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    limit_by_ip(
        Scope::PasswordResetIp,
        config.password_reset_limits.reset_ip_max,
        &config,
        store.as_ref(),
        request,
        next,
    )
    .await
}

async fn limit_by_ip(
    scope: Scope,
    max_requests: u32,
    config: &AppConfig,
    store: &dyn RateLimiter,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let ip = crate::rate_limit::client_ip(&request, config.trusted_proxy);
    if config.rate_limit_enabled {
        let identifier = ip.map(|ip| ip.to_string());
        check_limit(
            store,
            scope,
            identifier.as_deref().unwrap_or("unknown"),
            max_requests,
            config.password_reset_limits.window_secs,
        )
        .await?;
    }
    request.extensions_mut().insert(ClientAddress(ip));
    Ok(next.run(request).await)
}

async fn check_limit(
    store: &dyn RateLimiter,
    scope: Scope,
    identifier: &str,
    max_requests: u32,
    window_secs: u64,
) -> Result<(), ApiError> {
    let budget = Budget {
        max_requests,
        window: Duration::from_secs(window_secs),
    };
    match store.check(scope, identifier, budget).await {
        Decision::Refused { retry_after } => {
            Err(ApiError::rate_limited("Too many attempts", retry_after))
        }
        Decision::Unavailable { retry_after } => Err(ApiError::service_unavailable(
            "Password reset is temporarily unavailable",
            retry_after,
        )),
        Decision::Degraded { .. } => Err(ApiError::service_unavailable(
            "Password reset is temporarily unavailable",
            Duration::from_secs(30),
        )),
        Decision::Allowed { .. } => Ok(()),
    }
}
