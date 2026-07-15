//! # Escalations Module
//!
//! Owns the human handoff & routing subsystem: agent skill catalogs, per-agent
//! availability with presence-aware auto-revert, the escalation queue, a
//! routing engine that assigns the best available agent (skill match → load
//! fallback → queue), a per-tenant SSE event stream for real-time delivery,
//! and the claim/close-out lifecycle.
//!
//! ## Ownership
//! Four tables are entirely owned by this module:
//! - `skills` — tenant skill catalog (hard-delete, R7).
//! - `agent_skills` — per-agent skill assignments (join table, hard-link only).
//! - `agent_availability` — per-membership toggle state (default `away`).
//! - `escalations` — one row per handoff attempt; at most one active per
//!   conversation.
//!
//! ## Dependencies
//! One-way on `conversations` (two public tx interfaces: `assign_in_tx` and
//! `set_escalated_in_tx`) and `tenancy` (membership validity). The return
//! path uses the existing transactional outbox: conversations emits
//! `conversation.status_changed` / `conversation.assignment_changed` which
//! this module consumes to close out escalations and relabel routing reasons.
//!
//! ## Public Interfaces
//! ### Axum handlers (registered via `server::router`)
//! - `POST /tenant/conversations/{id}/escalate`
//! - `GET  /tenant/escalations/queue`
//! - `POST /tenant/escalations/{id}/claim`
//! - `GET  /tenant/availability/me`, `PUT /tenant/availability/me`
//! - `GET  /tenant/skills`, `POST /tenant/skills`,
//!        `PATCH /tenant/skills/{id}`, `DELETE /tenant/skills/{id}`
//! - `PUT  /tenant/members/{membershipId}/skills`
//! - `GET  /tenant/events` (SSE stream)
//!
//! ## Reusable query functions
//! - `queries::escalation_row_in_tx`, `active_escalation_for_conversation_in_tx`
//! - `queries::select_candidate_in_tx`, `route_new_escalation_in_tx`
//! - `queries::claim_in_tx`, `drain_one_for_membership_in_tx`
//! - `queries::queue_list_in_tx`, `latest_escalation_for_conversation_in_tx`
//! - `queries::get_availability_in_tx`, `upsert_availability_in_tx`
//! - `queries::list_skills_in_tx`, `create_skill_in_tx`, `rename_skill_in_tx`,
//!   `delete_skill_in_tx`, `set_member_skills_in_tx`
//! - `queries::skills_and_availability_for_members_in_tx`
//!
//! ## Extension Points
//! - **Presence extraction**: `escalations::presence::Runtime` wraps the
//!   process-local registry behind its own interface (research R2). If a
//!   future extraction splits the module into a service, the in-process
//!   registry is swapped for Redis pub/sub + TTL presence keys without
//!   changing callers.
//! - **Event fan-out**: the per-tenant `tokio::sync::broadcast` channel is
//!   likewise swap-friendly (same extraction path).
//! - **AI integration**: the `route_new_escalation_in_tx` application service
//!   function is the tool the future AI subsystem calls (Principle IV).
//!   The `POST …/escalate` endpoint is the interim test surface (research R6).

pub mod audit;
pub mod events;
pub mod model;
pub mod presence;
pub mod queries;
pub mod routes;
pub mod routing;
