use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Conversation lifecycle status.
///
/// Variants serialize to their snake_case wire names (`open`, `pending`,
/// `resolved`, `closed`) via the explicit serde rename; the OpenAPI schema
/// surfaces the same names so the documented vocabulary matches the wire
/// format (FR-006 / data-model.md).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationStatus {
    Open,
    Pending,
    Resolved,
    Closed,
}

/// Message kind — the sender role that produced a given message row.
///
/// Variants serialize to their snake_case wire names (`customer`, `reply`,
/// `note`) so the documented vocabulary matches the wire format
/// (FR-006 / data-model.md).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Customer,
    #[serde(rename = "ai")]
    Ai,
    #[serde(rename = "system")]
    System,
    Reply,
    Note,
}

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

/// Staff assignee reference embedded in conversation and message responses.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Assignee {
    pub membership_id: Uuid,
    pub display_name: String,
    pub active: bool,
}

/// Light-weight customer reference used in inbox and detail responses.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CustomerRef {
    pub id: Uuid,
    pub display_name: String,
}

/// Preview snippet of the most recent message on a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LastMessagePreview {
    pub kind: MessageKind,
    pub preview: String,
}

/// One party in a conversation: either the customer (and a `type` of
/// `customer` with `id` set) or a staff assignee (and a `type` of
/// `assignee` with `membership_id` and `active` set).  The `type` field is
/// renamed via serde to match the wire shape; the OpenAPI schema surfaces
/// the same field name.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Participant {
    #[serde(rename = "type")]
    pub participant_type: String,
    pub id: Option<Uuid>,
    pub membership_id: Option<Uuid>,
    pub display_name: String,
    pub active: Option<bool>,
}

/// Inbox-row projection of a conversation, returned by
/// `GET /tenant/conversations`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Conversation {
    pub id: Uuid,
    pub customer: CustomerRef,
    pub channel: String,
    pub status: ConversationStatus,
    pub assignee: Option<Assignee>,
    pub last_message: Option<LastMessagePreview>,
    pub last_activity_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub ai_handling: Option<String>,
    pub awaiting_ai_decision: bool,
}

/// Full conversation detail returned by the create / update / get handlers.
///
/// `participants` lists every party on the conversation (the customer plus
/// any staff assignees).  Optional fields follow the wire's nullable
/// semantics; `assignee` and `last_message` are both optional+nullable.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
    pub ai_handling: Option<String>,
    pub awaiting_ai_decision: bool,
}

/// One message in a conversation, returned by the timeline endpoint and
/// embedded in `AddMessageResponse`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Message {
    pub id: Uuid,
    pub kind: MessageKind,
    pub sender: Participant,
    pub logged_by: Option<Assignee>,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

/// Updated conversation status + activity timestamp returned alongside an
/// inserted message.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConversationStatusRef {
    pub status: ConversationStatus,
    pub last_activity_at: DateTime<Utc>,
}

/// Response of `POST /tenant/conversations/{id}/messages`: the inserted
/// message plus the refreshed conversation status reference.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddMessageResponse {
    pub message: Message,
    pub conversation: ConversationStatusRef,
}

// ---------------------------------------------------------------------------
// Request payloads
// ---------------------------------------------------------------------------

/// Body of `POST /tenant/conversations`.  Creates a conversation with the
/// first message (always a `reply` per FR-007).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateConversationPayload {
    pub customer_id: Uuid,
    pub channel: String,
    pub message: CreateMessagePayload,
}

/// First-message body carried by `CreateConversationPayload`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateMessagePayload {
    pub body: String,
}

/// Body of `POST /tenant/conversations/{id}/messages`.  `kind` is the
/// sender role; the handler resolves the membership id and audit metadata
/// from the authenticated principal.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddMessagePayload {
    pub kind: MessageKind,
    pub body: String,
}

/// Body of `PATCH /tenant/conversations/{id}`.  At least one of `status`
/// or `assigned_membership_id` MUST be supplied (the handler returns 422
/// otherwise).  `assigned_membership_id` is tri-state on the wire:
///
///   * field absent            → leave assignment unchanged.
///   * field = `null`          → unassign.
///   * field = `uuid`          → assign to that membership.
#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct PatchConversationPayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ConversationStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assigned_membership_id: Option<Option<Uuid>>,
}
