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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

struct LoginUser {
    id: Uuid,
    email: String,
    display_name: String,
    platform_role: Option<String>,
    password_hash: Option<String>,
}

pub struct LoginSuccess {
    pub principal: Principal,
    pub session_cookie: String,
}

pub enum LoginError {
    Validation,
    InvalidCredentials,
    Internal(String),
}

pub async fn authenticate_login(
    pool: &PgPool,
    config: &AppConfig,
    payload: LoginRequest,
) -> Result<LoginSuccess, LoginError> {
    let email = payload.email.trim().to_owned();
    let password = payload.password;

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

    let principal =
        crate::principal_from_row(user.id, user.email, user.display_name, user.platform_role)
            .expect("database user row always produces an authenticated principal");

    let (jwt, _, _) = session::issue_token(
        &config.auth_jwt_secret,
        config.auth_session_ttl_seconds,
        user.id,
    )
    .map_err(|error| LoginError::Internal(format!("Session token issuance failed: {error}")))?;

    Ok(LoginSuccess {
        principal,
        session_cookie: session::build_session_cookie(&jwt, config.auth_session_ttl_seconds),
    })
}

pub async fn logout(
    State(pool): State<PgPool>,
    principal: Principal,
    claims: Option<Extension<session::SessionClaims>>,
) -> Response {
    if let Some(Extension(claims)) = claims {
        let insert = sqlx::query(
            r#"
            INSERT INTO revoked_sessions (jti, user_id, expires_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (jti) DO NOTHING
            "#,
        )
        .bind(claims.jti)
        .bind(principal.user_id)
        .bind(chrono::DateTime::from_timestamp(claims.exp, 0).unwrap_or_else(chrono::Utc::now))
        .execute(&pool)
        .await;

        if let Err(e) = insert {
            return ApiError::internal_error(format!("Database query failed: {e}")).into_response();
        }

        audit::logged_out(&pool, principal.user_id, claims.jti).await;
    }

    (
        [(SET_COOKIE, session::clear_session_cookie())],
        StatusCode::NO_CONTENT,
    )
        .into_response()
}
async fn fetch_login_user(pool: &PgPool, email: &str) -> sqlx::Result<Option<LoginUser>> {
    let row = sqlx::query_as::<_, (Uuid, String, String, Option<String>, Option<String>)>(
        r#"
        SELECT id, email, display_name, platform_role, password_hash
        FROM users
        WHERE lower(email) = lower($1)
          AND deleted_at IS NULL
        "#,
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(id, email, display_name, platform_role, password_hash)| LoginUser {
            id,
            email,
            display_name,
            platform_role,
            password_hash,
        },
    ))
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
