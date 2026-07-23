use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WhatsappMessageMetaRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub message_id: Uuid,
    pub conversation_id: Uuid,
    pub direction: String,
    pub wamid: Option<String>,
    pub provider_timestamp: Option<DateTime<Utc>>,
    pub delivery_status: Option<String>,
    pub failure_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MessageAttachmentRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub message_id: Uuid,
    pub kind: String,
    pub status: String,
    pub provider_media_id: Option<String>,
    pub storage_key: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub file_name: Option<String>,
    pub fetch_attempts: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    Pending,
    Sent,
    Delivered,
    Read,
    Failed,
}

impl DeliveryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Sent => "sent",
            Self::Delivered => "delivered",
            Self::Read => "read",
            Self::Failed => "failed",
        }
    }

    pub fn rank(&self) -> u8 {
        match self {
            Self::Pending => 0,
            Self::Sent => 1,
            Self::Delivered => 2,
            Self::Read => 3,
            Self::Failed => 99,
        }
    }
}

impl std::fmt::Display for DeliveryStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for DeliveryStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "sent" => Ok(Self::Sent),
            "delivered" => Ok(Self::Delivered),
            "read" => Ok(Self::Read),
            "failed" => Ok(Self::Failed),
            _ => Err(format!("invalid delivery status: {s}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    Image,
    Audio,
    Video,
    Document,
}

impl AttachmentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Document => "document",
        }
    }
}

impl std::str::FromStr for AttachmentKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "image" => Ok(Self::Image),
            "audio" => Ok(Self::Audio),
            "video" => Ok(Self::Video),
            "document" => Ok(Self::Document),
            _ => Err(format!("invalid attachment kind: {s}")),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct WebhookEnvelope {
    pub entry: Vec<Entry>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Entry {
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Change {
    pub value: WebhookValue,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct WebhookValue {
    pub messages: Vec<IncomingMessage>,
    pub statuses: Vec<StatusUpdate>,
    pub contacts: Vec<Contact>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct IncomingMessage {
    pub id: String,
    pub from: String,
    pub timestamp: String,
    #[serde(rename = "type")]
    pub message_type: String,
    pub text: Option<TextContent>,
    pub image: Option<MediaContent>,
    pub audio: Option<MediaContent>,
    pub video: Option<MediaContent>,
    pub document: Option<MediaContent>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct TextContent {
    pub body: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct MediaContent {
    pub id: String,
    pub mime_type: Option<String>,
    pub caption: Option<String>,
    pub filename: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct StatusUpdate {
    pub id: String,
    pub status: String,
    pub timestamp: String,
    #[serde(default)]
    pub errors: Vec<StatusError>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct StatusError {
    pub code: i64,
    pub title: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Contact {
    pub profile: Option<Profile>,
    pub wa_id: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Profile {
    pub name: Option<String>,
}
