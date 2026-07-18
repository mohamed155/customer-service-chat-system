//! # Customer Profiles Module
//!
//! Manages customer records (profiles, channel identifiers, conversation
//! history) within a tenant scope.
//!
//! ## Public Interface
//! - `customer_exists(pool, tenant_id, customer_id) -> sqlx::Result<bool>` —
//!   lightweight existence check called by the `conversations` module.
//!
//! ## Sub-modules
//! - `audit` — customer-created / customer-updated audit log helpers.
//! - `model` — domain types, create/update payloads, validation.
//! - `routes` — Axum handlers for CRUD + search + conversation history.
//!
//! ## Dependencies
//! - `tenancy` (TenantContext middleware for isolation)
//! - `identity` (Principal extraction for audit actor)
//!
//! ## Data Model
//!
//! ### Tables
//! - **`customers`** — tenant-scoped, soft-deletable customer profiles.
//! - **`customer_channel_identifiers`** — channel-scoped contact identifiers
//!   (unique per `tenant_id, channel, identifier`).
//! - **`conversations`** — tenant + customer scoped conversation summaries.
//!
//! ### Migrations
//! - **0027** — Composite FK constraints on `customer_channel_identifiers` and
//!   `conversations`: `(tenant_id, customer_id)` → `customers(tenant_id, id)`.
//!   Prevents cross-tenant child rows at the database level.
//! - **0029** — Soft-delete support for `customer_channel_identifiers`
//!   (`deleted_at` column) + partial unique index
//!   `customer_channel_identifiers_live_unique_idx` enforcing uniqueness only
//!   over live (non-deleted) rows.
//! - **0030** — Database trigger: when a customer is soft-deleted, cascade
//!   `deleted_at = NOW()` to all child `customer_channel_identifiers` rows.
//!   Conversations are **excluded** from this cascade — they remain available
//!   in the history API for audit / review.
//!
//! ### Identifier Soft Deletion
//! Identifiers are soft-deleted (stamped `deleted_at`) rather than hard-deleted
//! when a customer is removed (migration 0030 cascade) or when the identifier
//! set is replaced during an update (PATCH handler). The partial unique index
//! (migration 0029) allows multiple `deleted_at`-stamped rows for the same
//! `(tenant_id, channel, identifier)` while keeping live rows unique.
//!
//! ### Physical Retention vs API Availability
//! Soft-deleted customer and identifier rows are physically retained but
//! filtered out of all API responses via `WHERE deleted_at IS NULL` on every
//! GET / LIST / SEARCH query (detailed below under *Physical Retention*).
//!
//! ## Soft-Delete Convention
//! Customers use a `deleted_at` timestamp for soft-delete. Active rows have
//! `deleted_at IS NULL`. The `customer_channel_identifiers` table follows the
//! same convention (migration 0029), with a partial unique index
//! (`customer_channel_identifiers_live_unique_idx`) enforcing uniqueness only
//! over live (non-deleted) rows. When a customer is soft-deleted, migration
//! 0030's cascade trigger automatically stamps its channel identifiers. The
//! `conversations` module is **not** cascade-deleted — conversation records
//! are retained for historical reference even after the customer is removed.
//!
//! ## Server Route Composition
//! This module defines its handlers in `routes`, but routes are registered
//! in `server/router.rs`, not within the module itself. The router layer
//! attaches `customers.view` / `customers.manage` permission guards.
//!
//! ## Relationship to Conversations
//! This module has **no dependency** on `conversations`. The customer profile
//! detail endpoint delegates to `conversations::get_conversation_history` for
//! the history section, but that composition happens at the server/router.rs
//! layer — the customers module itself never imports or calls conversations.
//! The dependency direction is `conversations → customers` (via
//! `customers::customer_exists`), not the reverse.
//!
//! ## Extension Points
//! - **Customer deletion**: The soft-delete infrastructure already exists.
//!   A future `DELETE /tenant/customers/{id}` handler would set `deleted_at`
//!   and let the cascade trigger handle channel identifiers. Get / List /
//!   Search handlers already filter `WHERE deleted_at IS NULL`.
//! - **Customer merging**: A merge endpoint would atomically reassign channel
//!   identifiers and conversations from a source customer to a target,
//!   then soft-delete the source. The existing composite FK constraints
//!   (`customer_channel_identifiers_parent_tenant_fkey` and
//!   `conversations_parent_tenant_fkey`) ensure `tenant_id` consistency
//!   during reassignment.
//! - **Bulk operations**: Import/export features would use the same
//!   validation logic in `model` and the same audit helpers in `audit`.
//! - **Custom fields**: The `metadata` JSONB column already supports
//!   arbitrary key-value pairs (capped at 50 keys). Future feature
//!   requirements like custom schema validation can be added to the
//!   `model` validation layer without schema changes.
//!
//! All public endpoints are gated by `customers.view` / `customers.manage`
//! permissions enforced at the router layer.

pub mod audit;
pub mod model;
pub mod queries;
pub mod routes;

/// Returns whether an active customer belongs to the supplied tenant.
pub async fn customer_exists(
    pool: &sqlx::PgPool,
    tenant_id: uuid::Uuid,
    customer_id: uuid::Uuid,
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS( \
         SELECT 1 FROM customers \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL \
         )",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(pool)
    .await
}

/// Transaction-aware variant — same check but inside an existing transaction
/// so callers can atomically verify the customer and read related data from
/// one consistent snapshot.
pub async fn customer_exists_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: uuid::Uuid,
    customer_id: uuid::Uuid,
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS( \
         SELECT 1 FROM customers \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL \
         )",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&mut **tx)
    .await
}
