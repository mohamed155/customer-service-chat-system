use uuid::Uuid;

pub struct CustomerToolProfile {
    pub display_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub conversation_count: i64,
}

pub async fn fetch_profile_for_tool(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    customer_id: Uuid,
) -> sqlx::Result<Option<CustomerToolProfile>> {
    sqlx::query_as::<_, (String, Option<String>, Option<String>, i64)>(
        "SELECT c.display_name, c.email, c.phone, \
         COALESCE(conv.count, 0) AS conversation_count \
         FROM customers c \
         LEFT JOIN LATERAL ( \
             SELECT COUNT(*) AS count FROM conversations \
             WHERE tenant_id = c.tenant_id AND customer_id = c.id AND deleted_at IS NULL \
         ) conv ON TRUE \
         WHERE c.tenant_id = $1 AND c.id = $2 AND c.deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_optional(pool)
    .await
    .map(|row| {
        row.map(
            |(display_name, email, phone, conversation_count)| CustomerToolProfile {
                display_name,
                email,
                phone,
                conversation_count,
            },
        )
    })
}

pub async fn update_contact_field_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    customer_id: Uuid,
    field: &str,
    value: &str,
) -> sqlx::Result<bool> {
    match field {
        "email" | "phone" => {}
        _ => {
            return Err(sqlx::Error::Protocol(format!(
                "Invalid contact field: {field}. Must be 'email' or 'phone'."
            )));
        }
    }

    let sql = format!(
        "UPDATE customers SET {field} = $1, updated_at = now() \
         WHERE tenant_id = $2 AND id = $3 AND deleted_at IS NULL"
    );

    let rows = sqlx::query(&sql)
        .bind(value)
        .bind(tenant_id)
        .bind(customer_id)
        .execute(&mut **tx)
        .await?;

    Ok(rows.rows_affected() > 0)
}
