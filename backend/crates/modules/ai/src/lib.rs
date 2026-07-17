//! AI application module — configuration, credential management, usage
//! tracking, agent configuration, prompt management, the outbox-driven agent
//! responder, and the [`AiService`] entry point.
//!
//! # Purpose
//! High-level AI orchestration layer that sits above the vendor-agnostic
//! [`ai_providers`] adapters.  Handles per-tenant configuration and credential
//! resolution, retry/failover policy, usage recording, audited admin APIs, and
//! the configurable AI agent that responds to customer messages via an outbox
//! worker.
//!
//! # Responsibilities
//! - Per-tenant config / credential resolution (tenant → platform fallback)
//! - AES-256-GCM credential encryption via [`crypto`]
//! - Retry/failover policy (exponential back-off with jitter, fallback models)
//! - Usage recording (token counts, latency, cost attribution)
//! - Audited admin API ([`routes`] with `authz` RBAC enforcement)
//! - Agent configuration ([`agent_config`], [`agent_routes`])
//! - Versioned prompt management ([`prompt_store`], [`prompt_validate`], [`prompt_routes`])
//! - Deterministic prompt composition ([`agent_prompt`])
//! - Escalation rule evaluation ([`agent_rules`])
//! - Outbox-driven agent responder ([`agent_responder`])
//!
//! # Public Interfaces
//! - [`AiService`] — main entry point for consuming modules
//! - [`AiCallContext`], [`AiCallResult`], [`AiCallError`]
//! - [`AiInput`], [`AiStreamEvent`], [`AiResultStream`]
//! - [`routes`] — Axum router (mounted by the server)
//! - [`agent_routes`] — agent configuration endpoints
//! - [`prompt_routes`] — versioned prompt CRUD endpoints
//! - [`prompt_store`] — prompt and version persistence
//! - [`prompt_validate`] — prompt content validation
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
//! Migrations 0038–0045 create the supporting tables:
//! - `ai_configurations` — per-tenant or platform-wide AI config
//! - `ai_credentials` — encrypted API keys (AES-256-GCM)
//! - `ai_usage_records` — per-call usage ledger
//! - `agent_configurations` — per-tenant AI agent settings (system_prompt dropped in 0045)
//! - `agent_avatar_uploads` — uploaded avatar images
//! - `agent_prompts` — tenant-level prompt object (one per tenant, prompt_kind = 'system')
//! - `agent_prompt_versions` — append-only, immutable version snapshots
//!
//! # Extension Points
//! - **KMS replacement**: swap `crypto.rs` for a cloud KMS while keeping
//!   the `MasterKey` / `seal` / `open` interface.
//! - **New provider**: add to [`ai_providers`] — this module picks it up
//!   automatically through the uniform trait.
//! - **Sidecar extraction**: the AI runtime (completions, streaming, retries)
//!   can be extracted into its own crate with no changes to the public API.
//! - **Multi-agent**: the schema is multi-agent-shaped; dropping the partial
//!   unique index on `(tenant_id)` is the entire unlock.

pub mod agent_audit;
pub mod agent_config;
pub mod agent_prompt;
pub mod agent_responder;
pub mod agent_routes;
pub mod agent_rules;
pub mod audit;
pub mod crypto;
pub mod model;
pub mod prompt_routes;
pub mod prompt_store;
pub mod prompt_validate;
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
