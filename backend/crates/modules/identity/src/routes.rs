use axum::{
    extract::State,
    http::{header::SET_COOKIE, StatusCode},
    response::{IntoResponse, Response},
    Extension,
};
use config::AppConfig;
use kernel::{ApiError, ErrorDetail};
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{audit, password, session, Principal};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountCreationInput {
    pub display_name: String,
    pub password: String,
}

pub fn validate_account_creation(
    display_name: Option<&str>,
    password: Option<&str>,
) -> Result<AccountCreationInput, ApiError> {
    let mut details: Vec<ErrorDetail> = Vec::new();

    let display_name = match display_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => value.to_owned(),
        None => {
            details.push(ErrorDetail {
                field: "displayName".into(),
                code: "required".into(),
                message: "Display name is required for new accounts".into(),
            });
            String::new()
        }
    };

    let password = match password.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => value.to_owned(),
        None => {
            details.push(ErrorDetail {
                field: "password".into(),
                code: "required".into(),
                message: "Password is required for new accounts".into(),
            });
            String::new()
        }
    };

    if !details.is_empty() {
        return Err(ApiError::unprocessable_entity("Validation failed").with_details(details));
    }

    Ok(AccountCreationInput {
        display_name,
        password,
    })
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    /// Email address of the user attempting to authenticate.
    pub email: String,
    /// Plain-text password supplied by the caller. Never echoed back in any
    /// response; the OpenAPI schema marks this field as `writeOnly`.
    #[schema(value_type = String, write_only, example = "********")]
    pub password: String,
}

#[derive(sqlx::FromRow)]
struct LoginUser {
    id: Uuid,
    email: String,
    display_name: String,
    platform_role: Option<String>,
    password_hash: Option<String>,
    password_reset_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct LoginSuccess {
    pub principal: Principal,
    /// The `Set-Cookie` values: the access token and the refresh token.
    pub session_cookies: Vec<String>,
}

/// The result of the password step.
pub enum LoginOutcome {
    /// No second factor is set up: the session starts now.
    Session(LoginSuccess),
    /// The user has a second factor. No session yet: this cookie carries a short-lived challenge
    /// that `POST /auth/mfa/verify` completes.
    MfaRequired {
        user_id: Uuid,
        pending_cookie: String,
    },
}

pub enum LoginError {
    Validation,
    InvalidCredentials,
    Internal(String),
}

pub async fn authenticate_login(
    pool: &PgPool,
    config: &AppConfig,
    headers: &axum::http::HeaderMap,
    payload: LoginRequest,
) -> Result<LoginOutcome, LoginError> {
    let email = payload.email.trim().to_owned();
    let password = payload.password;
    let password_for_rehash = password.clone();

    if email.is_empty() || password.trim().is_empty() {
        return Err(LoginError::Validation);
    }

    let user = match fetch_login_user(pool, &email).await {
        Ok(user) => user,
        Err(error) => {
            return Err(LoginError::Internal(format!(
                "Database query failed: {error}"
            )))
        }
    };

    let verified = verify_login_password(user.as_ref(), password).await;
    if !verified {
        audit::login_failed(pool, &email, "invalid_credentials").await;
        return Err(LoginError::InvalidCredentials);
    }

    let Some(user) = user else {
        audit::login_failed(pool, &email, "invalid_credentials").await;
        return Err(LoginError::InvalidCredentials);
    };

    rehash_if_outdated(pool, &user, &password_for_rehash).await;

    // A user with a second factor gets a challenge, not a session.
    match crate::mfa::is_enrolled(pool, user.id).await {
        Ok(true) => {
            let (jwt, _) = session::issue_pending_for_password(
                crate::keys::keys_for(&config.environment),
                user.id,
                user.password_reset_at,
            )
            .map_err(|e| LoginError::Internal(format!("Challenge token issuance failed: {e}")))?;
            return Ok(LoginOutcome::MfaRequired {
                user_id: user.id,
                pending_cookie: session::build_pending_cookie(&jwt),
            });
        }
        Ok(false) => {}
        Err(error) => {
            return Err(LoginError::Internal(format!(
                "Second-factor lookup failed: {error}"
            )))
        }
    }

    let principal =
        crate::principal_from_row(user.id, user.email, user.display_name, user.platform_role)
            .ok_or_else(|| {
                LoginError::Internal("user row does not produce an authenticated principal".into())
            })?;

    let session_cookies = crate::sessions::start_for_password(
        pool,
        &config.environment,
        user.id,
        user.password_reset_at,
        headers,
    )
    .await
    .map_err(|error| match error {
        crate::sessions::SessionError::StaleProof => LoginError::InvalidCredentials,
        _ => LoginError::Internal("Session issuance failed".into()),
    })?;

    Ok(LoginOutcome::Session(LoginSuccess {
        principal,
        session_cookies,
    }))
}

/// Logout the current session.
///
/// On success the session is revoked, so its access token stops working at once, and the response
/// clears the session cookies by setting already-expired `Set-Cookie` headers.
///
/// # OpenAPI
///
/// Documented as `POST /auth/logout` under the `auth` tag with cookie-based
/// security. The full response shape is omitted here because the endpoint
/// returns no body — the contract is captured in the `Set-Cookie` header
/// behaviour and the 204 status code (FR-011 / spec 016).
#[utoipa::path(
    post,
    path = "/auth/logout",
    tag = "auth",
    operation_id = "auth_logout",
    summary = "Logout and clear session cookie",
    description = "Revokes the current session (the whole refresh-token family) and clears the \
                  `__Host-app_session` and `__Secure-app_refresh` cookies. Returns 204 No \
                  Content on success. The `Set-Cookie` response headers set both cookies to an \
                  empty value with `Max-Age=0` so clients can drop them. Requires a valid \
                  session cookie; unauthenticated requests are rejected with 401.",
    responses(
        (status = 204, description = "Session revoked; the session cookies are cleared."),
        (status = 401, description = "Unauthenticated.", body = kernel::ErrorEnvelope),
    ),
    security(("session_cookie" = [])),
)]
pub async fn logout(
    State(pool): State<PgPool>,
    principal: Principal,
    claims: Option<Extension<session::SessionClaims>>,
) -> Response {
    if let Some(Extension(claims)) = claims {
        if let Err(e) =
            crate::sessions::revoke_claims(&pool, &claims, crate::sessions::reason::LOGOUT).await
        {
            return ApiError::internal_error_from("Database query failed", &e).into_response();
        }

        audit::logged_out(&pool, principal.user_id, claims.jti).await;
    }

    let mut response = StatusCode::NO_CONTENT.into_response();
    for cookie in crate::sessions::clear_cookies() {
        if let Ok(value) = cookie.parse() {
            response.headers_mut().append(SET_COOKIE, value);
        }
    }
    response
}
async fn fetch_login_user(pool: &PgPool, email: &str) -> sqlx::Result<Option<LoginUser>> {
    sqlx::query_as(
        "SELECT id, email::text, display_name, platform_role, password_hash, password_reset_at
         FROM users WHERE lower(email) = lower($1) AND deleted_at IS NULL",
    )
    .bind(email)
    .fetch_optional(pool)
    .await
}

/// Replace a hash made with older parameters by one made with the current ones, now that the
/// plain password is at hand. A failure is logged and ignored: the login already succeeded, and the
/// next login tries again. The update only applies if the stored hash is still the one that was
/// verified, so a password change that happens in between is never overwritten.
async fn rehash_if_outdated(pool: &PgPool, user: &LoginUser, password: &str) {
    let Some(old_hash) = user.password_hash.as_deref() else {
        return;
    };
    if !password::needs_rehash(old_hash) {
        return;
    }

    let password = password.to_owned();
    let new_hash =
        match tokio::task::spawn_blocking(move || password::hash_password(&password)).await {
            Ok(Ok(hash)) => hash,
            Ok(Err(error)) => {
                tracing::warn!(user.id = %user.id, %error, "password rehash failed");
                return;
            }
            Err(error) => {
                tracing::warn!(user.id = %user.id, %error, "password rehash task failed");
                return;
            }
        };

    let updated =
        sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2 AND password_hash = $3")
            .bind(&new_hash)
            .bind(user.id)
            .bind(old_hash)
            .execute(pool)
            .await;
    if let Err(error) = updated {
        tracing::warn!(user.id = %user.id, %error, "storing the rehashed password failed");
    }
}

async fn verify_login_password(user: Option<&LoginUser>, password: String) -> bool {
    let password_hash = user.and_then(|user| user.password_hash.clone());
    let result = tokio::task::spawn_blocking(move || match password_hash {
        Some(hash) => password::verify_password(&password, &hash),
        None => password::verify_dummy(),
    })
    .await;

    matches!(result, Ok(Ok(true)))
}

#[cfg(test)]
mod tests {
    use axum::{http::StatusCode, response::IntoResponse};

    use super::validate_account_creation;

    #[test]
    fn validate_account_creation_rejects_missing_fields() {
        let error = validate_account_creation(None, None).unwrap_err();
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn validate_account_creation_trims_and_accepts_inputs() {
        let account = validate_account_creation(Some("  New User  "), Some("  secret123  "))
            .expect("valid account creation");

        assert_eq!(account.display_name, "New User");
        assert_eq!(account.password, "secret123");
    }
}
