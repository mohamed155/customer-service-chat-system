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

pub async fn create_anonymous_customer_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    display_name: &str,
    channel: &str,
    identifier: &str,
) -> sqlx::Result<Uuid> {
    let customer_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO customers (id, tenant_id, display_name, email, phone) \
         VALUES ($1, $2, $3, '', '')",
    )
    .bind(customer_id)
    .bind(tenant_id)
    .bind(display_name)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "INSERT INTO customer_channel_identifiers (customer_id, tenant_id, channel, identifier) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(customer_id)
    .bind(tenant_id)
    .bind(channel)
    .bind(identifier)
    .execute(&mut **tx)
    .await?;

    Ok(customer_id)
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

pub async fn find_customer_by_identifier_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    channel: &str,
    identifier: &str,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "SELECT cci.customer_id \
         FROM customer_channel_identifiers cci \
         JOIN customers c ON c.id = cci.customer_id AND c.tenant_id = cci.tenant_id \
         WHERE cci.tenant_id = $1 AND cci.channel = $2 AND cci.identifier = $3 \
           AND cci.deleted_at IS NULL AND c.deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(channel)
    .bind(identifier)
    .fetch_optional(&mut **tx)
    .await
}

pub async fn attach_identifier_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    customer_id: Uuid,
    channel: &str,
    identifier: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO customer_channel_identifiers (tenant_id, customer_id, channel, identifier) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (tenant_id, customer_id, channel) WHERE deleted_at IS NULL DO NOTHING",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(channel)
    .bind(identifier)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn create_customer_with_identifier_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    display_name: &str,
    channel: &str,
    identifier: &str,
) -> sqlx::Result<Uuid> {
    let customer_id: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) \
         VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind(display_name)
    .fetch_one(&mut **tx)
    .await?;

    sqlx::query(
        "INSERT INTO customer_channel_identifiers (tenant_id, customer_id, channel, identifier) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(channel)
    .bind(identifier)
    .execute(&mut **tx)
    .await?;

    Ok(customer_id)
}
