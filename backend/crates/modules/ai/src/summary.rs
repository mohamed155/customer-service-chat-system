use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Extension;
use axum::Json;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use tenancy::TenantContext;

use crate::{AiCallContext, AiCallError, AiInput, AiService};

const SUMMARY_SYSTEM_PROMPT: &str = "\
You are a support-operations assistant helping a human agent take over a customer conversation. \
Summarize the conversation below for an internal teammate — not for the customer. \
Write 3 to 5 sentences, plain and factual, covering exactly:
1. What the customer wants (their goal or problem).
2. What has already been tried or answered so far.
3. The current state and any open question or next step.
Do not address the customer. Do not invent details that are not in the transcript. \
Do not include pleasantries, headings, or bullet formatting — return only the summary prose.";

#[derive(Serialize)]
pub struct SummaryResponse {
    summary: String,
    #[serde(rename = "generatedAt")]
    generated_at: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "messageCount")]
    message_count: usize,
}

/// Handle `POST /tenant/conversations/{id}/summary`.
///
/// Generates a staff-only, on-demand summary of the conversation.
/// Side-effect-free — nothing is persisted.
/// Authz is enforced by the route's permission layer (ConversationsView).
pub async fn handle_summary(
    State(pool): State<PgPool>,
    Extension(ai): Extension<AiService>,
    ctx: TenantContext,
    Path(conversation_id): Path<Uuid>,
) -> impl IntoResponse {
    let tenant_id = ctx.tenant_id;

    // Verify the conversation belongs to the tenant
    let conv_exists: bool = match sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM conversations WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL)",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    {
        Ok(e) => e,
        Err(_) => return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "internal", "message": "Database error"})),
        ),
    };

    if !conv_exists {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "not_found", "message": "Conversation not found"})),
        );
    }

    // Load conversation history
    let history = match conversations::queries::summary_history(
        &pool,
        tenant_id,
        conversation_id,
        50,
    )
    .await
    {
        Ok(h) => h,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "internal", "message": e.to_string()})),
            );
        }
    };

    if history.is_empty() {
        return (
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error": "validation_failed",
                "message": "Conversation has no messages to summarize"
            })),
        );
    }

    // Build the message turns for the summary prompt
    let mut turns = String::new();
    for (kind, body) in &history {
        match kind.as_str() {
            "customer" => turns.push_str(&format!("User: {body}\n")),
            "ai" | "reply" => turns.push_str(&format!("Assistant: {body}\n")),
            "system" => turns.push_str(&format!("[system: {body}]\n")),
            _ => {}
        }
    }

    let input = AiInput {
        system: Some(SUMMARY_SYSTEM_PROMPT.to_string()),
        messages: vec![ai_providers::Message {
            role: ai_providers::Role::User,
            content: turns,
        }],
    };

    let call_ctx = AiCallContext {
        tenant_id,
        request_id: None,
    };

    let result = ai.complete(call_ctx, input).await;

    match result {
        Ok(completion) => {
            let response = SummaryResponse {
                summary: completion.content,
                generated_at: chrono::Utc::now(),
                message_count: history.len(),
            };
            (
                axum::http::StatusCode::OK,
                Json(serde_json::to_value(&response).unwrap_or_default()),
            )
        }
        Err(AiCallError::NotConfigured) => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "ai_not_configured",
                "message": "AI is not configured for this tenant"
            })),
        ),
        Err(e) => {
            tracing::warn!(?e, "summary generation failed");
            (
                axum::http::StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": "provider_error",
                    "message": "AI provider failed to generate summary"
                })),
            )
        }
    }
}
