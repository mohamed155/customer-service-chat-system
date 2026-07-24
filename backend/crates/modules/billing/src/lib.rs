//! # Billing Module
//!
//! ## Purpose
//! Platform-global plan catalog and per-tenant subscription management with
//! real-time usage-limit enforcement. Usage is aggregated live from
//! existing tables — no rollups, no background workers.
//!
//! ## Responsibilities
//! - Own the `billing_plans` and `tenant_subscriptions` tables.
//! - Read `ai_usage_records`, `conversations`, and `tenants` via SQL only
//!   (no crate dependency on those modules).
//! - Expose a tenant-facing billing overview endpoint (`GET /tenant/billing/overview`).
//! - Expose platform endpoints for plan catalog CRUD and subscription assignment,
//!   separately authorized (`platform.billing.view` / `platform.billing.manage`).
//! - Provide `billing::enforcement::check(pool, tenant_id)` — the real-time gate
//!   called by `ai::agent_responder` to pause AI replies when subscription status is
//!   `past_due`/`canceled` or a metric allowance is exceeded. The gate runs after
//!   escalation-rule evaluation so over-limit tenants still auto-escalate.
//! - Record every plan/subscription mutation in the shared `audit_logs` table.
//!
//! ## Public Interfaces
//! - `GET  /tenant/billing/overview` — current plan, period, usage (tenant-scoped)
//! - `GET  /platform/billing/plans` — plan catalog (platform, view)
//! - `POST /platform/billing/plans` — create plan (platform, manage)
//! - `PATCH /platform/billing/plans/{plan_id}` — update plan status (platform, manage)
//! - `GET  /platform/billing/subscriptions` — per-tenant subscription list (platform, view)
//! - `PUT  /platform/billing/tenants/{tenant_id}/subscription` — upsert subscription (platform, manage)
//!
//! ## Dependencies
//! - `tenancy`: tenant context middleware + `audit::record_in_tx`
//! - `identity`: actor identity for audit trails
//! - `kernel`: shared types (`ApiError`, `ApiJson`, etc.)
//! - `sqlx` / `uuid` / `chrono`: database interaction
//! - **Called by**: `ai` crate (`ai → billing` only, never the reverse)
//!
//! ## Data Model
//! The module owns two tables (migration `0059_billing_foundation`):
//! - `billing_plans` — platform-global plan catalog (name, price, allowances, status).
//!   Seeded with a default Free plan (100k tokens / 100 conversations / $0).
//! - `tenant_subscriptions` — one row per tenant (tenant_id UNIQUE, FK to plan).
//!   Status transitions: `active ⇄ past_due ⇄ canceled`.
//!
//! Usage data is read-only SQL against tables owned by other crates:
//! - `ai_usage_records` (ai crate) — token consumption per tenant
//! - `conversations` — conversation counts per tenant (excluding soft-deletes)
//! - `tenants` — for the cross-tenant subscription list (excluding soft-deletes)
//!
//! Derived values (computed per query, never stored):
//! - Billing period = current UTC calendar month `[start, end)`
//! - Metric state: `normal < 80% ≤ warning < 100% ≤ exceeded`
//! - AI-paused reason: subscription_status | token_limit | conversation_limit
//!
//! ## Extension Points
//! - Future payment providers can be added behind a trait in this module.
//! - Plan templates can be loaded from configuration.
//! - Extracting the module later means replacing four SELECTs with a usage API.

pub mod audit;
pub mod enforcement;
pub mod model;
pub mod queries;
pub mod routes;
