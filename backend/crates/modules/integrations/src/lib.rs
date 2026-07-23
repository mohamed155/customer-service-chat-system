//! # Integrations Module
//!
//! ## Purpose
//! Tenant-scoped integration foundation: a platform-managed catalog of
//! connectable integrations, per-tenant connection lifecycle (connect,
//! update/rotate, disconnect, reconnect), AES-256-GCM encrypted secret
//! storage, a public inbound-webhook intake endpoint with HMAC verification
//! and rate limiting, derived health status, and a per-connection event
//! log with 90-day retention.
//!
//! ## Responsibilities
//! - Serve the read side of `integration_catalog`, `integration_connections`,
//!   `integration_secrets`, `integration_webhook_deliveries`, and
//!   `integration_events`.
//! - Expose tenant endpoints guarded by `integrations.view` /
//!   `integrations.manage`.
//! - Enforce tenant isolation on every read surface and on the intake
//!   handler (token → connection lookup never crosses tenants).
//! - Mint/encrypt intake tokens; never persist or return secret material
//!   in plaintext.
//! - Sweep expired events/deliveries after 90 days.
//!
//! ## Public Interfaces
//! - `GET /tenant/integrations` — catalog with per-tenant status
//! - `GET /tenant/integrations/{slug}` — detail
//! - `POST /tenant/integrations/{slug}/connect` — connect
//! - `PUT /tenant/integrations/{slug}/config` — update config / rotate secrets
//! - `POST /tenant/integrations/{slug}/disconnect` — disconnect
//! - `GET /tenant/integrations/{slug}/events` — paginated event log
//! - `POST /hooks/v1/{token}` — public inbound-webhook intake
//!
//! ## Dependencies
//! - `tenancy`: tenant context middleware, audit writers
//! - `kernel`: shared types (`ApiError`, rate-limit store)
//! - `sqlx` / `uuid` / `chrono`: database interaction
//! - `aes-gcm` / `hmac` / `sha2`: encryption and signature verification
//!
//! ## Data Model
//! This module owns the five tables created in migration
//! `0056_integrations_foundation.sql`: `integration_catalog` (global),
//! `integration_connections` (one per tenant+integration, forever),
//! `integration_secrets` (encrypted, with masked hint only on read),
//! `integration_webhook_deliveries` (accepted payloads only), and
//! `integration_events` (lifecycle + delivery outcomes).
//!
//! ## Extension Points
//! - Additional catalog entries can be added by inserting rows into
//!   `integration_catalog`; no code change required for entries with
//!   the standard `text` / `secret` config-schema shape.
//! - New event types or rejection reasons extend the `event_type` /
//!   `reason` `CHECK` constraints in the migration and the matching
//!   Rust enums in `model`.

pub mod model;
pub mod crypto;
pub mod queries;
pub mod routes;
pub mod webhook;
pub mod status;
pub mod audit;
pub mod retention;
