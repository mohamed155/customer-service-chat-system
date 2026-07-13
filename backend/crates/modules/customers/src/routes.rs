use axum::{
    extract::{
        rejection::{PathRejection, QueryRejection},
        Extension, Path, Query, State,
    },
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use kernel::{ApiError, ErrorDetail};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::audit;
use crate::model::{
    canonicalize_channel, normalize_phone_digits, validate_create, validate_update,
    ChannelIdentifier, ChannelIdentifierInput, CreateCustomerPayload, CustomerDetail,
    CustomerListItem, TriState, UpdateCustomerPayload,
};

type DateTimeUtc = sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>;

fn trim_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|trimmed| !trimmed.is_empty())
        .map(str::to_owned)
}

/// Normalize a channel-identifier value before it is persisted.  Email
/// addresses are lowercased; phone-bearing channels (phone, whatsapp) are
/// normalized to E.164 (+ prefix, digits only); handle-bearing channels
/// (web_chat, telegram) are kept verbatim apart from trimming.
/// The channel name is also trimmed so that "email " matched via equality
/// still triggers the email-lowercase path.
fn normalize_identifier(channel: &str, identifier: &str) -> String {
    let trimmed = identifier.trim();
    let channel = channel.trim();
    match channel {
        "email" => trimmed.to_lowercase(),
        "phone" | "whatsapp" => normalize_phone_digits(trimmed),
        _ => trimmed.to_owned(),
    }
}

fn metadata_to_map(value: serde_json::Value) -> BTreeMap<String, String> {
    match value {
        serde_json::Value::Object(map) => map
            .into_iter()
            .map(|(key, value)| {
                let value = match value {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                };
                (key, value)
            })
            .collect(),
        _ => BTreeMap::new(),
    }
}

async fn build_identifier_conflict(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    channel: &str,
    identifier: &str,
    request_id: &str,
) -> ApiError {
    let holder = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT c.id, c.display_name \
          FROM customer_channel_identifiers ci \
          JOIN customers c ON c.id = ci.customer_id AND c.tenant_id = ci.tenant_id \
          WHERE ci.tenant_id = $1 AND ci.channel = $2 AND ci.identifier = $3 \
            AND ci.deleted_at IS NULL AND c.deleted_at IS NULL \
          LIMIT 1",
    )
    .bind(tenant_id)
    .bind(channel)
    .bind(identifier)
    .fetch_optional(&mut **tx)
    .await;

    let (existing_id, existing_name) = match holder {
        Ok(Some(row)) => row,
        _ => {
            return ApiError::conflict("Channel identifier is already in use")
                .with_request_id(request_id)
        }
    };

    let mut detail = serde_json::to_value(ErrorDetail {
        field: "identifiers".into(),
        code: "identifier_conflict".into(),
        message: format!("Channel `{channel}:{identifier}` is already held by `{existing_name}`"),
    })
    .expect("ErrorDetail is always serializable");
    if let Some(obj) = detail.as_object_mut() {
        obj.insert("channel".into(), json!(channel));
        obj.insert("identifier".into(), json!(identifier));
        obj.insert(
            "existing_customer_id".into(),
            json!(existing_id.to_string()),
        );
        obj.insert("existing_customer_name".into(), json!(existing_name));
    }
    ApiError::conflict("Channel identifier is already in use")
        .with_details(vec![detail])
        .with_request_id(request_id)
}

/// Insert a channel identifier inside a savepoint so that a 23505
/// (unique violation) does not leave the transaction in an aborted
/// state — we roll back to the savepoint, then build the 409 conflict
/// response (which includes a holder lookup against the same transaction).
async fn try_insert_identifier(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
    customer_id: Uuid,
    channel: &str,
    identifier: &str,
    request_id: &str,
) -> Result<(Uuid, DateTimeUtc), Response> {
    sqlx::query("SAVEPOINT id_insert_sp")
        .execute(&mut **tx)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to create savepoint");
            ApiError::internal_error("Failed to create identifier")
                .with_request_id(request_id)
                .into_response()
        })?;

    let result = sqlx::query_as::<_, (Uuid, DateTimeUtc)>(
        "INSERT INTO customer_channel_identifiers \
         (tenant_id, customer_id, channel, identifier) \
         VALUES ($1, $2, $3, $4) \
         RETURNING id, created_at",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(channel)
    .bind(identifier)
    .fetch_one(&mut **tx)
    .await;

    match result {
        Ok(row) => {
            sqlx::query("RELEASE SAVEPOINT id_insert_sp")
                .execute(&mut **tx)
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, "failed to release savepoint");
                    ApiError::internal_error("Failed to create identifier")
                        .with_request_id(request_id)
                        .into_response()
                })?;
            Ok(row)
        }
        Err(sqlx::Error::Database(dbe)) if dbe.code().as_deref() == Some("23505") => {
            sqlx::query("ROLLBACK TO SAVEPOINT id_insert_sp")
                .execute(&mut **tx)
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, "failed to rollback savepoint");
                    ApiError::internal_error("Failed to create identifier")
                        .with_request_id(request_id)
                        .into_response()
                })?;

            let _ = sqlx::query("RELEASE SAVEPOINT id_insert_sp")
                .execute(&mut **tx)
                .await;

            let conflict =
                build_identifier_conflict(tx, tenant_id, channel, identifier, request_id).await;
            Err(conflict.into_response())
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to insert channel identifier");
            Err(ApiError::internal_error("Failed to create identifier")
                .with_request_id(request_id)
                .into_response())
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CustomerListQuery {
    pub q: Option<String>,
    pub cursor: Option<String>,
    pub limit: u32,
}

impl Default for CustomerListQuery {
    fn default() -> Self {
        Self {
            q: None,
            cursor: None,
            limit: 25,
        }
    }
}

#[derive(Serialize)]
struct Pagination {
    next_cursor: Option<String>,
    has_more: bool,
}

#[derive(Serialize)]
struct PaginatedResponse<T> {
    data: Vec<T>,
    pagination: Pagination,
}

/// Lists active customers belonging to the tenant resolved by middleware.
pub async fn list_customers(
    State(pool): State<sqlx::PgPool>,
    ctx: tenancy::TenantContext,
    params: Result<Query<CustomerListQuery>, QueryRejection>,
) -> Response {
    let Query(params) = match params {
        Ok(params) => params,
        Err(_) => {
            return kernel::ApiError::validation_failed("Invalid query parameters")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
    };
    let limit = params.limit.clamp(1, 100);
    let cursor = match params.cursor.as_deref() {
        Some(cursor) => match decode_cursor(cursor) {
            Some(cursor) => Some(cursor),
            None => {
                return kernel::ApiError::validation_failed("Invalid customer cursor")
                    .with_request_id(&ctx.request_id)
                    .into_response()
            }
        },
        None => None,
    };

    let mut clauses = vec![
        "c.tenant_id = $1".to_owned(),
        "c.deleted_at IS NULL".to_owned(),
    ];
    let mut next_bind = 2;
    let search_pattern = params.q.filter(|q| !q.is_empty()).map(|q| {
        format!(
            "%{}%",
            q.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        )
    });

    if search_pattern.is_some() {
        clauses.push(format!(
            "(c.display_name ILIKE ${0} ESCAPE '\\' \
              OR c.email::text ILIKE ${0} ESCAPE '\\' \
              OR c.phone ILIKE ${0} ESCAPE '\\' \
              OR EXISTS (SELECT 1 FROM customer_channel_identifiers search_identifier \
                         WHERE search_identifier.customer_id = c.id \
                           AND search_identifier.tenant_id = c.tenant_id \
                           AND search_identifier.identifier ILIKE ${0} ESCAPE '\\' \
                           AND search_identifier.deleted_at IS NULL))",
            next_bind
        ));
        next_bind += 1;
    }

    if cursor.is_some() {
        clauses.push(format!(
            "(c.created_at, c.id) < (${0}::timestamptz, ${1}::uuid)",
            next_bind,
            next_bind + 1
        ));
        next_bind += 2;
    }

    let sql = format!(
        "SELECT c.id, c.display_name, c.email::text AS email, c.phone, c.created_at, c.updated_at, \
                COALESCE(array_agg(DISTINCT identifier.channel) \
                         FILTER (WHERE identifier.channel IS NOT NULL), ARRAY[]::text[]) AS channels \
         FROM customers c \
         LEFT JOIN customer_channel_identifiers identifier \
           ON identifier.customer_id = c.id AND identifier.tenant_id = c.tenant_id \
              AND identifier.deleted_at IS NULL \
         WHERE {} \
         GROUP BY c.id \
         ORDER BY c.created_at DESC, c.id DESC \
         LIMIT ${}",
        clauses.join(" AND "),
        next_bind
    );

    let mut query = sqlx::query(&sql).bind(ctx.tenant_id);
    if let Some(pattern) = search_pattern {
        query = query.bind(pattern);
    }
    if let Some((created_at, id)) = cursor {
        query = query.bind(created_at).bind(id);
    }
    let rows = match query.bind(i64::from(limit) + 1).fetch_all(&pool).await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::error!(%error, "failed to list customers");
            return kernel::ApiError::internal_error("Failed to list customers")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let has_more = rows.len() > limit as usize;
    let data: Vec<CustomerListItem> = rows
        .into_iter()
        .take(limit as usize)
        .map(|row| CustomerListItem {
            id: row.get("id"),
            display_name: row.get("display_name"),
            email: row.get("email"),
            phone: row.get("phone"),
            channels: row.get("channels"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
        .collect();
    let next_cursor = has_more.then(|| {
        let item = data.last().expect("a page with more rows has a final item");
        encode_cursor(item.created_at, item.id)
    });

    Json(PaginatedResponse {
        data,
        pagination: Pagination {
            next_cursor,
            has_more,
        },
    })
    .into_response()
}

pub async fn get_customer(
    State(pool): State<sqlx::PgPool>,
    ctx: tenancy::TenantContext,
    path: Result<Path<Uuid>, PathRejection>,
) -> Response {
    let customer_id = match path {
        Ok(Path(id)) => id,
        Err(_) => {
            return kernel::ApiError::validation_failed("Invalid customer id")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(%error, "failed to begin transaction for get_customer");
            return kernel::ApiError::internal_error("Failed to fetch customer")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let row = match sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Option<String>,
            Option<String>,
            serde_json::Value,
            sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
            sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
        ),
    >(
        "SELECT id, display_name, email::text AS email, phone, metadata, created_at, updated_at \
         FROM customers \
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(customer_id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => {
            return kernel::ApiError::not_found("Customer not found")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
        Err(error) => {
            tracing::error!(%error, "failed to fetch customer");
            return kernel::ApiError::internal_error("Failed to fetch customer")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let identifier_rows = match sqlx::query(
        "SELECT id, channel, identifier \
         FROM customer_channel_identifiers \
         WHERE customer_id = $1 AND tenant_id = $2 AND deleted_at IS NULL \
         ORDER BY created_at, id",
    )
    .bind(customer_id)
    .bind(ctx.tenant_id)
    .fetch_all(&mut *tx)
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::error!(%error, "failed to fetch customer identifiers");
            return kernel::ApiError::internal_error("Failed to fetch customer identifiers")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if let Err(error) = tx.commit().await {
        tracing::error!(%error, "failed to commit get_customer transaction");
        return kernel::ApiError::internal_error("Failed to fetch customer")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let identifiers: Vec<ChannelIdentifier> = identifier_rows
        .iter()
        .map(|row| ChannelIdentifier {
            id: row.get("id"),
            channel: row.get("channel"),
            identifier: row.get("identifier"),
        })
        .collect();

    let channels: Vec<String> = identifiers.iter().map(|i| i.channel.clone()).collect();

    let metadata = metadata_to_map(row.4);

    Json(serde_json::json!({
        "data": CustomerDetail {
            id: row.0,
            display_name: row.1,
            email: row.2,
            phone: row.3,
            channels,
            created_at: row.5,
            updated_at: row.6,
            identifiers,
            metadata,
        }
    }))
    .into_response()
}

/// `create_customer` — `POST /tenant/customers`.
///
/// Validates the payload (T041 helpers), then inside a single transaction:
///   1. INSERT the customer row (T043 / FR-007).
///   2. INSERT every supplied channel-identifier row.
///   3. Map a unique-index violation on a (tenant_id, channel, identifier)
///      tuple to `409 conflict` whose `details[0]` names the holding
///      customer's id and display name (FR-014 / FR-003).
///   4. Write the `customer.created` audit row (T042 helper).
///   5. Commit.
///
/// On success returns `201 Created` with the rendered `CustomerDetail`
/// (identical shape to `GET /tenant/customers/{id}` so the UI can reuse its
/// detail view without an extra round trip).
pub async fn create_customer(
    State(pool): State<sqlx::PgPool>,
    ctx: tenancy::TenantContext,
    Extension(principal): Extension<identity::Principal>,
    kernel::ApiJson(payload): kernel::ApiJson<CreateCustomerPayload>,
) -> Response {
    if let Err(error) = validate_create(&payload) {
        return error.with_request_id(&ctx.request_id).into_response();
    }

    let display_name = payload.display_name.trim().to_owned();
    let email = trim_optional(payload.email.as_deref()).map(|s| s.to_lowercase());
    let phone = payload
        .phone
        .as_deref()
        .map(|s| normalize_phone_digits(s.trim()));
    let metadata_value = serde_json::to_value(&payload.metadata)
        .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
    let normalized_identifiers: Vec<(String, String)> = payload
        .identifiers
        .iter()
        .map(|entry: &ChannelIdentifierInput| {
            let channel = canonicalize_channel(&entry.channel);
            (
                channel.clone(),
                normalize_identifier(&channel, &entry.identifier),
            )
        })
        .collect();

    // Reject duplicate (channel, identifier) pairs after normalization.
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut dup_indices: Vec<usize> = Vec::new();
    for (i, (channel, identifier)) in normalized_identifiers.iter().enumerate() {
        if !seen.insert((channel.clone(), identifier.clone())) {
            dup_indices.push(i);
        }
    }
    if !dup_indices.is_empty() {
        let details: Vec<_> = dup_indices
            .iter()
            .map(|&i| kernel::ErrorDetail {
                field: format!("identifiers[{i}]"),
                code: "duplicate".into(),
                message: "Duplicate channel identifier in the same payload".into(),
            })
            .collect();
        return ApiError::unprocessable_entity("Duplicate identifiers in payload")
            .with_details(details)
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(error = %error, "failed to begin create-customer transaction");
            return ApiError::internal_error("Failed to begin transaction")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let customer_row = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Option<String>,
            Option<String>,
            serde_json::Value,
            DateTimeUtc,
            DateTimeUtc,
        ),
    >(
        "INSERT INTO customers (tenant_id, display_name, email, phone, metadata) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING id, display_name, email::text, phone, metadata, created_at, updated_at",
    )
    .bind(ctx.tenant_id)
    .bind(&display_name)
    .bind(email.as_deref())
    .bind(phone.as_deref())
    .bind(&metadata_value)
    .fetch_one(&mut *tx)
    .await;

    let (customer_id, row_name, row_email, row_phone, row_metadata, created_at, updated_at) =
        match customer_row {
            Ok(row) => row,
            Err(error) => {
                tracing::error!(error = %error, "failed to insert customer row");
                return ApiError::internal_error("Failed to create customer")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        };

    let mut inserted_identifiers: Vec<ChannelIdentifier> =
        Vec::with_capacity(normalized_identifiers.len());
    for (channel, identifier) in &normalized_identifiers {
        match try_insert_identifier(
            &mut tx,
            ctx.tenant_id,
            customer_id,
            channel,
            identifier,
            &ctx.request_id,
        )
        .await
        {
            Ok((id, _)) => inserted_identifiers.push(ChannelIdentifier {
                id,
                channel: channel.clone(),
                identifier: identifier.clone(),
            }),
            Err(response) => return response,
        }
    }

    let created_fields: Vec<&str> = {
        let mut fields = Vec::with_capacity(4);
        fields.push("display_name");
        if payload.email.is_some() {
            fields.push("email");
        }
        if payload.phone.is_some() {
            fields.push("phone");
        }
        if !payload.identifiers.is_empty() {
            fields.push("identifiers");
        }
        if !payload.metadata.is_empty() {
            fields.push("metadata");
        }
        fields
    };
    if let Err(error) = audit::record_customer_created(
        &mut tx,
        principal.user_id,
        ctx.tenant_id,
        customer_id,
        &created_fields,
    )
    .await
    {
        tracing::error!(error = %error, "failed to record customer.created audit");
        return ApiError::internal_error("Failed to record audit entry")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(error) = tx.commit().await {
        tracing::error!(error = %error, "failed to commit create-customer transaction");
        return ApiError::internal_error("Failed to commit transaction")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let channels: Vec<String> = inserted_identifiers
        .iter()
        .map(|identifier| identifier.channel.clone())
        .collect();
    let metadata = metadata_to_map(row_metadata);

    let body = json!({
        "data": CustomerDetail {
            id: customer_id,
            display_name: row_name,
            email: row_email,
            phone: row_phone,
            channels,
            created_at,
            updated_at,
            identifiers: inserted_identifiers,
            metadata,
        }
    });

    (StatusCode::CREATED, Json(body)).into_response()
}

/// `update_customer` — `PATCH /tenant/customers/{customer_id}`.
///
/// Partial update.  Inside a single transaction:
///   1. Look up the customer scoped to the resolved tenant (404 if missing,
///      soft-deleted, or belonging to another tenant — FR-011 / SC-003).
///   2. Compute the new state by overlaying the payload on the existing row.
///   3. Run a dynamic UPDATE that touches only fields whose value actually
///      changed, so `updated_at` is refreshed by the trigger precisely when
///      the data moved (FR-008).
///   4. If `identifiers` was supplied, replace the set in one DELETE+INSERT
///      cycle.  A unique-index violation on the new tuple surfaces as `409`
///      with the same holder-naming shape as the create handler.
///   5. Write the `customer.updated` audit row carrying only the names of
///      fields that actually changed (no values, per FR-017).
///   6. Commit and re-render the row into a `CustomerDetail` for the
///      response.
pub async fn update_customer(
    State(pool): State<sqlx::PgPool>,
    ctx: tenancy::TenantContext,
    Extension(principal): Extension<identity::Principal>,
    path: Result<Path<Uuid>, PathRejection>,
    kernel::ApiJson(payload): kernel::ApiJson<UpdateCustomerPayload>,
) -> Response {
    let customer_id = match path {
        Ok(Path(id)) => id,
        Err(_) => {
            return ApiError::validation_failed("Invalid customer id")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
    };

    if let Err(error) = validate_update(&payload) {
        return error.with_request_id(&ctx.request_id).into_response();
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::error!(error = %error, "failed to begin update-customer transaction");
            return ApiError::internal_error("Failed to begin transaction")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let existing = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Option<String>,
            Option<String>,
            serde_json::Value,
            DateTimeUtc,
            DateTimeUtc,
        ),
    >(
        "SELECT id, display_name, email::text, phone, metadata, created_at, updated_at \
         FROM customers \
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL \
         FOR UPDATE",
    )
    .bind(customer_id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *tx)
    .await;

    let (
        _,
        existing_name,
        existing_email,
        existing_phone,
        existing_metadata,
        _existing_created_at,
        _previous_updated_at,
    ) = match existing {
        Ok(Some(row)) => row,
        Ok(None) => {
            return ApiError::not_found("Customer not found")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
        Err(error) => {
            tracing::error!(error = %error, "failed to fetch customer for update");
            return ApiError::internal_error("Failed to load customer")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let mut set_clauses: Vec<String> = Vec::new();
    let mut changed_fields: Vec<String> = Vec::new();

    let new_name = payload
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    if let Some(name) = &new_name {
        if name != &existing_name {
            set_clauses.push(format!("display_name = ${}", set_clauses.len() + 1));
            changed_fields.push("display_name".to_owned());
        }
    }

    let new_email: Option<Option<String>> = match &payload.email {
        TriState::Absent => None,
        TriState::Clear => Some(None),
        TriState::Value(s) => Some(Some(s.trim().to_lowercase())),
    };
    if let Some(email) = &new_email {
        if email != &existing_email {
            set_clauses.push(format!("email = ${}", set_clauses.len() + 1));
            changed_fields.push("email".to_owned());
        }
    }

    let new_phone: Option<Option<String>> = match &payload.phone {
        TriState::Absent => None,
        TriState::Clear => Some(None),
        TriState::Value(s) => Some(Some(normalize_phone_digits(s.trim()))),
    };
    if let Some(phone) = &new_phone {
        if phone != &existing_phone {
            set_clauses.push(format!("phone = ${}", set_clauses.len() + 1));
            changed_fields.push("phone".to_owned());
        }
    }

    let new_metadata: Option<BTreeMap<String, String>> = payload.metadata.as_ref().map(|m| {
        m.iter()
            .map(|(k, v)| (k.trim().to_owned(), v.clone()))
            .collect()
    });
    let new_metadata_value: Option<serde_json::Value> = new_metadata
        .as_ref()
        .map(|m| serde_json::to_value(m).unwrap_or(serde_json::Value::Null));
    if let Some(value) = &new_metadata_value {
        if value != &existing_metadata {
            set_clauses.push(format!("metadata = ${}", set_clauses.len() + 1));
            changed_fields.push("metadata".to_owned());
        }
    }

    let normalized_new_identifiers: Option<Vec<(String, String)>> = payload
        .identifiers
        .as_ref()
        .map(|entries: &Vec<ChannelIdentifierInput>| {
            entries
                .iter()
                .map(|entry| {
                    let channel = canonicalize_channel(&entry.channel);
                    (
                        channel.clone(),
                        normalize_identifier(&channel, &entry.identifier),
                    )
                })
                .collect()
        });
    if !set_clauses.is_empty() {
        let where_offset = set_clauses.len() + 1;
        let sql = format!(
            "UPDATE customers SET {} WHERE id = ${} AND tenant_id = ${} AND deleted_at IS NULL",
            set_clauses.join(", "),
            where_offset,
            where_offset + 1
        );

        let mut query = sqlx::query(&sql);
        if let Some(name) = &new_name {
            if name != &existing_name {
                query = query.bind(name);
            }
        }
        if let Some(email) = &new_email {
            if email != &existing_email {
                query = query.bind(email.clone());
            }
        }
        if let Some(phone) = &new_phone {
            if phone != &existing_phone {
                query = query.bind(phone.clone());
            }
        }
        if let Some(value) = &new_metadata_value {
            if value != &existing_metadata {
                query = query.bind(value);
            }
        }
        query = query.bind(customer_id).bind(ctx.tenant_id);

        if let Err(error) = query.execute(&mut *tx).await {
            tracing::error!(error = %error, "failed to update customer row");
            return ApiError::internal_error("Failed to update customer")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    if let Some(new_identifiers) = &normalized_new_identifiers {
        // T082: Fetch current set of live identifiers for this customer under
        // the row lock we already hold (SELECT … FOR UPDATE on the customer).
        let current_rows = match sqlx::query(
            "SELECT channel, identifier \
             FROM customer_channel_identifiers \
             WHERE customer_id = $1 AND tenant_id = $2 AND deleted_at IS NULL \
             ORDER BY channel, identifier",
        )
        .bind(customer_id)
        .bind(ctx.tenant_id)
        .fetch_all(&mut *tx)
        .await
        {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!(error = %e, "failed to fetch current identifiers");
                return ApiError::internal_error("Failed to update identifiers")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        };

        let current_normalized: Vec<(String, String)> = current_rows
            .iter()
            .map(|row| {
                let channel: String = row.get("channel");
                let identifier: String = row.get("identifier");
                (
                    canonicalize_channel(&channel),
                    normalize_identifier(&channel, &identifier),
                )
            })
            .collect();

        let mut new_sorted = new_identifiers.clone();
        new_sorted.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

        if current_normalized != new_sorted {
            // Identifiers have changed — proceed with DELETE+INSERT cycle.
            changed_fields.push("identifiers".to_owned());

            // Soft-delete existing live identifiers.
            if let Err(e) = sqlx::query(
                "UPDATE customer_channel_identifiers \
                 SET deleted_at = now() \
                 WHERE customer_id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
            )
            .bind(customer_id)
            .bind(ctx.tenant_id)
            .execute(&mut *tx)
            .await
            {
                tracing::error!(error = %e, "failed to soft-delete existing identifiers");
                return ApiError::internal_error("Failed to update identifiers")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }

            // Insert each new identifier with savepoint protection.
            for (channel, identifier) in new_identifiers {
                if let Err(response) = try_insert_identifier(
                    &mut tx,
                    ctx.tenant_id,
                    customer_id,
                    channel,
                    identifier,
                    &ctx.request_id,
                )
                .await
                {
                    return response;
                }
            }
        }
        // If sets are identical, skip the entire cycle — no change to
        // updated_at, no changed_fields entry, no audit for identifiers.
    }

    // If identifiers changed but no scalar UPDATE ran (set_clauses was
    // empty), explicitly bump updated_at so the timestamp reflects the
    // identifier change.
    if set_clauses.is_empty() && changed_fields.iter().any(|f| f == "identifiers") {
        if let Err(error) =
            sqlx::query("UPDATE customers SET updated_at = now() WHERE id = $1 AND tenant_id = $2")
                .bind(customer_id)
                .bind(ctx.tenant_id)
                .execute(&mut *tx)
                .await
        {
            tracing::error!(error = %error, "failed to refresh updated_at for identifier change");
            return ApiError::internal_error("Failed to update customer")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    if !changed_fields.is_empty() {
        let changed_refs: Vec<&str> = changed_fields.iter().map(String::as_str).collect();
        if let Err(error) = audit::record_customer_updated(
            &mut tx,
            principal.user_id,
            ctx.tenant_id,
            customer_id,
            &changed_refs,
        )
        .await
        {
            tracing::error!(error = %error, "failed to record customer.updated audit");
            return ApiError::internal_error("Failed to record audit entry")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    let row = sqlx::query_as::<
        _,
        (
            String,
            Option<String>,
            Option<String>,
            serde_json::Value,
            DateTimeUtc,
            DateTimeUtc,
        ),
    >(
        "SELECT display_name, email::text, phone, metadata, created_at, updated_at \
         FROM customers \
         WHERE id = $1 AND tenant_id = $2",
    )
    .bind(customer_id)
    .bind(ctx.tenant_id)
    .fetch_one(&mut *tx)
    .await;

    let (final_name, final_email, final_phone, final_metadata, final_created_at, final_updated_at) =
        match row {
            Ok(row) => row,
            Err(error) => {
                tracing::error!(error = %error, "failed to refetch customer after update");
                return ApiError::internal_error("Failed to load customer")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        };

    let identifier_rows = sqlx::query(
        "SELECT id, channel, identifier \
         FROM customer_channel_identifiers \
         WHERE customer_id = $1 AND tenant_id = $2 AND deleted_at IS NULL \
         ORDER BY created_at, id",
    )
    .bind(customer_id)
    .bind(ctx.tenant_id)
    .fetch_all(&mut *tx)
    .await;

    let identifiers: Vec<ChannelIdentifier> = match identifier_rows {
        Ok(rows) => rows
            .iter()
            .map(|row| ChannelIdentifier {
                id: row.get("id"),
                channel: row.get("channel"),
                identifier: row.get("identifier"),
            })
            .collect(),
        Err(error) => {
            tracing::error!(error = %error, "failed to fetch updated identifiers");
            return ApiError::internal_error("Failed to load identifiers")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if let Err(error) = tx.commit().await {
        tracing::error!(error = %error, "failed to commit update-customer transaction");
        return ApiError::internal_error("Failed to commit transaction")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let channels: Vec<String> = identifiers.iter().map(|i| i.channel.clone()).collect();
    let metadata = metadata_to_map(final_metadata);

    Json(json!({
        "data": CustomerDetail {
            id: customer_id,
            display_name: final_name,
            email: final_email,
            phone: final_phone,
            channels,
            created_at: final_created_at,
            updated_at: final_updated_at,
            identifiers,
            metadata,
        }
    }))
    .into_response()
}

fn encode_cursor(
    created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    id: Uuid,
) -> String {
    format!("{}|{id}", created_at.to_rfc3339())
        .bytes()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn decode_cursor(
    cursor: &str,
) -> Option<(
    sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    Uuid,
)> {
    if cursor.len() % 2 != 0 {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..cursor.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&cursor[index..index + 2], 16).ok())
        .collect();
    let decoded = String::from_utf8(bytes?).ok()?;
    let (created_at, id) = decoded.split_once('|')?;
    Some((
        sqlx::types::chrono::DateTime::parse_from_rfc3339(created_at)
            .ok()?
            .with_timezone(&sqlx::types::chrono::Utc),
        Uuid::parse_str(id).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        extract::Extension,
        http::{Request, StatusCode},
        routing::get,
        Router,
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn tenant_context() -> tenancy::TenantContext {
        tenancy::TenantContext {
            tenant_id: Uuid::nil(),
            tenant_status: "active".into(),
            principal_kind: identity::PrincipalKind::Tenant,
            tenant_role: Some(authz::TenantRole::Admin),
            permissions: authz::PermissionSet::default(),
            request_id: String::new(),
        }
    }

    #[tokio::test]
    async fn malformed_list_query_returns_the_api_error_envelope() {
        let app = Router::new()
            .route("/", get(list_customers))
            .layer(Extension(tenant_context()))
            .with_state(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap());

        for query in ["limit=abc", "cursor=%FF"] {
            let response = app
                .clone()
                .oneshot(
                    Request::get(format!("/?{query}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(body["error"]["code"], "validation_failed");
            assert!(body["error"]["request_id"].is_string());
        }
    }
}
