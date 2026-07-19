use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WidgetInstanceRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub public_id: String,
    pub name: String,
    pub display_name: String,
    pub primary_color: Option<String>,
    pub welcome_message: Option<String>,
    pub position: Option<String>,
    pub theme: Option<String>,
    pub enabled: bool,
    pub allowed_domains: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WidgetSessionRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub widget_instance_id: Uuid,
    pub token_hash: Vec<u8>,
    pub customer_id: Option<Uuid>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublicWidgetConfigDto {
    pub widget_id: String,
    pub display_name: String,
    pub primary_color: Option<String>,
    pub welcome_message: Option<String>,
    pub position: Option<String>,
    pub theme: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionPayload {
    pub widget_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionResponseDto {
    pub session_token: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendMessagePayload {
    pub body: String,
}

// ── Admin DTOs ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WidgetInstanceDto {
    pub id: Uuid,
    pub public_id: String,
    pub name: String,
    pub display_name: String,
    pub primary_color: Option<String>,
    pub welcome_message: Option<String>,
    pub position: Option<String>,
    pub theme: Option<String>,
    pub enabled: bool,
    pub allowed_domains: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateWidgetInstancePayload {
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub primary_color: Option<String>,
    #[serde(default)]
    pub welcome_message: Option<String>,
    #[serde(default)]
    pub position: Option<String>,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub allowed_domains: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWidgetInstancePayload {
    pub name: String,
    pub display_name: String,
    pub primary_color: Option<String>,
    pub welcome_message: Option<String>,
    pub position: Option<String>,
    pub theme: Option<String>,
    pub enabled: bool,
    pub allowed_domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WidgetSnippetResponse {
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WidgetMessageDto {
    pub id: uuid::Uuid,
    pub sender: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_display_name: Option<String>,
    pub body: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WidgetConversationDto {
    pub id: uuid::Uuid,
    pub handling: String,
    pub team_online: bool,
    pub ended_note: bool,
    pub messages: Vec<WidgetMessageDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WidgetConversationResponse {
    pub data: Option<WidgetConversationDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WidgetMessageResponse {
    pub data: WidgetMessageResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WidgetMessageResponseData {
    pub message: WidgetMessageDto,
}
