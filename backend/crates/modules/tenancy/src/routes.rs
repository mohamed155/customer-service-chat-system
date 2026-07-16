use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Json, Response},
    Extension,
};
use config::AppConfig;
use identity::Principal;
use kernel::{ApiError, ErrorEnvelope, Page, PageParams};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use std::str::FromStr;
use std::sync::Arc;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{audit, TenantContext};

#[derive(Serialize, ToSchema)]
pub struct TenantSummary {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub plan: String,
}

/// `PlatformTenantDetail` — full record returned by create/get/update handlers
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlatformTenantDetail {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub plan: String,
    pub contact_name: Option<String>,
    pub contact_email: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Tuple shape used to read/write a full tenant row from `tenancy` handlers.
type TenantRow = (
    Uuid,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
);

/// Tenant plan values. Maps to the `plan` CHECK constraint in the DB.
pub const TENANT_PLANS: &[&str] = &["trial", "starter", "professional", "enterprise"];

/// Tenant status values. Matches the existing `tenants_status_check` CHECK.
pub const TENANT_STATUSES: &[&str] = &["active", "suspended"];

/// `CreateTenantRequest` — body for `POST /api/v1/platform/tenants`.
///
/// `name` and `slug` are typed as `Option<String>` (not `String`) so that a
/// missing field still deserializes successfully and the handler can return a
/// 422 with a precise per-field error.  Bad JSON syntax or unknown fields
/// still fall through to `ApiJson`'s 400, which is the desired split.
///
/// `plan` uses `Option<Option<String>>` with the optional-nullable-string
/// deserializer (T101) so the handler can distinguish:
///   * Field absent → `None` → default to "trial".
///   * Explicit JSON `null` → `Some(None)` → 422.
///   * Supplied value → `Some(Some(s))` → validate.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateTenantRequest {
    pub name: Option<String>,
    pub slug: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable_string")]
    pub plan: Option<Option<String>>,
    pub contact_name: Option<String>,
    pub contact_email: Option<String>,
}

/// `UpdateTenantRequest` — body for `PATCH /api/v1/platform/tenants/{id}`.
///
/// Every field is typed as `Option<Option<String>>` so the handler can
/// distinguish three cases per the spec's absent-vs-null semantics:
///
///   * Field absent → `None` (do not touch the column).
///   * Field = `null` → `Some(None)`. For non-nullable columns (`name`,
///     `slug`, `plan`, `status`) this is an explicit *invalid* value
///     and produces 422. For nullable columns (`contactName`,
///     `contactEmail`) it is an explicit *clear* signal.
///   * Field = "x" → `Some(Some("x"))` (set the column, with validation).
///
/// `contact_name` and `contact_email` use the same three-state encoding
/// but the handler treats `Some(None)` and `Some(Some(""))` as a clear.
#[derive(Debug, Deserialize, Default, ToSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UpdateTenantRequest {
    #[serde(default, deserialize_with = "deserialize_optional_nullable_string")]
    pub name: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable_string")]
    pub slug: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable_string")]
    pub plan: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable_string")]
    pub status: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable_string")]
    pub contact_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable_string")]
    pub contact_email: Option<Option<String>>,
}

/// Helper: deserializes a JSON field that can be a string OR null OR absent,
/// and wraps it in `Option<Option<T>>` so the handler can distinguish:
///
///   * field absent → `None` (don't touch)
///   * field = `null` → `Some(None)` (clear the value, or reject for
///     non-nullable columns — the handler decides)
///   * field = "x" → `Some(Some("x"))` (set to "x")
///
/// This is the standard `map(Some)` trick. Naïvely calling
/// `Option::<Option<String>>::deserialize` would return `None` for both
/// absent and JSON `null` (serde's `Option` special-case treats `null` as
/// `None`), collapsing the two cases.
fn deserialize_optional_nullable_string<'de, D>(
    deserializer: D,
) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    Option::<String>::deserialize(deserializer).map(Some)
}

pub(crate) fn is_valid_email(value: &str) -> bool {
    // Total length check (RFC 5321)
    if value.len() > 254 {
        return false;
    }

    // Reject control characters (ASCII 0-31, 127) anywhere in the email
    if value.bytes().any(|b| b < 32 || b == 127) {
        return false;
    }

    // Must contain exactly one @
    if !value.contains('@')
        || value.starts_with('@')
        || value.ends_with('@')
        || value.contains("@@")
    {
        return false;
    }

    let parts: Vec<&str> = value.splitn(2, '@').collect();
    let local = parts[0];
    let domain = parts[1];

    // ---- Local part checks ----
    if local.is_empty() || local.len() > 64 {
        return false;
    }
    // Reject whitespace characters (not just space) and invalid ASCII ranges
    if local.contains(' ') || local.contains("..") || local.bytes().any(|b| b < 32 || b == 127) {
        return false;
    }
    if local.starts_with('.') || local.ends_with('.') {
        return false;
    }
    // First and last char must be alphanumeric or `"`
    let first = local.chars().next().unwrap();
    let last = local.chars().last().unwrap();
    if !(first.is_alphanumeric() || first == '"') || !(last.is_alphanumeric() || last == '"') {
        return false;
    }

    // ---- Domain checks ----
    // Reject whitespace characters and control characters in domain
    if domain.is_empty()
        || domain.contains(' ')
        || domain.contains("..")
        || domain.starts_with('.')
        || domain.ends_with('.')
        || domain.bytes().any(|b| b < 32 || b == 127)
    {
        return false;
    }

    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() < 2 {
        return false;
    }

    for (i, label) in labels.iter().enumerate() {
        if label.is_empty() || label.len() > 63 {
            return false;
        }
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return false;
        }
        if label.starts_with('-') || label.ends_with('-') {
            return false;
        }
        // TLD (last label) must be at least 2 alpha characters
        if i == labels.len() - 1
            && (label.len() < 2 || !label.chars().all(|c| c.is_ascii_alphabetic()))
        {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_emails() {
        assert!(is_valid_email("user@example.com"));
        assert!(is_valid_email("a.b@c.co"));
        assert!(is_valid_email("user@sub.example.com"));
        assert!(is_valid_email("user.name@example.com"));
        assert!(is_valid_email("user+tag@example.com"));
        assert!(is_valid_email("x@y.co"));
        assert!(is_valid_email("user@example.co.uk"));
    }

    #[test]
    fn test_invalid_emails_no_at() {
        assert!(!is_valid_email("not-an-email"));
        assert!(!is_valid_email(""));
        assert!(!is_valid_email("justtext"));
    }

    #[test]
    fn test_invalid_emails_at_boundary() {
        assert!(!is_valid_email("user@"));
        assert!(!is_valid_email("@example.com"));
        assert!(!is_valid_email("user@@example.com"));
    }

    #[test]
    fn test_invalid_emails_space() {
        assert!(!is_valid_email("user@exam ple.com"));
    }

    #[test]
    fn test_invalid_emails_no_domain_dot() {
        assert!(!is_valid_email("user@localhost"));
        assert!(!is_valid_email("user@example"));
    }

    #[test]
    fn test_invalid_emails_domain_consecutive_dots() {
        assert!(!is_valid_email("user@example..com"));
    }

    #[test]
    fn test_invalid_emails_local_part_too_long() {
        let local = "a".repeat(65);
        let email = format!("{local}@example.com");
        assert!(!is_valid_email(&email));
    }

    #[test]
    fn test_invalid_emails_domain_label_leading_hyphen() {
        assert!(!is_valid_email("user@-example.com"));
    }

    #[test]
    fn test_invalid_emails_domain_label_trailing_hyphen() {
        assert!(!is_valid_email("user@example-.com"));
    }

    #[test]
    fn test_invalid_emails_total_too_long() {
        // Build an email that exceeds 254 total chars
        let local = "a".repeat(200);
        let email = format!("{local}@b.co");
        assert!(!is_valid_email(&email));
    }

    #[test]
    fn test_invalid_emails_tld_too_short() {
        assert!(!is_valid_email("user@example.c"));
    }

    #[test]
    fn test_invalid_emails_tld_numeric() {
        assert!(!is_valid_email("user@example.123"));
    }

    #[test]
    fn test_invalid_emails_control_chars() {
        assert!(!is_valid_email("user\x00@example.com"));
        assert!(!is_valid_email("user\tabc@example.com"));
        assert!(!is_valid_email("user\n@example.com"));
        assert!(!is_valid_email("user\r@example.com"));
        assert!(!is_valid_email("user@exa\x1fmple.com"));
        assert!(!is_valid_email("user@example\x7f.com"));
        assert!(!is_valid_email("\x01user@example.com"));
        assert!(!is_valid_email("user@\x1bexample.com"));
    }

    #[test]
    fn test_invalid_emails_domain_leading_trailing_dot() {
        assert!(!is_valid_email("user@.example.com"));
        assert!(!is_valid_email("user@example.com."));
        assert!(!is_valid_email("user@.example.com."));
        assert!(!is_valid_email("user@example..com"));
        assert!(!is_valid_email("user@example.com..uk"));
    }

    #[test]
    fn test_invalid_emails_domain_label_leading_trailing_dot() {
        assert!(!is_valid_email("user@.example.com"));
        assert!(!is_valid_email("user@example.com."));
        assert!(!is_valid_email("user@.example.com."));
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct ListTenantsParams {
    pub q: Option<String>,
    pub status: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[utoipa::path(
    get,
    path = "/platform/tenants",
    tag = "platform-tenants",
    operation_id = "list_platform_tenants",
    summary = "List platform tenants",
    description = "List all platform tenants with cursor-based pagination and optional name/slug \
                  `q` and `status` filters. Requires permission: platform.tenants.list",
    params(ListTenantsParams),
    responses(
        (status = 200, description = "Page of platform tenants.", body = Page<TenantSummary>),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (invalid `status` filter).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn list_tenants(
    _principal: Principal,
    Query(params): Query<ListTenantsParams>,
    State(pool): State<sqlx::PgPool>,
) -> Response {
    if let Some(ref s) = params.status {
        if !TENANT_STATUSES.contains(&s.as_str()) {
            return ApiError::unprocessable_entity("Validation failed")
                .with_details(vec![kernel::ErrorDetail {
                    field: "status".into(),
                    code: "invalid_value".into(),
                    message: format!("Status must be one of: {}", TENANT_STATUSES.join(", ")),
                }])
                .into_response();
        }
    }

    let page = PageParams {
        limit: params.limit.unwrap_or(25),
        cursor: params.cursor.clone(),
    }
    .normalized();
    let limit = i64::from(page.limit + 1);

    let mut where_clauses: Vec<String> = vec!["deleted_at IS NULL".to_string()];
    let mut next_bind: usize = 1;

    if params.q.is_some() {
        where_clauses.push(format!(
            "(name ILIKE ${next_bind} OR slug ILIKE ${next_bind})"
        ));
        next_bind += 1;
    }
    if params.status.is_some() {
        where_clauses.push(format!("status = ${next_bind}"));
        next_bind += 1;
    }
    if page.cursor.is_some() {
        where_clauses.push(format!("id > ${next_bind}"));
        next_bind += 1;
    }

    let where_sql = where_clauses.join(" AND ");
    let order_limit = format!("ORDER BY id ASC LIMIT ${}", next_bind);
    let sql =
        format!("SELECT id, name, slug, status, plan FROM tenants WHERE {where_sql} {order_limit}");

    let mut query = sqlx::query(&sql);
    if let Some(ref q) = params.q {
        query = query.bind(format!("%{q}%"));
    }
    if let Some(ref s) = params.status {
        query = query.bind(s);
    }
    if let Some(ref cursor) = page.cursor {
        match hex_to_uuid(cursor) {
            Some(cid) => query = query.bind(cid),
            None => return ApiError::validation_failed("Invalid cursor").into_response(),
        }
    }
    query = query.bind(limit);

    let result = query.fetch_all(&pool).await;
    let rows = match result {
        Ok(r) => r,
        Err(e) => {
            return ApiError::internal_error(format!("Database query failed: {e}")).into_response();
        }
    };

    let has_more = rows.len() > page.limit as usize;
    let items: Vec<TenantSummary> = rows
        .into_iter()
        .take(page.limit as usize)
        .map(|r| TenantSummary {
            id: r.get("id"),
            name: r.get("name"),
            slug: r.get("slug"),
            status: r.get("status"),
            plan: r.get("plan"),
        })
        .collect();

    let next_cursor = items.last().map(|t| uuid_to_hex(&t.id));

    Json(Page {
        items,
        next_cursor: if has_more { next_cursor } else { None },
        has_more,
    })
    .into_response()
}

fn uuid_to_hex(id: &Uuid) -> String {
    let (hi, lo) = id.as_u64_pair();
    format!("{:016x}{:016x}", hi, lo)
}

fn hex_to_uuid(hex_str: &str) -> Option<Uuid> {
    if hex_str.len() != 32 {
        return None;
    }
    let hi = u64::from_str_radix(&hex_str[..16], 16).ok()?;
    let lo = u64::from_str_radix(&hex_str[16..], 16).ok()?;
    Some(Uuid::from_u64_pair(hi, lo))
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MeResponse {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub platform_role: Option<String>,
    pub platform_permissions: Vec<String>,
    pub staff_tenant_permissions: Option<Vec<String>>,
    pub memberships: Vec<MembershipSummary>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MembershipSummary {
    pub tenant_id: Uuid,
    pub tenant_name: String,
    pub tenant_slug: String,
    pub role: String,
    pub permissions: Vec<String>,
}

#[utoipa::path(
    get,
    path = "/me",
    tag = "identity",
    responses(
        (status = 200, description = "Current user principal and tenant membership summary. Requires an authenticated session.", body = MeResponse),
        (status = 401, description = "Authentication required", body = kernel::ErrorEnvelope),
        (status = 500, description = "Internal server error", body = kernel::ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn me(
    principal: Principal,
    State(pool): State<sqlx::PgPool>,
    Extension(config): Extension<Arc<AppConfig>>,
) -> Response {
    match build_me_response(
        &pool,
        principal,
        config.environment == config::Environment::Production,
    )
    .await
    {
        Ok(response) => Json(response).into_response(),
        Err(error) => error.into_response(),
    }
}

pub async fn build_me_response(
    pool: &sqlx::PgPool,
    principal: Principal,
    is_production: bool,
) -> Result<MeResponse, ApiError> {
    let rows = sqlx::query(
        r#"
        SELECT tm.tenant_id, t.name, t.slug, tm.role
        FROM tenant_memberships tm
        JOIN tenants t ON t.id = tm.tenant_id
        WHERE tm.user_id = $1
          AND tm.deleted_at IS NULL
          AND t.deleted_at IS NULL
        "#,
    )
    .bind(principal.user_id)
    .fetch_all(pool)
    .await
    .map_err(|error| ApiError::internal_error(format!("Database query failed: {error}")))?;

    let memberships = rows
        .iter()
        .map(|r| {
            let role: String = r.get("role");
            let permissions = authz::TenantRole::from_str(&role)
                .map(authz::tenant_role_permissions)
                .map(permission_codes)
                .unwrap_or_else(|error| {
                    tracing::error!(tenant.role = %role, %error, "unrecognized stored tenant role in /me");
                    Vec::new()
                });
            MembershipSummary {
                tenant_id: r.get("tenant_id"),
                tenant_name: r.get("name"),
                tenant_slug: r.get("slug"),
                role,
                permissions,
            }
        })
        .collect();

    let platform_permissions = principal
        .platform_role
        .map(authz::platform_role_permissions)
        .map(permission_codes)
        .unwrap_or_default();
    let staff_tenant_permissions = principal
        .platform_role
        .map(|role| permission_codes(authz::staff_tenant_permissions(role, is_production)));

    Ok(MeResponse {
        id: principal.user_id,
        email: principal.email,
        display_name: principal.display_name,
        platform_role: principal.platform_role.map(|r| r.to_string()),
        platform_permissions,
        staff_tenant_permissions,
        memberships,
    })
}

fn permission_codes(permissions: &[authz::Permission]) -> Vec<String> {
    permissions.iter().map(ToString::to_string).collect()
}

#[utoipa::path(
    get,
    path = "/tenant",
    tag = "tenant",
    responses(
        (status = 200, description = "Current tenant profile. Requires permission: overview.view.", body = serde_json::Value),
        (status = 401, description = "Authentication required", body = kernel::ErrorEnvelope),
        (status = 403, description = "Insufficient permissions", body = kernel::ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_tenant(State(pool): State<sqlx::PgPool>, ctx: TenantContext) -> Response {
    let row = match sqlx::query_as::<_, (Uuid, String, String, String, String)>(
        "SELECT id, name, slug, status, plan \
         FROM tenants \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(ctx.tenant_id)
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return ApiError::internal_error("Tenant not found after middleware check")
                .into_response()
        }
        Err(e) => {
            return ApiError::internal_error(format!("Database query failed: {e}")).into_response()
        }
    };

    Json(TenantSummary {
        id: row.0,
        name: row.1,
        slug: row.2,
        status: row.3,
        plan: row.4,
    })
    .into_response()
}

/// `GET /api/v1/platform/tenants/{id}` — fetch a single tenant by id.
///
/// Returns the full `PlatformTenantDetail` (including contact fields) for any
/// platform role holding `Permission::PlatformTenantsList`. Soft-deleted rows
/// (`deleted_at IS NOT NULL`) and unknown ids both return 404 `not_found`.
#[utoipa::path(
    get,
    path = "/platform/tenants/{id}",
    tag = "platform-tenants",
    operation_id = "get_platform_tenant",
    summary = "Get a platform tenant by id",
    description = "Fetch the full record for a single platform tenant, including contact fields \
                  and timestamps. Requires permission: platform.tenants.list",
    params(("id" = Uuid, Path, description = "Tenant identifier")),
    responses(
        (status = 200, description = "Tenant found.", body = PlatformTenantDetail),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Tenant not found.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_tenant_detail(
    _principal: Principal,
    Path(id): Path<Uuid>,
    State(pool): State<sqlx::PgPool>,
) -> Response {
    let row = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            chrono::DateTime<chrono::Utc>,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        "SELECT id, name, slug, status, plan, contact_name, contact_email, created_at, updated_at \
         FROM tenants WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&pool)
    .await;

    match row {
        Ok(Some((
            id,
            name,
            slug,
            status,
            plan,
            contact_name,
            contact_email,
            created_at,
            updated_at,
        ))) => Json(PlatformTenantDetail {
            id,
            name,
            slug,
            status,
            plan,
            contact_name,
            contact_email,
            created_at,
            updated_at,
        })
        .into_response(),
        Ok(None) => ApiError::not_found("Tenant not found").into_response(),
        Err(e) => ApiError::internal_error(format!("Database query failed: {e}")).into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/platform/tenants/{id}/switch",
    tag = "platform-tenants",
    operation_id = "switch_platform_tenant",
    summary = "Record a platform tenant switch",
    description = "Record that the current platform user is switching to a different platform \
                  tenant context. Returns the target tenant summary. Requires permission: \
                  platform.tenants.switch",
    params(("id" = Uuid, Path, description = "Tenant identifier")),
    responses(
        (status = 200, description = "Switch recorded; returns target tenant summary.", body = TenantSummary),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Access denied (tenant not found or insufficient permissions).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn switch_tenant(
    principal: Principal,
    Path(id): Path<uuid::Uuid>,
    State(pool): State<sqlx::PgPool>,
) -> Response {
    let row = match sqlx::query_as::<_, (Uuid, String, String, String, String)>(
        "SELECT id, name, slug, status, plan \
         FROM tenants \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(r)) => r,
        _ => {
            return ApiError::unauthorized("Access denied").into_response();
        }
    };

    let (id_v, name, slug, status, plan) = row;
    let id_str = id_v.to_string();
    audit::record(
        &pool,
        "platform.tenant_switched",
        Some(principal.user_id),
        Some(id_v),
        "tenant",
        Some(&id_str),
        &json!({"tenant_slug": &slug}),
    )
    .await;

    Json(TenantSummary {
        id: id_v,
        name,
        slug,
        status,
        plan,
    })
    .into_response()
}

/// `POST /api/v1/platform/tenants` — create a new customer organization.
///
/// Requires `Permission::PlatformTenantsManage` (enforced by the router).
/// Returns `201 Created` with the new tenant's full record, or a 4xx with
/// a per-field `ErrorDetail` envelope on validation/conflict.
#[utoipa::path(
    post,
    path = "/platform/tenants",
    tag = "platform-tenants",
    operation_id = "create_platform_tenant",
    summary = "Create a platform tenant",
    description = "Create a new customer organization. Returns 201 with the full record. \
                  Requires permission: platform.tenants.manage",
    request_body = CreateTenantRequest,
    responses(
        (status = 201, description = "Tenant created.", body = PlatformTenantDetail),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 409, description = "Slug is already in use.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (per-field).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn create_tenant(
    principal: Principal,
    State(pool): State<sqlx::PgPool>,
    kernel::ApiJson(payload): kernel::ApiJson<CreateTenantRequest>,
) -> Response {
    use kernel::ErrorDetail;

    // ---- Validate every field, accumulating errors so the UI can surface
    // them all at once (T059 / FR-011).  Each branch pushes a typed
    // `ErrorDetail` into the vec; nothing returns early.
    let mut details: Vec<ErrorDetail> = Vec::new();

    // ---- name: 1..=200 characters (required) ----
    // Character count uses `chars().count()` to match PostgreSQL `length()`
    // (T058): the CHECK constraint counts characters (code points), not
    // bytes. A 200-char multibyte name like 200×"日" is 600 bytes but
    // 200 characters and must be accepted.
    let name = match payload.name.as_deref().map(str::trim) {
        Some(n) if !n.is_empty() && n.chars().count() <= 200 => n.to_string(),
        _ => {
            details.push(ErrorDetail {
                field: "name".into(),
                code: "invalid_length".into(),
                message: "Name must be between 1 and 200 characters".into(),
            });
            String::new()
        }
    };

    // ---- slug: ^[a-z0-9](-?[a-z0-9])*$, 1..=63 chars, no double-hyphens ----
    // T044: validate the supplied slug AS-IS. Per the contract the slug must
    // match `^[a-z0-9](-?[a-z0-9])*$` exactly, so an uppercase character
    // produces 422 — the handler never silently lowercases caller input.
    // T096: validate the slug EXACTLY as supplied — no `str::trim()` or other
    // mutation before validation. Whitespace and trailing hyphens are invalid.
    let slug = payload.slug.as_deref().unwrap_or_default();
    let slug_valid = !slug.is_empty()
        && slug.len() <= 63
        && slug
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !slug.contains("--")
        && slug
            .chars()
            .last()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    if !slug_valid {
        details.push(ErrorDetail {
            field: "slug".into(),
            code: "invalid_format".into(),
            message: "Slug must be lowercase alphanumeric with optional single hyphens, starting with a letter or digit, max 63 characters".into(),
        });
    }

    // ---- plan: omitted → "trial" (default); explicit null → 422 (T101);
    // supplied blank → 422 (T057); supplied non-blank must be in TENANT_PLANS.
    // A blank value is NOT a clearing signal on create; clearing is only
    // possible through PATCH with an explicit JSON null.
    let plan = match payload.plan {
        None => "trial".to_string(),
        Some(None) => {
            details.push(ErrorDetail {
                field: "plan".into(),
                code: "invalid_value".into(),
                message: "Plan cannot be null; omit the field to use the default".into(),
            });
            String::new()
        }
        Some(Some(s)) => {
            if s.is_empty() {
                details.push(ErrorDetail {
                    field: "plan".into(),
                    code: "invalid_value".into(),
                    message: format!(
                        "Plan must be one of: {}; omit the field to use the default",
                        TENANT_PLANS.join(", ")
                    ),
                });
                String::new()
            } else if TENANT_PLANS.contains(&s.as_str()) {
                s.to_string()
            } else {
                details.push(ErrorDetail {
                    field: "plan".into(),
                    code: "invalid_value".into(),
                    message: format!("Plan must be one of: {}", TENANT_PLANS.join(", ")),
                });
                String::new()
            }
        }
    };

    // ---- contact_name: required when supplied; blank supplied is invalid (T057) ----
    // `None` and `Some(None)` both mean "no contact name"; `Some("")` is a
    // contract violation.  The DB column is nullable, so on omission we
    // simply do not bind anything. Character count uses `chars().count()`
    // to match PostgreSQL `length()` (T058 / migration 0016).
    let contact_name: Option<String> = match payload.contact_name.as_deref() {
        None => None,
        Some("") => {
            details.push(ErrorDetail {
                field: "contactName".into(),
                code: "invalid_value".into(),
                message:
                    "Contact name must not be blank when supplied; omit the field to leave it unset"
                        .into(),
            });
            None
        }
        Some(s) if s.chars().count() <= 200 => Some(s.to_string()),
        Some(_) => {
            details.push(ErrorDetail {
                field: "contactName".into(),
                code: "invalid_length".into(),
                message: "Contact name must be at most 200 characters".into(),
            });
            None
        }
    };

    // ---- contact_email: required when supplied; blank supplied is invalid (T057) ----
    let contact_email: Option<String> = match payload.contact_email.as_deref() {
        None => None,
        Some("") => {
            details.push(ErrorDetail {
                field: "contactEmail".into(),
                code: "invalid_value".into(),
                message: "Contact email must not be blank when supplied; omit the field to leave it unset".into(),
            });
            None
        }
        Some(e) => {
            if !is_valid_email(e) {
                details.push(ErrorDetail {
                    field: "contactEmail".into(),
                    code: "invalid_format".into(),
                    message: "Contact email must be a valid email address".into(),
                });
                None
            } else {
                Some(e.to_lowercase())
            }
        }
    };

    if !details.is_empty() {
        return ApiError::unprocessable_entity("Validation failed")
            .with_details(details)
            .into_response();
    }

    // ---- Begin transaction (T042 / FR-009 / Constitution III) ----
    // The tenant row and the platform.tenant_created audit row MUST commit
    // together — a tenant visible to the API without its audit row, or vice
    // versa, would violate the audit invariants. The slug-audit trigger
    // (migrations 0012-0015) only fires on UPDATE, not INSERT, so a plain
    // INSERT is sufficient for the data path here.
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return ApiError::internal_error(format!("Database transaction begin failed: {e}"))
                .into_response();
        }
    };

    let insert_result = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            chrono::DateTime<chrono::Utc>,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        "INSERT INTO tenants (name, slug, plan, contact_name, contact_email) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING id, name, slug, status, plan, contact_name, contact_email, created_at, updated_at",
    )
    .bind(&name)
    .bind(slug)
    .bind(&plan)
    .bind(contact_name.as_deref())
    .bind(contact_email.as_deref())
    .fetch_one(&mut *tx)
    .await;

    let row = match insert_result {
        Ok(r) => r,
        Err(sqlx::Error::Database(dbe)) if dbe.code().as_deref() == Some("23505") => {
            return ApiError::conflict("Slug is already in use")
                .with_details(vec![ErrorDetail {
                    field: "slug".into(),
                    code: "conflict".into(),
                    message: "A live tenant with this slug already exists".into(),
                }])
                .into_response();
        }
        Err(e) => {
            return ApiError::internal_error(format!("Database insert failed: {e}"))
                .into_response();
        }
    };

    let (id, name, slug, status, plan, contact_name, contact_email, created_at, updated_at) = row;
    let id_str = id.to_string();

    // ---- Audit (inside the same transaction as the INSERT) ----
    if let Err(e) = crate::audit::record_in_tx(
        &mut tx,
        "platform.tenant_created",
        Some(principal.user_id),
        Some(id),
        "tenant",
        Some(&id_str),
        &json!({
            "name": &name,
            "slug": &slug,
            "plan": &plan,
        }),
    )
    .await
    {
        return ApiError::internal_error(format!("Audit insert failed: {e}")).into_response();
    }

    if let Err(e) = tx.commit().await {
        return ApiError::internal_error(format!("Transaction commit failed: {e}")).into_response();
    }

    (
        axum::http::StatusCode::CREATED,
        Json(PlatformTenantDetail {
            id,
            name,
            slug,
            status,
            plan,
            contact_name,
            contact_email,
            created_at,
            updated_at,
        }),
    )
        .into_response()
}

/// `PATCH /api/v1/platform/tenants/{id}` — partial update of a tenant.
///
/// Requires `Permission::PlatformTenantsManage` (enforced by the router).
/// Returns 200 with the updated `PlatformTenantDetail`, 404 if unknown,
/// 409 on slug collision, 422 on validation failure.
#[utoipa::path(
    patch,
    path = "/platform/tenants/{id}",
    tag = "platform-tenants",
    operation_id = "update_platform_tenant",
    summary = "Update a platform tenant",
    description = "Partially update a platform tenant. Each field uses the absent-vs-null \
                  convention: omit a field to leave it unchanged; send `null` to clear a \
                  nullable field; supply a value to set it. Returns 200 with the full record. \
                  Requires permission: platform.tenants.manage",
    params(("id" = Uuid, Path, description = "Tenant identifier")),
    request_body = UpdateTenantRequest,
    responses(
        (status = 200, description = "Tenant updated.", body = PlatformTenantDetail),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Tenant not found.", body = ErrorEnvelope),
        (status = 409, description = "Slug is already in use.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (per-field).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn update_tenant(
    principal: Principal,
    Path(id): Path<Uuid>,
    State(pool): State<sqlx::PgPool>,
    kernel::ApiJson(payload): kernel::ApiJson<UpdateTenantRequest>,
) -> Response {
    use kernel::ErrorDetail;

    // ---- Validate provided fields, accumulating errors so the UI can
    // surface every problem in one round-trip (T059 / FR-011).  Nothing
    // returns early; the response is built from the accumulator at the end.
    //
    // Every PATCH field is `Option<Option<String>>` so we can distinguish
    // three cases per the spec's absent-vs-null contract (T056):
    //
    //   * `None`         → field absent, do not touch the column.
    //   * `Some(None)`   → explicit JSON `null`. For non-nullable columns
    //                      (`name`, `slug`, `plan`, `status`) this is
    //                      invalid and produces 422. For nullable columns
    //                      (`contactName`, `contactEmail`) it clears.
    //   * `Some(Some(s))`→ set the column, with validation. Blank `s` is
    //                      invalid for non-nullable columns and is a clear
    //                      signal for nullable columns.
    //
    // For the non-nullable fields, an invalid `new_*` (null, blank, or
    // otherwise invalid) is represented as `None` — meaning "do not SET
    // this column" — and the corresponding detail is pushed so the 422
    // response surfaces every problem.
    let mut details: Vec<ErrorDetail> = Vec::new();

    // ---- name ----
    // Character count uses `chars().count()` to match PostgreSQL `length()`
    // (T058): the CHECK constraint counts characters (code points), not
    // bytes. A 200-char multibyte name like 200×"日" is 600 bytes but
    // 200 characters and must be accepted.
    let new_name: Option<String> = match payload.name {
        None => None,
        Some(None) => {
            details.push(ErrorDetail {
                field: "name".into(),
                code: "invalid_value".into(),
                message: "Name cannot be null; omit the field to leave it unchanged".into(),
            });
            None
        }
        Some(Some(s)) => {
            let trimmed = s.trim();
            if trimmed.is_empty() || trimmed.chars().count() > 200 {
                details.push(ErrorDetail {
                    field: "name".into(),
                    code: "invalid_length".into(),
                    message: "Name must be between 1 and 200 characters".into(),
                });
                None
            } else {
                Some(trimmed.to_string())
            }
        }
    };

    // T044: validate the supplied slug AS-IS on PATCH as well. An uppercase
    // character must surface as 422, never be silently rewritten to lowercase.
    // T096: validate the slug EXACTLY as supplied — no `str::trim()` or mutation.
    // Whitespace and trailing hyphens are invalid.
    let new_slug: Option<String> = match payload.slug {
        None => None,
        Some(None) => {
            details.push(ErrorDetail {
                field: "slug".into(),
                code: "invalid_value".into(),
                message: "Slug cannot be null; omit the field to leave it unchanged".into(),
            });
            None
        }
        Some(Some(s)) => {
            if s.is_empty() {
                details.push(ErrorDetail {
                    field: "slug".into(),
                    code: "invalid_format".into(),
                    message: "Slug must be lowercase alphanumeric with optional single hyphens, \
                         starting with a letter or digit, max 63 characters"
                        .into(),
                });
                None
            } else {
                let valid = s.len() <= 63
                    && s.chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                    && s.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                    && !s.contains("--")
                    && s.chars()
                        .last()
                        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
                if !valid {
                    details.push(ErrorDetail {
                        field: "slug".into(),
                        code: "invalid_format".into(),
                        message:
                            "Slug must be lowercase alphanumeric with optional single hyphens, \
                             starting with a letter or digit, max 63 characters"
                                .into(),
                    });
                    None
                } else {
                    Some(s.to_string())
                }
            }
        }
    };

    // ---- plan ----
    let new_plan: Option<String> = match payload.plan {
        None => None,
        Some(None) => {
            details.push(ErrorDetail {
                field: "plan".into(),
                code: "invalid_value".into(),
                message: "Plan cannot be null; omit the field to leave it unchanged".into(),
            });
            None
        }
        Some(Some(s)) => {
            if s.is_empty() {
                details.push(ErrorDetail {
                    field: "plan".into(),
                    code: "invalid_value".into(),
                    message: format!(
                        "Plan must be one of: {}; omit the field to use the default",
                        TENANT_PLANS.join(", ")
                    ),
                });
                None
            } else if !TENANT_PLANS.contains(&s.as_str()) {
                details.push(ErrorDetail {
                    field: "plan".into(),
                    code: "invalid_value".into(),
                    message: format!("Plan must be one of: {}", TENANT_PLANS.join(", ")),
                });
                None
            } else {
                Some(s.to_string())
            }
        }
    };

    // ---- status ----
    let new_status: Option<String> = match payload.status {
        None => None,
        Some(None) => {
            details.push(ErrorDetail {
                field: "status".into(),
                code: "invalid_value".into(),
                message: "Status cannot be null; omit the field to leave it unchanged".into(),
            });
            None
        }
        Some(Some(s)) => {
            if s.is_empty() {
                details.push(ErrorDetail {
                    field: "status".into(),
                    code: "invalid_value".into(),
                    message: format!(
                        "Status must be one of: {}; omit the field to leave it unchanged",
                        TENANT_STATUSES.join(", ")
                    ),
                });
                None
            } else if !TENANT_STATUSES.contains(&s.as_str()) {
                details.push(ErrorDetail {
                    field: "status".into(),
                    code: "invalid_value".into(),
                    message: format!("Status must be one of: {}", TENANT_STATUSES.join(", ")),
                });
                None
            } else {
                Some(s.to_string())
            }
        }
    };

    // contact_name and contact_email use Option<Option<String>>:
    //   None       => absent (don't touch)
    //   Some(None) => explicit null (clear the column)
    //   Some(Some(s)) => validate, then set (blank `s` is a 422 error)
    //
    // The contact-name character count uses `chars().count()` to match
    // PostgreSQL `length()` (T058 / migration 0016).
    let new_contact_name: Option<Option<String>> = match payload.contact_name {
        None => None,
        Some(None) => Some(None),
        Some(Some(s)) => {
            if s.is_empty() {
                details.push(ErrorDetail {
                    field: "contactName".into(),
                    code: "invalid_value".into(),
                    message: "Contact name cannot be blank; use null to clear".into(),
                });
                None
            } else if s.chars().count() > 200 {
                details.push(ErrorDetail {
                    field: "contactName".into(),
                    code: "invalid_length".into(),
                    message: "Contact name must be at most 200 characters".into(),
                });
                None
            } else {
                Some(Some(s.to_string()))
            }
        }
    };

    let new_contact_email: Option<Option<String>> = match payload.contact_email {
        None => None,
        Some(None) => Some(None),
        Some(Some(e)) => {
            if e.is_empty() {
                details.push(ErrorDetail {
                    field: "contactEmail".into(),
                    code: "invalid_value".into(),
                    message: "Contact email cannot be blank; use null to clear".into(),
                });
                None
            } else if !is_valid_email(&e) {
                details.push(ErrorDetail {
                    field: "contactEmail".into(),
                    code: "invalid_format".into(),
                    message: "Contact email must be a valid email address".into(),
                });
                None
            } else {
                Some(Some(e.to_lowercase()))
            }
        }
    };

    if !details.is_empty() {
        return ApiError::unprocessable_entity("Validation failed")
            .with_details(details)
            .into_response();
    }

    // ---- Begin transaction (T042 / FR-009 / Constitution III) ----
    // The old-row read, the UPDATE, the slug-audit trigger (if the slug
    // changes), and both app-level audit rows (`platform.tenant_status_changed`
    // and `platform.tenant_updated`) must all commit atomically. Reading the
    // old row inside the transaction with `SELECT ... FOR UPDATE` serialises
    // concurrent PATCHes so each one's `old` value matches the actual
    // pre-PATCH state for that PATCH's transaction (not the latest committed
    // value at audit-insert time).
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return ApiError::internal_error(format!("Database transaction begin failed: {e}"))
                .into_response();
        }
    };

    // Set the audit actor (required by the slug-audit trigger if slug changes).
    // Setting it unconditionally is safe: the trigger only consumes it on a
    // slug change, and `set_config(..., true)` is transaction-local so it is
    // rolled back automatically on tx drop.
    if let Err(e) = sqlx::query("SELECT set_audit_actor($1)")
        .bind(principal.user_id)
        .execute(&mut *tx)
        .await
    {
        return ApiError::internal_error(format!("Failed to set audit actor: {e}")).into_response();
    }

    // ---- Build dynamic UPDATE ----
    let mut set_clauses: Vec<String> = Vec::new();
    let mut next_bind: usize = 1;
    if new_name.is_some() {
        set_clauses.push(format!("name = ${next_bind}"));
        next_bind += 1;
    }
    if new_slug.is_some() {
        set_clauses.push(format!("slug = ${next_bind}"));
        next_bind += 1;
    }
    if new_plan.is_some() {
        set_clauses.push(format!("plan = ${next_bind}"));
        next_bind += 1;
    }
    if new_status.is_some() {
        set_clauses.push(format!("status = ${next_bind}"));
        next_bind += 1;
    }
    if new_contact_name.is_some() {
        set_clauses.push(format!("contact_name = ${next_bind}"));
        next_bind += 1;
    }
    if new_contact_email.is_some() {
        set_clauses.push(format!("contact_email = ${next_bind}"));
        next_bind += 1;
    }

    // ---- Lock the existing row (or perform a no-op read) ----
    // For an actual update we MUST take a row lock first so the `old_*`
    // values used in the diff are serialised. For a no-op (no fields set)
    // we still need a row existence check, but a plain SELECT suffices —
    // there is nothing to mutate and no audit row to write.
    let existing_row: Option<(TenantRow, TenantRow)> = if set_clauses.is_empty() {
        let sql = "SELECT id, name, slug, status, plan, contact_name, contact_email, created_at, updated_at \
                   FROM tenants WHERE id = $1 AND deleted_at IS NULL";
        match sqlx::query_as::<_, TenantRow>(sql)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
        {
            // For a no-op the `old` and `new` snapshots are the same; the
            // diff at the bottom is empty so no audit row is written.
            Ok(Some(r)) => Some((r.clone(), r)),
            Ok(None) => None,
            Err(e) => {
                return ApiError::internal_error(format!("Database read failed: {e}"))
                    .into_response();
            }
        }
    } else {
        // Lock the row first so the `old_*` values reflect a serialised
        // snapshot of the row, not whatever happens to be the latest
        // committed value at the moment the audit row is written.
        let existing = match sqlx::query_as::<_, TenantRow>(
            "SELECT id, name, slug, status, plan, contact_name, contact_email, created_at, updated_at \
             FROM tenants WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        {
            Ok(Some(r)) => r,
            Ok(None) => return ApiError::not_found("Tenant not found").into_response(),
            Err(e) => {
                return ApiError::internal_error(format!("Database read failed: {e}"))
                    .into_response();
            }
        };

        let where_bind = next_bind;
        let sql = format!(
            "UPDATE tenants SET {} WHERE id = ${where_bind} AND deleted_at IS NULL \
             RETURNING id, name, slug, status, plan, contact_name, contact_email, created_at, updated_at",
            set_clauses.join(", ")
        );
        let mut query = sqlx::query_as::<_, TenantRow>(&sql);
        if let Some(ref n) = new_name {
            query = query.bind(n);
        }
        if let Some(ref s) = new_slug {
            query = query.bind(s);
        }
        if let Some(ref p) = new_plan {
            query = query.bind(p);
        }
        if let Some(ref s) = new_status {
            query = query.bind(s);
        }
        if let Some(ref cn) = new_contact_name {
            query = query.bind(cn.as_deref());
        }
        if let Some(ref ce) = new_contact_email {
            query = query.bind(ce.as_deref());
        }
        query = query.bind(id);

        match query.fetch_optional(&mut *tx).await {
            Ok(Some(r)) => Some((existing, r)),
            Ok(None) => return ApiError::not_found("Tenant not found").into_response(),
            Err(sqlx::Error::Database(dbe)) if dbe.code().as_deref() == Some("23505") => {
                return ApiError::conflict("Slug is already in use")
                    .with_details(vec![ErrorDetail {
                        field: "slug".into(),
                        code: "conflict".into(),
                        message: "A live tenant with this slug already exists".into(),
                    }])
                    .into_response();
            }
            Err(e) => {
                return ApiError::internal_error(format!("Database update failed: {e}"))
                    .into_response();
            }
        }
    };

    // For the no-op branch we have no `(old, new)` pair — just return the
    // current row.
    let (existing, row) = match existing_row {
        Some((existing, row)) => (existing, row),
        None => return ApiError::not_found("Tenant not found").into_response(),
    };
    let (
        _existing_id,
        existing_name,
        _existing_slug,
        existing_status,
        existing_plan,
        existing_contact_name,
        existing_contact_email,
        _existing_created_at,
        _existing_updated_at,
    ) = existing;
    let (
        id_v,
        name_v,
        slug_v,
        status_v,
        plan_v,
        contact_name_v,
        contact_email_v,
        created_at_v,
        updated_at_v,
    ) = row;

    // ---- Audit: status change (inside the same transaction) ----
    if let Some(ref new_s) = new_status {
        if new_s != &existing_status {
            let id_str = id_v.to_string();
            if let Err(e) = crate::audit::record_in_tx(
                &mut tx,
                "platform.tenant_status_changed",
                Some(principal.user_id),
                Some(id_v),
                "tenant",
                Some(&id_str),
                &json!({
                    "old_status": &existing_status,
                    "new_status": new_s,
                }),
            )
            .await
            {
                return ApiError::internal_error(format!("Audit insert failed: {e}"))
                    .into_response();
            }
        }
    }

    // ---- Audit: field changes (excluding slug and status) ----
    let mut changes = serde_json::Map::new();
    if let Some(ref n) = new_name {
        if n != &existing_name {
            changes.insert("name".to_string(), json!({"old": &existing_name, "new": n}));
        }
    }
    if let Some(ref p) = new_plan {
        if p != &existing_plan {
            changes.insert("plan".to_string(), json!({"old": &existing_plan, "new": p}));
        }
    }
    if let Some(ref new_cn) = new_contact_name {
        if new_cn.as_deref() != existing_contact_name.as_deref() {
            changes.insert(
                "contactName".to_string(),
                json!({
                    "old": existing_contact_name,
                    "new": new_cn,
                }),
            );
        }
    }
    if let Some(ref new_ce) = new_contact_email {
        if new_ce.as_deref() != existing_contact_email.as_deref() {
            changes.insert(
                "contactEmail".to_string(),
                json!({
                    "old": existing_contact_email,
                    "new": new_ce,
                }),
            );
        }
    }
    if !changes.is_empty() {
        let id_str = id_v.to_string();
        if let Err(e) = crate::audit::record_in_tx(
            &mut tx,
            "platform.tenant_updated",
            Some(principal.user_id),
            Some(id_v),
            "tenant",
            Some(&id_str),
            &json!({ "changes": serde_json::Value::Object(changes) }),
        )
        .await
        {
            return ApiError::internal_error(format!("Audit insert failed: {e}")).into_response();
        }
    }

    // ---- Commit the transaction ----
    // The slug-audit trigger (tenant.slug_changed) also fires here, but it
    // is owned by the DB trigger machinery (migrations 0014-0015) — it
    // runs in this same transaction and rolls back together with us if
    // the commit fails.
    if let Err(e) = tx.commit().await {
        return ApiError::internal_error(format!("Transaction commit failed: {e}")).into_response();
    }

    Json(PlatformTenantDetail {
        id: id_v,
        name: name_v,
        slug: slug_v,
        status: status_v,
        plan: plan_v,
        contact_name: contact_name_v,
        contact_email: contact_email_v,
        created_at: created_at_v,
        updated_at: updated_at_v,
    })
    .into_response()
}
