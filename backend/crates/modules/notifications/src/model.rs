use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum NotificationKind {
    #[serde(rename = "escalation.new")]
    EscalationNew,
    #[serde(rename = "conversation.assigned")]
    ConversationAssigned,
    #[serde(rename = "ai.response_failed")]
    AiResponseFailed,
    #[serde(rename = "tool.approval_required")]
    ToolApprovalRequired,
}

impl NotificationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EscalationNew => "escalation.new",
            Self::ConversationAssigned => "conversation.assigned",
            Self::AiResponseFailed => "ai.response_failed",
            Self::ToolApprovalRequired => "tool.approval_required",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum NotificationState {
    Unread,
    Read,
    Resolved,
}

impl NotificationState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unread => "unread",
            Self::Read => "read",
            Self::Resolved => "resolved",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum SubjectType {
    Conversation,
    Escalation,
    ToolRequest,
}

impl SubjectType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Conversation => "conversation",
            Self::Escalation => "escalation",
            Self::ToolRequest => "tool_request",
        }
    }
}

#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct NotificationRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub recipient_membership_id: Uuid,
    pub kind: String,
    pub state: String,
    pub title: String,
    pub body: Option<String>,
    pub subject_type: String,
    pub subject_id: Uuid,
    pub dedupe_key: String,
    pub actor_membership_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotificationActorDto {
    pub membership_id: Uuid,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotificationDto {
    pub id: Uuid,
    pub kind: String,
    pub state: String,
    pub title: String,
    pub body: Option<String>,
    pub subject_type: String,
    pub subject_id: Uuid,
    pub actor: Option<NotificationActorDto>,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PaginationInfo {
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct NotificationListResponse {
    pub data: Vec<NotificationDto>,
    pub pagination: PaginationInfo,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MarkedResponse {
    pub marked: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnreadCountResponse {
    pub count: i64,
}
