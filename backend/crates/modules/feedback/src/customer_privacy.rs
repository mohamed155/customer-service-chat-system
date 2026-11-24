use sqlx::PgConnection;
use uuid::Uuid;

/// Streams feedback records for the customer's conversation IDs to the export.
///
/// Pass a connection from the caller-managed tenant transaction and the matching validated
/// tenant ID. Returns a SQLx error if the query or export fails.
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

/// Clears comments and widget session IDs from the customer's feedback, retaining ratings.
///
/// Pass the customer's conversation IDs and a connection from the caller-managed transaction
/// for the validated tenant. Returns a SQLx error if the update fails.
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
