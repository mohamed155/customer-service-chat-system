use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ConversationFeedbackRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub conversation_id: Uuid,
    pub widget_session_id: Option<Uuid>,
    pub channel: String,
    pub agent_configuration_id: Option<Uuid>,
    pub assigned_membership_id: Option<Uuid>,
    pub rating: i16,
    pub comment: Option<String>,
    pub submitted_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubmitFeedbackPayload {
    pub rating: i16,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WidgetFeedbackDto {
    pub rating: i16,
    pub comment: Option<String>,
    pub submitted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WidgetFeedbackResponse {
    pub data: WidgetFeedbackResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WidgetFeedbackResponseData {
    pub feedback: WidgetFeedbackDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PendingFeedbackDto {
    pub conversation_id: Uuid,
    pub ended_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PendingFeedbackResponse {
    pub data: Option<PendingFeedbackDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FeedbackSummaryDto {
    pub average_rating: Option<f64>,
    pub feedback_count: i64,
}
