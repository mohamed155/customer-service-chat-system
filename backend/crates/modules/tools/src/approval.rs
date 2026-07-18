use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRequestRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub conversation_id: Uuid,
    pub generation_id: Uuid,
    pub tool_name: String,
    pub tool_source: String,
    pub tenant_tool_id: Option<Uuid>,
    pub arguments: serde_json::Value,
    pub status: String,
    pub approval_required: bool,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub chain_index: i16,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub decided_by_membership_id: Option<Uuid>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone)]
pub enum DecideOutcome {
    Applied(ToolRequestRow),
    AlreadySettled(ToolRequestRow),
}

pub async fn decide(
    pool: &PgPool,
    tenant_id: Uuid,
    tool_request_id: Uuid,
    decided_by: Uuid,
    approve: bool,
) -> sqlx::Result<DecideOutcome> {
    let new_status = if approve { "approved" } else { "denied" };

    let updated = sqlx::query(
        "UPDATE tool_requests \
         SET status = $1, decided_by_membership_id = $2, decided_at = now() \
         WHERE id = $3 AND tenant_id = $4 AND status = 'awaiting_approval'",
    )
    .bind(new_status)
    .bind(decided_by)
    .bind(tool_request_id)
    .bind(tenant_id)
    .execute(pool)
    .await?;

    if updated.rows_affected() == 0 {
        let row: ToolRequestRow = sqlx::query_as(
            "SELECT id, tenant_id, conversation_id, generation_id, tool_name, \
             tool_source, tenant_tool_id, arguments, status, approval_required, \
             expires_at, chain_index, started_at, finished_at, result, error, \
             created_at, decided_by_membership_id, decided_at \
             FROM tool_requests \
             WHERE id = $1 AND tenant_id = $2",
        )
        .bind(tool_request_id)
        .bind(tenant_id)
        .fetch_one(pool)
        .await?;
        return Ok(DecideOutcome::AlreadySettled(row));
    }

    let outcome = if approve { "approved" } else { "denied" };

    let mut tx = pool.begin().await?;

    let row: ToolRequestRow = sqlx::query_as(
        "SELECT id, tenant_id, conversation_id, generation_id, tool_name, \
         tool_source, tenant_tool_id, arguments, status, approval_required, \
         expires_at, chain_index, started_at, finished_at, result, error, \
         created_at, decided_by_membership_id, decided_at \
         FROM tool_requests \
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(tool_request_id)
    .bind(tenant_id)
    .fetch_one(&mut *tx)
    .await?;

    conversations::outbox::emit_tool_decision_in_tx(
        &mut tx,
        tenant_id,
        row.conversation_id,
        tool_request_id,
        outcome,
    )
    .await?;

    tx.commit().await?;

    Ok(DecideOutcome::Applied(row))
}

pub async fn sweep_expired(pool: &PgPool) -> sqlx::Result<u64> {
    let rows: Vec<ToolRequestRow> = sqlx::query_as(
        "SELECT id, tenant_id, conversation_id, generation_id, tool_name, \
         tool_source, tenant_tool_id, arguments, status, approval_required, \
         expires_at, chain_index, started_at, finished_at, result, error, \
         created_at, decided_by_membership_id, decided_at \
         FROM tool_requests \
         WHERE status = 'awaiting_approval' AND expires_at < now()",
    )
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(0);
    }

    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();

    let mut tx = pool.begin().await?;

    let updated = sqlx::query(
        "UPDATE tool_requests \
         SET status = 'expired', decided_at = now() \
         WHERE id = ANY($1) AND status = 'awaiting_approval'",
    )
    .bind(&ids)
    .execute(&mut *tx)
    .await?;

    for row in &rows {
        let outcome = "expired";
        conversations::outbox::emit_tool_decision_in_tx(
            &mut tx,
            row.tenant_id,
            row.conversation_id,
            row.id,
            outcome,
        )
        .await?;
    }

    tx.commit().await?;

    Ok(updated.rows_affected())
}

pub async fn cancel_pending_for_conversation(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Vec<Uuid>> {
    let ids: Vec<(Uuid,)> = sqlx::query_as(
        "UPDATE tool_requests \
         SET status = 'cancelled', decided_at = now() \
         WHERE tenant_id = $1 AND conversation_id = $2 AND status = 'awaiting_approval' \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(ids.into_iter().map(|(id,)| id).collect())
}

pub async fn fetch_awaiting_approval_for_conversation(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Vec<ToolRequestRow>> {
    let rows = sqlx::query_as::<_, ToolRequestRow>(
        "SELECT id, tenant_id, conversation_id, generation_id, tool_name, \
         tool_source, tenant_tool_id, arguments, status, approval_required, \
         expires_at, chain_index, started_at, finished_at, result, error, \
         created_at, decided_by_membership_id, decided_at \
         FROM tool_requests \
         WHERE tenant_id = $1 AND conversation_id = $2 AND status = 'awaiting_approval'",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
