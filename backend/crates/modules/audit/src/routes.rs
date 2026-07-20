use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{NaiveDate, Utc};
use kernel::ApiError;
use sqlx::PgPool;

use crate::model;
use crate::queries;

pub(crate) struct ResolvedAuditQuery {
    pub from_ts: Option<chrono::DateTime<Utc>>,
    pub to_ts: Option<chrono::DateTime<Utc>>,
    pub category_prefixes: Option<Vec<String>>,
    pub cursor: Option<(chrono::DateTime<Utc>, uuid::Uuid)>,
    pub limit: i64,
}

pub(crate) fn parse_audit_query(
    query: &model::AuditQuery,
) -> Result<ResolvedAuditQuery, Response> {
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let from_ts = match query.from.as_deref() {
        Some(s) => {
            let d = NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map_err(|_| ApiError::unprocessable_entity("Invalid 'from' date, expected YYYY-MM-DD").into_response())?;
            Some(d.and_hms_opt(0, 0, 0).unwrap().and_utc())
        }
        None => None,
    };
    let to_ts = match query.to.as_deref() {
        Some(s) => {
            let d = NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map_err(|_| ApiError::unprocessable_entity("Invalid 'to' date, expected YYYY-MM-DD").into_response())?;
            Some(d.and_hms_opt(23, 59, 59).unwrap().and_utc())
        }
        None => None,
    };
    let category_prefixes = match query.category.as_deref() {
        Some(cat) => Some(model::prefixes_for_category(cat).ok_or_else(|| {
            ApiError::unprocessable_entity(format!("Unknown category: {cat}")).into_response()
        })?),
        None => None,
    };
    let cursor = match query.cursor.as_deref() {
        Some(c) => Some(
            queries::decode_cursor(c)
                .ok_or_else(|| ApiError::unprocessable_entity("Invalid cursor").into_response())?,
        ),
        None => None,
    };
    Ok(ResolvedAuditQuery {
        from_ts,
        to_ts,
        category_prefixes,
        cursor,
        limit,
    })
}

fn row_to_dto(row: &queries::AuditRow) -> model::AuditEntryDto {
    let (actor_kind, actor_id, display_name, email, is_platform_staff, deleted) =
        match &row.actor_user_id {
            Some(uid) => (
                "user".to_string(),
                Some(*uid),
                row.display_name.clone(),
                row.email.clone(),
                row.platform_role.is_some(),
                row.deleted_at.is_some(),
            ),
            None => ("system".to_string(), None, None, None, false, false),
        };
    model::AuditEntryDto {
        id: row.id,
        action: row.action.clone(),
        category: model::category_for_action(&row.action).to_string(),
        actor: model::AuditActorDto {
            kind: actor_kind,
            id: actor_id,
            display_name,
            email,
            is_platform_staff,
            deleted,
        },
        resource_type: row.resource_type.clone(),
        resource_id: row.resource_id.clone(),
        tenant_id: row.tenant_id,
        details: row.details.clone(),
        created_at: row.created_at,
    }
}

fn build_response(rows: &[queries::AuditRow], limit: i64) -> model::AuditListResponse {
    let has_more = rows.len() > limit as usize;
    let rows = if has_more {
        &rows[..limit as usize]
    } else {
        rows
    };
    let entries: Vec<model::AuditEntryDto> = rows.iter().map(row_to_dto).collect();
    let next_cursor = has_more.then(|| {
        let last = rows.last().expect("rows must be non-empty when has_more is true");
        queries::encode_cursor(last.created_at, last.id)
    });
    model::AuditListResponse {
        data: entries,
        pagination: model::AuditPagination {
            next_cursor,
            has_more,
        },
    }
}

#[utoipa::path(
    get,
    path = "/tenant/audit-logs",
    tag = "audit",
    operation_id = "list_tenant_audit_logs",
    summary = "Tenant-scoped audit log list",
    params(
        ("cursor" = Option<String>, Query, description = "Opaque pagination cursor"),
        ("limit" = Option<i64>, Query, description = "Items per page, clamped 1..=100"),
        ("from" = Option<String>, Query, description = "Inclusive UTC start date YYYY-MM-DD"),
        ("to" = Option<String>, Query, description = "Inclusive UTC end date YYYY-MM-DD"),
        ("category" = Option<String>, Query, description = "Category filter"),
        ("actor_id" = Option<uuid::Uuid>, Query, description = "Filter by actor user id"),
    ),
    responses(
        (status = 200, description = "Audit log entries.", body = model::AuditListResponse),
        (status = 403, description = "Forbidden — missing audit.view permission."),
        (status = 422, description = "Invalid query parameters."),
    ),
)]
pub async fn list_tenant_audit_logs(
    State(pool): State<PgPool>,
    ctx: tenancy::TenantContext,
    Query(query): Query<model::AuditQuery>,
) -> Response {
    let resolved = match parse_audit_query(&query) {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    let fetch_limit = resolved.limit + 1;
    let rows = match queries::list_entries(
        &pool,
        Some(ctx.tenant_id),
        None,
        resolved.from_ts,
        resolved.to_ts,
        resolved.category_prefixes,
        query.actor_id,
        resolved.cursor,
        fetch_limit,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(%e, "list_tenant_audit_logs: list_entries failed");
            return ApiError::internal_error("Failed to load audit logs").into_response();
        }
    };

    let response = build_response(&rows, resolved.limit);
    (StatusCode::OK, Json(response)).into_response()
}

#[utoipa::path(
    get,
    path = "/platform/audit-logs",
    tag = "audit",
    operation_id = "list_platform_audit_logs",
    summary = "Platform-wide audit log list",
    params(
        ("cursor" = Option<String>, Query, description = "Opaque pagination cursor"),
        ("limit" = Option<i64>, Query, description = "Items per page, clamped 1..=100"),
        ("from" = Option<String>, Query, description = "Inclusive UTC start date YYYY-MM-DD"),
        ("to" = Option<String>, Query, description = "Inclusive UTC end date YYYY-MM-DD"),
        ("category" = Option<String>, Query, description = "Category filter"),
        ("actor_id" = Option<uuid::Uuid>, Query, description = "Filter by actor user id"),
        ("tenant_id" = Option<uuid::Uuid>, Query, description = "Optional tenant filter for platform-level queries"),
    ),
    responses(
        (status = 200, description = "Audit log entries.", body = model::AuditListResponse),
        (status = 403, description = "Forbidden — missing platform_audit.view permission."),
        (status = 422, description = "Invalid query parameters."),
    ),
)]
pub async fn list_platform_audit_logs(
    State(pool): State<PgPool>,
    Query(query): Query<model::AuditQuery>,
) -> Response {
    let resolved = match parse_audit_query(&query) {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    let fetch_limit = resolved.limit + 1;
    let rows = match queries::list_entries(
        &pool,
        None,
        query.tenant_id,
        resolved.from_ts,
        resolved.to_ts,
        resolved.category_prefixes,
        query.actor_id,
        resolved.cursor,
        fetch_limit,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(%e, "list_platform_audit_logs: list_entries failed");
            return ApiError::internal_error("Failed to load audit logs").into_response();
        }
    };

    let response = build_response(&rows, resolved.limit);
    (StatusCode::OK, Json(response)).into_response()
}
