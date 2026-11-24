use std::sync::Arc;

use authz::Permission;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use identity::Principal;
use kernel::{ApiError, ApiJson, ErrorEnvelope};
use serde::Deserialize;
use sqlx::PgPool;
use tenancy::TenantContext;
use tokio_util::io::ReaderStream;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{Kind, RequestError, RequestStatus};

#[derive(Deserialize, ToSchema)]
pub struct Confirmation {
    confirm: Option<String>,
}

#[utoipa::path(
    post, path = "/tenant/customers/{id}/export", tag = "customers",
    request_body = Confirmation, params(("id" = Uuid, Path, description = "Customer identifier")),
    responses(
        (status = 202, description = "Export queued", body = RequestStatus),
        (status = 403, description = "Insufficient permissions", body = ErrorEnvelope),
        (status = 404, description = "Customer unavailable", body = ErrorEnvelope),
        (status = 409, description = "Customer is being erased", body = ErrorEnvelope),
        (status = 422, description = "Confirmation required", body = ErrorEnvelope)
    ), security(("session_cookie" = []))
)]
pub async fn export_customer(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    ApiJson(body): ApiJson<Confirmation>,
) -> Result<(StatusCode, Json<RequestStatus>), ApiError> {
    submit(&pool, ctx, principal.user_id, id, body, Kind::Export).await
}

#[utoipa::path(
    post, path = "/tenant/customers/{id}/erase", tag = "customers",
    request_body = Confirmation, params(("id" = Uuid, Path, description = "Customer identifier")),
    responses(
        (status = 202, description = "Erasure receipt", body = RequestStatus),
        (status = 403, description = "Insufficient permissions", body = ErrorEnvelope),
        (status = 404, description = "Customer unavailable", body = ErrorEnvelope),
        (status = 422, description = "Confirmation required", body = ErrorEnvelope)
    ), security(("session_cookie" = []))
)]
pub async fn erase_customer(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    ApiJson(body): ApiJson<Confirmation>,
) -> Result<(StatusCode, Json<RequestStatus>), ApiError> {
    submit(&pool, ctx, principal.user_id, id, body, Kind::Erase).await
}

async fn submit(
    pool: &PgPool,
    ctx: TenantContext,
    actor: Uuid,
    id: Uuid,
    body: Confirmation,
    kind: Kind,
) -> Result<(StatusCode, Json<RequestStatus>), ApiError> {
    if body
        .confirm
        .as_deref()
        .and_then(|value| Uuid::parse_str(value).ok())
        != Some(id)
    {
        return Err(
            ApiError::unprocessable_entity("Confirm with the customer identifier")
                .with_request_id(&ctx.request_id),
        );
    }
    let request = crate::request(pool, ctx.tenant_id, id, actor, kind)
        .await
        .map_err(|error| {
            match error {
                RequestError::NotFound => ApiError::not_found("Customer not found"),
                RequestError::Erasing => ApiError::conflict("Customer is being erased"),
                RequestError::Database(error) => internal(error),
            }
            .with_request_id(&ctx.request_id)
        })?;
    Ok((StatusCode::ACCEPTED, Json(request)))
}

#[utoipa::path(
    get, path = "/tenant/customers/{id}/privacy-requests/{request_id}", tag = "customers",
    params(("id" = Uuid, Path, description = "Customer identifier"),
           ("request_id" = Uuid, Path, description = "Privacy request identifier")),
    responses(
        (status = 200, description = "Request status", body = RequestStatus),
        (status = 403, description = "Insufficient permissions", body = ErrorEnvelope),
        (status = 404, description = "Request unavailable", body = ErrorEnvelope)
    ), security(("session_cookie" = []))
)]
pub async fn request_status(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Path((customer, request_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<RequestStatus>, ApiError> {
    if !ctx.permissions.contains(Permission::CustomersErase)
        && !ctx.permissions.contains(Permission::CustomersExport)
    {
        return Err(ApiError::unauthorized("Insufficient permissions"));
    }
    let mut work = db::UnitOfWork::begin(&pool, &ctx).await.map_err(internal)?;
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT kind, status FROM privacy_requests
         WHERE tenant_id = $1 AND customer_id = $2 AND id = $3",
    )
    .bind(ctx.tenant_id)
    .bind(customer)
    .bind(request_id)
    .fetch_optional(work.conn())
    .await
    .map_err(internal)?;
    work.commit().await.map_err(internal)?;
    let (kind, status) = row.ok_or_else(not_found)?;
    let permission = if kind == "erase" {
        Permission::CustomersErase
    } else {
        Permission::CustomersExport
    };
    if !ctx.permissions.contains(permission) {
        return Err(ApiError::unauthorized("Insufficient permissions"));
    }
    Ok(Json(RequestStatus { request_id, status }))
}

#[utoipa::path(
    get, path = "/tenant/customers/{id}/export/{request_id}/download", tag = "customers",
    params(("id" = Uuid, Path, description = "Customer identifier"),
           ("request_id" = Uuid, Path, description = "Privacy request identifier")),
    responses(
        (status = 200, description = "Customer export ZIP", content_type = "application/zip"),
        (status = 403, description = "Insufficient permissions", body = ErrorEnvelope),
        (status = 404, description = "Bundle unavailable", body = ErrorEnvelope)
    ), security(("session_cookie" = []))
)]
pub async fn download(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(storage): Extension<Arc<dyn storage::ObjectStorage>>,
    Path((customer, request)): Path<(Uuid, Uuid)>,
) -> Result<Response, ApiError> {
    let mut work = db::UnitOfWork::begin(&pool, &ctx).await.map_err(internal)?;
    let key: Option<String> = sqlx::query_scalar(
        "SELECT object_key FROM privacy_requests
         WHERE tenant_id = $1 AND customer_id = $2 AND id = $3 AND kind = 'export'
           AND status = 'completed' AND expires_at > now() AND object_key IS NOT NULL",
    )
    .bind(ctx.tenant_id)
    .bind(customer)
    .bind(request)
    .fetch_optional(work.conn())
    .await
    .map_err(internal)?;
    let key = key.ok_or_else(not_found)?;
    if key != format!("privacy-exports/{}/{request}.zip", ctx.tenant_id) {
        return Err(not_found());
    }
    if customers::customer_privacy::lock_customer(work.conn(), ctx.tenant_id, customer)
        .await
        .map_err(internal)?
        != Some(false)
    {
        return Err(not_found());
    }
    let reader = storage
        .get_reader(&key)
        .await
        .map_err(|error| match error {
            storage::StorageError::NotFound => not_found(),
            error => internal(error),
        })?;
    work.commit().await.map_err(internal)?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=customer-export.zip",
            ),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        Body::from_stream(ReaderStream::new(reader)),
    )
        .into_response())
}

fn internal(error: impl std::fmt::Display) -> ApiError {
    ApiError::internal_error_from("Privacy operation failed", error)
}

fn not_found() -> ApiError {
    ApiError::not_found("Privacy request or bundle not found")
}
