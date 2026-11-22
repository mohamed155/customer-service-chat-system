//! What the ports hand to the use cases: rows of the store, without any query machinery.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// One live conversation, as stored.
#[derive(Debug, Clone)]
pub struct ConversationRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub customer_id: Uuid,
    pub channel: String,
    pub status: String,
    pub assigned_membership_id: Option<Uuid>,
    pub last_activity_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// A message that has just been stored.
#[derive(Debug, Clone)]
pub struct InsertedMessage {
    pub id: Uuid,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

/// A staff member, as the tenant directory knows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub user_id: Uuid,
    pub membership_id: Uuid,
    pub display_name: String,
    pub active: bool,
}

/// One inbox row: the conversation with its customer, latest message and widget.
#[derive(Debug, Clone)]
pub struct InboxRecord {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub customer_display_name: String,
    pub channel: String,
    pub status: String,
    pub assigned_membership_id: Option<Uuid>,
    pub last_message_kind: Option<String>,
    pub last_message_preview: Option<String>,
    pub last_activity_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub ai_handling: Option<String>,
    pub widget_instance_id: Option<Uuid>,
    pub widget_instance_name: Option<String>,
    pub feedback_rating: Option<i16>,
}

/// The row behind the conversation detail: an inbox row plus the customer's feedback.
#[derive(Debug, Clone)]
pub struct DetailRecord {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub customer_display_name: String,
    pub channel: String,
    pub status: String,
    pub assigned_membership_id: Option<Uuid>,
    pub last_message_kind: Option<String>,
    pub last_message_preview: Option<String>,
    pub last_activity_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub ai_handling: Option<String>,
    pub widget_instance_id: Option<Uuid>,
    pub widget_instance_name: Option<String>,
    pub feedback_rating: Option<i16>,
    pub feedback_comment: Option<String>,
    pub feedback_submitted_at: Option<DateTime<Utc>>,
}

/// One message of a timeline, with the customer it is addressed from or to.
#[derive(Debug, Clone)]
pub struct TimelineRecord {
    pub id: Uuid,
    pub kind: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub seq: i64,
    pub customer_id: Option<Uuid>,
    pub customer_display_name: Option<String>,
    pub sender_membership_id: Option<Uuid>,
    pub logged_by_membership_id: Option<Uuid>,
    pub ai_confidence_score: Option<f32>,
}
