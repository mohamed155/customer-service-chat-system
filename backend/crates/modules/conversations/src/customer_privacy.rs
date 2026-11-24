use sqlx::PgConnection;
use uuid::Uuid;

pub async fn conversation_ids(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    customer_id: Uuid,
) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar("SELECT id FROM conversations WHERE tenant_id = $1 AND customer_id = $2")
        .bind(tenant_id)
        .bind(customer_id)
        .fetch_all(conn)
        .await
}

pub async fn export_customer(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    conversations: &[Uuid],
    export: &mut dyn db::json_export::JsonExport,
) -> sqlx::Result<()> {
    db::json_export::write_rows(conn, export, "conversations.json", sqlx::query_scalar(
        "SELECT to_jsonb(c) FROM conversations c WHERE tenant_id = $1 AND id = ANY($2) ORDER BY id",
    )
    .bind(tenant_id)
    .bind(conversations)).await?;
    db::json_export::write_rows(
        conn,
        export,
        "messages.json",
        sqlx::query_scalar(
            "SELECT to_jsonb(m) FROM messages m
         WHERE tenant_id = $1 AND conversation_id = ANY($2) ORDER BY conversation_id, seq",
        )
        .bind(tenant_id)
        .bind(conversations),
    )
    .await?;
    db::json_export::write_rows(
        conn,
        export,
        "message_citations.json",
        sqlx::query_scalar(
            "SELECT to_jsonb(c) FROM message_citations c
         WHERE tenant_id = $1 AND message_id IN
            (SELECT id FROM messages WHERE tenant_id = $1 AND conversation_id = ANY($2))
         ORDER BY id",
        )
        .bind(tenant_id)
        .bind(conversations),
    )
    .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
pub struct Attachment {
    pub id: Uuid,
    pub message_id: Uuid,
    pub storage_key: Option<String>,
    pub file_name: Option<String>,
}

pub async fn attachments(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    conversations: &[Uuid],
) -> sqlx::Result<Vec<Attachment>> {
    sqlx::query_as(
        "SELECT id, message_id, storage_key, file_name FROM message_attachments
         WHERE tenant_id = $1 AND message_id IN
            (SELECT id FROM messages WHERE tenant_id = $1 AND conversation_id = ANY($2))
         ORDER BY id FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(conversations)
    .fetch_all(conn)
    .await
}

pub async fn erase_customer(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    conversations: &[Uuid],
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE messages SET body = '[erased]' WHERE tenant_id = $1 AND conversation_id = ANY($2)",
    )
    .bind(tenant_id)
    .bind(conversations)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "DELETE FROM message_citations WHERE tenant_id = $1 AND message_id IN
            (SELECT id FROM messages WHERE tenant_id = $1 AND conversation_id = ANY($2))",
    )
    .bind(tenant_id)
    .bind(conversations)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn delete_attachment(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    attachment_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM message_attachments WHERE tenant_id = $1 AND id = $2")
        .bind(tenant_id)
        .bind(attachment_id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn message_ids(
    conn: &mut PgConnection,
    tenant: Uuid,
    conversations: &[Uuid],
) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar("SELECT id FROM messages WHERE tenant_id = $1 AND conversation_id = ANY($2)")
        .bind(tenant)
        .bind(conversations)
        .fetch_all(conn)
        .await
}
