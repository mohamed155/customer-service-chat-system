//! HTTP handlers for a signed-in user's own multi-factor authentication: start enrolment, confirm
//! it, and read the status. The second sign-in step (`POST /auth/mfa/verify`) lives in the server
//! crate with the login handler, because it returns the same profile body as login.
//!
//! These are self-service routes: they act only on the signed-in user, so they need a session and
//! no permission.

use std::sync::Arc;

use axum::{
    extract::State,
    http::{header::SET_COOKIE, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use config::AppConfig;
use kernel::rate_limit::{Budget, Decision, Scope};
use kernel::{ApiError, ApiJson, RateLimiter};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;

use crate::mfa::{self, MfaError, MfaKeys};
use crate::{session, Principal};

/// Attempts to start enrolment per account, per window.
const ENROL_PASSWORD_ATTEMPTS: u32 = 5;
const ENROL_PASSWORD_WINDOW: std::time::Duration = std::time::Duration::from_secs(15 * 60);

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnrolmentResponse {
    /// The secret as base32, to type into an authenticator app. Shown once, here.
    pub secret: String,
    /// The `otpauth://` URI a QR code carries. Shown once, here.
    pub otpauth_uri: String,
}

/// Starting enrolment proves the account owner is present: it needs the current password, so a
/// stolen session alone cannot add an attacker's authenticator.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EnrolRequest {
    /// The signed-in user's current password. Never echoed back.
    #[schema(value_type = String, write_only, example = "********")]
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ConfirmRequest {
    /// A six-digit code from the authenticator app.
    pub code: String,
}

/// The second sign-in step: exactly one of `code` and `recoveryCode`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VerifyRequest {
    /// A six-digit code from the authenticator app.
    pub code: Option<String>,
    /// One of the single-use recovery codes.
    pub recovery_code: Option<String>,
}

/// The body of the response to a password step that needs a second factor.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MfaRequiredResponse {
    pub mfa_required: bool,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryCodesResponse {
    /// Ten single-use codes. Shown once, here; only their hashes are kept.
    pub recovery_codes: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MfaStatusResponse {
    pub enrolled: bool,
    pub recovery_codes_remaining: i64,
}

/// Map a service error to the response the API returns for it. The wrong-code cases never say
/// whether the code was wrong, already used, or the user has no factor.
pub fn error_response(error: MfaError) -> Response {
    match error {
        MfaError::AlreadyEnrolled => {
            ApiError::conflict("Two-factor authentication is already set up").into_response()
        }
        MfaError::NotEnrolling => {
            ApiError::conflict("Start setting up two-factor authentication first").into_response()
        }
        MfaError::Invalid => {
            ApiError::unprocessable_entity("That code is not valid").into_response()
        }
        MfaError::Locked(seconds) => ApiError::rate_limited(
            "Too many wrong codes. Try again later.",
            std::time::Duration::from_secs(seconds.max(1) as u64),
        )
        .into_response(),
        MfaError::Internal(message) => {
            ApiError::internal_error_from("Two-factor authentication failed", message)
                .into_response()
        }
    }
}

fn keys(pool: &PgPool, config: &AppConfig) -> Result<MfaKeys, Box<Response>> {
    MfaKeys::from_config(pool, config).map_err(|e| {
        Box::new(
            ApiError::internal_error_from("Two-factor authentication is not configured", e)
                .into_response(),
        )
    })
}

/// Start setting up an authenticator app.
#[utoipa::path(
    post,
    path = "/auth/mfa/enrol",
    tag = "auth",
    operation_id = "auth_mfa_enrol",
    summary = "Start setting up two-factor authentication",
    description = "Creates a new authenticator secret for the signed-in user and returns it once, \
                  as base32 and as an `otpauth://` URI. Needs the current password, so a stolen \
                  session alone cannot add an authenticator. The factor stays inactive until \
                  `POST /auth/mfa/enrol/confirm` proves a code. Starting again replaces an \
                  unconfirmed secret. Returns 409 when the user already has an active factor.",
    request_body = EnrolRequest,
    responses(
        (status = 201, description = "A secret to add to an authenticator app.", body = EnrolmentResponse),
        (status = 401, description = "Unauthenticated.", body = kernel::ErrorEnvelope),
        (status = 409, description = "Two-factor authentication is already set up.", body = kernel::ErrorEnvelope),
        (status = 422, description = "The password is not correct.", body = kernel::ErrorEnvelope),
        (status = 429, description = "Too many attempts.", body = kernel::ErrorEnvelope),
    ),
    security(("session_cookie" = [])),
)]
pub async fn begin_enrolment(
    State(pool): State<PgPool>,
    Extension(config): Extension<Arc<AppConfig>>,
    Extension(limiter): Extension<Arc<dyn RateLimiter>>,
    principal: Principal,
    ApiJson(payload): ApiJson<EnrolRequest>,
) -> Response {
    // The password check is a guessing oracle for whoever holds a session, so it has a budget of
    // its own per account, on top of the per-client limit on the route.
    if config.rate_limit_enabled {
        if let Decision::Refused { retry_after } = limiter
            .check(
                Scope::MfaEnrol,
                &principal.user_id.to_string(),
                Budget {
                    max_requests: ENROL_PASSWORD_ATTEMPTS,
                    window: ENROL_PASSWORD_WINDOW,
                },
            )
            .await
        {
            return ApiError::rate_limited("Too many attempts", retry_after).into_response();
        }
    }
    match password_is_correct(&pool, principal.user_id, payload.password).await {
        Ok(true) => {}
        Ok(false) => {
            return ApiError::unprocessable_entity("The password is not correct").into_response()
        }
        Err(e) => return ApiError::internal_error_from("Password check failed", e).into_response(),
    }
    let keys = match keys(&pool, &config) {
        Ok(k) => k,
        Err(response) => return *response,
    };
    match mfa::begin_enrolment(&pool, &keys, principal.user_id, &principal.email).await {
        Ok(enrolment) => (
            StatusCode::CREATED,
            Json(EnrolmentResponse {
                secret: enrolment.secret,
                otpauth_uri: enrolment.otpauth_uri,
            }),
        )
            .into_response(),
        Err(error) => error_response(error),
    }
}

/// Finish setting up: prove a code, receive the recovery codes, and get a session that records the
/// second factor.
#[utoipa::path(
    post,
    path = "/auth/mfa/enrol/confirm",
    tag = "auth",
    operation_id = "auth_mfa_enrol_confirm",
    summary = "Confirm two-factor authentication with a code",
    description = "Checks a code from the authenticator app. On success the factor becomes active, \
                  ten recovery codes are returned once (stored only as hashes), and the \
                  `app_session` cookie is replaced by a session that records the second factor. \
                  Wrong codes count toward the lockout.",
    request_body = ConfirmRequest,
    responses(
        (status = 200, description = "Active. The recovery codes, shown once; the session cookie is replaced.", body = RecoveryCodesResponse),
        (status = 401, description = "Unauthenticated.", body = kernel::ErrorEnvelope),
        (status = 409, description = "No setup in progress.", body = kernel::ErrorEnvelope),
        (status = 422, description = "The code is not valid.", body = kernel::ErrorEnvelope),
        (status = 429, description = "Too many wrong codes; the factor is locked.", body = kernel::ErrorEnvelope),
    ),
    security(("session_cookie" = [])),
)]
pub async fn confirm_enrolment(
    State(pool): State<PgPool>,
    Extension(config): Extension<Arc<AppConfig>>,
    principal: Principal,
    claims: Option<Extension<session::SessionClaims>>,
    ApiJson(payload): ApiJson<ConfirmRequest>,
) -> Response {
    let keys = match keys(&pool, &config) {
        Ok(k) => k,
        Err(response) => return *response,
    };
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let codes =
        match mfa::confirm_enrolment(&pool, &keys, principal.user_id, &payload.code, now).await {
            Ok(codes) => codes,
            Err(error) => return error_response(error),
        };

    // The session that confirmed the factor is replaced by one that records it, and the old one
    // stops working.
    let (jwt, _, _) = match session::issue_session(
        &config.auth_jwt_secret,
        config.auth_session_ttl_seconds,
        principal.user_id,
        session::SessionAuth::second_factor(session::AMR_OTP),
    ) {
        Ok(issued) => issued,
        Err(e) => {
            return ApiError::internal_error_from("Session token issuance failed", e)
                .into_response()
        }
    };
    if let Some(Extension(old)) = claims {
        revoke(&pool, principal.user_id, old.jti, old.exp).await;
    }

    (
        [(
            SET_COOKIE,
            session::build_session_cookie(&jwt, config.auth_session_ttl_seconds),
        )],
        Json(RecoveryCodesResponse {
            recovery_codes: codes,
        }),
    )
        .into_response()
}

/// Check the user's password against its stored hash. A user without a password never passes.
async fn password_is_correct(
    pool: &PgPool,
    user_id: uuid::Uuid,
    password: String,
) -> Result<bool, String> {
    let hash: Option<Option<String>> =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1 AND deleted_at IS NULL")
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    let hash = hash.flatten();
    let verified = tokio::task::spawn_blocking(move || match hash {
        Some(hash) => crate::password::verify_password(&password, &hash),
        None => crate::password::verify_dummy(),
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(matches!(verified, Ok(true)))
}

/// Whether two-factor authentication is set up.
#[utoipa::path(
    get,
    path = "/auth/mfa/status",
    tag = "auth",
    operation_id = "auth_mfa_status",
    summary = "Read the two-factor authentication status",
    description = "Whether the signed-in user has an active factor, and how many recovery codes \
                  are left. Never returns a secret or a code.",
    responses(
        (status = 200, description = "The status.", body = MfaStatusResponse),
        (status = 401, description = "Unauthenticated.", body = kernel::ErrorEnvelope),
    ),
    security(("session_cookie" = [])),
)]
pub async fn status(State(pool): State<PgPool>, principal: Principal) -> Response {
    let enrolled = match mfa::is_enrolled(&pool, principal.user_id).await {
        Ok(v) => v,
        Err(e) => return ApiError::internal_error_from("Status read failed", e).into_response(),
    };
    let remaining = if enrolled {
        mfa::recovery_codes_remaining(&pool, principal.user_id)
            .await
            .unwrap_or(0)
    } else {
        0
    };
    Json(MfaStatusResponse {
        enrolled,
        recovery_codes_remaining: remaining,
    })
    .into_response()
}

/// Stop a session id from working again (logout does the same).
pub async fn revoke(pool: &PgPool, user_id: uuid::Uuid, jti: uuid::Uuid, exp: i64) {
    let result = sqlx::query(
        "INSERT INTO revoked_sessions (jti, user_id, expires_at) VALUES ($1, $2, $3) \
         ON CONFLICT (jti) DO NOTHING",
    )
    .bind(jti)
    .bind(user_id)
    .bind(chrono::DateTime::from_timestamp(exp, 0).unwrap_or_else(chrono::Utc::now))
    .execute(pool)
    .await;
    if let Err(error) = result {
        tracing::error!(%error, "could not revoke a session id");
    }
}
