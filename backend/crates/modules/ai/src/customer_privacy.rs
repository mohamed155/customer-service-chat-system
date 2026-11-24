use sqlx::PgConnection;
use uuid::Uuid;

pub async fn export_customer(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    conversations: &[Uuid],
    export: &mut dyn db::json_export::JsonExport,
) -> sqlx::Result<()> {
    db::json_export::write_rows(conn, export, "ai_usage_records.json", sqlx::query_scalar(
        "SELECT to_jsonb(r) FROM ai_usage_records r
         WHERE tenant_id = $1 AND id IN (SELECT usage_record_id FROM ai_generations WHERE tenant_id = $1 AND conversation_id = ANY($2)) ORDER BY id",
    )
    .bind(tenant_id)
    .bind(conversations)).await?;
    db::json_export::write_rows(
        conn,
        export,
        "ai_generations.json",
        sqlx::query_scalar(
            "SELECT to_jsonb(r) FROM ai_generations r
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
        "UPDATE ai_usage_records SET request_content = NULL, response_content = NULL, request_id = NULL
         WHERE tenant_id = $1 AND id IN (SELECT usage_record_id FROM ai_generations WHERE tenant_id = $1 AND conversation_id = ANY($2))",
    )
    .bind(tenant_id)
    .bind(conversations)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE ai_generations SET request_id = NULL
         WHERE tenant_id = $1 AND conversation_id = ANY($2)",
    )
    .bind(tenant_id)
    .bind(conversations)
    .execute(&mut *conn)
    .await?;
    Ok(())
}
