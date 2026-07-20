//! In-app notification delivery and lifecycle management.
//!
//! # Purpose
//! Provides the domain model, persistence, and emission machinery for
//! tenant-scoped in-app notifications. Notifications inform human agents
//! about escalations, conversation assignments, AI failures, and tool
//! approval requests.
//!
//! # Responsibilities
//! - Define notification kinds, states, and the persisted data model.
//! - Emit notifications from domain events (via `emit`).
//! - Resolve the correct recipients for a given subject.
//! - Query and paginate a recipient's inbox.
//! - Transition state (unread → read → resolved) and enforce deduplication.
//! - Background retention pruning via the `worker` module.
//!
//! # Public Interfaces
//! - `model` — `NotificationKind`, `NotificationState`, `Notification` struct.
//! - `emit` — functions to persist a new notification.
//! - `recipients` — recipient resolution logic.
//! - `queries` — inbox listing, unread count, state transitions.
//! - `worker` — background jobs (retention, maybe digest).
//! - `routes` — Axum handlers mounted under `/api/v1/notifications`.
//!
//! # Dependencies
//! - `authz` — authorization checks for notification-scoped endpoints.
//! - `identity` — agent/actor lookups.
//! - `tenancy` — `TenantContext` for all queries.
//! - `escalations` — escalation events that trigger notifications.
//! - Postgres via `sqlx` — persistent storage.
//!
//! # Data Model
//! Single table `notifications` keyed by `(tenant_id, id)` with a
//! dedicated deduplication constraint per recipient. See migration 0054.
//!
//! # Extension Points
//! - New notification kinds can be added by extending `NotificationKind`
//!   and adding the string to the DB CHECK constraint.
//! - Additional delivery channels (email, push) can be implemented as
//!   observers within `emit` without changing the core model.

pub mod model;
pub mod emit;
pub mod recipients;
pub mod queries;
pub mod worker;
pub mod routes;
