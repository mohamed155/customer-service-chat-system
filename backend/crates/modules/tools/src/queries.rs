use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Tenant-defined tool CRUD
// ---------------------------------------------------------------------------

pub struct NewTenantTool {
    pub tenant_id: Uuid,
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub endpoint_url: String,
    pub credential_ciphertext: Option<String>,
    pub classification: String,
    pub created_by_membership_id: Uuid,
}

pub async fn insert_tenant_tool(pool: &sqlx::PgPool, new: NewTenantTool) -> sqlx::Result<Uuid> {
    // Enforce partial-unique (tenant_id, name) WHERE deleted_at IS NULL
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM tenant_tools \
         WHERE tenant_id = $1 AND name = $2 AND deleted_at IS NULL",
    )
    .bind(new.tenant_id)
    .bind(&new.name)
    .fetch_optional(pool)
    .await?;

    if existing.is_some() {
        return Err(sqlx::Error::Protocol(format!(
            "a tool named '{}' already exists in this tenant",
            new.name
        )));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_tools \
         (tenant_id, name, description, input_schema, endpoint_url, \
          credential_ciphertext, classification, created_by_membership_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         RETURNING id",
    )
    .bind(new.tenant_id)
    .bind(&new.name)
    .bind(&new.description)
    .bind(&new.input_schema)
    .bind(&new.endpoint_url)
    .bind(new.credential_ciphertext)
    .bind(&new.classification)
    .bind(new.created_by_membership_id)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub struct TenantToolPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
    pub endpoint_url: Option<String>,
    pub credential_ciphertext: Option<String>,
    pub classification: Option<String>,
    pub enabled: Option<bool>,
}

pub async fn update_tenant_tool(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    id: Uuid,
    patch: TenantToolPatch,
) -> sqlx::Result<bool> {
    let rows = sqlx::query(
        "UPDATE tenant_tools SET \
         name = COALESCE($3, name), \
         description = COALESCE($4, description), \
         input_schema = COALESCE($5, input_schema), \
         endpoint_url = COALESCE($6, endpoint_url), \
         credential_ciphertext = COALESCE($7, credential_ciphertext), \
         classification = COALESCE($8, classification), \
         enabled = COALESCE($9, enabled), \
         updated_at = now() \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(id)
    .bind(patch.name)
    .bind(patch.description)
    .bind(patch.input_schema)
    .bind(patch.endpoint_url)
    .bind(patch.credential_ciphertext)
    .bind(patch.classification)
    .bind(patch.enabled)
    .execute(pool)
    .await?;
    Ok(rows.rows_affected() > 0)
}

pub async fn soft_delete_tenant_tool(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    id: Uuid,
) -> sqlx::Result<bool> {
    let rows = sqlx::query(
        "UPDATE tenant_tools SET deleted_at = now(), updated_at = now() \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(rows.rows_affected() > 0)
}

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TenantToolRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub endpoint_url: String,
    pub classification: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn list_tenant_tools(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
) -> sqlx::Result<Vec<TenantToolRow>> {
    sqlx::query_as::<_, TenantToolRow>(
        "SELECT id, tenant_id, name, description, input_schema, endpoint_url, \
         classification, enabled, created_at, updated_at \
         FROM tenant_tools \
         WHERE tenant_id = $1 AND deleted_at IS NULL \
         ORDER BY name ASC",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
}

pub async fn fetch_tenant_tool_with_credential(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    id: Uuid,
) -> sqlx::Result<Option<(TenantToolRow, Option<String>)>> {
    #[derive(sqlx::FromRow)]
    struct TenantToolWithCredential {
        id: Uuid,
        tenant_id: Uuid,
        name: String,
        description: String,
        input_schema: serde_json::Value,
        endpoint_url: String,
        classification: String,
        enabled: bool,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        credential_ciphertext: Option<String>,
    }

    let row: Option<TenantToolWithCredential> = sqlx::query_as(
        "SELECT id, tenant_id, name, description, input_schema, endpoint_url, \
         classification, enabled, created_at, updated_at, credential_ciphertext \
         FROM tenant_tools \
         WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| {
        let safe = TenantToolRow {
            id: r.id,
            tenant_id: r.tenant_id,
            name: r.name,
            description: r.description,
            input_schema: r.input_schema,
            endpoint_url: r.endpoint_url,
            classification: r.classification,
            enabled: r.enabled,
            created_at: r.created_at,
            updated_at: r.updated_at,
        };
        (safe, r.credential_ciphertext)
    }))
}

// ---------------------------------------------------------------------------
// Existing code below
// ---------------------------------------------------------------------------

pub async fn upsert_policy(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    tool_name: &str,
    enabled: bool,
    require_approval: bool,
    updated_by_membership_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO tenant_tool_policies \
         (tenant_id, tool_name, enabled, require_approval, updated_by_membership_id) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (tenant_id, tool_name) \
         DO UPDATE SET \
           enabled = EXCLUDED.enabled, \
           require_approval = EXCLUDED.require_approval, \
           updated_by_membership_id = EXCLUDED.updated_by_membership_id, \
           updated_at = now()",
    )
    .bind(tenant_id)
    .bind(tool_name)
    .bind(enabled)
    .bind(require_approval)
    .bind(updated_by_membership_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PolicyRow {
    pub tool_name: String,
    pub enabled: bool,
    pub require_approval: bool,
    pub updated_at: DateTime<Utc>,
}

pub async fn list_policies(pool: &sqlx::PgPool, tenant_id: Uuid) -> sqlx::Result<Vec<PolicyRow>> {
    sqlx::query_as::<_, PolicyRow>(
        "SELECT tool_name, enabled, require_approval, updated_at \
         FROM tenant_tool_policies \
         WHERE tenant_id = $1 \
         ORDER BY tool_name ASC",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ToolRequestFullRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub conversation_id: Uuid,
    pub generation_id: Uuid,
    pub tool_name: String,
    pub tool_source: String,
    pub tenant_tool_id: Option<Uuid>,
    pub arguments: Value,
    pub status: String,
    pub approval_required: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub chain_index: i16,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub decided_by_membership_id: Option<Uuid>,
    pub decided_at: Option<DateTime<Utc>>,
}

pub struct NewToolRequest {
    pub tenant_id: Uuid,
    pub conversation_id: Uuid,
    pub generation_id: Uuid,
    pub tool_name: String,
    pub tool_source: String,
    pub tenant_tool_id: Option<Uuid>,
    pub arguments: Value,
    pub status: String,
    pub approval_required: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub chain_index: i16,
}

pub async fn insert_request(pool: &sqlx::PgPool, req: NewToolRequest) -> sqlx::Result<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO tool_requests \
         (tenant_id, conversation_id, generation_id, tool_name, tool_source, \
          tenant_tool_id, arguments, status, approval_required, expires_at, chain_index) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
         RETURNING id",
    )
    .bind(req.tenant_id)
    .bind(req.conversation_id)
    .bind(req.generation_id)
    .bind(&req.tool_name)
    .bind(&req.tool_source)
    .bind(req.tenant_tool_id)
    .bind(&req.arguments)
    .bind(&req.status)
    .bind(req.approval_required)
    .bind(req.expires_at)
    .bind(req.chain_index)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Transition a request from `pending` or `approved` to `executing`.
/// Returns `false` if no row matched (race lost, wrong prior state).
pub async fn mark_executing(pool: &sqlx::PgPool, id: Uuid, tenant_id: Uuid) -> sqlx::Result<bool> {
    let rows = sqlx::query(
        "UPDATE tool_requests \
         SET status = 'executing', started_at = now() \
         WHERE id = $1 AND tenant_id = $2 AND status IN ('pending', 'approved')",
    )
    .bind(id)
    .bind(tenant_id)
    .execute(pool)
    .await?;
    Ok(rows.rows_affected() > 0)
}

/// Transition a request from `executing` to a terminal status with an optional
/// result or error. Sets `finished_at = now()`. Returns `false` if no row
/// matched (race lost, wrong prior state).
pub async fn mark_terminal(
    pool: &sqlx::PgPool,
    id: Uuid,
    tenant_id: Uuid,
    status: &str,
    result: Option<Value>,
    error: Option<String>,
) -> sqlx::Result<bool> {
    let rows = sqlx::query(
        "UPDATE tool_requests \
         SET status = $3, result = $4, error = $5, finished_at = now() \
         WHERE id = $1 AND tenant_id = $2 AND status = 'executing'",
    )
    .bind(id)
    .bind(tenant_id)
    .bind(status)
    .bind(result)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(rows.rows_affected() > 0)
}
