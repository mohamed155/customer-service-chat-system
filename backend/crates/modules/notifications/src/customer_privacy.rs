use sqlx::PgConnection;
use uuid::Uuid;

pub async fn export_customer(
    conn: &mut PgConnection,
    tenant: Uuid,
    conversations: &[Uuid],
    escalations: &[Uuid],
    tools: &[Uuid],
    export: &mut dyn db::json_export::JsonExport,
) -> sqlx::Result<()> {
    db::json_export::write_rows(
        conn,
        export,
        "notifications.json",
        sqlx::query_scalar(
            "SELECT to_jsonb(n) FROM notifications n WHERE tenant_id = $1 AND (
            (subject_type = 'conversation' AND subject_id = ANY($2)) OR
            (subject_type = 'escalation' AND subject_id = ANY($3)) OR
            (subject_type = 'tool_request' AND subject_id = ANY($4))
         ) ORDER BY id",
        )
        .bind(tenant)
        .bind(conversations)
        .bind(escalations)
        .bind(tools),
    )
    .await?;
    Ok(())
}

pub async fn erase_customer(
    conn: &mut PgConnection,
    tenant: Uuid,
    conversations: &[Uuid],
    escalations: &[Uuid],
    tools: &[Uuid],
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE notifications SET title = '[erased]', body = NULL,
            dedupe_key = 'privacy-erased:' || id::text
         WHERE tenant_id = $1 AND (
            (subject_type = 'conversation' AND subject_id = ANY($2)) OR
            (subject_type = 'escalation' AND subject_id = ANY($3)) OR
            (subject_type = 'tool_request' AND subject_id = ANY($4))
         )",
    )
    .bind(tenant)
    .bind(conversations)
    .bind(escalations)
    .bind(tools)
    .execute(conn)
    .await?;
    Ok(())
}
