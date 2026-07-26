//! # Platform Module
//!
//! ## Purpose
//! Platform-internal, cross-tenant monitoring surface: global business summary
//! (tenant counts, conversation counts, AI usage totals), three-state system
//! health (database, cache, queue, storage), and AI provider status with
//! per-provider metrics and recent-failure listing.
//!
//! All queries are live SQL aggregations over tables owned by other modules
//! (`tenants`, `conversations`, `ai_usage_records`, `outbox_events`) — no
//! rollup tables, no cache. Cross-tenant by construction and never narrowed
//! by a switched tenant context.
//!
//! ## Responsibilities
//! - Compute cross-tenant aggregate business metrics from tenants,
//!   conversations, and ai_usage_records.
//! - Report three-state system health by probing registered HealthCheck
//!   trait objects (database, cache) and reading queue/storage evidence.
//! - Report per-provider AI metrics and recent failures with verdicts
//!   derived from configurable failure-rate thresholds.
//! - Enforce permission gating — `platform.analytics.view` for the
//!   overview, `platform.diagnostics.view` for health and provider pages.
//!
//! ## Public Interfaces
//! - `GET /platform/analytics/summary` — global business metrics
//! - `GET /platform/health` — three-state system health
//! - `GET /platform/ai-providers` — AI provider status and recent failures
//!
//! ## Dependencies
//! - `kernel`: shared types (`ApiError`, etc.)
//! - `observability`: health-check trait objects
//! - `storage`: object-storage abstraction (for the storage backend probe)
//! - `sqlx` / `uuid` / `chrono`: database interaction
//!
//! ## Data Model
//! This module owns no tables. It reads `tenants`, `conversations`,
//! `ai_usage_records`, and `outbox_events` via aggregation queries.
//! Migration 0060 adds four covering indexes for the cross-tenant,
//! time-ranged read paths.
//!
//! ## Extension Points
//! - Future rollup tables can be swapped in behind the same API contract.
//! - New monitored dependencies can be added as new HealthCheck trait
//!   objects and probe functions in `health.rs`.
//! - Additional providers are handled by extending the PROVIDERS constant.

pub mod health;
pub mod model;
pub mod queries;
pub mod routes;
