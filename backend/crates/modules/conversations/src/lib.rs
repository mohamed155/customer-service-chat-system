//! # Conversations Module
//!
//! Owns full conversation and message management within a tenant-scoped
//! isolation boundary. This module covers the entire lifecycle of
//! conversations: creation, messaging (customer replies, agent replies,
//! internal notes), status transitions, assignment management, and the
//! original read-only history endpoint for the customer profile dashboard.
//!
//! Built as a superset of the earlier narrow-scope module — the original
//! `list_recent_for_customer` / `get_conversation_history` endpoints remain
//! in `lib.rs` unchanged.
//!
//! ## Ownership Model
//! Each conversation is owned by exactly one customer within one tenant:
//! `tenant 1 ──── N conversations` and `customer 1 ──── N conversations`.
//! Messages belong to exactly one conversation. The
//! `conversations_parent_tenant_fkey` composite FK (migration 0027) ensures
//! that `(tenant_id, customer_id)` always references a valid customer row in
//! the same tenant, preventing cross-tenant child rows at the DB level.
//!
//! ## One-Way Dependency: conversations → customers
//! This module depends on `customers::customer_exists` for a tenant-safe
//! existence check before issuing any conversation query. There is no
//! reverse dependency — the customers module does not import conversations.
//!
//! ## Responsibilities
//! - **Conversation CRUD**: create, list, read, update (status, assignment),
//!   soft-delete conversations.
//! - **Message management**: add customer messages, agent replies, and
//!   internal notes to a conversation.
//! - **Conversation history** (original): `list_recent_for_customer`,
//!   `list_recent_for_customer_in_tx`, and the `GET
//!   /tenant/customers/{id}/conversations` Axum handler.
//! - **Audit trail**: every write operation records an audit event via
//!   `audit::*` functions.
//!
//! ## Public Interfaces
//! ### Axum handlers (registered in `routes.rs`)
//! - `GET  /tenant/customers/{id}/conversations` — history (unchanged).
//! - `POST /tenant/conversations` — create conversation with first message.
//! - `GET  /tenant/conversations` — list conversations (staff dashboard).
//! - `GET  /tenant/conversations/{id}` — conversation detail with messages.
//! - `PATCH /tenant/conversations/{id}` — update status / assignment.
//! - `POST /tenant/conversations/{id}/messages` — add message.
//!
//! ### Reusable query functions
//! - `list_recent_for_customer(pool, tenant_id, customer_id) -> Result<Vec<ConversationSummary>>`
//! - `list_recent_for_customer_in_tx(tx, tenant_id, customer_id) -> Result<Vec<ConversationSummary>>`
//!
//! ## Dependencies
//! - `customers::customer_exists` — tenant-safe existence check before
//!   issuing any conversation query (enforces FR-011).
//! - `tenancy::TenantContext` — resolved tenant id injected by middleware.
//! - `kernel::ApiError` — error response construction.
//! - `model` — DTOs and payload types.
//! - `queries` — SQL query functions for CRUD and message operations.
//! - `audit` — audit log recording helpers.
//!
//! ## Data Model
//! ### Core tables
//! - **`conversations`**: `id`, `tenant_id`, `customer_id`, `channel`,
//!   `status`, `assigned_membership_id`, `last_activity_at`, `created_at`,
//!   `updated_at`, `deleted_at`
//! - **`messages`**: `id`, `conversation_id`, `tenant_id`, `kind`,
//!   `body`, `sender_type`, `sender_id`, `sender_membership_id`,
//!   `created_at`, `deleted_at`
//!
//! ### Constraints
//! - **Channel constraint**: `channel IN ('email', 'phone', 'web_chat',
//!   'whatsapp', 'telegram')`
//! - **Status constraint**: `status IN ('open', 'pending', 'resolved', 'closed')`
//! - **Message kind constraint**: `kind IN ('customer', 'reply', 'note')`
//! - **Foreign keys**: `conversations.tenant_id → tenants(id)`,
//!   `conversations.customer_id → customers(id)`, composite FK
//!   `conversations_parent_tenant_fkey` on `(tenant_id, customer_id)`
//!   referencing `customers(tenant_id, id)`, preventing cross-tenant child
//!   rows at the DB level.
//! - **Soft-delete**: both tables filter by `deleted_at IS NULL`.
//! - **Not cascade-deleted**: conversations are **not** cascade-soft-deleted
//!   when the parent customer is removed (migration 0030 explicitly excludes
//!   conversations), so the history section remains available for audit /
//!   review.
//!
//! ## Extension Points
//! - **Real-time updates**: WebSocket / SSE subscriptions for live
//!   conversation activity.
//! - **Attachments**: File upload and reference tracking on messages.
//! - **Rich message types**: Templates, quick replies, structured payloads.
//!
//! ## Boundaries
//! - **Tenant-scoped**: every query includes `tenant_id` from the resolved
//!   `TenantContext`.
//! - **No cross-tenant access**: composite FKs and application-layer checks
//!   enforce strict tenant isolation.
//!
//! ## Relationship to Customers
//! The `customers::customer_exists` gate ensures a request for a non-existent
//! or cross-tenant customer returns `404 not_found` before any conversation
//! query runs. This keeps the conversation module agnostic of tenant
//! isolation — it simply queries by `(tenant_id, customer_id)`.

pub mod audit;
pub mod model;
pub mod outbox;
pub mod queries;
pub mod routes;

use axum::{
    extract::{rejection::PathRejection, Path, State},
    response::{IntoResponse, Json, Response},
};
use chrono::{DateTime, Utc};
use kernel::ApiError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ConversationSummary {
    pub id: Uuid,
    pub channel: String,
    pub status: String,
    pub last_activity_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

const HISTORY_PAGE_SIZE: i64 = 20;

#[derive(Serialize)]
struct HistoryPagination {
    next_cursor: Option<String>,
    has_more: bool,
}

#[derive(Serialize)]
struct HistoryResponse {
    data: Vec<ConversationSummary>,
    pagination: HistoryPagination,
}

/// Fetches up to `HISTORY_PAGE_SIZE + 1` conversations for the supplied
/// customer, ordered by `last_activity_at DESC` (newest first). The
/// over-fetch by one row is what lets the handler report `has_more` without
/// a second count query (mirrors the pattern in `customers::list_customers`).
///
/// Filters out soft-deleted rows and is constrained to the resolved tenant —
/// the caller is responsible for the `customer_exists` check (T028 calls
/// `customers::customer_exists` first so an unknown / cross-tenant id
/// returns `not_found` before this query runs).
pub async fn list_recent_for_customer(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    customer_id: Uuid,
) -> sqlx::Result<Vec<ConversationSummary>> {
    sqlx::query_as::<_, ConversationSummary>(
        "SELECT id, channel, status, last_activity_at, created_at \
         FROM conversations \
         WHERE tenant_id = $1 AND customer_id = $2 AND deleted_at IS NULL \
         ORDER BY last_activity_at DESC \
         LIMIT $3",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(HISTORY_PAGE_SIZE + 1)
    .fetch_all(pool)
    .await
}

/// Transaction-aware variant — same query but runs inside an existing
/// transaction so callers can verify the customer and fetch conversation
/// history from one consistent snapshot.
pub async fn list_recent_for_customer_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    customer_id: Uuid,
) -> sqlx::Result<Vec<ConversationSummary>> {
    sqlx::query_as::<_, ConversationSummary>(
        "SELECT id, channel, status, last_activity_at, created_at \
         FROM conversations \
         WHERE tenant_id = $1 AND customer_id = $2 AND deleted_at IS NULL \
         ORDER BY last_activity_at DESC \
         LIMIT $3",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(HISTORY_PAGE_SIZE + 1)
    .fetch_all(&mut **tx)
    .await
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
/// a single Postgres transaction, ensuring both reads come from the same
/// database snapshot (T139).
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

    let mut tx = match pool.begin().await {
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

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use serde_json::json;
    use uuid::Uuid;

    use super::ConversationSummary;

    #[test]
    fn conversation_summary_serializes_to_the_contract_representation() {
        let summary = ConversationSummary {
            id: Uuid::parse_str("018f1f4d-5e6a-7b8c-9d0e-1f2a3b4c5d6e").unwrap(),
            channel: "web_chat".to_owned(),
            status: "open".to_owned(),
            last_activity_at: DateTime::parse_from_rfc3339("2026-07-13T09:30:00Z")
                .unwrap()
                .into(),
            created_at: DateTime::parse_from_rfc3339("2026-07-12T14:00:00Z")
                .unwrap()
                .into(),
        };

        assert_eq!(
            serde_json::to_value(summary).unwrap(),
            json!({
                "id": "018f1f4d-5e6a-7b8c-9d0e-1f2a3b4c5d6e",
                "channel": "web_chat",
                "status": "open",
                "last_activity_at": "2026-07-13T09:30:00Z",
                "created_at": "2026-07-12T14:00:00Z"
            })
        );
    }
}
