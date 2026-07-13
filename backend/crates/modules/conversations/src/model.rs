use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationStatus {
    Open,
    Pending,
    Resolved,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Customer,
    Reply,
    Note,
}

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assignee {
    pub membership_id: Uuid,
    pub display_name: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerRef {
    pub id: Uuid,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastMessagePreview {
    pub kind: MessageKind,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    #[serde(rename = "type")]
    pub participant_type: String,
    pub id: Option<Uuid>,
    pub membership_id: Option<Uuid>,
    pub display_name: String,
    pub active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Uuid,
    pub customer: CustomerRef,
    pub channel: String,
    pub status: ConversationStatus,
    pub assignee: Option<Assignee>,
    pub last_message: Option<LastMessagePreview>,
    pub last_activity_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationDetail {
    pub id: Uuid,
    pub customer: CustomerRef,
    pub channel: String,
    pub status: ConversationStatus,
    pub assignee: Option<Assignee>,
    pub last_message: Option<LastMessagePreview>,
    pub last_activity_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub participants: Vec<Participant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub kind: MessageKind,
    pub sender: Participant,
    pub logged_by: Option<Assignee>,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationStatusRef {
    pub status: ConversationStatus,
    pub last_activity_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddMessageResponse {
    pub message: Message,
    pub conversation: ConversationStatusRef,
}

// ---------------------------------------------------------------------------
// Request payloads
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConversationPayload {
    pub customer_id: Uuid,
    pub channel: String,
    pub message: CreateMessagePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMessagePayload {
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddMessagePayload {
    pub kind: MessageKind,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PatchConversationPayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ConversationStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assigned_membership_id: Option<Option<Uuid>>,
}
