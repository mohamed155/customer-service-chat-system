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

/// Feedback left by a customer on a conversation, rendered in the dashboard.
/// Defined locally in the conversations crate to avoid a circular dependency
/// with the feedback crate.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TenantFeedbackDto {
    pub rating: i16,
    pub comment: Option<String>,
    pub submitted_at: DateTime<Utc>,
}

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

/// Reference to a widget instance that originated a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WidgetInstanceRef {
    pub id: Uuid,
    pub name: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget_instance: Option<WidgetInstanceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<i16>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget_instance: Option<WidgetInstanceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback: Option<TenantFeedbackDto>,
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
    #[serde(default)]
    pub citations: Vec<CitationView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<ConfidenceView>,
}

/// Deterministic confidence metadata attached to AI messages.
/// Band is derived server-side using a local 3-line function (see
/// `confidence_band`) — intentionally duplicated from `ai::confidence`
/// to avoid a circular crate dependency.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ConfidenceView {
    pub score: f32,
    pub band: String,
}

pub fn confidence_band(score: f32) -> &'static str {
    if score >= 0.70 {
        "high"
    } else if score >= 0.40 {
        "medium"
    } else {
        "low"
    }
}

/// A citation linking an AI message to a knowledge-base passage at the
/// time the reply was generated.  Snapshotted so the citation remains
/// readable even if the source item is later deleted or updated.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CitationView {
    pub knowledge_item_id: Uuid,
    pub item_title: String,
    pub passage_text: String,
    pub relevance_score: f32,
    pub item_available: bool,
}

/// Input type for inserting a batch of citations inside a transaction.
pub struct CitationToInsert {
    pub knowledge_item_id: Uuid,
    pub item_title: String,
    pub passage_text: String,
    pub relevance_score: f32,
    pub ordinal: i32,
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
