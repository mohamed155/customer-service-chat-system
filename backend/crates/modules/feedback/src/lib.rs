//! # Feedback Module
//!
//! ## Purpose
//! Collect post-conversation feedback from widget customers: a 1–5 star rating
//! with optional comment, prompted once a conversation has ended, immutable
//! once submitted, one per conversation. Feedback is stored in an append-only,
//! tenant-scoped, analytics-ready fact table.
//!
//! ## Responsibilities
//! - Accept feedback submissions from authenticated widget sessions.
//! - Enforce one-feedback-per-conversation via DB uniqueness.
//! - Enforce rating range (1–5) and comment length (≤ 2000 chars).
//! - Provide a pending-feedback lookup keyed by session identity.
//! - Provide a tenant-wide feedback summary (average rating + count).
//!
//! ## Public Interfaces
//! - `POST /widget/v1/conversations/{conversationId}/feedback` — submit feedback
//! - `GET /widget/v1/feedback/pending` — lookup ended unrated conversation
//! - `GET /tenant/feedback/summary` — tenant-wide average and count
//!
//! ## Dependencies
//! - `widgets`: session authentication (`widgets::session::authenticate_session`)
//! - `tenancy`: tenant context middleware for tenant routes
//! - `kernel`: shared types (`ApiError`, `ApiJson`, rate limiting)
//! - `sqlx` / `uuid` / `chrono`: database interaction
//!
//! ## Data Model
//! Table `conversation_feedback` (migration 0051): append-only fact table with
//! `tenant_id`, `conversation_id`, `widget_session_id`, `channel`,
//! `agent_configuration_id`, `assigned_membership_id`, `rating`, `comment`,
//! `submitted_at`, `created_at`. One row per conversation, enforced by
//! unique index on `(tenant_id, conversation_id)`.
//!
//! ## Extension Points
//! - Future analytics feature can aggregate satisfaction by tenant, channel,
//!   AI agent, or human agent from the stored fact table without backfilling.
//! - Additional channels (email, social) can reuse the same storage model.

pub mod model;
pub mod public_routes;
pub mod queries;
pub mod tenant_routes;
