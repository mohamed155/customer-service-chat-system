use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Extension, Router,
};
use kernel::{ApiError, ErrorEnvelope};
use sqlx::PgPool;
use uuid::Uuid;

/// Public router for WhatsApp webhook endpoints.
/// Mounted outside tenant middleware — no auth, the token in the URL is the
/// credential. The server layers `Arc<MasterKey>` (via Extension) and body
/// size limit onto this router before merging into the main API router.
pub fn public_router() -> Router<PgPool> {
    Router::new()
        .route(
            "/integrations/whatsapp/webhook/{token}",
            get(super::webhook::verify_subscription),
        )
        .route(
            "/integrations/whatsapp/webhook/{token}",
            post(super::webhook::receive_message),
        )
}

/// Tenant-scoped router for WhatsApp endpoints.
/// Mounted under the tenant middleware so `TenantContext` is available.
pub fn tenant_router() -> Router<PgPool> {
    Router::new().route(
        "/conversations/{conversation_id}/attachments/{attachment_id}",
        get(download_attachment),
    )
}

/// `GET /tenant/conversations/{conversation_id}/attachments/{attachment_id}`
///
/// Stream a stored WhatsApp attachment from object storage. Returns 404 when
/// the attachment does not exist, is not in `stored` status, or belongs to a
/// different tenant / conversation.
#[utoipa::path(
    get,
    path = "/tenant/conversations/{conversation_id}/attachments/{attachment_id}",
    tag = "whatsapp",
    operation_id = "download_attachment",
    summary = "Download a WhatsApp attachment",
    description = "Stream a stored WhatsApp media attachment (image, audio, video, or document) \
                  from object storage. The attachment is looked up by id, verified to belong to \
                  the given conversation and tenant, and must be in `stored` status. Documents \
                  are returned with `Content-Disposition: attachment`. Requires permission: conversations.view",
    params(
        ("conversation_id" = Uuid, Path, description = "Conversation identifier"),
        ("attachment_id" = Uuid, Path, description = "Attachment identifier"),
    ),
    responses(
        (status = 200, description = "Attachment content.", content_type = "application/octet-stream"),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Attachment not found.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn download_attachment(
    State(pool): State<PgPool>,
    ctx: tenancy::TenantContext,
    Path((conversation_id, attachment_id)): Path<(Uuid, Uuid)>,
    Extension(storage): Extension<Arc<dyn storage::ObjectStorage>>,
) -> Response {
    let attachment = match super::queries::attachment_for_download(
        &pool,
        ctx.tenant_id,
        conversation_id,
        attachment_id,
    )
    .await
    {
        Ok(Some(a)) => a,
        Ok(None) => {
            return ApiError::not_found("Attachment not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(error) => {
            tracing::error!(%error, %attachment_id, %conversation_id, "attachment_for_download query failed");
            return ApiError::internal_error("Failed to retrieve attachment")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let storage_key = match attachment.storage_key {
        Some(ref key) => key.clone(),
        None => {
            return ApiError::not_found("Attachment not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let (bytes, content_type) = match storage.get(&storage_key).await {
        Ok(result) => result,
        Err(storage::StorageError::NotFound) => {
            return ApiError::not_found("Attachment not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(storage::StorageError::Other(error)) => {
            tracing::error!(%error, %storage_key, "storage get failed");
            return ApiError::internal_error("Failed to retrieve attachment")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );

    if attachment.kind == "document" {
        if let Some(ref file_name) = attachment.file_name {
            if let Ok(value) =
                HeaderValue::from_str(&format!("attachment; filename=\"{}\"", file_name))
            {
                headers.insert(header::CONTENT_DISPOSITION, value);
            }
        }
    }

    (StatusCode::OK, headers, bytes).into_response()
}
