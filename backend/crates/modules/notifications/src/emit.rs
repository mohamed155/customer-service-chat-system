use crate::model::{NotificationKind, SubjectType};
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub struct NotificationRequest {
    pub tenant_id: Uuid,
    pub kind: NotificationKind,
    pub subject_type: SubjectType,
    pub subject_id: Uuid,
    pub actor_membership_id: Option<Uuid>,
    pub target_membership_id: Option<Uuid>,
    pub dedupe_key: String,
    pub title: String,
    pub body: Option<String>,
}

pub async fn emit_requested_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    req: &NotificationRequest,
) -> sqlx::Result<()> {
    let payload = json!({
        "tenantId": req.tenant_id,
        "kind": req.kind.as_str(),
        "subjectType": req.subject_type.as_str(),
        "subjectId": req.subject_id,
        "actorMembershipId": req.actor_membership_id,
        "targetMembershipId": req.target_membership_id,
        "dedupeKey": req.dedupe_key,
        "title": req.title,
        "body": req.body,
    });

    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, 'notification', $2, $3, 'notification.requested', $4, now())",
    )
    .bind(Uuid::new_v4())
    .bind(req.subject_id)
    .bind(req.tenant_id)
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn emit_requested_on_pool(pool: &PgPool, req: &NotificationRequest) {
    let payload = json!({
        "tenantId": req.tenant_id,
        "kind": req.kind.as_str(),
        "subjectType": req.subject_type.as_str(),
        "subjectId": req.subject_id,
        "actorMembershipId": req.actor_membership_id,
        "targetMembershipId": req.target_membership_id,
        "dedupeKey": req.dedupe_key,
        "title": req.title,
        "body": req.body,
    });

    let result = sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, 'notification', $2, $3, 'notification.requested', $4, now())",
    )
    .bind(Uuid::new_v4())
    .bind(req.subject_id)
    .bind(req.tenant_id)
    .bind(payload)
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::error!(error = %e, "failed to emit notification.requested");
    }
}

pub async fn emit_resolved_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    subject_type: &SubjectType,
    subject_id: Uuid,
    resolved_by: Option<Uuid>,
) -> sqlx::Result<()> {
    let payload = json!({
        "tenantId": tenant_id,
        "subjectType": subject_type.as_str(),
        "subjectId": subject_id,
        "resolvedByMembershipId": resolved_by,
    });

    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, 'notification', $2, $3, 'notification.resolved', $4, now())",
    )
    .bind(Uuid::new_v4())
    .bind(subject_id)
    .bind(tenant_id)
    .bind(payload)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn emit_resolved_on_pool(
    pool: &PgPool,
    tenant_id: Uuid,
    subject_type: &SubjectType,
    subject_id: Uuid,
    resolved_by: Option<Uuid>,
) {
    let payload = json!({
        "tenantId": tenant_id,
        "subjectType": subject_type.as_str(),
        "subjectId": subject_id,
        "resolvedByMembershipId": resolved_by,
    });

    let result = sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, 'notification', $2, $3, 'notification.resolved', $4, now())",
    )
    .bind(Uuid::new_v4())
    .bind(subject_id)
    .bind(tenant_id)
    .bind(payload)
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::error!(error = %e, "failed to emit notification.resolved");
    }
}

// ── Dedupe key builders ──────────────────────────────────────────────

pub fn dedupe_key_escalation(escalation_id: Uuid) -> String {
    format!("escalation:{escalation_id}")
}

pub fn dedupe_key_assigned(conversation_id: Uuid, assigned_membership_id: Uuid) -> String {
    format!("assigned:{conversation_id}:{assigned_membership_id}")
}

pub fn dedupe_key_tool_approval(tool_request_id: Uuid) -> String {
    format!("tool_approval:{tool_request_id}")
}

pub fn dedupe_key_ai_failed(
    conversation_id: Uuid,
    timestamp: &chrono::DateTime<chrono::Utc>,
) -> String {
    let bucket = timestamp.timestamp() / 900;
    format!("ai_failed:{conversation_id}:{bucket}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn ai_failed_same_bucket_for_five_minute_separation() {
        let conv_id = Uuid::new_v4();
        let t1 = chrono::DateTime::parse_from_rfc3339("2026-07-20T14:03:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let t2 = chrono::DateTime::parse_from_rfc3339("2026-07-20T14:08:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        assert_eq!(dedupe_key_ai_failed(conv_id, &t1), dedupe_key_ai_failed(conv_id, &t2));
    }

    #[test]
    fn ai_failed_different_bucket_for_twenty_minute_separation() {
        let conv_id = Uuid::new_v4();
        let t1 = chrono::DateTime::parse_from_rfc3339("2026-07-20T14:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let t2 = chrono::DateTime::parse_from_rfc3339("2026-07-20T14:20:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        assert_ne!(dedupe_key_ai_failed(conv_id, &t1), dedupe_key_ai_failed(conv_id, &t2));
    }

    #[test]
    fn escalation_key_format() {
        let id = Uuid::parse_str("9d2c4a1e-1234-5678-9abc-def012345678").unwrap();
        let key = dedupe_key_escalation(id);
        assert_eq!(key, format!("escalation:{id}"));
    }

    #[test]
    fn assigned_key_format() {
        let conv_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let mem_id = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let key = dedupe_key_assigned(conv_id, mem_id);
        assert_eq!(key, format!("assigned:{conv_id}:{mem_id}"));
    }

    #[test]
    fn tool_approval_key_format() {
        let id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        let key = dedupe_key_tool_approval(id);
        assert_eq!(key, format!("tool_approval:{id}"));
    }
}
