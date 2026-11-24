use sqlx::PgConnection;
use uuid::Uuid;

pub async fn export_customer(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    conversations: &[Uuid],
    export: &mut dyn db::json_export::JsonExport,
) -> sqlx::Result<()> {
    db::json_export::write_rows(
        conn,
        export,
        "conversation_feedback.json",
        sqlx::query_scalar(
            "SELECT to_jsonb(r) FROM conversation_feedback r
         WHERE tenant_id = $1 AND conversation_id = ANY($2) ORDER BY id",
        )
        .bind(tenant_id)
        .bind(conversations),
    )
    .await?;
    Ok(())
}

pub async fn erase_customer(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    conversations: &[Uuid],
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE conversation_feedback SET comment = NULL, widget_session_id = NULL
         WHERE tenant_id = $1 AND conversation_id = ANY($2)",
    )
    .bind(tenant_id)
    .bind(conversations)
    .execute(&mut *conn)
    .await?;
    Ok(())
}
