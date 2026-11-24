use sqlx::PgConnection;
use uuid::Uuid;

/// Only structured, tenant-scoped subject IDs establish a webhook delivery's attribution.
pub async fn erase_customer(
    conn: &mut PgConnection,
    tenant: Uuid,
    customer: Uuid,
    conversations: &[Uuid],
    messages: &[Uuid],
) -> sqlx::Result<()> {
    let conversations: Vec<_> = conversations.iter().map(Uuid::to_string).collect();
    let messages: Vec<_> = messages.iter().map(Uuid::to_string).collect();
    sqlx::query(
        "UPDATE integration_webhook_deliveries SET payload = '{}'::jsonb
         WHERE tenant_id = $1
           AND (payload->>'customer_id' = $2 OR payload->>'customerId' = $2
                OR payload->>'conversation_id' = ANY($3) OR payload->>'conversationId' = ANY($3)
                OR payload->>'message_id' = ANY($4) OR payload->>'messageId' = ANY($4))
           AND COALESCE(payload->>'customer_id' = $2, true)
           AND COALESCE(payload->>'customerId' = $2, true)
           AND COALESCE(payload->>'conversation_id' = ANY($3), true)
           AND COALESCE(payload->>'conversationId' = ANY($3), true)
           AND COALESCE(payload->>'message_id' = ANY($4), true)
           AND COALESCE(payload->>'messageId' = ANY($4), true)",
    )
    .bind(tenant)
    .bind(customer.to_string())
    .bind(conversations)
    .bind(messages)
    .execute(conn)
    .await?;
    Ok(())
}
