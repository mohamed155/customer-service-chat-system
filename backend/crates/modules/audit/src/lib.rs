//! # Audit Module
//!
//! ## Purpose
//! Read-side service for the existing append-only `audit_logs` table. Exposes
//! tenant-scoped and platform-wide audit trails with cursor pagination,
//! category/date/actor filters, and actor display (user or system, with
//! platform-staff and deleted-actor labels).
//!
//! ## Responsibilities
//! - Serve the read side of the `audit_logs` table (the write side lives in
//!   per-module crates — identity, tenancy, ai, tools, etc.).
//! - Enforce tenant isolation on the tenant endpoint via `TenantContext`.
//! - Expose two REST endpoints guarded by `audit.view` / `platform.audit.view`.
//!
//! ## Public Interfaces
//! - `GET /tenant/audit-logs` — tenant-scoped audit list
//! - `GET /platform/audit-logs` — platform-wide audit list
//!
//! ## Dependencies
//! - `tenancy`: tenant context middleware
//! - `kernel`: shared types (`ApiError`, etc.)
//! - `sqlx` / `uuid` / `chrono`: database interaction
//!
//! ## Data Model
//! This module owns no tables. It reads the existing `audit_logs` table
//! (created in migration `0006_audit_logs.sql`, amended in `0010`/`0013`),
//! which is append-only (DB-trigger `audit_logs_append_only`). The table
//! stores immutable event records with a nullable actor FK to `users`,
//! a namespaced `action` string, resource identification, and free-form
//! JSONB `details`.
//!
//! ## Extension Points
//! - A future detail endpoint (`GET /tenant/audit-logs/{id}`) can be added
//!   if metadata payloads grow beyond what fits in the list response.
//! - Export/streaming of audit data can be added behind a new permission.
//! - New categories are added by extending `CATEGORY_PREFIXES`.

pub mod model;
pub mod queries;
pub mod routes;
