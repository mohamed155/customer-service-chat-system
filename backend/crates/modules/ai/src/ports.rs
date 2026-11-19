//! The ports `ai` depends on instead of the modules that implement them (ADR-001 M3, ADR-017,
//! MOH-195). `ai` says what it needs and owns the DTOs; the server implements each port on top of
//! the module that has the data and passes the result in as [`AiPorts`]. This keeps `ai` free of
//! the `knowledge`, `tools` and `escalations` crates, so the generation engine can be read, and
//! tested, without them.
//!
//! A port that touches tenant data takes the caller's pool or connection and the `tenant_id`, the
//! same as the function behind it: the adapter must not open a transaction of its own where the
//! caller passes one.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// A passage of the tenant's knowledge base that matched a customer's question.
#[derive(Debug, Clone)]
pub struct RetrievedChunk {
    pub chunk_id: Uuid,
    pub item_id: Uuid,
    pub tenant_id: Uuid,
    pub content: String,
    pub content_hash: String,
    /// Cosine similarity to the question, `0.0..=1.0`; higher is closer.
    pub similarity: f64,
    pub item_title: String,
}

/// Finds the passages of a tenant's knowledge base closest to an embedded question.
#[async_trait]
pub trait KnowledgeRetrieval: Send + Sync {
    /// The up-to-`top_k` passages of `tenant_id`'s knowledge base whose similarity to
    /// `query_embedding` is at least `threshold`, closest first. Only that tenant's passages.
    async fn search(
        &self,
        pool: &PgPool,
        tenant_id: Uuid,
        query_embedding: &[f32],
        top_k: i32,
        threshold: f32,
    ) -> sqlx::Result<Vec<RetrievedChunk>>;
}

/// Where a tool comes from: the platform's own catalogue, or an endpoint the tenant registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolSource {
    Builtin,
    Tenant,
}

/// A tool the model may call for this tenant, with the policy that applies to it.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTool {
    /// What the model is told about the tool: name, description and argument schema.
    pub spec: ai_providers::ToolSpec,
    pub source: ToolSource,
    /// A person must approve each call before it runs.
    pub approval_required: bool,
    pub tenant_tool_id: Option<Uuid>,
}

/// Why a call the model made cannot run.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationFailure {
    /// No such tool, or not enabled for this tenant.
    UnknownOrDisabled,
    /// The arguments do not match the tool's schema; the text says how.
    SchemaMismatch(String),
}

/// How a tool call ended.
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionOutcome {
    Succeeded(Value),
    Failed(String),
    TimedOut,
}

/// A tool call to record. `status` is one of the request states the `tool_requests` table allows
/// (`pending`, `awaiting_approval`, `refused`).
#[derive(Debug, Clone)]
pub struct NewToolRequest {
    pub tenant_id: Uuid,
    pub conversation_id: Uuid,
    pub generation_id: Uuid,
    pub tool_name: String,
    pub tool_source: String,
    pub tenant_tool_id: Option<Uuid>,
    pub arguments: Value,
    pub status: String,
    pub approval_required: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub chain_index: i16,
}

/// A recorded tool call, as much of it as the generation that follows an approval needs.
#[derive(Debug, Clone)]
pub struct ToolRequest {
    pub generation_id: Uuid,
    pub tool_name: String,
    pub tool_source: ToolSource,
    pub tenant_tool_id: Option<Uuid>,
    pub arguments: Value,
}

/// A tool call still waiting for a person to approve it.
#[derive(Debug, Clone)]
pub struct PendingToolRequest {
    pub id: Uuid,
    pub conversation_id: Uuid,
}

/// What a tool needs to run: whose conversation it is, and the means to reach what it calls.
pub struct ToolRun {
    pub tenant_id: Uuid,
    pub conversation_id: Uuid,
    pub pool: PgPool,
    /// Keys to open tool credentials; `None` when none are configured.
    pub vault: Option<Arc<secrets::Vault>>,
    /// Client for tenant-registered tool endpoints (no redirects, bounded timeouts).
    pub http: reqwest::Client,
}

/// Resolving, recording and running the model's tool calls.
///
/// The functions that take a `PgConnection` run in the caller's transaction and must not open one
/// of their own: the caller commits the call and whatever must stay consistent with it together
/// (an approval request and its event, a cancellation and the escalation that caused it).
#[async_trait]
pub trait ToolRuntime: Send + Sync {
    /// The most tool calls one generation may chain before the model must answer.
    fn max_calls_per_generation(&self) -> u8;

    /// The tools enabled for `tenant_id`, with their approval policy.
    async fn resolve_available(
        &self,
        pool: &PgPool,
        tenant_id: Uuid,
    ) -> sqlx::Result<Vec<ResolvedTool>>;

    /// Checks a call the model made against the resolved tools and their argument schemas.
    fn validate(
        &self,
        resolved: &[ResolvedTool],
        name: &str,
        arguments: &Value,
    ) -> Result<ResolvedTool, ValidationFailure>;

    /// Records a tool call in its own transaction and returns its id.
    async fn insert_request(&self, pool: &PgPool, request: NewToolRequest) -> sqlx::Result<Uuid>;

    /// Records a tool call in the caller's transaction and returns its id.
    async fn insert_request_in_tx(
        &self,
        conn: &mut PgConnection,
        request: NewToolRequest,
    ) -> sqlx::Result<Uuid>;

    /// Moves a request to `executing`; `false` when it was not in a state that may start.
    async fn mark_executing(&self, pool: &PgPool, id: Uuid, tenant_id: Uuid) -> sqlx::Result<bool>;

    /// Moves an `executing` request to its final `status` and stores the outcome.
    async fn mark_terminal(
        &self,
        pool: &PgPool,
        id: Uuid,
        tenant_id: Uuid,
        status: &str,
        result: Option<Value>,
        error: Option<String>,
    ) -> sqlx::Result<bool>;

    /// Runs the tool and reports how it ended. Never panics and never fails the caller: a tool
    /// that errors or runs out of time is an outcome, not an error.
    async fn execute(
        &self,
        run: &ToolRun,
        tool: &ResolvedTool,
        arguments: Value,
        tool_request_id: Uuid,
    ) -> ExecutionOutcome;

    /// One recorded tool call of `tenant_id`, read in the caller's transaction.
    async fn find_request(
        &self,
        conn: &mut PgConnection,
        tenant_id: Uuid,
        id: Uuid,
    ) -> sqlx::Result<Option<ToolRequest>>;

    /// The conversation's tool calls that wait for a person's approval.
    async fn awaiting_approval(
        &self,
        pool: &PgPool,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> sqlx::Result<Vec<PendingToolRequest>>;

    /// Cancels the conversation's calls that wait for approval, in the caller's transaction, and
    /// returns their ids.
    async fn cancel_pending(
        &self,
        conn: &mut PgConnection,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> sqlx::Result<Vec<Uuid>>;
}

/// Everything `ai` needs from other modules, built once by the server and passed to the engine.
#[derive(Clone)]
pub struct AiPorts {
    pub knowledge: Arc<dyn KnowledgeRetrieval>,
    pub tools: Arc<dyn ToolRuntime>,
}
