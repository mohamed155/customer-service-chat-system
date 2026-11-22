//! `GET /tenant/customers/{id}/conversations`: the conversation history on a customer's profile.

use axum::{
    extract::{rejection::PathRejection, Path, State},
    response::{IntoResponse, Json, Response},
};
use kernel::{ApiError, ErrorEnvelope};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::infrastructure::history::{
    list_recent_for_customer_in_tx, ConversationSummary, HISTORY_PAGE_SIZE,
};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HistoryPagination {
    next_cursor: Option<String>,
    has_more: bool,
}

/// `GET /tenant/customers/{id}/conversations` response envelope
/// (`{data, pagination}`).  Doc-only — the handler builds the body inline
/// with `Json(HistoryResponse { ... })`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HistoryResponse {
    pub data: Vec<ConversationSummary>,
    pub pagination: HistoryPagination,
}

/// `GET /tenant/customers/{customer_id}/conversations` — history section.
///
/// Calls `customers::customer_exists_in_tx` first so an unknown id, soft-deleted
/// row, or cross-tenant reference returns `404 not_found` (indistinguishable
/// from each other per FR-011). When the customer exists, returns the top
/// 20 conversations ordered by `last_activity_at DESC` with `has_more: true`
/// signalling that more rows exist beyond the 20-row window (FR-010).
///
/// Both the customer-existence check and the conversation query execute inside
/// one Postgres transaction. It runs at the default `READ COMMITTED` level, so each
/// statement sees the data committed when it starts, not one shared snapshot.
#[utoipa::path(
    get,
    path = "/tenant/customers/{id}/conversations",
    tag = "conversations",
    operation_id = "get_conversation_history",
    summary = "List a customer's recent conversations",
    description = "Return the top 20 conversations for the given customer, ordered by \
                  `last_activity_at DESC` (newest first). `pagination.has_more` is `true` when \
                  more than 20 rows exist beyond the returned window; `pagination.next_cursor` \
                  is currently always `null` (the windowed view does not yet support cursor \
                  pagination — the client can re-request to refresh). Returns 404 when the \
                  customer does not exist, is soft-deleted, or belongs to another tenant. \
                  Requires permission: customers.view",
    params(("id" = Uuid, Path, description = "Customer identifier")),
    responses(
        (status = 200, description = "Windowed history of conversations (data + pagination).", body = HistoryResponse),
        (status = 400, description = "Validation failed (invalid customer id).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Customer not found.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_conversation_history(
    State(pool): State<sqlx::PgPool>,
    ctx: tenancy::TenantContext,
    path: Result<Path<Uuid>, PathRejection>,
) -> Response {
    let customer_id = match path {
        Ok(Path(id)) => id,
        Err(_) => {
            return kernel::ApiError::validation_failed("Invalid customer id")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let mut tx = match db::UnitOfWork::begin(&pool, &ctx).await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "failed to begin history transaction");
            return ApiError::internal_error("Failed to load conversation history")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    match customers::customer_exists_in_tx(&mut tx, ctx.tenant_id, customer_id).await {
        Ok(true) => {}
        Ok(false) => {
            return ApiError::not_found("Customer not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(error) => {
            tracing::error!(%error, "failed to check customer existence");
            return ApiError::internal_error("Failed to verify customer")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    let summaries = match list_recent_for_customer_in_tx(&mut tx, ctx.tenant_id, customer_id).await
    {
        Ok(summaries) => summaries,
        Err(error) => {
            tracing::error!(
                %error,
                customer_id = %customer_id,
                "failed to fetch conversation history"
            );
            return kernel::ApiError::internal_error("Failed to load conversation history")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if let Err(error) = tx.commit().await {
        tracing::error!(%error, "failed to commit history transaction");
        return ApiError::internal_error("Failed to load conversation history")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let has_more = summaries.len() > HISTORY_PAGE_SIZE as usize;
    let data: Vec<ConversationSummary> = summaries
        .into_iter()
        .take(HISTORY_PAGE_SIZE as usize)
        .collect();

    Json(HistoryResponse {
        data,
        pagination: HistoryPagination {
            next_cursor: None,
            has_more,
        },
    })
    .into_response()
}
