//! # Conversations Module
//!
//! Owns conversations and messages within a tenant-scoped isolation boundary: creating
//! conversations, adding customer messages, agent replies and internal notes, status transitions,
//! assignment, the inbox, the message timeline, attachments, and the history shown on a customer's
//! profile.
//!
//! ## Layers (ADR-017)
//! - `domain/`: the rules, as pure functions with no framework, database or HTTP in them: status
//!   and reopening, assignment, message attribution and validation, inbox filters, cursors,
//!   WhatsApp's reply window.
//! - `application/`: the use cases and the ports they need (store, member and customer directories,
//!   outbox, audit), the events the module publishes, and the wire DTOs. Tested against an
//!   in-memory store.
//! - `infrastructure/`: all SQL, implementing the ports on one database transaction.
//! - `api/`: the Axum handlers, query parameters and response envelopes.
//!
//! The layers are private. What other modules and the server see is the facade below.
//!
//! ## Ownership Model
//! Each conversation is owned by exactly one customer within one tenant: `tenant 1 ──── N
//! conversations` and `customer 1 ──── N conversations`. Messages belong to exactly one
//! conversation. The `conversations_parent_tenant_fkey` composite FK (migration 0027) ensures that
//! `(tenant_id, customer_id)` always references a valid customer row in the same tenant,
//! preventing cross-tenant child rows at the DB level. Conversations are **not** cascade-deleted
//! when the parent customer is removed (migration 0030), so the history stays available for
//! audit.
//!
//! ## Public surface
//! - [`routes`] and [`get_conversation_history`]: the Axum handlers and their OpenAPI envelopes.
//! - [`model`]: the wire DTOs, including [`model::ConversationStatus`] and [`model::MessageKind`].
//! - [`queries`]: the use cases other modules run on their own transaction (add a message, assign,
//!   open a conversation, the detail and the timeline) and the single-statement reads and writes
//!   of the AI responder, the WhatsApp sender, the widget and escalations.
//! - [`outbox`]: the events other modules ask this one to publish, whose names and payloads are
//!   [`application::ConversationEvent`].
//! - [`directory`], [`inbound`] and [`attachments`]: what the widget, the escalation queue and the
//!   channels may ask of conversations, messages and attachments (MOH-194).
//! - [`AiAgentStatus`]: what the inbox needs to know about the tenant's AI agent, implemented by
//!   the server.
//!
//! ## Boundaries
//! - **Tenant-scoped**: every query includes `tenant_id` from the resolved `TenantContext` and runs
//!   in the tenant Unit of Work.
//! - **One-way dependency on customers**: opening a conversation checks `customers` for the
//!   customer; `customers` does not import this module.

mod api;
mod application;
mod domain;
mod infrastructure;

pub mod outbox;
pub mod queries;

pub use api::history::*;
pub use application::AiAgentStatus;
pub use infrastructure::history::{
    list_recent_for_customer, list_recent_for_customer_in_tx, ConversationSummary,
};

/// The wire DTOs: what the dashboard, the widget and the other modules receive and send.
pub mod model {
    pub use crate::application::model::*;
}

/// The Axum handlers of the conversation API and their OpenAPI response envelopes.
pub mod routes {
    pub use crate::api::routes::*;
}

/// What the widget, the escalation queue and the WhatsApp sender may ask about a conversation.
pub mod directory {
    pub use crate::infrastructure::directory::*;
}

/// How a channel hands a customer's message to conversations.
pub mod inbound {
    pub use crate::infrastructure::inbound::*;
}

/// A message's attachments.
pub mod attachments {
    pub use crate::infrastructure::attachments::*;
}
