//! # Analytics Module
//!
//! ## Purpose
//! Tenant-scoped analytics: headline metric cards (conversation volume, AI
//! resolution rate, handoff rate, response times, satisfaction, token usage),
//! daily time-series charts, and date-range / channel filtering. All queries
//! are live SQL aggregations over existing tables — no rollup tables.
//!
//! ## Responsibilities
//! - Compute tenant-scoped aggregation metrics from conversations, messages,
//!   escalations, conversation_feedback, ai_usage_records, and ai_generations.
//! - Enforce tenant isolation on every query.
//! - Expose two REST endpoints guarded by `analytics.view`.
//!
//! ## Public Interfaces
//! - `GET /tenant/analytics/summary` — headline metrics + channel breakdown
//! - `GET /tenant/analytics/timeseries` — daily per-metric series, zero-filled
//!
//! ## Dependencies
//! - `tenancy`: tenant context middleware
//! - `kernel`: shared types (`ApiError`, etc.)
//! - `sqlx` / `uuid` / `chrono`: database interaction
//!
//! ## Data Model
//! This module owns no tables. It reads `conversations`, `messages`,
//! `escalations`, `conversation_feedback`, `ai_usage_records`, and
//! `ai_generations` via aggregation queries. Migration 0052 adds two
//! covering indexes. `ai_usage_records` lacks a direct channel column —
//! per-channel token attribution joins through `ai_generations` to
//! `conversations.channel`.
//!
//! ## Extension Points
//! - Future rollup tables can be swapped in behind the same API contract.
//! - Per-tenant timezone display can be added as a query parameter.
//! - Additional metrics can be added as new query functions.

pub mod model;
pub mod queries;
pub mod routes;
