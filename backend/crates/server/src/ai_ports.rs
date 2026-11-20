//! Wires the ports `ai` declares (`ai::ports`) to the modules that implement them (ADR-001 M3,
//! ADR-017, MOH-195). This is the one place that knows both sides: `ai` does not depend on
//! `knowledge`, and `knowledge` does not depend on `ai`.
//!
//! # Public Interfaces
//! - [`AppState::ai_ports`] builds the [`ai::ports::AiPorts`] the engine runs on
//! - [`AiEmbedder`] lets the knowledge indexer embed through the `ai` service

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use ai::ports::{
    AiPorts, AiRealtime, AiRealtimeEvent, EscalationError, EscalationRouting, ExecutionOutcome,
    FailureCategory, KnowledgeRetrieval, NewToolRequest, NewToolRequestStatus, PendingToolRequest,
    ResolvedTool, RetrievedChunk, SupersededReason, ToolRequest, ToolRun, ToolRuntime, ToolSource,
    ValidationFailure,
};
use std::sync::Arc;

use crate::state::AppState;

/// `ai::ports::KnowledgeRetrieval` on top of `knowledge::retrieval`.
struct KnowledgeAdapter;

#[async_trait]
impl KnowledgeRetrieval for KnowledgeAdapter {
    async fn search(
        &self,
        pool: &PgPool,
        tenant_id: Uuid,
        query_embedding: &[f32],
        top_k: i32,
        threshold: f32,
    ) -> sqlx::Result<Vec<RetrievedChunk>> {
        let chunks =
            knowledge::retrieval::search(pool, tenant_id, query_embedding, top_k, threshold)
                .await?;
        Ok(chunks
            .into_iter()
            .map(|chunk| RetrievedChunk {
                chunk_id: chunk.chunk_id,
                item_id: chunk.item_id,
                tenant_id: chunk.tenant_id,
                content: chunk.content,
                content_hash: chunk.content_hash,
                similarity: chunk.similarity,
                item_title: chunk.item_title,
            })
            .collect())
    }
}

/// `ai::ports::ToolRuntime` on top of the `tools` module. Each method forwards to the function of
/// the same job and converts the types; it opens no transaction where the caller passes a
/// connection.
struct ToolsAdapter;

fn source_from_tools(source: tools::model::ToolSource) -> ToolSource {
    match source {
        tools::model::ToolSource::Builtin => ToolSource::Builtin,
        tools::model::ToolSource::Tenant => ToolSource::Tenant,
    }
}

fn source_to_tools(source: &ToolSource) -> tools::model::ToolSource {
    match source {
        ToolSource::Builtin => tools::model::ToolSource::Builtin,
        ToolSource::Tenant => tools::model::ToolSource::Tenant,
    }
}

/// A stored `tool_source` value. Anything but `tenant` is a built-in, as the engine always read it.
fn source_from_column(value: &str) -> ToolSource {
    match value {
        "tenant" => ToolSource::Tenant,
        _ => ToolSource::Builtin,
    }
}

fn tool_from_tools(tool: tools::policy::ResolvedTool) -> ResolvedTool {
    ResolvedTool {
        spec: tool.spec,
        source: source_from_tools(tool.source),
        approval_required: tool.approval_required,
        tenant_tool_id: tool.tenant_tool_id,
    }
}

fn tool_to_tools(tool: &ResolvedTool) -> tools::policy::ResolvedTool {
    tools::policy::ResolvedTool {
        spec: tool.spec.clone(),
        source: source_to_tools(&tool.source),
        approval_required: tool.approval_required,
        tenant_tool_id: tool.tenant_tool_id,
    }
}

/// The value of the `tool_requests.status` column a new request is stored with.
fn status_to_column(status: NewToolRequestStatus) -> &'static str {
    match status {
        NewToolRequestStatus::Pending => "pending",
        NewToolRequestStatus::AwaitingApproval => "awaiting_approval",
        NewToolRequestStatus::Refused => "refused",
    }
}

fn new_request_to_tools(request: NewToolRequest) -> tools::queries::NewToolRequest {
    tools::queries::NewToolRequest {
        tenant_id: request.tenant_id,
        conversation_id: request.conversation_id,
        generation_id: request.generation_id,
        tool_name: request.tool_name,
        tool_source: source_to_tools(&request.tool_source).as_str().to_string(),
        tenant_tool_id: request.tenant_tool_id,
        arguments: request.arguments,
        status: status_to_column(request.status).to_string(),
        approval_required: request.approval_required,
        expires_at: request.expires_at,
        chain_index: request.chain_index,
    }
}

#[async_trait]
impl ToolRuntime for ToolsAdapter {
    fn max_calls_per_generation(&self) -> u8 {
        tools::executor::MAX_TOOL_CALLS_PER_GENERATION
    }

    async fn resolve_available(
        &self,
        pool: &PgPool,
        tenant_id: Uuid,
    ) -> sqlx::Result<Vec<ResolvedTool>> {
        Ok(tools::policy::resolve_available_builtins(pool, tenant_id)
            .await?
            .into_iter()
            .map(tool_from_tools)
            .collect())
    }

    fn validate(
        &self,
        resolved: &[ResolvedTool],
        name: &str,
        arguments: &serde_json::Value,
    ) -> Result<ResolvedTool, ValidationFailure> {
        let resolved: Vec<tools::policy::ResolvedTool> =
            resolved.iter().map(tool_to_tools).collect();
        tools::executor::validate(&resolved, name, arguments)
            .map(tool_from_tools)
            .map_err(|failure| match failure {
                tools::executor::ValidationFailure::UnknownOrDisabled => {
                    ValidationFailure::UnknownOrDisabled
                }
                tools::executor::ValidationFailure::SchemaMismatch(detail) => {
                    ValidationFailure::SchemaMismatch(detail)
                }
            })
    }

    async fn insert_request(&self, pool: &PgPool, request: NewToolRequest) -> sqlx::Result<Uuid> {
        tools::queries::insert_request(pool, new_request_to_tools(request)).await
    }

    async fn insert_request_in_tx(
        &self,
        conn: &mut sqlx::PgConnection,
        request: NewToolRequest,
    ) -> sqlx::Result<Uuid> {
        tools::queries::insert_request_in_tx(conn, new_request_to_tools(request)).await
    }

    async fn mark_executing(&self, pool: &PgPool, id: Uuid, tenant_id: Uuid) -> sqlx::Result<bool> {
        tools::queries::mark_executing(pool, id, tenant_id).await
    }

    async fn mark_terminal(
        &self,
        pool: &PgPool,
        id: Uuid,
        tenant_id: Uuid,
        status: &str,
        result: Option<serde_json::Value>,
        error: Option<String>,
    ) -> sqlx::Result<bool> {
        tools::queries::mark_terminal(pool, id, tenant_id, status, result, error).await
    }

    async fn execute(
        &self,
        run: &ToolRun,
        tool: &ResolvedTool,
        arguments: serde_json::Value,
        tool_request_id: Uuid,
    ) -> ExecutionOutcome {
        let ctx = tools::registry::ToolExecutionCtx {
            tenant_id: run.tenant_id,
            conversation_id: run.conversation_id,
            pool: run.pool.clone(),
            vault: run.vault.clone(),
            http: run.http.clone(),
        };
        match tools::executor::execute(&ctx, &tool_to_tools(tool), arguments, tool_request_id).await
        {
            tools::executor::ExecutionOutcome::Succeeded(value) => {
                ExecutionOutcome::Succeeded(value)
            }
            tools::executor::ExecutionOutcome::Failed(error) => ExecutionOutcome::Failed(error),
            tools::executor::ExecutionOutcome::TimedOut => ExecutionOutcome::TimedOut,
        }
    }

    async fn find_request(
        &self,
        conn: &mut sqlx::PgConnection,
        tenant_id: Uuid,
        id: Uuid,
    ) -> sqlx::Result<Option<ToolRequest>> {
        Ok(tools::queries::find_request_in_tx(conn, tenant_id, id)
            .await?
            .map(|row| ToolRequest {
                generation_id: row.generation_id,
                tool_name: row.tool_name,
                tool_source: source_from_column(&row.tool_source),
                tenant_tool_id: row.tenant_tool_id,
                arguments: row.arguments,
            }))
    }

    async fn awaiting_approval(
        &self,
        pool: &PgPool,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> sqlx::Result<Vec<PendingToolRequest>> {
        Ok(tools::approval::fetch_awaiting_approval_for_conversation(
            pool,
            tenant_id,
            conversation_id,
        )
        .await?
        .into_iter()
        .map(|row| PendingToolRequest {
            id: row.id,
            conversation_id: row.conversation_id,
        })
        .collect())
    }

    async fn cancel_pending(
        &self,
        conn: &mut sqlx::PgConnection,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> sqlx::Result<Vec<Uuid>> {
        tools::approval::cancel_pending_for_conversation(conn, tenant_id, conversation_id).await
    }
}

/// `ai::ports::EscalationRouting` on top of `escalations::routing`. Who is present to take the
/// conversation comes from the presence runtime, which `ai` does not know about.
struct EscalationAdapter {
    pool: PgPool,
    presence: Arc<escalations::presence::Runtime>,
}

#[async_trait]
impl EscalationRouting for EscalationAdapter {
    async fn has_open_escalation(
        &self,
        pool: &PgPool,
        tenant_id: Uuid,
        conversation_id: Uuid,
    ) -> sqlx::Result<bool> {
        escalations::routing::has_open_escalation(pool, tenant_id, conversation_id).await
    }

    async fn route_new_escalation(
        &self,
        tx: &mut db::UnitOfWork,
        tenant_id: Uuid,
        conversation_id: Uuid,
        reason: &str,
        required_skill_ids: &[Uuid],
        actor_user_id: Uuid,
    ) -> Result<serde_json::Value, EscalationError> {
        let present_ids = self.presence.present_membership_ids_async(tenant_id).await;
        let outcome = escalations::routing::route_new_escalation_in_tx(
            tx,
            &self.pool,
            tenant_id,
            conversation_id,
            reason,
            required_skill_ids,
            &[],
            &present_ids,
            actor_user_id,
        )
        .await
        .map_err(|error| match error {
            escalations::routing::RouteError::Duplicate => EscalationError::Duplicate,
            escalations::routing::RouteError::ConversationNotFound => {
                EscalationError::ConversationNotFound
            }
            escalations::routing::RouteError::InvalidState => EscalationError::InvalidState,
            escalations::routing::RouteError::Internal(detail) => EscalationError::Internal(detail),
        })?;
        let escalation = match outcome {
            escalations::routing::RouteOutcome::Assigned { escalation, .. }
            | escalations::routing::RouteOutcome::Queued { escalation } => escalation,
        };
        serde_json::to_value(&escalation)
            .map_err(|error| EscalationError::Internal(error.to_string()))
    }
}

/// `ai::ports::AiRealtime` on top of the presence runtime's tenant broadcast. The payloads are the
/// types the SSE stream already serialises, so what the dashboard reads does not change.
struct RealtimeAdapter {
    presence: Arc<escalations::presence::Runtime>,
}

impl AiRealtime for RealtimeAdapter {
    fn publish(&self, tenant_id: Uuid, event: AiRealtimeEvent) {
        self.presence.broadcast(tenant_id, presence_event(event));
    }
}

fn superseded_reason(reason: SupersededReason) -> escalations::model::SupersededReason {
    match reason {
        SupersededReason::NewerMessage => escalations::model::SupersededReason::NewerMessage,
        SupersededReason::Escalated => escalations::model::SupersededReason::Escalated,
    }
}

fn failure_category(category: FailureCategory) -> escalations::model::FailureCategory {
    match category {
        FailureCategory::Unavailable => escalations::model::FailureCategory::Unavailable,
        FailureCategory::Timeout => escalations::model::FailureCategory::Timeout,
        FailureCategory::RateLimited => escalations::model::FailureCategory::RateLimited,
        FailureCategory::Authentication => escalations::model::FailureCategory::Authentication,
        FailureCategory::InvalidRequest => escalations::model::FailureCategory::InvalidRequest,
        FailureCategory::Internal => escalations::model::FailureCategory::Internal,
    }
}

/// An AI event as the presence runtime broadcasts it, field for field.
pub fn presence_event(event: AiRealtimeEvent) -> escalations::presence::Event {
    use escalations::model::{
        ConversationAiCompleted, ConversationAiDelta, ConversationAiEvent, ConversationAiFailed,
        ConversationAiStarted, ConversationAiSuperseded, ToolRequestCreated, ToolRequestUpdated,
    };
    use escalations::presence::{ConversationToolEvent, Event};

    match event {
        AiRealtimeEvent::Started(e) => {
            Event::ConversationAi(ConversationAiEvent::Started(ConversationAiStarted {
                conversation_id: e.conversation_id,
                generation_id: e.generation_id,
                trigger_message_id: e.trigger_message_id,
                started_at: e.started_at,
            }))
        }
        AiRealtimeEvent::Delta(e) => {
            Event::ConversationAi(ConversationAiEvent::Delta(ConversationAiDelta {
                conversation_id: e.conversation_id,
                generation_id: e.generation_id,
                text: e.text,
            }))
        }
        AiRealtimeEvent::Completed(e) => {
            Event::ConversationAi(ConversationAiEvent::Completed(ConversationAiCompleted {
                conversation_id: e.conversation_id,
                generation_id: e.generation_id,
                message: e.message,
            }))
        }
        AiRealtimeEvent::Superseded(e) => {
            Event::ConversationAi(ConversationAiEvent::Superseded(ConversationAiSuperseded {
                conversation_id: e.conversation_id,
                generation_id: e.generation_id,
                reason: superseded_reason(e.reason),
            }))
        }
        AiRealtimeEvent::Failed(e) => {
            Event::ConversationAi(ConversationAiEvent::Failed(ConversationAiFailed {
                conversation_id: e.conversation_id,
                generation_id: e.generation_id,
                category: failure_category(e.category),
            }))
        }
        AiRealtimeEvent::ToolRequestCreated(e) => {
            Event::ConversationTool(ConversationToolEvent::Created(ToolRequestCreated {
                id: e.id,
                conversation_id: e.conversation_id,
                tool_name: e.tool_name,
                tool_source: source_to_tools(&e.tool_source).as_str().to_string(),
                arguments: e.arguments,
                approval_required: e.approval_required,
                expires_at: e.expires_at,
                chain_index: e.chain_index,
                created_at: e.created_at,
            }))
        }
        AiRealtimeEvent::ToolRequestUpdated(e) => {
            Event::ConversationTool(ConversationToolEvent::Updated(ToolRequestUpdated {
                id: e.id,
                conversation_id: e.conversation_id,
                status: e.status,
                decided_by_display_name: e.decided_by_display_name,
                duration_ms: e.duration_ms,
                has_result: e.has_result,
                error: e.error,
            }))
        }
    }
}

/// The ports the AI engine runs on, wired to the modules behind them.
pub fn ai_ports(pool: PgPool, presence: Arc<escalations::presence::Runtime>) -> AiPorts {
    AiPorts {
        knowledge: Arc::new(KnowledgeAdapter),
        tools: Arc::new(ToolsAdapter),
        escalation: Arc::new(EscalationAdapter {
            pool,
            presence: presence.clone(),
        }),
        realtime: Arc::new(RealtimeAdapter { presence }),
    }
}

impl AppState {
    /// The ports the AI engine runs on (see [`ai_ports()`]).
    pub fn ai_ports(&self) -> AiPorts {
        ai_ports(self.db.clone(), self.escalations.clone())
    }
}

/// The knowledge indexer's `Embedder`, answered by the `ai` service. A newtype because the server
/// owns neither the trait (`knowledge`) nor the type (`ai`), and `ai` may not implement a
/// `knowledge` trait without depending on it.
pub struct AiEmbedder(pub ai::AiService);

#[async_trait]
impl knowledge::indexer::Embedder for AiEmbedder {
    async fn embed(
        &self,
        tenant_id: Uuid,
        texts: Vec<String>,
        request_id: String,
    ) -> Result<Vec<Vec<f32>>, knowledge::indexer::EmbedError> {
        use knowledge::indexer::EmbedError;

        let ctx = ai::AiCallContext {
            tenant_id,
            request_id: Some(request_id),
        };
        match self.0.embed_platform(ctx, texts).await {
            Ok(embeddings) => Ok(embeddings),
            Err(ai::AiCallError::Provider { category, .. }) if category.retriable() => {
                Err(EmbedError::Retriable(category.as_str().to_string()))
            }
            Err(ai::AiCallError::Provider { category, .. }) => {
                Err(EmbedError::Permanent(category.as_str().to_string()))
            }
            Err(ai::AiCallError::NotConfigured) => Err(EmbedError::NotConfigured),
            Err(ai::AiCallError::Internal(e)) => Err(EmbedError::Permanent(e)),
        }
    }
}
