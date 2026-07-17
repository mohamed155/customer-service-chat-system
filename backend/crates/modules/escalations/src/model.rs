use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum RoutingReason {
    #[serde(rename = "skill_match")]
    SkillMatch,
    #[serde(rename = "load_fallback")]
    LoadFallback,
    #[serde(rename = "manual_claim")]
    ManualClaim,
    #[serde(rename = "queue_auto")]
    QueueAuto,
    #[serde(rename = "manual_reassignment")]
    ManualReassignment,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequiredSkillRef {
    pub id: Option<Uuid>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RoutingInfo {
    pub reason: RoutingReason,
    pub matched_skills: Vec<String>,
    pub assigned_membership_id: Uuid,
    pub assigned_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum EscalationStatus {
    #[serde(rename = "queued")]
    Queued,
    #[serde(rename = "assigned")]
    Assigned,
    #[serde(rename = "closed")]
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Escalation {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub reason: String,
    pub required_skills: Vec<RequiredSkillRef>,
    pub status: EscalationStatus,
    pub routing: Option<RoutingInfo>,
    pub escalated_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomerRef {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueueEntryConversationRef {
    pub id: Uuid,
    pub channel: String,
    pub customer: CustomerRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueueEntry {
    pub escalation: Escalation,
    pub conversation: QueueEntryConversationRef,
    pub waiting_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: Uuid,
    pub name: String,
    pub agent_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum AvailabilityState {
    #[serde(rename = "available")]
    Available,
    #[serde(rename = "away")]
    Away,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    pub membership_id: Uuid,
    pub state: AvailabilityState,
    pub state_changed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EscalatePayload {
    pub reason: String,
    pub required_skill_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetAvailabilityPayload {
    pub state: AvailabilityState,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSkillPayload {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RenameSkillPayload {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetMemberSkillsPayload {
    pub skill_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EscalationAssignedEvent {
    pub v: u32,
    pub escalation_id: Uuid,
    pub conversation_id: Uuid,
    pub reason: String,
    pub routing_reason: RoutingReason,
    pub matched_skills: Vec<String>,
    pub assigned_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EscalationQueuedEvent {
    pub v: u32,
    pub escalation_id: Uuid,
    pub conversation_id: Uuid,
    pub escalated_at: DateTime<Utc>,
    pub required_skills: Vec<RequiredSkillRef>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EscalationRemovedEvent {
    pub v: u32,
    pub escalation_id: Uuid,
    pub cause: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AvailabilityChangedEvent {
    pub v: u32,
    pub membership_id: Uuid,
    pub state: AvailabilityState,
    pub cause: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TeamMemberSkill {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TeamMemberWithSkills {
    #[serde(flatten)]
    pub member: serde_json::Value,
    pub skills: Vec<TeamMemberSkill>,
    pub availability: AvailabilityState,
}

// ── AI conversation SSE event payloads ────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ConversationAiEvent {
    Started(ConversationAiStarted),
    Delta(ConversationAiDelta),
    Completed(ConversationAiCompleted),
    Superseded(ConversationAiSuperseded),
    Failed(ConversationAiFailed),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationAiStarted {
    pub conversation_id: Uuid,
    pub generation_id: Uuid,
    pub trigger_message_id: Uuid,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationAiDelta {
    pub conversation_id: Uuid,
    pub generation_id: Uuid,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationAiCompleted {
    pub conversation_id: Uuid,
    pub generation_id: Uuid,
    pub message: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SupersededReason {
    NewerMessage,
    Escalated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationAiSuperseded {
    pub conversation_id: Uuid,
    pub generation_id: Uuid,
    pub reason: SupersededReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FailureCategory {
    Unavailable,
    Timeout,
    RateLimited,
    Authentication,
    InvalidRequest,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationAiFailed {
    pub conversation_id: Uuid,
    pub generation_id: Uuid,
    pub category: FailureCategory,
}

pub mod sql {
    use sqlx::Postgres;
    use uuid::Uuid;

    pub async fn skill_ids_exist_in_tenant_in_tx(
        tx: &mut sqlx::Transaction<'_, Postgres>,
        tenant_id: Uuid,
        ids: &[Uuid],
    ) -> sqlx::Result<bool> {
        if ids.is_empty() {
            return Ok(true);
        }
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM skills WHERE tenant_id = $1 AND id = ANY($2)")
                .bind(tenant_id)
                .bind(ids)
                .fetch_one(&mut **tx)
                .await?;
        Ok(count as usize == ids.len())
    }

    pub async fn skill_names_for_ids_in_tx(
        tx: &mut sqlx::Transaction<'_, Postgres>,
        tenant_id: Uuid,
        ids: &[Uuid],
    ) -> sqlx::Result<Vec<String>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM skills WHERE tenant_id = $1 AND id = ANY($2) ORDER BY id",
        )
        .bind(tenant_id)
        .bind(ids)
        .fetch_all(&mut **tx)
        .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }
}
