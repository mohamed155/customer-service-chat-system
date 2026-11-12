//! Policy middleware for second factors (ADR-005 phase M1, MOH-34).
//!
//! Both apply to sessions that came from a sign-in (they carry [`SessionClaims`]). A request
//! authenticated by the development header has no claims and is never held to them; that header
//! exists only in development and tests.
//!
//! Both do nothing unless `AUTH_MFA_REQUIRED` is on.

use std::sync::Arc;

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Extension,
};
use config::AppConfig;
use kernel::ApiError;

use crate::session::SessionClaims;

/// How long a proven second factor counts as fresh for a sensitive action.
pub const STEP_UP_WINDOW_SECONDS: i64 = 300;

/// The code a client looks for to send the user to set up two-factor authentication.
pub const ENROLMENT_REQUIRED: &str = "mfa_enrolment_required";
/// The code a client looks for to ask for a fresh code and retry the request.
pub const STEP_UP_REQUIRED: &str = "mfa_step_up_required";

/// Refuse a session that has no second factor. The routes that let a user set one up, read their
/// profile or sign out are not behind this layer.
pub async fn enforce_second_factor(
    Extension(config): Extension<Arc<AppConfig>>,
    request: Request,
    next: Next,
) -> Response {
    if config.auth_mfa_required {
        if let Some(claims) = request.extensions().get::<SessionClaims>() {
            if !claims.has_second_factor() {
                return ApiError::new_with_code(
                    StatusCode::FORBIDDEN,
                    ENROLMENT_REQUIRED,
                    "Set up two-factor authentication to continue",
                )
                .into_response();
            }
        }
    }
    next.run(request).await
}

/// Require a second factor proven within [`STEP_UP_WINDOW_SECONDS`] for a sensitive action.
pub async fn require_step_up(
    Extension(config): Extension<Arc<AppConfig>>,
    request: Request,
    next: Next,
) -> Response {
    if config.auth_mfa_required {
        if let Some(claims) = request.extensions().get::<SessionClaims>() {
            let now = chrono::Utc::now().timestamp();
            if !claims.second_factor_fresh(now, STEP_UP_WINDOW_SECONDS) {
                return ApiError::new_with_code(
                    StatusCode::UNAUTHORIZED,
                    STEP_UP_REQUIRED,
                    "Enter a code from your authenticator app to continue",
                )
                .into_response();
            }
        }
    }
    next.run(request).await
}
