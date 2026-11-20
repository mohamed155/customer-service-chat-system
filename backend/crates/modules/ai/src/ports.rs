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

/// The state a tool call is recorded in. A call that has run, failed or been cancelled got there
/// from one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewToolRequestStatus {
    /// Allowed to run without approval; the engine runs it next.
    Pending,
    /// Waits for a person to approve it.
    AwaitingApproval,
    /// Not run: unknown, disabled, or the model's arguments did not fit the tool.
    Refused,
}

/// A tool call to record.
#[derive(Debug, Clone)]
pub struct NewToolRequest {
    pub tenant_id: Uuid,
    pub conversation_id: Uuid,
    pub generation_id: Uuid,
    pub tool_name: String,
    pub tool_source: ToolSource,
    pub tenant_tool_id: Option<Uuid>,
    pub arguments: Value,
    pub status: NewToolRequestStatus,
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

/// Why an escalation could not be opened.
#[derive(Debug)]
pub enum EscalationError {
    /// The conversation already has an open escalation.
    Duplicate,
    ConversationNotFound,
    /// The conversation is in a state that cannot be escalated (resolved or closed).
    InvalidState,
    Internal(String),
}

impl std::fmt::Display for EscalationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Duplicate => write!(f, "conversation already has an open escalation"),
            Self::ConversationNotFound => write!(f, "conversation not found"),
            Self::InvalidState => write!(f, "conversation cannot be escalated in its state"),
            Self::Internal(detail) => write!(f, "internal error: {detail}"),
        }
    }
}

impl std::error::Error for EscalationError {}

/// Handing a conversation over to people.
#[async_trait]
pub trait EscalationRouting: Send + Sync {
    /// Whether the conversation has an escalation that is still open.
    async fn has_open_escalation(
        &self,
        pool: &PgPool,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> sqlx::Result<bool>;

    /// Opens an escalation for the conversation and routes it to a person: the best-matching agent
    /// who is present and has the `required_skill_ids`, or the queue when nobody does. Runs in the
    /// caller's unit of work, which the caller commits or rolls back, and returns the escalation as
    /// the API shows it (`ai` hands it on and does not read it). `actor_user_id` is who asked; the
    /// nil id means the AI itself.
    async fn route_new_escalation(
        &self,
        tx: &mut db::UnitOfWork,
        tenant_id: Uuid,
        conversation_id: Uuid,
        reason: &str,
        required_skill_ids: &[Uuid],
        actor_user_id: Uuid,
    ) -> Result<serde_json::Value, EscalationError>;
}

/// Why a reply the AI was writing was dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupersededReason {
    /// The customer wrote again while the AI was still answering.
    NewerMessage,
    /// A person took the conversation over.
    Escalated,
}

/// How a generation failed, in terms a person can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCategory {
    Unavailable,
    Timeout,
    RateLimited,
    Authentication,
    InvalidRequest,
    Internal,
}

#[derive(Debug, Clone)]
pub struct GenerationStarted {
    pub conversation_id: Uuid,
    pub generation_id: Uuid,
    pub trigger_message_id: Uuid,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct GenerationDelta {
    pub conversation_id: Uuid,
    pub generation_id: Uuid,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct GenerationCompleted {
    pub conversation_id: Uuid,
    pub generation_id: Uuid,
    /// The reply as the API shows a message.
    pub message: Value,
}

#[derive(Debug, Clone)]
pub struct GenerationSuperseded {
    pub conversation_id: Uuid,
    pub generation_id: Uuid,
    pub reason: SupersededReason,
}

#[derive(Debug, Clone)]
pub struct GenerationFailed {
    pub conversation_id: Uuid,
    pub generation_id: Uuid,
    pub category: FailureCategory,
}

#[derive(Debug, Clone)]
pub struct ToolRequestCreated {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub tool_name: String,
    pub tool_source: ToolSource,
    pub arguments: Value,
    pub approval_required: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub chain_index: i16,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ToolRequestUpdated {
    pub id: Uuid,
    pub conversation_id: Uuid,
    /// The request's new status: `cancelled`, `refused`, `succeeded`, `failed` or `timed_out`.
    pub status: String,
    pub decided_by_display_name: Option<String>,
    pub duration_ms: Option<i64>,
    pub has_result: bool,
    pub error: Option<String>,
}

/// What `ai` tells the people watching a conversation, as it happens.
#[derive(Debug, Clone)]
pub enum AiRealtimeEvent {
    Started(GenerationStarted),
    Delta(GenerationDelta),
    Completed(GenerationCompleted),
    Superseded(GenerationSuperseded),
    Failed(GenerationFailed),
    ToolRequestCreated(ToolRequestCreated),
    ToolRequestUpdated(ToolRequestUpdated),
}

/// Telling the tenant's connected people what the AI is doing. Delivery is best effort and never
/// blocks or fails the caller: an event nobody is connected to hear is dropped.
pub trait AiRealtime: Send + Sync {
    fn publish(&self, tenant_id: Uuid, event: AiRealtimeEvent);
}

/// Everything `ai` needs from other modules, built once by the server and passed to the engine.
#[derive(Clone)]
pub struct AiPorts {
    pub knowledge: Arc<dyn KnowledgeRetrieval>,
    pub tools: Arc<dyn ToolRuntime>,
    pub escalation: Arc<dyn EscalationRouting>,
    pub realtime: Arc<dyn AiRealtime>,
}
