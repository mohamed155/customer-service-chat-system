//! # Billing Module
//!
//! ## Purpose
//! Tenant-scoped billing: usage-based metering, invoice generation, payment
//! tracking, plan enforcement, and audit logging for all billable events.
//!
//! ## Responsibilities
//! - Meter and record billable events (messages, AI generations, storage, etc.)
//!   per tenant.
//! - Enforce plan limits (rate caps, overage thresholds) at request time.
//! - Generate periodic invoices and track payment lifecycle.
//! - Maintain an append-only audit trail of billing-related mutations.
//! - Expose REST endpoints guarded by `billing.admin`.
//!
//! ## Public Interfaces
//! - `GET /tenant/billing/usage` — current billing-period usage breakdown
//! - `GET /tenant/billing/invoices` — paginated invoice list
//! - `GET /tenant/billing/invoices/:id` — single invoice detail
//! - `POST /tenant/billing/invoices/:id/pay` — record a payment
//! - `GET /tenant/billing/audit-log` — billing mutation history
//!
//! ## Dependencies
//! - `tenancy`: tenant context middleware
//! - `identity`: actor identity for audit trails
//! - `kernel`: shared types (`ApiError`, etc.)
//! - `sqlx` / `uuid` / `chrono`: database interaction
//!
//! ## Data Model
//! The module owns the `tenant_plans`, `usage_records`, `invoices`,
//! `invoice_line_items`, and `billing_audit_log` tables. Migrations are
//! defined under `backend/migrations/` with the `billing_` prefix.
//!
//! ## Extension Points
//! - Metering aggregators can be added as new query backends (e.g. Prometheus
//!   pushgateway for real-time usage).
//! - Future payment providers can be plugged in behind the `PaymentGateway`
//!   trait.
//! - Plan templates can be loaded from configuration or a provider catalog.

pub mod audit;
pub mod enforcement;
pub mod model;
pub mod queries;
pub mod routes;
