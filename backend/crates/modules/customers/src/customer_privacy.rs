use sqlx::PgConnection;
use uuid::Uuid;

pub async fn lock_customer(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    customer_id: Uuid,
) -> sqlx::Result<Option<bool>> {
    sqlx::query_scalar(
        "SELECT erasure_pending_at IS NOT NULL OR erased_at IS NOT NULL FROM customers
         WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_optional(conn)
    .await
}

pub async fn is_active(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    customer_id: Uuid,
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM customers WHERE tenant_id = $1 AND id = $2
         AND erasure_pending_at IS NULL AND erased_at IS NULL)",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(conn)
    .await
}

pub async fn mark_pending(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    customer_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE customers SET erasure_pending_at = COALESCE(erasure_pending_at, now()),
                              display_name = '[erasure pending]', email = NULL, phone = NULL,
                              metadata = '{}'::jsonb, updated_at = now()
         WHERE tenant_id = $1 AND id = $2 AND erased_at IS NULL",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn export_customer(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    customer_id: Uuid,
    export: &mut dyn db::json_export::JsonExport,
) -> sqlx::Result<()> {
    db::json_export::write_rows(
        conn,
        export,
        "customer.json",
        sqlx::query_scalar(
            "SELECT to_jsonb(c) FROM customers c
         WHERE tenant_id = $1 AND id = $2
           AND erasure_pending_at IS NULL AND erased_at IS NULL",
        )
        .bind(tenant_id)
        .bind(customer_id),
    )
    .await?;
    db::json_export::write_rows(
        conn,
        export,
        "channel_identifiers.json",
        sqlx::query_scalar(
            "SELECT to_jsonb(c) FROM customer_channel_identifiers c
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
) -> sqlx::Result<()> {
    sqlx::query(
        "DELETE FROM customer_channel_identifiers WHERE tenant_id = $1 AND customer_id = $2",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn finalize_erasure(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    customer_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE customers
         SET display_name = '[erased]', email = NULL, phone = NULL, metadata = '{}'::jsonb,
             erased_at = COALESCE(erased_at, now()), erasure_pending_at = NULL, updated_at = now()
         WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .execute(conn)
    .await?;
    Ok(())
}
