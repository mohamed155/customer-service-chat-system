use sqlx::PgConnection;
use uuid::Uuid;

/// Streams escalation records for the customer's conversation IDs to the export.
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
        "escalations.json",
        sqlx::query_scalar(
            "SELECT to_jsonb(r) FROM escalations r
         WHERE tenant_id = $1 AND conversation_id = ANY($2) ORDER BY id",
        )
        .bind(tenant_id)
        .bind(conversations),
    )
    .await?;
    Ok(())
}

/// Redacts reasons and skill names from the customer's escalations, retaining the records.
///
/// Pass the customer's conversation IDs and a connection from the caller-managed transaction
/// for the validated tenant. Returns a SQLx error if the update fails.
pub async fn erase_customer(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    conversations: &[Uuid],
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE escalations SET reason = '[erased]', required_skill_names = '{}', matched_skill_names = '{}'
         WHERE tenant_id = $1 AND conversation_id = ANY($2)",
    )
    .bind(tenant_id)
    .bind(conversations)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Returns IDs of escalation records linked to the customer's conversation IDs.
///
/// Pass a connection from the caller-managed tenant transaction and the matching validated
/// tenant ID. Returns an empty list for no matches and a SQLx error if the query fails.
pub async fn subject_ids(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    conversations: &[Uuid],
) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar(
        "SELECT id FROM escalations WHERE tenant_id = $1 AND conversation_id = ANY($2)",
    )
    .bind(tenant_id)
    .bind(conversations)
    .fetch_all(conn)
    .await
}
