use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::model::{Availability, AvailabilityState, Escalation, EscalationStatus, RequiredSkillRef, RoutingInfo, RoutingReason};

// ---------------------------------------------------------------------------
// Cursor helpers
// ---------------------------------------------------------------------------

pub fn encode_queue_cursor(escalated_at: &DateTime<Utc>, id: &Uuid) -> String {
    hex::encode(format!("{}|{}", escalated_at.to_rfc3339(), id))
}

pub fn decode_queue_cursor(cursor: &str) -> Option<(DateTime<Utc>, Uuid)> {
    let decoded = hex::decode(cursor).ok()?;
    let s = std::str::from_utf8(&decoded).ok()?;
    let (date_str, id_str) = s.split_once('|')?;
    let escalated_at = DateTime::parse_from_rfc3339(date_str)
        .ok()?
        .with_timezone(&Utc);
    let id = Uuid::parse_str(id_str).ok()?;
    Some((escalated_at, id))
}

// ---------------------------------------------------------------------------
// Escalation queries
// ---------------------------------------------------------------------------

pub async fn escalation_row_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    id: Uuid,
) -> sqlx::Result<Option<EscalationRow>> {
    sqlx::query_as::<_, EscalationRow>(
        "SELECT id, tenant_id, conversation_id, reason, required_skill_ids, \
                required_skill_names, status, routing_reason, matched_skill_ids, \
                matched_skill_names, assigned_membership_id, escalated_at, \
                assigned_at, closed_at, created_at \
         FROM escalations \
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant_id)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
}

pub async fn active_escalation_for_conversation_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Option<EscalationRow>> {
    sqlx::query_as::<_, EscalationRow>(
        "SELECT id, tenant_id, conversation_id, reason, required_skill_ids, \
                required_skill_names, status, routing_reason, matched_skill_ids, \
                matched_skill_names, assigned_membership_id, escalated_at, \
                assigned_at, closed_at, created_at \
         FROM escalations \
         WHERE tenant_id = $1 AND conversation_id = $2 AND status IN ('queued', 'assigned') \
         LIMIT 1",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_optional(&mut **tx)
    .await
}

pub async fn latest_escalation_for_conversation_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Option<Escalation>> {
    let row: Option<EscalationRow> = sqlx::query_as::<_, EscalationRow>(
        "SELECT id, tenant_id, conversation_id, reason, required_skill_ids, \
                required_skill_names, status, routing_reason, matched_skill_ids, \
                matched_skill_names, assigned_membership_id, escalated_at, \
                assigned_at, closed_at, created_at \
         FROM escalations \
         WHERE tenant_id = $1 AND conversation_id = $2 \
         ORDER BY created_at DESC \
         LIMIT 1",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(row_to_escalation))
}

// ---------------------------------------------------------------------------
// Routing queries (the ranked candidate selection statement)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Candidate {
    pub membership_id: Uuid,
    pub match_count: i64,
    pub matched_ids: Vec<Uuid>,
    pub load_count: i64,
}

pub async fn select_candidate_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    required_skill_ids: &[Uuid],
    present_ids: &[Uuid],
) -> sqlx::Result<Option<Candidate>> {
    let candidate = sqlx::query_as::<_, Candidate>(
        "SELECT tm.id AS membership_id, \
                COALESCE(m.match_count, 0) AS match_count, \
                COALESCE(m.matched_ids, '{}') AS matched_ids, \
                COALESCE(l.load_count, 0) AS load_count \
         FROM tenant_memberships tm \
         JOIN agent_availability aa ON aa.tenant_id = tm.tenant_id AND aa.membership_id = tm.id \
         LEFT JOIN LATERAL ( \
             SELECT COUNT(*) AS match_count, array_agg(ask.skill_id) AS matched_ids \
             FROM agent_skills ask \
             WHERE ask.tenant_id = tm.tenant_id AND ask.membership_id = tm.id \
               AND ask.skill_id = ANY($3) \
         ) m ON true \
         LEFT JOIN LATERAL ( \
             SELECT COUNT(*) AS load_count FROM conversations c \
             WHERE c.tenant_id = tm.tenant_id AND c.assigned_membership_id = tm.id \
               AND c.status IN ('open','pending') AND c.deleted_at IS NULL \
         ) l ON true \
         WHERE tm.tenant_id = $1 AND tm.status = 'active' AND tm.deleted_at IS NULL \
           AND tm.role IN ('owner','admin','manager','agent') \
           AND aa.state = 'available' \
           AND tm.id = ANY($4) \
         ORDER BY match_count DESC, load_count ASC, tm.id ASC \
         LIMIT 1",
    )
    .bind(tenant_id)
    .bind(required_skill_ids)
    .bind(required_skill_ids)
    .bind(present_ids)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(candidate)
}

pub async fn take_tenant_routing_lock_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('escalations.routing'), hashtext($1::text))")
        .bind(tenant_id.to_string())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Availability queries
// ---------------------------------------------------------------------------

pub async fn get_availability_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    membership_id: Uuid,
) -> sqlx::Result<Option<AvailabilityRow>> {
    sqlx::query_as::<_, AvailabilityRow>(
        "SELECT tenant_id, membership_id, state, state_changed_at \
         FROM agent_availability \
         WHERE tenant_id = $1 AND membership_id = $2",
    )
    .bind(tenant_id)
    .bind(membership_id)
    .fetch_optional(&mut **tx)
    .await
}

pub async fn upsert_availability_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    membership_id: Uuid,
    state: &str,
) -> sqlx::Result<Availability> {
    let row: AvailabilityRow = sqlx::query_as::<_, AvailabilityRow>(
        "INSERT INTO agent_availability (tenant_id, membership_id, state, state_changed_at) \
         VALUES ($1, $2, $3, now()) \
         ON CONFLICT (tenant_id, membership_id) \
         DO UPDATE SET state = EXCLUDED.state, state_changed_at = now() \
         RETURNING tenant_id, membership_id, state, state_changed_at",
    )
    .bind(tenant_id)
    .bind(membership_id)
    .bind(state)
    .fetch_one(&mut **tx)
    .await?;
    Ok(Availability {
        membership_id: row.membership_id,
        state: serde_json::from_value(serde_json::Value::String(row.state))
            .unwrap_or(AvailabilityState::Away),
        state_changed_at: Some(row.state_changed_at),
    })
}

// ---------------------------------------------------------------------------
// Skills queries
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SkillRow {
    pub id: Uuid,
    pub name: String,
    pub agent_count: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AgentCountRow {
    pub skill_id: Uuid,
    pub count: i64,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SkillIdName {
    pub id: Uuid,
    pub name: String,
}

pub async fn skill_ids_exist_in_tenant_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    ids: &[Uuid],
) -> sqlx::Result<bool> {
    if ids.is_empty() {
        return Ok(true);
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM skills WHERE tenant_id = $1 AND id = ANY($2)",
    )
    .bind(tenant_id)
    .bind(ids)
    .fetch_one(&mut **tx)
    .await?;
    Ok(count as usize == ids.len())
}

pub async fn list_skills_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> sqlx::Result<Vec<crate::model::Skill>> {
    let rows: Vec<SkillRow> = sqlx::query_as::<_, SkillRow>(
        "SELECT s.id, s.name, COUNT(ag.membership_id)::bigint AS agent_count \
         FROM skills s \
         LEFT JOIN agent_skills ag ON ag.skill_id = s.id AND ag.tenant_id = s.tenant_id \
         WHERE s.tenant_id = $1 \
         GROUP BY s.id, s.name \
         ORDER BY s.name",
    )
    .bind(tenant_id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| crate::model::Skill {
            id: r.id,
            name: r.name,
            agent_count: r.agent_count,
        })
        .collect())
}

pub async fn create_skill_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    name: &str,
) -> sqlx::Result<crate::model::Skill> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO skills (tenant_id, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind(name)
    .fetch_one(&mut **tx)
    .await?;
    Ok(crate::model::Skill {
        id,
        name: name.to_owned(),
        agent_count: 0,
    })
}

pub async fn rename_skill_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    skill_id: Uuid,
    new_name: &str,
) -> sqlx::Result<crate::model::Skill> {
    let row: SkillIdName = sqlx::query_as::<_, SkillIdName>(
        "UPDATE skills SET name = $1 WHERE tenant_id = $2 AND id = $3 \
         RETURNING id, name",
    )
    .bind(new_name)
    .bind(tenant_id)
    .bind(skill_id)
    .fetch_one(&mut **tx)
    .await?;
    let agent_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM agent_skills WHERE tenant_id = $1 AND skill_id = $2",
    )
    .bind(tenant_id)
    .bind(skill_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(crate::model::Skill {
        id: row.id,
        name: row.name,
        agent_count,
    })
}

pub async fn delete_skill_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    skill_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE escalations SET required_skill_ids = array_remove(required_skill_ids, $1) \
         WHERE tenant_id = $2 AND status = 'queued' AND $1 = ANY(required_skill_ids)",
    )
    .bind(skill_id)
    .bind(tenant_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM skills WHERE tenant_id = $1 AND id = $2")
        .bind(tenant_id)
        .bind(skill_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn set_member_skills_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    membership_id: Uuid,
    skill_ids: &[Uuid],
) -> sqlx::Result<Vec<crate::model::Skill>> {
    if !skill_ids.is_empty() && !skill_ids_exist_in_tenant_in_tx(tx, tenant_id, skill_ids).await? {
        return Err(sqlx::Error::Protocol("Unknown skill id in set".into()));
    }
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM tenant_memberships WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(membership_id)
    .fetch_optional(&mut **tx)
    .await?;
    match role.as_deref() {
        Some("owner" | "admin" | "manager" | "agent") => {}
        _ => {
            return Err(sqlx::Error::Protocol(
                "Target membership is not agent-capable".into(),
            ));
        }
    }

    sqlx::query("DELETE FROM agent_skills WHERE tenant_id = $1 AND membership_id = $2")
        .bind(tenant_id)
        .bind(membership_id)
        .execute(&mut **tx)
        .await?;

    for skill_id in skill_ids {
        sqlx::query(
            "INSERT INTO agent_skills (tenant_id, membership_id, skill_id) VALUES ($1, $2, $3)",
        )
        .bind(tenant_id)
        .bind(membership_id)
        .bind(skill_id)
        .execute(&mut **tx)
        .await?;
    }

    let rows: Vec<SkillRow> = sqlx::query_as::<_, SkillRow>(
        "SELECT s.id, s.name, COUNT(ag.membership_id)::bigint AS agent_count \
         FROM skills s \
         LEFT JOIN agent_skills ag ON ag.skill_id = s.id AND ag.tenant_id = s.tenant_id \
         WHERE s.tenant_id = $1 AND s.id = ANY($2) \
         GROUP BY s.id, s.name",
    )
    .bind(tenant_id)
    .bind(skill_ids)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| crate::model::Skill {
            id: r.id,
            name: r.name,
            agent_count: r.agent_count,
        })
        .collect())
}

pub async fn skills_and_availability_for_members_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    membership_ids: &[Uuid],
) -> sqlx::Result<std::collections::HashMap<Uuid, (Vec<crate::model::Skill>, AvailabilityState)>> {
    use std::collections::HashMap;

    let skill_rows: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT ag.membership_id, s.id, s.name \
         FROM agent_skills ag \
         JOIN skills s ON s.id = ag.skill_id AND s.tenant_id = ag.tenant_id \
         WHERE ag.tenant_id = $1 AND ag.membership_id = ANY($2)",
    )
    .bind(tenant_id)
    .bind(membership_ids)
    .fetch_all(&mut **tx)
    .await?;

    let avail_rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT membership_id, state FROM agent_availability \
         WHERE tenant_id = $1 AND membership_id = ANY($2)",
    )
    .bind(tenant_id)
    .bind(membership_ids)
    .fetch_all(&mut **tx)
    .await?;

    let mut result: HashMap<Uuid, (Vec<crate::model::Skill>, AvailabilityState)> = HashMap::new();
    for mid in membership_ids {
        result.insert(*mid, (Vec::new(), AvailabilityState::Away));
    }
    for (mid, sid, sname) in skill_rows {
        if let Some((skills, _)) = result.get_mut(&mid) {
            skills.push(crate::model::Skill {
                id: sid,
                name: sname,
                agent_count: 0,
            });
        }
    }
    for (mid, state) in avail_rows {
        if let Some((_, avail)) = result.get_mut(&mid) {
            *avail = serde_json::from_value(serde_json::Value::String(state))
                .unwrap_or(AvailabilityState::Away);
        }
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Row types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EscalationRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub conversation_id: Uuid,
    pub reason: String,
    pub required_skill_ids: Vec<Uuid>,
    pub required_skill_names: Vec<String>,
    pub status: String,
    pub routing_reason: Option<String>,
    pub matched_skill_ids: Vec<Uuid>,
    pub matched_skill_names: Vec<String>,
    pub assigned_membership_id: Option<Uuid>,
    pub escalated_at: DateTime<Utc>,
    pub assigned_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AvailabilityRow {
    pub tenant_id: Uuid,
    pub membership_id: Uuid,
    pub state: String,
    pub state_changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct QueueEntryRow {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub reason: String,
    pub required_skill_ids: Vec<Uuid>,
    pub required_skill_names: Vec<String>,
    pub status: String,
    pub escalated_at: DateTime<Utc>,
    pub conv_channel: String,
    pub cust_id: Uuid,
    pub cust_name: String,
    pub waiting_seconds: i64,
}

pub fn row_to_escalation(row: EscalationRow) -> Escalation {
    let required_skills: Vec<RequiredSkillRef> = row
        .required_skill_ids
        .iter()
        .cloned()
        .zip(
            row.required_skill_names
                .iter()
                .cloned()
                .chain(std::iter::repeat(String::new())),
        )
        .map(|(id, name)| RequiredSkillRef {
            id: Some(id),
            name,
        })
        .collect();

    let routing = match row.routing_reason {
        Some(reason_str) => {
            let reason = match reason_str.as_str() {
                "skill_match" => RoutingReason::SkillMatch,
                "load_fallback" => RoutingReason::LoadFallback,
                "manual_claim" => RoutingReason::ManualClaim,
                "queue_auto" => RoutingReason::QueueAuto,
                "manual_reassignment" => RoutingReason::ManualReassignment,
                _ => return Escalation {
                    id: row.id,
                    conversation_id: row.conversation_id,
                    reason: row.reason,
                    required_skills,
                    status: serde_json::from_value(serde_json::Value::String(row.status))
                        .unwrap_or(EscalationStatus::Queued),
                    routing: None,
                    escalated_at: row.escalated_at,
                    closed_at: row.closed_at,
                },
            };
            Some(RoutingInfo {
                reason,
                matched_skills: row.matched_skill_names,
                assigned_membership_id: row.assigned_membership_id.unwrap_or_default(),
                assigned_at: row.assigned_at.unwrap_or(row.escalated_at),
            })
        }
        None => None,
    };

    Escalation {
        id: row.id,
        conversation_id: row.conversation_id,
        reason: row.reason,
        required_skills,
        status: serde_json::from_value(serde_json::Value::String(row.status))
            .unwrap_or(EscalationStatus::Queued),
        routing,
        escalated_at: row.escalated_at,
        closed_at: row.closed_at,
    }
}
