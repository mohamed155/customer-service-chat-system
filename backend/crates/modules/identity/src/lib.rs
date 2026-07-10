//! Identity module — principal types, middleware, and extractors.

pub mod audit;

pub mod password;

pub mod routes;

pub mod session;

use axum::{
    extract::{FromRequestParts, Request, State},
    http::{header, request::Parts},
    middleware::Next,
    response::Response,
};
use config::Environment;
use kernel::ApiError;
use sqlx::PgPool;
use std::str::FromStr;
use tracing::field;
use uuid::Uuid;

use crate::session::SessionClaims;

/// Platform-level roles for internal (staff) users.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformRole {
    SuperAdmin,
    Developer,
    Sales,
    Support,
    Finance,
}

impl std::fmt::Display for PlatformRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SuperAdmin => write!(f, "super_admin"),
            Self::Developer => write!(f, "developer"),
            Self::Sales => write!(f, "sales"),
            Self::Support => write!(f, "support"),
            Self::Finance => write!(f, "finance"),
        }
    }
}

impl FromStr for PlatformRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "super_admin" => Ok(Self::SuperAdmin),
            "developer" => Ok(Self::Developer),
            "sales" => Ok(Self::Sales),
            "support" => Ok(Self::Support),
            "finance" => Ok(Self::Finance),
            _ => Err(format!("invalid platform role: {s}")),
        }
    }
}

/// Distinguishes platform-internal users from tenant-scoped (customer) users.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrincipalKind {
    Platform,
    Tenant,
    InvalidPlatformRole,
}

/// Authenticated user principal resolved from the request context.
#[derive(Debug, Clone)]
pub struct Principal {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub platform_role: Option<PlatformRole>,
    pub invalid_platform_role: bool,
}

impl Principal {
    /// Returns [`PrincipalKind::Platform`] when the user carries a platform
    /// role and [`PrincipalKind::Tenant`] otherwise.
    pub fn kind(&self) -> PrincipalKind {
        if self.invalid_platform_role {
            PrincipalKind::InvalidPlatformRole
        } else if self.platform_role.is_some() {
            PrincipalKind::Platform
        } else {
            PrincipalKind::Tenant
        }
    }
}

/// Configuration injected into [`principal_middleware`] via Axum state.
#[derive(Clone)]
pub struct IdentityConfig {
    pub pool: PgPool,
    pub environment: Environment,
    pub auth_jwt_secret: String,
    pub auth_session_ttl_seconds: u64,
}

/// Axum middleware that resolves the current [`Principal`] from the request.
///
/// # Environment gating
///
/// | Environment     | Behaviour                                       |
/// |-----------------|-------------------------------------------------|
/// | Development     | Reads `X-Dev-User-Id` header, queries database  |
/// | Test            | Reads `X-Dev-User-Id` header, queries database  |
/// | Production      | Ignores the header entirely                     |
/// | Staging         | Ignores the header entirely                     |
///
/// When a valid principal is resolved it is inserted into request extensions
/// and recorded on the current tracing span as `principal.id`.
///
/// # Usage
///
/// ```rust,ignore
/// use axum::middleware::from_fn_with_state;
///
/// Router::new()
///     .route("/api/protected", get(handler))
///     .layer(from_fn_with_state(identity_config, principal_middleware));
/// ```
pub async fn principal_middleware(
    State(cfg): State<IdentityConfig>,
    mut request: Request,
    next: Next,
) -> Response {
    let session_cookie = request
        .headers()
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(extract_app_session_cookie)
        .map(str::to_owned);
    let dev_user_id = request
        .headers()
        .get("X-Dev-User-Id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::from_str(value).ok());

    let cookie_principal = resolve_cookie_principal(&cfg, session_cookie.as_deref()).await;
    if let Some((principal, claims)) = cookie_principal {
        tracing::Span::current().record("principal.id", field::display(&principal.user_id));
        request.extensions_mut().insert(claims);
        request.extensions_mut().insert(principal);
        return next.run(request).await;
    }

    match cfg.environment {
        Environment::Development | Environment::Test => {
            if let Some(principal) = resolve_dev_header_principal(&cfg, dev_user_id).await {
                tracing::Span::current().record("principal.id", field::display(&principal.user_id));
                request.extensions_mut().insert(principal);
            }
        }
        Environment::Production | Environment::Staging => {}
    }

    next.run(request).await
}

async fn resolve_cookie_principal(
    cfg: &IdentityConfig,
    jwt: Option<&str>,
) -> Option<(Principal, SessionClaims)> {
    let jwt = jwt?;
    let claims = session::validate_token(&cfg.auth_jwt_secret, jwt).ok()?;

    let result = sqlx::query_as::<_, (Uuid, String, String, Option<String>)>(
        r#"
        SELECT id, email, display_name, platform_role
        FROM users
        WHERE id = $1
          AND deleted_at IS NULL
          AND NOT EXISTS (
              SELECT 1 FROM revoked_sessions WHERE jti = $2
          )
        "#,
    )
    .bind(claims.sub)
    .bind(claims.jti)
    .fetch_optional(&cfg.pool)
    .await
    .ok()?;

    let (id, email, display_name, role_str) = result?;
    principal_from_row(id, email, display_name, role_str).map(|principal| (principal, claims))
}

fn extract_app_session_cookie(cookie_header: &str) -> Option<&str> {
    cookie_header.split(';').find_map(|part| {
        let value = part.trim().strip_prefix("app_session=")?;
        (!value.is_empty()).then_some(value)
    })
}

async fn resolve_dev_header_principal(
    cfg: &IdentityConfig,
    user_id: Option<Uuid>,
) -> Option<Principal> {
    let user_id = user_id?;
    let result = sqlx::query_as::<_, (Uuid, String, String, Option<String>)>(
        "SELECT id, email, display_name, platform_role \
         FROM users WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(user_id)
    .fetch_optional(&cfg.pool)
    .await
    .ok()?;

    let (id, email, display_name, role_str) = result?;
    principal_from_row(id, email, display_name, role_str)
}

fn principal_from_row(
    user_id: Uuid,
    email: String,
    display_name: String,
    stored_platform_role: Option<String>,
) -> Option<Principal> {
    let (platform_role, invalid_platform_role) = match stored_platform_role {
        Some(stored_role) => match PlatformRole::from_str(&stored_role) {
            Ok(role) => (Some(role), false),
            Err(error) => {
                tracing::error!(
                    user.id = %user_id,
                    platform.role = %stored_role,
                    %error,
                    "unrecognized stored platform role"
                );
                (None, true)
            }
        },
        None => (None, false),
    };

    Some(Principal {
        user_id,
        email,
        display_name,
        platform_role,
        invalid_platform_role,
    })
}

// ---------------------------------------------------------------------------
// Extractors
// ---------------------------------------------------------------------------

/// Optional principal extractor.
///
/// Returns `None` when no principal has been attached to request extensions
/// by [`principal_middleware`].
///
/// # Handler usage
///
/// ```rust,ignore
/// async fn handler(principal: OptionalPrincipal) -> impl IntoResponse {
///     if let Some(p) = principal.0 {
///         // authenticated
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct OptionalPrincipal(pub Option<Principal>);

impl<S: Send + Sync> FromRequestParts<S> for OptionalPrincipal {
    type Rejection = core::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(parts.extensions.get::<Principal>().cloned()))
    }
}

/// Required principal extractor.
///
/// Rejects with 401 when no principal has been attached to request extensions
/// by [`principal_middleware`].
///
/// # Handler usage
///
/// ```rust,ignore
/// async fn handler(principal: Principal) -> impl IntoResponse {
///     // guaranteed to be authenticated
/// }
/// ```
impl<S: Send + Sync> FromRequestParts<S> for Principal {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Principal>()
            .cloned()
            .ok_or_else(|| ApiError::unauthenticated("Authentication required"))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- PlatformRole Display / FromStr round-trip -----------------------

    #[test]
    fn platform_role_display_fromstr_roundtrip() {
        let roles = [
            PlatformRole::SuperAdmin,
            PlatformRole::Developer,
            PlatformRole::Sales,
            PlatformRole::Support,
            PlatformRole::Finance,
        ];
        for role in &roles {
            let s = role.to_string();
            let parsed: PlatformRole = s.parse().unwrap();
            assert_eq!(*role, parsed);
        }
    }

    #[test]
    fn platform_role_invalid_fromstr_returns_err() {
        assert!("bogus_role".parse::<PlatformRole>().is_err());
        assert!("admin".parse::<PlatformRole>().is_err());
        assert!("SUPER_ADMIN".parse::<PlatformRole>().is_err());
    }

    #[test]
    fn unknown_stored_platform_role_rejects_principal_construction() {
        let principal = principal_from_row(
            Uuid::nil(),
            "legacy@example.com".into(),
            "Legacy".into(),
            Some("legacy_admin".into()),
        )
        .expect("recognized user remains authenticated");

        assert!(principal.invalid_platform_role);
        assert_eq!(principal.kind(), PrincipalKind::InvalidPlatformRole);
    }

    #[tokio::test]
    async fn unknown_stored_platform_role_is_denied_at_middleware_boundary() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
            middleware,
            response::IntoResponse,
            routing::get,
            Router,
        };
        use tower::ServiceExt;

        let app = Router::new()
            .route(
                "/",
                get(|principal: Principal| async move {
                    if principal.kind() == PrincipalKind::InvalidPlatformRole {
                        ApiError::unauthorized("Access denied").into_response()
                    } else {
                        StatusCode::OK.into_response()
                    }
                }),
            )
            .layer(middleware::from_fn(
                |mut request: axum::extract::Request, next: middleware::Next| async move {
                    if let Some(principal) = principal_from_row(
                        Uuid::nil(),
                        "legacy@example.com".into(),
                        "Legacy".into(),
                        Some("legacy_admin".into()),
                    ) {
                        request.extensions_mut().insert(principal);
                    }
                    next.run(request).await
                },
            ));

        let response = app
            .oneshot(Request::get("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    // -- PrincipalKind classification -----------------------------------

    #[test]
    fn principal_kind_is_platform_when_role_present() {
        let p = Principal {
            user_id: Uuid::nil(),
            email: "admin@test.com".into(),
            display_name: "Admin".into(),
            platform_role: Some(PlatformRole::SuperAdmin),
            invalid_platform_role: false,
        };
        assert_eq!(p.kind(), PrincipalKind::Platform);
    }

    #[test]
    fn principal_kind_is_tenant_when_role_absent() {
        let p = Principal {
            user_id: Uuid::nil(),
            email: "user@test.com".into(),
            display_name: "User".into(),
            platform_role: None,
            invalid_platform_role: false,
        };
        assert_eq!(p.kind(), PrincipalKind::Tenant);
    }

    // -- Environment gating ---------------------------------------------

    #[test]
    fn development_and_test_match_processing_arm() {
        assert!(
            matches!(
                Environment::Development,
                Environment::Development | Environment::Test
            ),
            "Development should match the header-processing arm"
        );
        assert!(
            matches!(
                Environment::Test,
                Environment::Development | Environment::Test
            ),
            "Test should match the header-processing arm"
        );
    }

    #[test]
    fn production_and_staging_match_ignoring_arm() {
        assert!(
            matches!(
                Environment::Production,
                Environment::Production | Environment::Staging
            ),
            "Production should match the ignoring arm"
        );
        assert!(
            matches!(
                Environment::Staging,
                Environment::Production | Environment::Staging
            ),
            "Staging should match the ignoring arm"
        );
    }
}
