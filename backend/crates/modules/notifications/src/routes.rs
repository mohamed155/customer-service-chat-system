use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use identity::Principal;
use kernel::ApiError;
use serde::Deserialize;
use sqlx::PgPool;
use tenancy::TenantContext;
use uuid::Uuid;

use crate::model::{
    MarkedResponse, NotificationActorDto, NotificationDto, NotificationListResponse,
    NotificationState, PaginationInfo, UnreadCountResponse,
};
use crate::queries;

#[derive(Deserialize)]
pub struct ListNotificationsQuery {
    cursor: Option<String>,
    limit: Option<i64>,
    state: Option<String>,
}

async fn active_membership_id(pool: &PgPool, tenant_id: Uuid, user_id: Uuid) -> Result<Uuid, Response> {
    match sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM tenant_memberships \
         WHERE tenant_id = $1 AND user_id = $2 AND status = 'active' AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    {
        Ok(Some(mid)) => Ok(mid),
        Ok(None) => Err(ApiError::validation_failed("No active membership in this tenant").into_response()),
        Err(e) => {
            tracing::error!(%e, "active_membership_id: query failed");
            Err(ApiError::internal_error("Database error").into_response())
        }
    }
}

async fn actor_display_names(pool: &PgPool, membership_ids: &[Uuid]) -> HashMap<Uuid, String> {
    if membership_ids.is_empty() {
        return HashMap::new();
    }
    let rows = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT DISTINCT ON (tm.id) tm.id, u.display_name \
         FROM tenant_memberships tm \
         JOIN users u ON u.id = tm.user_id \
         WHERE tm.id = ANY($1)",
    )
    .bind(membership_ids)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    rows.into_iter().collect()
}

fn row_to_dto(row: &crate::model::NotificationRow, names: &HashMap<Uuid, String>) -> NotificationDto {
    let actor = row.actor_membership_id.map(|mid| NotificationActorDto {
        membership_id: mid,
        display_name: names.get(&mid).cloned().unwrap_or_default(),
    });

    NotificationDto {
        id: row.id,
        kind: row.kind.clone(),
        state: row.state.clone(),
        title: row.title.clone(),
        body: row.body.clone(),
        subject_type: row.subject_type.clone(),
        subject_id: row.subject_id,
        actor,
        created_at: row.created_at,
        read_at: row.read_at,
    }
}

#[utoipa::path(
    get,
    path = "/tenant/notifications",
    tag = "notifications",
    operation_id = "list_notifications",
    summary = "List notifications for the current tenant member",
    params(
        ("cursor" = Option<String>, Query, description = "Opaque pagination cursor"),
        ("limit" = Option<i64>, Query, description = "Items per page, clamped 1..=50"),
        ("state" = Option<String>, Query, description = "Filter by state: unread, read, resolved"),
    ),
    responses(
        (status = 200, description = "Notification list.", body = NotificationListResponse),
        (status = 400, description = "No active membership in this tenant."),
        (status = 422, description = "Invalid query parameters."),
    ),
)]
pub async fn list_notifications(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Query(query): Query<ListNotificationsQuery>,
) -> Response {
    let membership_id = match active_membership_id(&pool, ctx.tenant_id, principal.user_id).await {
        Ok(mid) => mid,
        Err(resp) => return resp,
    };

    let state = match query.state.as_deref() {
        Some(s) => match s {
            "unread" => Some(NotificationState::Unread),
            "read" => Some(NotificationState::Read),
            "resolved" => Some(NotificationState::Resolved),
            _ => {
                return ApiError::unprocessable_entity(format!("Unknown state: {s}"))
                    .into_response()
            }
        },
        None => None,
    };

    if let Some(ref cursor) = query.cursor {
        if queries::decode_cursor(cursor).is_none() {
            return ApiError::unprocessable_entity("Invalid cursor").into_response();
        }
    }

    let limit = query.limit.unwrap_or(20).clamp(1, 50);

    let (rows, next_cursor) = match queries::list(
        &pool,
        ctx.tenant_id,
        membership_id,
        state.as_ref(),
        query.cursor.as_deref(),
        limit,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(%e, "list_notifications: list failed");
            return ApiError::internal_error("Failed to list notifications").into_response();
        }
    };

    let actor_mids: Vec<Uuid> = rows.iter().filter_map(|r| r.actor_membership_id).collect();
    let names = actor_display_names(&pool, &actor_mids).await;

    let data: Vec<NotificationDto> = rows.iter().map(|r| row_to_dto(r, &names)).collect();

    let has_more = next_cursor.is_some();
    let response = NotificationListResponse {
        data,
        pagination: PaginationInfo {
            next_cursor,
            has_more,
        },
    };

    (StatusCode::OK, Json(response)).into_response()
}

#[utoipa::path(
    get,
    path = "/tenant/notifications/unread-count",
    tag = "notifications",
    operation_id = "get_unread_notification_count",
    summary = "Get unread notification count",
    responses(
        (status = 200, description = "Unread count.", body = UnreadCountResponse),
        (status = 400, description = "No active membership in this tenant."),
    ),
)]
pub async fn get_unread_notification_count(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
) -> Response {
    let membership_id = match active_membership_id(&pool, ctx.tenant_id, principal.user_id).await {
        Ok(mid) => mid,
        Err(resp) => return resp,
    };

    let count = match queries::unread_count(&pool, ctx.tenant_id, membership_id).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(%e, "get_unread_notification_count failed");
            return ApiError::internal_error("Failed to get unread count").into_response();
        }
    };

    (StatusCode::OK, Json(UnreadCountResponse { count })).into_response()
}

#[utoipa::path(
    post,
    path = "/tenant/notifications/{id}/read",
    tag = "notifications",
    operation_id = "mark_notification_read",
    summary = "Mark a notification as read",
    params(
        ("id" = Uuid, Path, description = "Notification ID"),
    ),
    responses(
        (status = 200, description = "Updated notification.", body = NotificationDto),
        (status = 400, description = "No active membership in this tenant."),
        (status = 404, description = "Notification not found or belongs to another member."),
    ),
)]
pub async fn mark_notification_read(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Path(notification_id): Path<Uuid>,
) -> Response {
    let membership_id = match active_membership_id(&pool, ctx.tenant_id, principal.user_id).await {
        Ok(mid) => mid,
        Err(resp) => return resp,
    };

    let row = match queries::mark_read(&pool, ctx.tenant_id, membership_id, notification_id).await {
        Ok(Some(row)) => row,
        Ok(None) => return ApiError::not_found("Notification not found").into_response(),
        Err(e) => {
            tracing::error!(%e, "mark_notification_read failed");
            return ApiError::internal_error("Failed to mark notification read").into_response();
        }
    };

    let mids: Vec<Uuid> = row.actor_membership_id.into_iter().collect();
    let names = actor_display_names(&pool, &mids).await;
    let dto = row_to_dto(&row, &names);

    (StatusCode::OK, Json(dto)).into_response()
}

#[utoipa::path(
    post,
    path = "/tenant/notifications/read-all",
    tag = "notifications",
    operation_id = "mark_all_notifications_read",
    summary = "Mark all unread notifications as read",
    responses(
        (status = 200, description = "Number of notifications marked read.", body = MarkedResponse),
        (status = 400, description = "No active membership in this tenant."),
    ),
)]
pub async fn mark_all_notifications_read(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
) -> Response {
    let membership_id = match active_membership_id(&pool, ctx.tenant_id, principal.user_id).await {
        Ok(mid) => mid,
        Err(resp) => return resp,
    };

    let marked = match queries::mark_all_read(&pool, ctx.tenant_id, membership_id).await {
        Ok(m) => m,
        Err(e) => {
            tracing::error!(%e, "mark_all_notifications_read failed");
            return ApiError::internal_error("Failed to mark all read").into_response();
        }
    };

    (StatusCode::OK, Json(MarkedResponse { marked })).into_response()
}
