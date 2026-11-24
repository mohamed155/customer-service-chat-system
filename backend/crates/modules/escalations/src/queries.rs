use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::model::{
    Availability, AvailabilityState, Escalation, EscalationStatus, RequiredSkillRef, RoutingInfo,
    RoutingReason,
};

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
    pub capacity: i32,
    pub last_assigned_at: Option<DateTime<Utc>>,
}

/// A comparable key for the deterministic assignment order. Capacity is bounded to 200 and load is
/// a non-negative PostgreSQL row count, so i128 cross-products compare normalized load exactly
/// without floating point rounding.
#[derive(Debug, Clone)]
struct RankKey {
    match_count: i64,
    load_count: i64,
    capacity: i32,
    last_assigned_at: Option<DateTime<Utc>>,
    membership_id: Uuid,
}

impl RankKey {
    fn for_candidate(candidate: &Candidate) -> Self {
        Self {
            match_count: candidate.match_count,
            load_count: candidate.load_count,
            capacity: candidate.capacity,
            last_assigned_at: candidate.last_assigned_at,
            membership_id: candidate.membership_id,
        }
    }
}

impl PartialEq for RankKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for RankKey {}

impl PartialOrd for RankKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RankKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .match_count
            .cmp(&self.match_count)
            .then_with(|| {
                (i128::from(self.load_count) * i128::from(other.capacity))
                    .cmp(&(i128::from(other.load_count) * i128::from(self.capacity)))
            })
            .then_with(|| self.last_assigned_at.cmp(&other.last_assigned_at))
            .then_with(|| self.membership_id.cmp(&other.membership_id))
    }
}

/// Returns the best candidate after removing zero-capacity and full members. This is the single
/// ranking implementation used by the database selection path and the pure ranking tests.
pub fn rank_candidates(candidates: impl IntoIterator<Item = Candidate>) -> Option<Candidate> {
    candidates
        .into_iter()
        .filter(|candidate| {
            candidate.capacity > 0
                && candidate.load_count >= 0
                && candidate.load_count < i64::from(candidate.capacity)
        })
        .min_by(|left, right| RankKey::for_candidate(left).cmp(&RankKey::for_candidate(right)))
}

/// The best of `eligible` for an escalation: most required skills matched, then the lowest exact
/// open-load/capacity ratio, then longest idle, then the lowest ID. Capacity/availability/skills
/// belong to this module; load belongs to `conversations`; membership eligibility belongs to
/// `tenancy`.
async fn rank_candidate_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    required_skill_ids: &[Uuid],
    eligible: &[Uuid],
) -> sqlx::Result<Option<Candidate>> {
    let default_capacity = default_capacity_in_tx(tx, tenant_id).await?;
    let available: Vec<CandidateRow> = sqlx::query_as(
        "SELECT aa.membership_id, \
                COALESCE(m.match_count, 0) AS match_count, \
                COALESCE(m.matched_ids, '{}') AS matched_ids, \
                COALESCE(aa.capacity_override, $4) AS capacity, \
                aa.last_assigned_at AS last_assigned_at \
         FROM agent_availability aa \
         LEFT JOIN LATERAL ( \
             SELECT COUNT(*) AS match_count, array_agg(ask.skill_id) AS matched_ids \
             FROM agent_skills ask \
             WHERE ask.tenant_id = aa.tenant_id AND ask.membership_id = aa.membership_id \
               AND ask.skill_id = ANY($2) \
         ) m ON true \
         WHERE aa.tenant_id = $1 \
           AND aa.state = 'available' \
           AND aa.membership_id = ANY($3) \
           AND COALESCE(aa.capacity_override, $4) > 0",
    )
    .bind(tenant_id)
    .bind(required_skill_ids)
    .bind(eligible)
    .bind(default_capacity)
    .fetch_all(&mut **tx)
    .await?;
    let ids: Vec<Uuid> = available
        .iter()
        .map(|candidate| candidate.membership_id)
        .collect();
    let loads = conversations::directory::open_load_by_member(tx, tenant_id, &ids).await?;
    Ok(rank_candidates(available.into_iter().map(|candidate| {
        Candidate {
            membership_id: candidate.membership_id,
            match_count: candidate.match_count,
            matched_ids: candidate.matched_ids,
            load_count: loads.get(&candidate.membership_id).copied().unwrap_or(0),
            capacity: candidate.capacity,
            last_assigned_at: candidate.last_assigned_at,
        }
    })))
}

/// The roles whose members can take an escalation.
pub const ASSIGNABLE_ROLES: [&str; 4] = ["owner", "admin", "manager", "agent"];

#[derive(Debug, sqlx::FromRow)]
struct CandidateRow {
    membership_id: Uuid,
    match_count: i64,
    matched_ids: Vec<Uuid>,
    capacity: i32,
    last_assigned_at: Option<DateTime<Utc>>,
}

pub async fn select_candidate_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    required_skill_ids: &[Uuid],
    present_ids: &[Uuid],
) -> sqlx::Result<Option<Candidate>> {
    // Who may take an escalation (an active member with a role that handles conversations, among
    // those present) is `tenancy`'s to say. This statement then ranks those members by skill match
    // and current load from this module's own tables.
    let mut eligible = tenancy::directory::active_member_ids(
        &mut **tx,
        tenant_id,
        tenancy::directory::ActiveMembers {
            roles: Some(&ASSIGNABLE_ROLES),
            among: Some(present_ids),
            ..Default::default()
        },
    )
    .await?;
    // Eligibility, availability and load are read in separate statements, so a member could be
    // disabled, demoted or set to away between them and the assignment. Share-lock the one we pick
    // (their membership, and their availability row): a roster change or a presence change takes
    // the same row `FOR UPDATE`, so it waits until this transaction has written the assignment.
    // If the pick no longer qualifies, drop it and rank the rest.
    loop {
        if eligible.is_empty() {
            return Ok(None);
        }
        let candidate = rank_candidate_in_tx(tx, tenant_id, required_skill_ids, &eligible).await?;
        let Some(mut candidate) = candidate else {
            return Ok(None);
        };
        if tenancy::directory::lock_active_member(
            &mut **tx,
            tenant_id,
            candidate.membership_id,
            &ASSIGNABLE_ROLES,
        )
        .await?
            && lock_available_in_tx(tx, tenant_id, candidate.membership_id).await?
        {
            // The advisory routing lock serializes automatic assignments. Re-read the selected
            // member's load after its row locks as a final guard against load changed by another
            // conversation operation since the ranked snapshot.
            let loads = conversations::directory::open_load_by_member(
                tx,
                tenant_id,
                &[candidate.membership_id],
            )
            .await?;
            candidate.load_count = loads.get(&candidate.membership_id).copied().unwrap_or(0);
            if candidate.load_count < i64::from(candidate.capacity) {
                return Ok(Some(candidate));
            }
        }
        eligible.retain(|id| *id != candidate.membership_id);
    }
}

/// Takes a share lock on the member's availability row when they are still `available`, and says
/// whether they are. A presence change updates the same row, so once this returns `true` the
/// member cannot go away until the caller's transaction ends.
pub async fn lock_available_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    membership_id: Uuid,
) -> sqlx::Result<bool> {
    let locked: Option<Uuid> = sqlx::query_scalar(
        "SELECT membership_id FROM agent_availability \
         WHERE tenant_id = $1 AND membership_id = $2 AND state = 'available' \
         FOR SHARE",
    )
    .bind(tenant_id)
    .bind(membership_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(locked.is_some())
}

pub async fn take_tenant_routing_lock_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtext('escalations.routing'), hashtext($1::text))",
    )
    .bind(tenant_id.to_string())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Availability queries
// ---------------------------------------------------------------------------

pub const DEFAULT_ROUTING_CAPACITY: i32 = 10;

/// Read the tenant setting, using the documented default until the tenant saves a value.
pub async fn default_capacity_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
) -> sqlx::Result<i32> {
    sqlx::query_scalar(
        "SELECT COALESCE( \
            (SELECT default_capacity FROM routing_settings WHERE tenant_id = $1), \
            $2 \
         )",
    )
    .bind(tenant_id)
    .bind(DEFAULT_ROUTING_CAPACITY)
    .fetch_one(&mut **tx)
    .await
}

pub async fn set_default_capacity_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    capacity: i32,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO routing_settings (tenant_id, default_capacity) VALUES ($1, $2) \
         ON CONFLICT (tenant_id) DO UPDATE SET default_capacity = EXCLUDED.default_capacity, \
         updated_at = now()",
    )
    .bind(tenant_id)
    .bind(capacity)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn capacity_override_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    membership_id: Uuid,
) -> sqlx::Result<Option<i32>> {
    sqlx::query_scalar(
        "SELECT capacity_override FROM agent_availability \
         WHERE tenant_id = $1 AND membership_id = $2",
    )
    .bind(tenant_id)
    .bind(membership_id)
    .fetch_optional(&mut **tx)
    .await
    .map(Option::flatten)
}

pub async fn capacity_overrides_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    membership_ids: &[Uuid],
) -> sqlx::Result<std::collections::HashMap<Uuid, Option<i32>>> {
    let rows: Vec<(Uuid, Option<i32>)> = sqlx::query_as(
        "SELECT membership_id, capacity_override FROM agent_availability \
         WHERE tenant_id = $1 AND membership_id = ANY($2)",
    )
    .bind(tenant_id)
    .bind(membership_ids)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows.into_iter().collect())
}

pub async fn set_capacity_override_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    membership_id: Uuid,
    capacity: Option<i32>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO agent_availability (tenant_id, membership_id, capacity_override) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (tenant_id, membership_id) \
         DO UPDATE SET capacity_override = EXCLUDED.capacity_override",
    )
    .bind(tenant_id)
    .bind(membership_id)
    .bind(capacity)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Record the assignment clock without changing the member's current availability.
pub async fn record_last_assignment_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    membership_id: Uuid,
) -> sqlx::Result<()> {
    // `clock_timestamp()` advances inside a batch queue drain; PostgreSQL `now()` is fixed at the
    // transaction start and would make every assignment in that batch look equally recent.
    sqlx::query(
        "INSERT INTO agent_availability (tenant_id, membership_id, last_assigned_at) \
         VALUES ($1, $2, clock_timestamp()) \
         ON CONFLICT (tenant_id, membership_id) \
         DO UPDATE SET last_assigned_at = clock_timestamp()",
    )
    .bind(tenant_id)
    .bind(membership_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

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
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM skills WHERE tenant_id = $1 AND id = ANY($2)")
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
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO skills (tenant_id, name) VALUES ($1, $2) RETURNING id")
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
    let role: Option<String> =
        tenancy::directory::membership_role(&mut **tx, tenant_id, membership_id).await?;
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
        .map(|(id, name)| RequiredSkillRef { id: Some(id), name })
        .collect();

    let routing = match row.routing_reason {
        Some(reason_str) => {
            let reason = match reason_str.as_str() {
                "skill_match" => RoutingReason::SkillMatch,
                "load_fallback" => RoutingReason::LoadFallback,
                "manual_claim" => RoutingReason::ManualClaim,
                "queue_auto" => RoutingReason::QueueAuto,
                "manual_reassignment" => RoutingReason::ManualReassignment,
                _ => {
                    return Escalation {
                        id: row.id,
                        conversation_id: row.conversation_id,
                        reason: row.reason,
                        required_skills,
                        status: serde_json::from_value(serde_json::Value::String(row.status))
                            .unwrap_or(EscalationStatus::Queued),
                        routing: None,
                        escalated_at: row.escalated_at,
                        closed_at: row.closed_at,
                    }
                }
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

#[cfg(test)]
mod routing_rank_tests {
    use super::{rank_candidates, Candidate};
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    fn candidate(
        id: u128,
        match_count: i64,
        load_count: i64,
        capacity: i32,
        last_assigned_seconds: Option<i64>,
    ) -> Candidate {
        Candidate {
            membership_id: Uuid::from_u128(id),
            match_count,
            matched_ids: Vec::new(),
            load_count,
            capacity,
            last_assigned_at: last_assigned_seconds
                .map(|seconds| Utc.timestamp_opt(seconds, 0).single().unwrap()),
        }
    }

    #[test]
    fn rank_never_returns_a_member_at_or_over_capacity() {
        let mut seed = 0x2f8a_4c6d_91b3_5071_u64;
        for _ in 0..10_000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let capacity = (seed % 201) as i32;
            let load = (seed.rotate_left(19) % 205) as i64;
            let ranked = rank_candidates([candidate(1, 0, load, capacity, None)]);
            assert_eq!(
                ranked.is_some(),
                capacity > 0 && load < i64::from(capacity),
                "capacity={capacity}, load={load}"
            );
            if let Some(candidate) = ranked {
                assert!(candidate.load_count < i64::from(candidate.capacity));
            }
        }
    }

    #[test]
    fn skill_matches_precede_load_and_normalized_load_precedes_raw_load() {
        let skilled_heavy = candidate(1, 1, 19, 20, None);
        let unskilled_light = candidate(2, 0, 0, 1, None);
        assert_eq!(
            rank_candidates([skilled_heavy, unskilled_light]).map(|c| c.membership_id),
            Some(Uuid::from_u128(1))
        );

        let higher_ratio = candidate(3, 0, 1, 2, None);
        let lower_ratio_with_more_open = candidate(4, 0, 5, 20, None);
        assert_eq!(
            rank_candidates([higher_ratio, lower_ratio_with_more_open]).map(|c| c.membership_id),
            Some(Uuid::from_u128(4))
        );
    }

    #[test]
    fn equal_match_and_normalized_load_rotate_before_repeating() {
        let mut candidates = [
            candidate(1, 1, 1, 4, Some(100)),
            candidate(2, 1, 2, 8, Some(100)),
            candidate(3, 1, 1, 4, Some(100)),
            candidate(4, 1, 2, 8, Some(100)),
        ];
        let mut order = Vec::new();
        for seconds in 0..4 {
            let selected = rank_candidates(candidates.clone()).expect("capacity remains");
            order.push(selected.membership_id);
            candidates
                .iter_mut()
                .find(|candidate| candidate.membership_id == selected.membership_id)
                .expect("selected candidate exists")
                .last_assigned_at = Some(Utc.timestamp_opt(1_000 + seconds, 0).single().unwrap());
        }
        assert_eq!(order, (1..=4).map(Uuid::from_u128).collect::<Vec<_>>());
    }

    #[test]
    fn missing_last_assignment_is_oldest_and_final_ties_use_membership_id() {
        let candidates = [
            candidate(9, 2, 2, 10, Some(100)),
            candidate(7, 2, 2, 10, Some(100)),
            candidate(4, 2, 2, 10, None),
        ];
        assert_eq!(
            rank_candidates(candidates).map(|c| c.membership_id),
            Some(Uuid::from_u128(4))
        );
    }
}
