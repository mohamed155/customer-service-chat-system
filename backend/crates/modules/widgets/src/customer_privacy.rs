use sqlx::PgConnection;
use uuid::Uuid;

pub async fn export_customer(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    customer_id: Uuid,
    export: &mut dyn db::json_export::JsonExport,
) -> sqlx::Result<()> {
    db::json_export::write_rows(
        conn,
        export,
        "widget_sessions.json",
        sqlx::query_scalar(
            "SELECT to_jsonb(s) - 'token_hash' FROM widget_sessions s
         WHERE tenant_id = $1 AND customer_id = $2 ORDER BY id",
        )
        .bind(tenant_id)
        .bind(customer_id),
    )
    .await?;
    Ok(())
}

pub async fn erase_customer(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    customer_id: Uuid,
) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar(
        "DELETE FROM widget_sessions WHERE tenant_id = $1 AND customer_id = $2 RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_all(conn)
    .await
}
