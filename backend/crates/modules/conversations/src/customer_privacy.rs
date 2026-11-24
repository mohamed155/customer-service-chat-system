use sqlx::PgConnection;
use uuid::Uuid;

/// Returns conversation IDs for the customer ID in the validated tenant.
///
/// Pass a connection from the caller-managed tenant transaction. Returns an empty list for
/// no matches and a SQLx error if the query fails.
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

/// Streams conversations, messages and citations for the customer's conversation IDs.
///
/// Pass a connection from the caller-managed tenant transaction and the matching validated
/// tenant ID. Returns a SQLx error if a query or export write fails.
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

/// Attachment identifiers and metadata used to locate stored media for export and erasure.
#[derive(sqlx::FromRow)]
pub struct Attachment {
    pub id: Uuid,
    pub message_id: Uuid,
    pub storage_key: Option<String>,
    pub file_name: Option<String>,
}

/// Retrieves and locks attachments for the customer's conversation IDs in the validated tenant.
///
/// Pass a connection from the caller-managed tenant transaction. Row locks last until the
/// transaction ends. Returns an empty list for no matches and a SQLx error if the query fails.
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

/// Redacts message bodies and removes citations while retaining conversations and message rows.
///
/// Pass the customer's conversation IDs and a connection from the caller-managed transaction
/// for the validated tenant. Returns a SQLx error if either mutation fails.
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

/// Removes the attachment row after the caller has deleted its stored media.
///
/// Pass the attachment ID obtained from [`attachments`] and the same validated tenant ID
/// and caller-managed transaction to hold the row lock until commit or rollback. Returns
/// a SQLx error if deletion fails. A missing row is an idempotent success rather than
/// [`sqlx::Error::RowNotFound`].
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

/// Returns message IDs linked to the customer's conversation IDs.
///
/// Pass a connection from the caller-managed tenant transaction and the matching validated
/// tenant ID. Returns an empty list for no matches and a SQLx error if the query fails.
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
