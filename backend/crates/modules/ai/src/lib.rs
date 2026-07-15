//! AI application module — configuration, credential management, usage
//! tracking, and the [`AiService`] entry point.
//!
//! # Purpose
//! High-level AI orchestration layer that sits above the vendor-agnostic
//! [`ai_providers`] adapters.  Handles per-tenant configuration and credential
//! resolution, retry/failover policy, usage recording, and audited admin APIs.
//!
//! # Responsibilities
//! - Per-tenant config / credential resolution (tenant → platform fallback)
//! - AES-256-GCM credential encryption via [`crypto`]
//! - Retry/failover policy (exponential back-off with jitter, fallback models)
//! - Usage recording (token counts, latency, cost attribution)
//! - Audited admin API ([`routes`] with `authz` RBAC enforcement)
//!
//! # Public Interfaces
//! - [`AiService`] — main entry point for consuming modules
//! - [`AiCallContext`], [`AiCallResult`], [`AiCallError`]
//! - [`AiInput`], [`AiStreamEvent`], [`AiResultStream`]
//! - [`routes`] — Axum router (mounted by the server)
//! - [`crypto`] — encrypt / decrypt / `MasterKey`
//! - [`model`] — row types, payloads, views
//!
//! # Dependencies
//! - [`ai_providers`] — vendor adapters (OpenAI, Anthropic, Gemini)
//! - [`authz`] — RBAC middleware and policy checks
//! - [`tenancy`] — multi-tenant context and scope resolution
//! - [`kernel`] — shared error types and response envelope
//!
//! # Data Model
//! Migrations 0038–0040 create the supporting tables:
//! - `ai_configurations` — per-tenant or platform-wide AI config
//! - `ai_credentials` — encrypted API keys (AES-256-GCM)
//! - `ai_usage_records` — per-call usage ledger
//!
//! # Extension Points
//! - **KMS replacement**: swap `crypto.rs` for a cloud KMS while keeping
//!   the `MasterKey` / `seal` / `open` interface.
//! - **New provider**: add to [`ai_providers`] — this module picks it up
//!   automatically through the uniform trait.
//! - **Sidecar extraction**: the AI runtime (completions, streaming, retries)
//!   can be extracted into its own crate with no changes to the public API.

pub mod audit;
pub mod crypto;
pub mod model;
pub mod resolution;
pub mod routes;
pub mod service;
pub mod usage;
#[doc(hidden)]
pub use service::{run_attempts, Attempt};
pub use service::{
    AiCallContext, AiCallError, AiCallResult, AiInput, AiResultStream, AiService, AiStreamEvent,
};
// Re-export ai_providers types that appear in the public API so consuming
// crates never need a direct ai-providers dependency.
pub use ai_providers::{ErrorCategory, Message, Role, TokenUsage};
