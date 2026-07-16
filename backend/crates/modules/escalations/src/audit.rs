use serde_json::json;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

const RESOURCE_TYPE_ESCALATION: &str = "escalation";
const RESOURCE_TYPE_SKILL: &str = "skill";
const RESOURCE_TYPE_MEMBER: &str = "member";
const RESOURCE_TYPE_AVAILABILITY: &str = "availability";

pub async fn record_escalation_created(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Uuid,
    escalation_id: Uuid,
    conversation_id: Uuid,
    reason: &str,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        "escalation.created",
        Some(actor_user_id),
        Some(tenant_id),
        RESOURCE_TYPE_ESCALATION,
        Some(&escalation_id.to_string()),
        &json!({ "conversation_id": conversation_id.to_string(), "reason": reason }),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn record_escalation_assigned(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Uuid,
    escalation_id: Uuid,
    routing_reason: &str,
    matched_skill_names: &[String],
    load_count: i64,
    assigned_membership_id: Uuid,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        "escalation.assigned",
        Some(actor_user_id),
        Some(tenant_id),
        RESOURCE_TYPE_ESCALATION,
        Some(&escalation_id.to_string()),
        &json!({
            "routing_reason": routing_reason,
            "matched_skills": matched_skill_names,
            "load_count": load_count,
            "assigned_membership_id": assigned_membership_id.to_string(),
        }),
    )
    .await
}

pub async fn record_escalation_queued(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Uuid,
    escalation_id: Uuid,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        "escalation.queued",
        Some(actor_user_id),
        Some(tenant_id),
        RESOURCE_TYPE_ESCALATION,
        Some(&escalation_id.to_string()),
        &json!({}),
    )
    .await
}

pub async fn record_escalation_claimed(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Uuid,
    escalation_id: Uuid,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        "escalation.claimed",
        Some(actor_user_id),
        Some(tenant_id),
        RESOURCE_TYPE_ESCALATION,
        Some(&escalation_id.to_string()),
        &json!({}),
    )
    .await
}

pub async fn record_escalation_closed(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Uuid,
    escalation_id: Uuid,
    cause: &str,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        "escalation.closed",
        Some(actor_user_id),
        Some(tenant_id),
        RESOURCE_TYPE_ESCALATION,
        Some(&escalation_id.to_string()),
        &json!({ "cause": cause }),
    )
    .await
}

pub async fn record_skill_created(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Uuid,
    skill_id: Uuid,
    name: &str,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        "skill.created",
        Some(actor_user_id),
        Some(tenant_id),
        RESOURCE_TYPE_SKILL,
        Some(&skill_id.to_string()),
        &json!({ "name": name }),
    )
    .await
}

pub async fn record_skill_updated(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Uuid,
    skill_id: Uuid,
    previous_name: &str,
    new_name: &str,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        "skill.updated",
        Some(actor_user_id),
        Some(tenant_id),
        RESOURCE_TYPE_SKILL,
        Some(&skill_id.to_string()),
        &json!({ "previous_name": previous_name, "new_name": new_name }),
    )
    .await
}

pub async fn record_skill_deleted(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Uuid,
    skill_id: Uuid,
    name: &str,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        "skill.deleted",
        Some(actor_user_id),
        Some(tenant_id),
        RESOURCE_TYPE_SKILL,
        Some(&skill_id.to_string()),
        &json!({ "name": name }),
    )
    .await
}

pub async fn record_member_skills_changed(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Uuid,
    membership_id: Uuid,
    previous_skill_ids: &[Uuid],
    new_skill_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        "member.skills_changed",
        Some(actor_user_id),
        Some(tenant_id),
        RESOURCE_TYPE_MEMBER,
        Some(&membership_id.to_string()),
        &json!({
            "previous_skill_ids": previous_skill_ids.iter().map(Uuid::to_string).collect::<Vec<_>>(),
            "new_skill_ids": new_skill_ids.iter().map(Uuid::to_string).collect::<Vec<_>>(),
        }),
    )
    .await
}

pub async fn record_availability_changed(
    tx: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    tenant_id: Uuid,
    membership_id: Uuid,
    previous_state: Option<&str>,
    new_state: &str,
    cause: &str,
) -> Result<(), sqlx::Error> {
    tenancy::audit::record_in_tx(
        tx,
        "availability.changed",
        Some(actor_user_id),
        Some(tenant_id),
        RESOURCE_TYPE_AVAILABILITY,
        Some(&membership_id.to_string()),
        &json!({
            "previous_state": previous_state,
            "new_state": new_state,
            "cause": cause,
        }),
    )
    .await
}
