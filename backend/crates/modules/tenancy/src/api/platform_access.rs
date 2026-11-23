use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    Extension,
};
use chrono::{DateTime, Utc};
use config::AppConfig;
use identity::{session::SessionClaims, sessions::ClientMeta, Principal};
use kernel::{ApiError, ApiJson, ErrorDetail, ErrorEnvelope};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    application::platform_access::{AccessSession, AccessTerms},
    domain::{AccessDenial, DurationError, ReasonError},
    infrastructure::platform_access::{self as repository, StartError},
    routes::TenantSummary,
    PlatformAccessGrant,
};

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StartPlatformAccessRequest {
    /// Trimmed justification, visible in the tenant's audit log.
    #[schema(required = true, value_type = String, min_length = 10, max_length = 500)]
    reason: Option<String>,
    /// Optional duration in minutes, from 5 through 240. The default is 60.
    #[serde(default, deserialize_with = "deserialize_duration")]
    #[schema(value_type = i32, minimum = 5, maximum = 240, default = 60)]
    duration_minutes: Option<i32>,
}

fn deserialize_duration<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<i32>, D::Error> {
    i32::deserialize(deserializer).map(Some)
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StartedPlatformAccessSession {
    id: Uuid,
    tenant_id: Uuid,
    reason: String,
    started_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl From<AccessSession> for StartedPlatformAccessSession {
    fn from(session: AccessSession) -> Self {
        Self {
            id: session.id,
            tenant_id: session.tenant_id,
            reason: session.reason,
            started_at: session.started_at,
            expires_at: session.expires_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SwitchTenantResponse {
    #[serde(flatten)]
    tenant: TenantSummary,
    access_session: StartedPlatformAccessSession,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlatformAccessSession {
    #[serde(flatten)]
    session: StartedPlatformAccessSession,
    tenant_name: String,
}

const REQUIRED: &str = "platform_access_required";
const EXPIRED: &str = "platform_access_expired";

#[utoipa::path(
    post,
    path = "/platform/tenants/{id}/switch",
    tag = "platform-tenants",
    operation_id = "switch_platform_tenant",
    summary = "Start a platform tenant access session",
    description = "Starts time-limited access to the target tenant for the current sign-in. \
                   A previous access session ends when the new one starts. \
                   Requires permission: platform.tenants.switch",
    params(("id" = Uuid, Path, description = "Tenant identifier")),
    request_body = StartPlatformAccessRequest,
    responses(
        (status = 200, description = "Access started. Returns the tenant and access session.", body = SwitchTenantResponse),
        (status = 400, description = "Invalid JSON body.", body = ErrorEnvelope),
        (status = 401, description = "Authentication or fresh two-step proof required.", body = ErrorEnvelope),
        (status = 403, description = "Tenant unavailable or insufficient permissions.", body = ErrorEnvelope),
        (status = 409, description = "Another tenant access start is in progress.", body = ErrorEnvelope),
        (status = 422, description = "Invalid reason or duration.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn switch_tenant(
    (principal, claims): (Principal, Option<Extension<SessionClaims>>),
    Path(id): Path<Uuid>,
    State(pool): State<PgPool>,
    Extension(config): Extension<Arc<AppConfig>>,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<StartPlatformAccessRequest>,
) -> Result<Json<SwitchTenantResponse>, ApiError> {
    let terms = AccessTerms::validate(request.reason.as_deref(), request.duration_minutes)
        .map_err(validation_error)?;
    let Some(Extension(claims)) = claims else {
        return Err(ApiError::unauthorized(
            "A sign-in session is required to start tenant access",
        ));
    };
    let forwarded_ip = if config.trusted_proxy {
        ClientMeta::from_headers(&headers)
            .ip
            .and_then(|ip| ip.parse::<std::net::IpAddr>().ok())
    } else {
        None
    };
    let ip = forwarded_ip
        .or_else(|| peer.map(|Extension(ConnectInfo(address))| address.ip()))
        .map(|ip| ip.to_string());

    let (tenant, session) = repository::start(
        &pool,
        principal.user_id,
        id,
        claims.sid,
        terms,
        ip.as_deref(),
    )
    .await
    .map_err(|error| match error {
        StartError::TenantUnavailable => ApiError::unauthorized("Access denied"),
        StartError::StartInProgress => {
            ApiError::conflict("A tenant access start is already in progress")
        }
        StartError::Storage(error) => {
            ApiError::internal_error_from("Failed to start tenant access", error)
        }
    })?;
    Ok(Json(SwitchTenantResponse {
        tenant: TenantSummary {
            id: tenant.id,
            name: tenant.name,
            slug: tenant.slug,
            status: tenant.status,
            plan: tenant.plan,
        },
        access_session: session.into(),
    }))
}

#[utoipa::path(
    get,
    path = "/platform/access-session",
    tag = "platform-tenants",
    operation_id = "get_platform_access_session",
    summary = "Get the current tenant access session",
    description = "Returns the current sign-in's open access session. \
                   Requires permission: platform.tenants.switch",
    responses(
        (status = 200, description = "Current access session.", body = PlatformAccessSession),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "No current access session.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_access_session(
    principal: Principal,
    claims: Option<Extension<SessionClaims>>,
    State(pool): State<PgPool>,
) -> Result<Json<PlatformAccessSession>, ApiError> {
    let no_session = || ApiError::not_found("No current tenant access session");
    let Extension(claims) = claims.ok_or_else(no_session)?;
    let (tenant_name, session) = repository::current(&pool, principal.user_id, claims.sid)
        .await
        .map_err(|error| ApiError::internal_error_from("Failed to read tenant access", error))?
        .ok_or_else(no_session)?;
    Ok(Json(PlatformAccessSession {
        tenant_name,
        session: session.into(),
    }))
}

#[utoipa::path(
    post,
    path = "/platform/access-session/end",
    tag = "platform-tenants",
    operation_id = "end_platform_access_session",
    summary = "End the current tenant access session",
    description = "Ends the current sign-in's access session, if any. \
                   Requires permission: platform.tenants.switch",
    responses(
        (status = 204, description = "No access session remains for this sign-in."),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn end_access_session(
    principal: Principal,
    claims: Option<Extension<SessionClaims>>,
    State(pool): State<PgPool>,
) -> Result<StatusCode, ApiError> {
    if let Some(Extension(claims)) = claims {
        repository::end(&pool, principal.user_id, claims.sid)
            .await
            .map_err(|error| ApiError::internal_error_from("Failed to end tenant access", error))?;
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn require_access_session(
    pool: &PgPool,
    user_id: Uuid,
    tenant_id: Uuid,
    claims: Option<&SessionClaims>,
) -> Result<PlatformAccessGrant, ApiError> {
    let status = match claims {
        Some(claims) => repository::access_status(pool, user_id, tenant_id, claims.sid)
            .await
            .map_err(|error| {
                ApiError::internal_error_from("Failed to check tenant access", error)
                    .for_tenant(tenant_id)
            })?,
        None => Err(AccessDenial::Required),
    };
    let (code, message) = match status {
        Ok(grant) => return Ok(grant),
        Err(AccessDenial::Expired) => (
            EXPIRED,
            "Your access to this tenant ended; start a new one with a reason",
        ),
        Err(AccessDenial::Required) => (
            REQUIRED,
            "Start an access session with a reason to access this tenant",
        ),
    };
    crate::audit::access_denied(pool, Some(user_id), &tenant_id.to_string(), code).await;
    Err(ApiError::new_with_code(
        StatusCode::FORBIDDEN,
        code,
        message,
    ))
}

fn validation_error((reason, duration): (Option<ReasonError>, Option<DurationError>)) -> ApiError {
    let mut details = Vec::new();
    if let Some(error) = reason {
        let (code, message) = match error {
            ReasonError::Required => ("required", "Enter a reason"),
            ReasonError::TooShort => ("too_short", "The reason must have at least 10 characters"),
            ReasonError::TooLong => ("too_long", "The reason must have at most 500 characters"),
            ReasonError::InvalidValue => {
                ("invalid_value", "The reason contains an invalid character")
            }
        };
        details.push(ErrorDetail {
            field: "reason".into(),
            code: code.into(),
            message: message.into(),
        });
    }
    if duration.is_some() {
        details.push(ErrorDetail {
            field: "durationMinutes".into(),
            code: "out_of_range".into(),
            message: "The duration must be from 5 through 240 minutes".into(),
        });
    }
    ApiError::unprocessable_entity("Validation failed").with_details(details)
}
