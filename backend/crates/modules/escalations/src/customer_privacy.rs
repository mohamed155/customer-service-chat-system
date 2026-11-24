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
