use sqlx::PgConnection;
use uuid::Uuid;

// UUID parameters come from Uuid::to_string, which uses canonical lowercase spelling.
// Require one structured match and reject any supplied identifier that conflicts with
// it or cannot be verified. Text comparisons keep malformed UUIDs from causing errors.
const CUSTOMER_DELIVERY_PREDICATE: &str = "
    tenant_id = $1
    AND (lower(payload->>'customer_id') = $2 OR lower(payload->>'customerId') = $2
         OR lower(payload->>'conversation_id') = ANY($3) OR lower(payload->>'conversationId') = ANY($3)
         OR lower(payload->>'message_id') = ANY($4) OR lower(payload->>'messageId') = ANY($4))
    AND COALESCE(lower(payload->>'customer_id') = $2, true)
    AND COALESCE(lower(payload->>'customerId') = $2, true)
    AND COALESCE(lower(payload->>'conversation_id') = ANY($3), true)
    AND COALESCE(lower(payload->>'conversationId') = ANY($3), true)
    AND COALESCE(lower(payload->>'message_id') = ANY($4), true)
    AND COALESCE(lower(payload->>'messageId') = ANY($4), true)";

/// Stream all webhook deliveries attributed to the customer into the export.
/// The connection must belong to the caller's tenant Unit of Work, and the
/// conversation and message IDs must belong to this customer in that tenant.
/// Returns any query or export write error.
pub async fn export_customer(
    conn: &mut PgConnection,
    tenant: Uuid,
    customer: Uuid,
    conversations: &[Uuid],
    messages: &[Uuid],
    export: &mut dyn db::json_export::JsonExport,
) -> sqlx::Result<()> {
    let conversations: Vec<_> = conversations.iter().map(Uuid::to_string).collect();
    let messages: Vec<_> = messages.iter().map(Uuid::to_string).collect();
    let sql = format!(
        "SELECT to_jsonb(r) FROM integration_webhook_deliveries r
         WHERE {CUSTOMER_DELIVERY_PREDICATE} ORDER BY id"
    );
    db::json_export::write_rows(
        conn,
        export,
        "integration_webhook_deliveries.json",
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .bind(tenant)
            .bind(customer.to_string())
            .bind(conversations)
            .bind(messages),
    )
    .await
}

/// Clear the payload of attributed webhook deliveries, retaining delivery metadata.
/// The connection must belong to the caller's tenant Unit of Work, and the
/// conversation and message IDs must belong to this customer in that tenant.
/// Returns any database error within the caller-managed transaction.
pub async fn erase_customer(
    conn: &mut PgConnection,
    tenant: Uuid,
    customer: Uuid,
    conversations: &[Uuid],
    messages: &[Uuid],
) -> sqlx::Result<()> {
    let conversations: Vec<_> = conversations.iter().map(Uuid::to_string).collect();
    let messages: Vec<_> = messages.iter().map(Uuid::to_string).collect();
    let sql = format!(
        "UPDATE integration_webhook_deliveries SET payload = '{{}}'::jsonb
         WHERE {CUSTOMER_DELIVERY_PREDICATE}"
    );
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(tenant)
        .bind(customer.to_string())
        .bind(conversations)
        .bind(messages)
        .execute(conn)
        .await?;
    Ok(())
}
