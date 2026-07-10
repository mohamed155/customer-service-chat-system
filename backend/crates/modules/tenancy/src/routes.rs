use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Json, Response},
    Extension,
};
use config::AppConfig;
use identity::Principal;
use kernel::{ApiError, Page, PageParams};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

use crate::{audit, authorize::fetch_tenant, TenantContext};

#[derive(Serialize)]
pub struct TenantSummary {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct ListTenantsQuery {
    pub q: Option<String>,
}

pub async fn list_tenants(
    _principal: Principal,
    Query(query): Query<ListTenantsQuery>,
    Query(params): Query<PageParams>,
    State(pool): State<sqlx::PgPool>,
) -> Response {
    let params = params.normalized();
    let limit = params.limit + 1;

    let limit = i64::from(limit);

    let result = if let Some(ref q) = query.q {
        let pattern = format!("%{}%", q);
        if let Some(ref cursor) = params.cursor {
            let cid = hex_to_uuid(cursor);
            match cid {
                Some(cid) => sqlx::query(
                    "SELECT id, name, slug, status FROM tenants WHERE deleted_at IS NULL AND id > $1 AND (name ILIKE $2 OR slug ILIKE $2) ORDER BY id ASC LIMIT $3"
                )
                .bind(cid)
                .bind(&pattern)
                .bind(limit)
                .fetch_all(&pool)
                .await,
                None => {
                    return ApiError::validation_failed("Invalid cursor").into_response();
                }
            }
        } else {
            sqlx::query(
                "SELECT id, name, slug, status FROM tenants WHERE deleted_at IS NULL AND (name ILIKE $1 OR slug ILIKE $1) ORDER BY id ASC LIMIT $2"
            )
            .bind(&pattern)
            .bind(limit)
            .fetch_all(&pool)
            .await
        }
    } else if let Some(ref cursor) = params.cursor {
        let cid = hex_to_uuid(cursor);
        match cid {
            Some(cid) => sqlx::query(
                "SELECT id, name, slug, status FROM tenants WHERE deleted_at IS NULL AND id > $1 ORDER BY id ASC LIMIT $2"
            )
            .bind(cid)
            .bind(limit)
            .fetch_all(&pool)
            .await,
            None => {
                return ApiError::validation_failed("Invalid cursor").into_response();
            }
        }
    } else {
        sqlx::query(
            "SELECT id, name, slug, status FROM tenants WHERE deleted_at IS NULL ORDER BY id ASC LIMIT $1"
        )
        .bind(limit)
        .fetch_all(&pool)
        .await
    };

    let rows = match result {
        Ok(r) => r,
        Err(e) => {
            return ApiError::internal_error(format!("Database query failed: {e}")).into_response();
        }
    };

    let has_more = rows.len() > params.limit as usize;
    let items: Vec<TenantSummary> = rows
        .into_iter()
        .take(params.limit as usize)
        .map(|r| TenantSummary {
            id: r.get("id"),
            name: r.get("name"),
            slug: r.get("slug"),
            status: r.get("status"),
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

#[derive(Serialize)]
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MembershipSummary {
    pub tenant_id: Uuid,
    pub tenant_name: String,
    pub tenant_slug: String,
    pub role: String,
    pub permissions: Vec<String>,
}

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

pub async fn get_tenant(State(pool): State<sqlx::PgPool>, ctx: TenantContext) -> Response {
    let row = match fetch_tenant(&pool, ctx.tenant_id).await {
        Some(r) => r,
        None => {
            return ApiError::internal_error("Tenant not found after middleware check")
                .into_response()
        }
    };

    Json(TenantSummary {
        id: row.id,
        name: row.name,
        slug: row.slug,
        status: row.status,
    })
    .into_response()
}

pub async fn switch_tenant(
    principal: Principal,
    Path(id): Path<uuid::Uuid>,
    State(pool): State<sqlx::PgPool>,
) -> Response {
    let row = match fetch_tenant(&pool, id).await {
        Some(r) => r,
        None => {
            return ApiError::unauthorized("Access denied").into_response();
        }
    };

    let id_str = id.to_string();
    audit::record(
        &pool,
        "platform.tenant_switched",
        Some(principal.user_id),
        Some(id),
        "tenant",
        Some(&id_str),
        &json!({"tenant_slug": &row.slug}),
    )
    .await;

    Json(TenantSummary {
        id: row.id,
        name: row.name,
        slug: row.slug,
        status: row.status,
    })
    .into_response()
}
