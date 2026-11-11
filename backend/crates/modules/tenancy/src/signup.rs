//! `POST /auth/signup` — public self-serve signup.
//!
//! Creates a new tenant (workspace), a new user account as that tenant's
//! Owner, and starts a session — mirroring the anonymous-accept path in
//! [`crate::invitations::accept_invitation`], but provisioning a brand new
//! tenant instead of joining an existing one.

use axum::{
    extract::State,
    http::{header::SET_COOKIE, StatusCode},
    response::{IntoResponse, Json, Response},
    Extension,
};
use config::AppConfig;
use identity::{password, session, Principal};
use kernel::{ApiError, ErrorDetail};
use rand::Rng;
use serde::Deserialize;
use std::sync::Arc;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{audit, routes};

const MAX_SLUG_ATTEMPTS: u8 = 5;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SignupRequest {
    /// Name for the new workspace (tenant). 1-200 characters.
    pub workspace_name: String,
    /// The signing-up user's display name. 1-200 characters.
    pub display_name: String,
    /// Email address for the new account.
    pub email: String,
    /// Plain-text password supplied by the caller. Never echoed back in any
    /// response; the OpenAPI schema marks this field as `writeOnly`.
    #[schema(value_type = String, write_only, example = "********")]
    pub password: String,
}

#[derive(Debug)]
struct SignupInput {
    workspace_name: String,
    display_name: String,
    email: String,
    password: String,
}

fn validate_signup(payload: SignupRequest) -> Result<SignupInput, ApiError> {
    let mut details: Vec<ErrorDetail> = Vec::new();

    let workspace_name = match payload.workspace_name.trim() {
        s if !s.is_empty() && s.chars().count() <= 200 => s.to_owned(),
        _ => {
            details.push(ErrorDetail {
                field: "workspaceName".into(),
                code: "invalid_length".into(),
                message: "Workspace name must be between 1 and 200 characters".into(),
            });
            String::new()
        }
    };

    let display_name = match payload.display_name.trim() {
        s if !s.is_empty() && s.chars().count() <= 200 => s.to_owned(),
        _ => {
            details.push(ErrorDetail {
                field: "displayName".into(),
                code: "invalid_length".into(),
                message: "Name must be between 1 and 200 characters".into(),
            });
            String::new()
        }
    };

    let email_trimmed = payload.email.trim();
    let email = if email_trimmed.is_empty() {
        details.push(ErrorDetail {
            field: "email".into(),
            code: "required".into(),
            message: "Email is required".into(),
        });
        String::new()
    } else if !is_plausible_email(email_trimmed) {
        details.push(ErrorDetail {
            field: "email".into(),
            code: "invalid_format".into(),
            message: "Email must be a valid email address".into(),
        });
        String::new()
    } else {
        email_trimmed.to_lowercase()
    };

    let password = if payload.password.trim().is_empty() {
        details.push(ErrorDetail {
            field: "password".into(),
            code: "required".into(),
            message: "Password is required".into(),
        });
        String::new()
    } else if payload.password.chars().count() < 8 {
        details.push(ErrorDetail {
            field: "password".into(),
            code: "invalid_length".into(),
            message: "Password must be at least 8 characters".into(),
        });
        String::new()
    } else {
        payload.password
    };

    if !details.is_empty() {
        return Err(ApiError::unprocessable_entity("Validation failed").with_details(details));
    }

    Ok(SignupInput {
        workspace_name,
        display_name,
        email,
        password,
    })
}

/// Deliberately lightweight — mirrors the `users_email_format` DB CHECK
/// (single `@`, non-empty local/domain parts) plus a domain-has-a-dot check.
/// Lives here (rather than reusing `tenancy::routes::is_valid_email`) to
/// avoid widening that function's visibility for a single extra caller.
fn is_plausible_email(value: &str) -> bool {
    if value.is_empty() || value.len() > 254 {
        return false;
    }
    if value.bytes().any(|b| b < 32 || b == 127 || b == b' ') {
        return false;
    }
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        return false;
    }
    domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

/// Derive a tenant-slug candidate from a workspace name: lowercase,
/// non-alphanumeric runs collapsed to a single hyphen, trimmed of leading/
/// trailing hyphens, capped at 63 chars (the `tenants_slug_format` CHECK).
/// Falls back to `"workspace"` when the name has no ASCII alphanumerics
/// (e.g. a name written entirely in a non-Latin script).
fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut last_was_hyphen = true; // suppress a leading hyphen
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_hyphen = false;
        } else if !last_was_hyphen {
            slug.push('-');
            last_was_hyphen = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug.truncate(63);
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "workspace".to_string()
    } else {
        slug
    }
}

fn random_suffix() -> String {
    let mut bytes = [0u8; 4];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[utoipa::path(
    post,
    path = "/auth/signup",
    tag = "auth",
    operation_id = "auth_signup",
    summary = "Create a workspace and its first (Owner) user",
    description = "Public, self-serve signup (FR-AUTH-001). Creates a new tenant (workspace) on the \
                  `trial` plan, a new user account as that tenant's Owner, and starts a session — \
                  the response body is the same `MeResponse` shape returned by `GET /me`, and the \
                  `app_session` cookie is set the same way as `POST /auth/login`. The workspace slug \
                  is derived automatically from `workspaceName` (and disambiguated on collision); it \
                  can be changed later from tenant settings. Returns 409 if the email is already \
                  registered.",
    request_body = SignupRequest,
    responses(
        (status = 201, description = "Workspace and account created. Returns the user profile and sets the `app_session` cookie.", body = serde_json::Value),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = kernel::ErrorEnvelope),
        (status = 409, description = "An account with this email already exists.", body = kernel::ErrorEnvelope),
        (status = 422, description = "Validation failed (per-field).", body = kernel::ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = kernel::ErrorEnvelope),
    ),
    security(())
)]
pub async fn signup(
    State(pool): State<sqlx::PgPool>,
    Extension(config): Extension<Arc<AppConfig>>,
    kernel::ApiJson(payload): kernel::ApiJson<SignupRequest>,
) -> Response {
    let input = match validate_signup(payload) {
        Ok(input) => input,
        Err(error) => return error.into_response(),
    };

    let existing_user: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(&input.email)
        .fetch_optional(&pool)
        .await
        .unwrap_or(None);

    if existing_user.is_some() {
        return ApiError::conflict(
            "An account with this email already exists. Please sign in instead.",
        )
        .into_response();
    }

    let password_for_hash = input.password.clone();
    let password_hash = match tokio::task::spawn_blocking(move || {
        password::hash_password(&password_for_hash)
    })
    .await
    {
        Ok(Ok(hash)) => hash,
        _ => return ApiError::internal_error("Failed to hash password").into_response(),
    };

    let base_slug = slugify(&input.workspace_name);

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return ApiError::internal_error_from("Database transaction begin failed", &e)
                .into_response();
        }
    };

    // ---- Insert the tenant, retrying with a random suffix on slug collision.
    // Unlike `create_tenant` (platform-admin, caller-supplied slug -> 409 on
    // conflict), this slug is system-generated, so we self-heal instead of
    // failing the signup over an internal naming detail the user never sees.
    let mut tenant_id: Option<Uuid> = None;
    let mut slug_used = base_slug.clone();
    for attempt in 0..MAX_SLUG_ATTEMPTS {
        let candidate = if attempt == 0 {
            base_slug.clone()
        } else {
            let mut prefix = base_slug.clone();
            prefix.truncate(54);
            format!("{prefix}-{}", random_suffix())
        };

        // Each attempt runs in a savepoint: a unique violation aborts the
        // enclosing transaction in Postgres, so without it every retry failed
        // with "current transaction is aborted" and signup returned 500.
        let mut attempt_tx = match sqlx::Acquire::begin(&mut *tx).await {
            Ok(sp) => sp,
            Err(e) => {
                return ApiError::internal_error_from("Tenant insert failed", &e).into_response();
            }
        };
        let result: Result<Uuid, sqlx::Error> = sqlx::query_scalar(
            "INSERT INTO tenants (name, slug, plan) VALUES ($1, $2, 'trial') RETURNING id",
        )
        .bind(&input.workspace_name)
        .bind(&candidate)
        .fetch_one(&mut *attempt_tx)
        .await;

        match result {
            Ok(id) => {
                if let Err(e) = attempt_tx.commit().await {
                    return ApiError::internal_error_from("Tenant insert failed", &e)
                        .into_response();
                }
                tenant_id = Some(id);
                slug_used = candidate;
                break;
            }
            Err(sqlx::Error::Database(dbe)) if dbe.code().as_deref() == Some("23505") => {
                let _ = attempt_tx.rollback().await;
                continue;
            }
            Err(e) => {
                return ApiError::internal_error_from("Tenant insert failed", &e).into_response();
            }
        }
    }

    let Some(tenant_id) = tenant_id else {
        return ApiError::internal_error("Could not allocate a unique workspace slug")
            .into_response();
    };

    // The tenant exists now; everything from here writes its own rows, so carry it in the
    // transaction (the Unit of Work does this up front for every other flow).
    if let Err(e) = db::set_tenant_in_tx(&mut tx, tenant_id).await {
        return ApiError::internal_error_from("Tenant context failed", &e).into_response();
    }

    let new_user_id: Uuid = match sqlx::query_scalar(
        "INSERT INTO users (email, display_name, password_hash) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(&input.email)
    .bind(&input.display_name)
    .bind(&password_hash)
    .fetch_one(&mut *tx)
    .await
    {
        Ok(id) => id,
        Err(sqlx::Error::Database(dbe)) if dbe.code().as_deref() == Some("23505") => {
            return ApiError::conflict("An account with this email already exists").into_response();
        }
        Err(e) => {
            return ApiError::internal_error_from("User insert failed", &e).into_response();
        }
    };

    if let Err(e) = sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, 'owner', 'active')",
    )
    .bind(tenant_id)
    .bind(new_user_id)
    .execute(&mut *tx)
    .await
    {
        return ApiError::internal_error_from("Membership insert failed", &e).into_response();
    }

    if let Err(e) = audit::record_tenant_signup_completed(
        &mut tx,
        new_user_id,
        tenant_id,
        &input.workspace_name,
        &slug_used,
        &input.email,
    )
    .await
    {
        return ApiError::internal_error_from("Audit insert failed", &e).into_response();
    }

    if let Err(e) = tx.commit().await {
        return ApiError::internal_error_from("Transaction commit failed", &e).into_response();
    }

    let (jwt, _, _) = match session::issue_token(
        &config.auth_jwt_secret,
        config.auth_session_ttl_seconds,
        new_user_id,
    ) {
        Ok(t) => t,
        Err(e) => {
            return ApiError::internal_error_from("Session token issuance failed", &e)
                .into_response();
        }
    };
    let session_cookie = session::build_session_cookie(&jwt, config.auth_session_ttl_seconds);

    let principal = Principal {
        user_id: new_user_id,
        email: input.email,
        display_name: input.display_name,
        platform_role: None,
        invalid_platform_role: false,
    };

    match routes::build_me_response(
        &pool,
        principal,
        config.environment == config::Environment::Production,
    )
    .await
    {
        Ok(response) => (
            StatusCode::CREATED,
            [(SET_COOKIE, session_cookie)],
            Json(response),
        )
            .into_response(),
        Err(error) => error.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_lowercases_and_collapses_separators() {
        assert_eq!(slugify("Acme  Corp!"), "acme-corp");
        assert_eq!(slugify("  --Leading & Trailing--  "), "leading-trailing");
    }

    #[test]
    fn slugify_falls_back_when_no_ascii_alphanumerics() {
        assert_eq!(slugify("日本語"), "workspace");
        assert_eq!(slugify("---"), "workspace");
    }

    #[test]
    fn slugify_truncates_to_63_chars() {
        let long_name = "a".repeat(100);
        let slug = slugify(&long_name);
        assert_eq!(slug.len(), 63);
        assert!(!slug.ends_with('-'));
    }

    #[test]
    fn is_plausible_email_accepts_simple_addresses() {
        assert!(is_plausible_email("owner@example.com"));
        assert!(!is_plausible_email("not-an-email"));
        assert!(!is_plausible_email("missing-domain@"));
        assert!(!is_plausible_email("@missing-local.com"));
        assert!(!is_plausible_email("two@at@signs.com"));
        assert!(!is_plausible_email("no-dot-domain@localhost"));
    }

    #[test]
    fn validate_signup_rejects_missing_fields() {
        let error = validate_signup(SignupRequest {
            workspace_name: "  ".into(),
            display_name: "".into(),
            email: "".into(),
            password: "".into(),
        })
        .unwrap_err();
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn validate_signup_rejects_short_password() {
        let error = validate_signup(SignupRequest {
            workspace_name: "Acme".into(),
            display_name: "Jane".into(),
            email: "jane@example.com".into(),
            password: "short".into(),
        })
        .unwrap_err();
        assert_eq!(
            error.into_response().status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn validate_signup_trims_and_lowercases_email() {
        let input = validate_signup(SignupRequest {
            workspace_name: "  Acme Corp  ".into(),
            display_name: "  Jane Doe  ".into(),
            email: "  Jane@Example.COM  ".into(),
            password: "correct horse battery".into(),
        })
        .expect("valid signup input");

        assert_eq!(input.workspace_name, "Acme Corp");
        assert_eq!(input.display_name, "Jane Doe");
        assert_eq!(input.email, "jane@example.com");
    }
}
