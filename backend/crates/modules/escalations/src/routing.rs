use chrono::Utc;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::audit;
use crate::model::{Escalation, EscalationStatus, RequiredSkillRef, RoutingInfo, RoutingReason};
use crate::queries;

#[derive(Debug)]
pub enum RouteOutcome {
    Assigned {
        escalation: Escalation,
        assigned_membership_id: Uuid,
        matched_skill_names: Vec<String>,
    },
    Queued {
        escalation: Escalation,
    },
}

#[derive(Debug)]
pub enum RouteError {
    Duplicate,
    ConversationNotFound,
    InvalidState,
    Internal(String),
}

impl std::fmt::Display for RouteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouteError::Duplicate => write!(f, "duplicate escalation"),
            RouteError::ConversationNotFound => write!(f, "conversation not found"),
            RouteError::InvalidState => write!(f, "invalid conversation state"),
            RouteError::Internal(msg) => write!(f, "internal error: {}", msg),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn route_new_escalation_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    _pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
    reason: &str,
    required_skill_ids: &[Uuid],
    required_skill_names: &[String],
    present_ids: &[Uuid],
    actor_user_id: Uuid,
) -> Result<RouteOutcome, RouteError> {
    queries::take_tenant_routing_lock_in_tx(tx, tenant_id)
        .await
        .map_err(|e| RouteError::Internal(e.to_string()))?;

    let escalation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO escalations (tenant_id, conversation_id, reason, \
         required_skill_ids, required_skill_names, status) \
         VALUES ($1, $2, $3, $4, $5, 'queued') \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(reason)
    .bind(required_skill_ids)
    .bind(required_skill_names)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref dbe) = e {
            if dbe.constraint() == Some("escalations_one_active_uniq") {
                return RouteError::Duplicate;
            }
        }
        RouteError::Internal(e.to_string())
    })?;

    audit::record_escalation_created(
        tx,
        actor_user_id,
        tenant_id,
        escalation_id,
        conversation_id,
        reason,
    )
    .await
    .map_err(|e| RouteError::Internal(e.to_string()))?;

    let candidate = queries::select_candidate_in_tx(tx, tenant_id, required_skill_ids, present_ids)
        .await
        .map_err(|e| RouteError::Internal(e.to_string()))?;

    if let Some(cand) = candidate {
        let matched_names = if cand.match_count > 0 && !cand.matched_ids.is_empty() {
            crate::model::sql::skill_names_for_ids_in_tx(tx, tenant_id, &cand.matched_ids)
                .await
                .map_err(|e| RouteError::Internal(e.to_string()))?
        } else {
            Vec::new()
        };

        sqlx::query(
            "UPDATE escalations SET status = 'assigned', routing_reason = $1, \
             assigned_membership_id = $2, matched_skill_ids = $3, matched_skill_names = $4, \
             assigned_at = now() \
             WHERE id = $5",
        )
        .bind(if cand.match_count > 0 {
            "skill_match"
        } else {
            "load_fallback"
        })
        .bind(cand.membership_id)
        .bind(&cand.matched_ids)
        .bind(&matched_names)
        .bind(escalation_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| RouteError::Internal(e.to_string()))?;

        conversations::queries::assign_in_tx(
            tx,
            tenant_id,
            conversation_id,
            Some(cand.membership_id),
            Some(actor_user_id),
            "escalations",
        )
        .await
        .map_err(|e| RouteError::Internal(e.to_string()))?;

        conversations::queries::set_escalated_in_tx(
            tx,
            tenant_id,
            conversation_id,
            Some(Utc::now()),
        )
        .await
        .map_err(|e| RouteError::Internal(e.to_string()))?;

        audit::record_escalation_assigned(
            tx,
            actor_user_id,
            tenant_id,
            escalation_id,
            if cand.match_count > 0 {
                "skill_match"
            } else {
                "load_fallback"
            },
            &matched_names,
            cand.load_count,
            cand.membership_id,
        )
        .await
        .map_err(|e| RouteError::Internal(e.to_string()))?;

        let escalation = build_escalation(
            tx,
            escalation_id,
            tenant_id,
            conversation_id,
            reason,
            required_skill_ids,
            required_skill_names,
            Some(if cand.match_count > 0 {
                "skill_match"
            } else {
                "load_fallback"
            }),
            &matched_names,
            Some(cand.membership_id),
        )
        .await?;

        let actor_mid = resolve_actor_membership_id_in_tx(tx, tenant_id, actor_user_id).await;
        // If the actor is also the assignee, suppress the actor for the assignment
        // notification so the assignee is included as the recipient.
        let notify_actor = actor_mid.filter(|m| *m != cand.membership_id);
        notify_escalation_assigned(
            tx,
            tenant_id,
            escalation_id,
            conversation_id,
            cand.membership_id,
            notify_actor,
        )
        .await;

        Ok(RouteOutcome::Assigned {
            escalation,
            assigned_membership_id: cand.membership_id,
            matched_skill_names: matched_names,
        })
    } else {
        conversations::queries::set_escalated_in_tx(
            tx,
            tenant_id,
            conversation_id,
            Some(Utc::now()),
        )
        .await
        .map_err(|e| RouteError::Internal(e.to_string()))?;

        audit::record_escalation_queued(tx, actor_user_id, tenant_id, escalation_id)
            .await
            .map_err(|e| RouteError::Internal(e.to_string()))?;

        let escalation = build_escalation(
            tx,
            escalation_id,
            tenant_id,
            conversation_id,
            reason,
            required_skill_ids,
            required_skill_names,
            None,
            &[],
            None,
        )
        .await?;

        let actor_mid = resolve_actor_membership_id_in_tx(tx, tenant_id, actor_user_id).await;
        notify_escalation_queued(
            tx,
            tenant_id,
            escalation_id,
            conversation_id,
            actor_mid,
        )
        .await;

        Ok(RouteOutcome::Queued { escalation })
    }
}

#[derive(Debug)]
pub enum ClaimError {
    NotFound,
    AlreadyClaimed { assigned_membership_id: Uuid },
    Internal(String),
}

pub async fn claim_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    escalation_id: Uuid,
    claimant_membership_id: Uuid,
    actor_user_id: Uuid,
) -> Result<crate::model::Escalation, ClaimError> {
    let updated = sqlx::query(
        "UPDATE escalations SET status = 'assigned', routing_reason = 'manual_claim', \
         assigned_membership_id = $1, assigned_at = now() \
         WHERE id = $2 AND tenant_id = $3 AND status = 'queued'",
    )
    .bind(claimant_membership_id)
    .bind(escalation_id)
    .bind(tenant_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| ClaimError::Internal(e.to_string()))?;

    if updated.rows_affected() == 0 {
        let row = queries::escalation_row_in_tx(tx, tenant_id, escalation_id)
            .await
            .map_err(|e| ClaimError::Internal(e.to_string()))?;
        match row {
            None => Err(ClaimError::NotFound),
            Some(r) => match r.assigned_membership_id {
                Some(mid) => Err(ClaimError::AlreadyClaimed {
                    assigned_membership_id: mid,
                }),
                None => Err(ClaimError::NotFound),
            },
        }
    } else {
        let row = queries::escalation_row_in_tx(tx, tenant_id, escalation_id)
            .await
            .map_err(|e| ClaimError::Internal(e.to_string()))?
            .ok_or_else(|| ClaimError::Internal("just-updated escalation not found".into()))?;

        conversations::queries::assign_in_tx(
            tx,
            tenant_id,
            row.conversation_id,
            Some(claimant_membership_id),
            Some(actor_user_id),
            "escalations",
        )
        .await
        .map_err(|e| ClaimError::Internal(e.to_string()))?;

        notify_escalation_assigned(
            tx,
            tenant_id,
            escalation_id,
            row.conversation_id,
            claimant_membership_id,
            Some(claimant_membership_id),
        )
        .await;

        audit::record_escalation_claimed(tx, actor_user_id, tenant_id, escalation_id)
            .await
            .map_err(|e| ClaimError::Internal(e.to_string()))?;

        let escalation = queries::row_to_escalation(row);
        Ok(escalation)
    }
}

pub async fn drain_one_for_membership_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    membership_id: Uuid,
    _present_ids: &[Uuid],
    actor_user_id: Uuid,
) -> sqlx::Result<Option<Uuid>> {
    queries::take_tenant_routing_lock_in_tx(tx, tenant_id).await?;

    // First try: skill-matching entry
    let candidate: Option<(Uuid, Vec<Uuid>, Vec<String>)> = sqlx::query_as(
        "SELECT e.id, e.required_skill_ids, e.required_skill_names \
         FROM escalations e \
         WHERE e.tenant_id = $1 AND e.status = 'queued' \
         AND EXISTS(SELECT 1 FROM agent_skills ask \
                    WHERE ask.membership_id = $2 AND ask.skill_id = ANY(e.required_skill_ids)) \
         ORDER BY e.escalated_at ASC \
         LIMIT 1",
    )
    .bind(tenant_id)
    .bind(membership_id)
    .fetch_optional(&mut **tx)
    .await?;

    // Fallback: oldest entry outright when no skill match
    let candidate = if candidate.is_some() {
        candidate
    } else {
        sqlx::query_as(
            "SELECT e.id, e.required_skill_ids, e.required_skill_names \
             FROM escalations e \
             WHERE e.tenant_id = $1 AND e.status = 'queued' \
             ORDER BY e.escalated_at ASC \
             LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(&mut **tx)
        .await?
    };

    if let Some((escalation_id, req_skill_ids, _req_skill_names)) = candidate {
        let matched_names = if req_skill_ids.is_empty() {
            Vec::new()
        } else {
            crate::model::sql::skill_names_for_ids_in_tx(tx, tenant_id, &req_skill_ids).await?
        };

        sqlx::query(
            "UPDATE escalations SET status = 'assigned', routing_reason = 'queue_auto', \
             assigned_membership_id = $1, matched_skill_ids = $2, matched_skill_names = $3, \
             assigned_at = now() \
             WHERE id = $4",
        )
        .bind(membership_id)
        .bind(&req_skill_ids)
        .bind(&matched_names)
        .bind(escalation_id)
        .execute(&mut **tx)
        .await?;

        let conv_id: Uuid =
            sqlx::query_scalar("SELECT conversation_id FROM escalations WHERE id = $1")
                .bind(escalation_id)
                .fetch_one(&mut **tx)
                .await?;

        conversations::queries::assign_in_tx(
            tx,
            tenant_id,
            conv_id,
            Some(membership_id),
            Some(actor_user_id),
            "escalations",
        )
        .await?;

        conversations::queries::set_escalated_in_tx(tx, tenant_id, conv_id, Some(Utc::now()))
            .await?;

        audit::record_escalation_assigned(
            tx,
            actor_user_id,
            tenant_id,
            escalation_id,
            "queue_auto",
            &matched_names,
            0,
            membership_id,
        )
        .await?;

        let actor_mid = resolve_actor_membership_id_in_tx(tx, tenant_id, actor_user_id).await;
        notify_escalation_assigned(
            tx,
            tenant_id,
            escalation_id,
            conv_id,
            membership_id,
            actor_mid,
        )
        .await;

        return Ok(Some(escalation_id));
    }

    Ok(None)
}

pub async fn drain_any_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    present_ids: &[Uuid],
    actor_user_id: Uuid,
) -> sqlx::Result<Option<Uuid>> {
    queries::take_tenant_routing_lock_in_tx(tx, tenant_id).await?;

    let oldest: Option<(Uuid, Vec<Uuid>)> = sqlx::query_as(
        "SELECT id, required_skill_ids FROM escalations \
         WHERE tenant_id = $1 AND status = 'queued' \
         ORDER BY escalated_at ASC \
         LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&mut **tx)
    .await?;

    if let Some((escalation_id, req_ids)) = oldest {
        let candidate =
            queries::select_candidate_in_tx(tx, tenant_id, &req_ids, present_ids).await?;
        if let Some(cand) = candidate {
            let matched_names = if cand.match_count > 0 && !cand.matched_ids.is_empty() {
                crate::model::sql::skill_names_for_ids_in_tx(tx, tenant_id, &cand.matched_ids)
                    .await?
            } else {
                Vec::new()
            };

            sqlx::query(
                "UPDATE escalations SET status = 'assigned', routing_reason = 'queue_auto', \
                 assigned_membership_id = $1, matched_skill_ids = $2, matched_skill_names = $3, \
                 assigned_at = now() \
                 WHERE id = $4",
            )
            .bind(cand.membership_id)
            .bind(&cand.matched_ids)
            .bind(&matched_names)
            .bind(escalation_id)
            .execute(&mut **tx)
            .await?;

            let conv_id: Uuid =
                sqlx::query_scalar("SELECT conversation_id FROM escalations WHERE id = $1")
                    .bind(escalation_id)
                    .fetch_one(&mut **tx)
                    .await?;

            conversations::queries::assign_in_tx(
                tx,
                tenant_id,
                conv_id,
                Some(cand.membership_id),
                Some(actor_user_id),
                "escalations",
            )
            .await?;

            audit::record_escalation_assigned(
                tx,
                actor_user_id,
                tenant_id,
                escalation_id,
                "queue_auto",
                &matched_names,
                cand.load_count,
                cand.membership_id,
            )
            .await?;

            let actor_mid = resolve_actor_membership_id_in_tx(tx, tenant_id, actor_user_id).await;
            notify_escalation_assigned(
                tx,
                tenant_id,
                escalation_id,
                conv_id,
                cand.membership_id,
                actor_mid,
            )
            .await;

            return Ok(Some(escalation_id));
        }
    }

    Ok(None)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn build_escalation(
    _tx: &mut Transaction<'_, Postgres>,
    escalation_id: Uuid,
    _tenant_id: Uuid,
    conversation_id: Uuid,
    reason: &str,
    required_skill_ids: &[Uuid],
    required_skill_names: &[String],
    routing_reason: Option<&str>,
    matched_skill_names: &[String],
    assigned_membership_id: Option<Uuid>,
) -> Result<Escalation, RouteError> {
    let required_skills: Vec<RequiredSkillRef> = required_skill_ids
        .iter()
        .cloned()
        .zip(
            required_skill_names
                .iter()
                .cloned()
                .chain(std::iter::repeat(String::new())),
        )
        .map(|(id, name)| RequiredSkillRef { id: Some(id), name })
        .collect();

    let routing = routing_reason.map(|rr| RoutingInfo {
        reason: match rr {
            "skill_match" => RoutingReason::SkillMatch,
            "load_fallback" => RoutingReason::LoadFallback,
            "manual_claim" => RoutingReason::ManualClaim,
            "queue_auto" => RoutingReason::QueueAuto,
            "manual_reassignment" => RoutingReason::ManualReassignment,
            _ => RoutingReason::LoadFallback,
        },
        matched_skills: matched_skill_names.to_vec(),
        assigned_membership_id: assigned_membership_id.unwrap_or_default(),
        assigned_at: Utc::now(),
    });

    Ok(Escalation {
        id: escalation_id,
        conversation_id,
        reason: reason.to_owned(),
        required_skills,
        status: if routing_reason.is_some() {
            EscalationStatus::Assigned
        } else {
            EscalationStatus::Queued
        },
        routing,
        escalated_at: Utc::now(),
        closed_at: None,
    })
}

async fn resolve_actor_membership_id_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    actor_user_id: Uuid,
) -> Option<Uuid> {
    sqlx::query_scalar(
        "SELECT id FROM tenant_memberships \
         WHERE tenant_id = $1 AND user_id = $2 AND status = 'active' AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(actor_user_id)
    .fetch_optional(&mut **tx)
    .await
    .unwrap_or(None)
}

async fn notify_escalation_queued(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    escalation_id: Uuid,
    conversation_id: Uuid,
    actor_membership_id: Option<Uuid>,
) {
    let dedupe_key = format!("escalation:{escalation_id}");
    let payload = serde_json::json!({
        "tenantId": tenant_id,
        "kind": "escalation.new",
        "subjectType": "escalation",
        "subjectId": escalation_id,
        "actorMembershipId": actor_membership_id,
        "targetMembershipId": null,
        "dedupeKey": dedupe_key,
        "title": "Escalation queued",
        "body": format!("Conversation {conversation_id} has been escalated"),
    });
    if let Err(e) = sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, 'notification', $2, $3, 'notification.requested', $4, now())",
    )
    .bind(Uuid::new_v4())
    .bind(escalation_id)
    .bind(tenant_id)
    .bind(payload)
    .execute(&mut **tx)
    .await
    {
        tracing::error!(error = %e, %escalation_id, "failed to emit notification for queued escalation");
    }
}

async fn notify_escalation_assigned(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    escalation_id: Uuid,
    conversation_id: Uuid,
    assignee_membership_id: Uuid,
    actor_membership_id: Option<Uuid>,
) {
    let dedupe_key = format!("escalation:{escalation_id}");
    let req_payload = serde_json::json!({
        "tenantId": tenant_id,
        "kind": "escalation.new",
        "subjectType": "escalation",
        "subjectId": escalation_id,
        "actorMembershipId": actor_membership_id,
        "targetMembershipId": assignee_membership_id,
        "dedupeKey": dedupe_key,
        "title": "Escalation assigned",
        "body": format!("Conversation {conversation_id} has been escalated and assigned"),
    });
    if let Err(e) = sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, 'notification', $2, $3, 'notification.requested', $4, now())",
    )
    .bind(Uuid::new_v4())
    .bind(escalation_id)
    .bind(tenant_id)
    .bind(req_payload)
    .execute(&mut **tx)
    .await
    {
        tracing::error!(error = %e, %escalation_id, "failed to emit requested notification for assigned escalation");
    }
    let resolve_payload = serde_json::json!({
        "tenantId": tenant_id,
        "subjectType": "escalation",
        "subjectId": escalation_id,
        "resolvedByMembershipId": assignee_membership_id,
    });
    if let Err(e) = sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, 'notification', $2, $3, 'notification.resolved', $4, now())",
    )
    .bind(Uuid::new_v4())
    .bind(escalation_id)
    .bind(tenant_id)
    .bind(resolve_payload)
    .execute(&mut **tx)
    .await
    {
        tracing::error!(error = %e, %escalation_id, "failed to emit resolved notification for assigned escalation");
    }
}

pub async fn has_open_escalation(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS( \
         SELECT 1 FROM escalations \
         WHERE tenant_id = $1 AND conversation_id = $2 AND status IN ('queued', 'assigned') \
         )",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_one(pool)
    .await
}
