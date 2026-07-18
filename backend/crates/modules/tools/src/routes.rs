use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use kernel::ApiError;
use tracing;

#[derive(Deserialize)]
pub struct DecideRequest {
    pub decision: String,
}

#[derive(Deserialize)]
pub struct PageParams {
    pub cursor: Option<Uuid>,
    pub limit: Option<i64>,
}

#[derive(serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ToolRequestRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub conversation_id: Uuid,
    pub generation_id: Uuid,
    pub tool_name: String,
    pub tool_source: String,
    pub tenant_tool_id: Option<Uuid>,
    pub arguments: serde_json::Value,
    pub status: String,
    pub approval_required: bool,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub chain_index: i16,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub decided_by_membership_id: Option<Uuid>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
}

// T047: Enriched response struct for the tool-activity endpoint
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecidedByInfo {
    pub membership_id: Uuid,
    pub display_name: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolActivityItem {
    pub id: Uuid,
    pub generation_id: Uuid,
    pub tool_name: String,
    pub tool_source: String,
    pub arguments: serde_json::Value,
    pub status: String,
    pub approval_required: bool,
    pub chain_index: i16,
    pub decided_by: Option<DecidedByInfo>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub duration_ms: Option<i64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub has_result: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Query row that includes the JOINed display name for the decider.
#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct ToolActivityQueryRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub conversation_id: Uuid,
    pub generation_id: Uuid,
    pub tool_name: String,
    pub tool_source: String,
    pub tenant_tool_id: Option<Uuid>,
    pub arguments: serde_json::Value,
    pub status: String,
    pub approval_required: bool,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub chain_index: i16,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub decided_by_membership_id: Option<Uuid>,
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    pub decided_by_display_name: Option<String>,
}

impl From<ToolActivityQueryRow> for ToolActivityItem {
    fn from(row: ToolActivityQueryRow) -> Self {
        let duration_ms = match (row.started_at, row.finished_at) {
            (Some(start), Some(finish)) => {
                let dur = finish - start;
                Some(dur.num_milliseconds())
            }
            _ => None,
        };
        let has_result = row.result.is_some();
        let decided_by = match (row.decided_by_membership_id, row.decided_by_display_name) {
            (Some(mid), Some(dn)) => Some(DecidedByInfo {
                membership_id: mid,
                display_name: dn,
            }),
            (Some(mid), None) => Some(DecidedByInfo {
                membership_id: mid,
                display_name: "Unknown".into(),
            }),
            _ => None,
        };
        ToolActivityItem {
            id: row.id,
            generation_id: row.generation_id,
            tool_name: row.tool_name,
            tool_source: row.tool_source,
            arguments: row.arguments,
            status: row.status,
            approval_required: row.approval_required,
            chain_index: row.chain_index,
            decided_by,
            decided_at: row.decided_at,
            expires_at: row.expires_at,
            started_at: row.started_at,
            finished_at: row.finished_at,
            duration_ms,
            result: row.result,
            error: row.error,
            has_result,
            created_at: row.created_at,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/tenant/conversations/{id}/tool-activity",
    tag = "tools",
    operation_id = "tool_activity",
    summary = "Get tool activity for a conversation",
    description = "Returns paged tool request history with enrichment (decider display name, duration, has_result). Staff-only.",
    params(
        ("id" = Uuid, Path, description = "Conversation ID"),
        ("cursor" = Option<Uuid>, Query, description = "Pagination cursor"),
        ("limit" = Option<i64>, Query, description = "Page size"),
    ),
    responses(
        (status = 200, description = "Tool activity items", body = serde_json::Value),
        (status = 500, description = "Internal error", body = kernel::ErrorEnvelope),
    ),
)]
pub async fn tool_activity(
    State(pool): State<PgPool>,
    ctx: tenancy::TenantContext,
    Path(conversation_id): Path<Uuid>,
    Query(params): Query<PageParams>,
) -> Response {
    let limit = params.limit.unwrap_or(50).min(100);

    let base_sql = "SELECT tr.id, tr.tenant_id, tr.conversation_id, tr.generation_id, \
                    tr.tool_name, tr.tool_source, tr.tenant_tool_id, tr.arguments, \
                    tr.status, tr.approval_required, tr.expires_at, tr.chain_index, \
                    tr.started_at, tr.finished_at, tr.result, tr.error, \
                    tr.created_at, tr.decided_by_membership_id, tr.decided_at, \
                    u.display_name AS decided_by_display_name \
                    FROM tool_requests tr \
                    LEFT JOIN tenant_memberships tm \
                      ON tr.decided_by_membership_id = tm.id AND tm.deleted_at IS NULL \
                    LEFT JOIN users u ON tm.user_id = u.id";

    let result = match params.cursor {
        Some(cursor) => {
            sqlx::query_as::<_, ToolActivityQueryRow>(
                &format!(
                    "{} WHERE tr.tenant_id = $1 AND tr.conversation_id = $2 \
                     AND tr.created_at < (SELECT created_at FROM tool_requests WHERE id = $3 AND tenant_id = $1) \
                     ORDER BY tr.created_at DESC LIMIT $4",
                    base_sql
                ),
            )
            .bind(ctx.tenant_id)
            .bind(conversation_id)
            .bind(cursor)
            .bind(limit)
            .fetch_all(&pool)
            .await
        }
        None => {
            sqlx::query_as::<_, ToolActivityQueryRow>(
                &format!(
                    "{} WHERE tr.tenant_id = $1 AND tr.conversation_id = $2 \
                     ORDER BY tr.created_at DESC LIMIT $3",
                    base_sql
                ),
            )
            .bind(ctx.tenant_id)
            .bind(conversation_id)
            .bind(limit)
            .fetch_all(&pool)
            .await
        }
    };

    match result {
        Ok(rows) => {
            let items: Vec<ToolActivityItem> = rows.into_iter().map(Into::into).collect();
            Json(serde_json::json!({ "items": items })).into_response()
        }
        Err(e) => {
            tracing::error!(%e, "tool_activity query failed");
            ApiError::internal_error("Failed to fetch tool activity")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/tenant/tool-requests/{id}/decide",
    tag = "tools",
    operation_id = "decide_tool_request",
    summary = "Decide (approve/deny) a pending tool request",
    description = "Allows an authorized staff member to approve or deny an awaiting-approval tool request. Returns 200 on first decision, 409 with the settled state on a duplicate.",
    params(
        ("id" = Uuid, Path, description = "Tool request ID"),
    ),
    responses(
        (status = 200, description = "Decision applied", body = serde_json::Value),
        (status = 409, description = "Already settled", body = serde_json::Value),
        (status = 422, description = "Invalid decision value", body = kernel::ErrorEnvelope),
        (status = 500, description = "Internal error", body = kernel::ErrorEnvelope),
    ),
)]
pub async fn decide_tool_request(
    State(pool): State<PgPool>,
    ctx: tenancy::TenantContext,
    Extension(principal): Extension<identity::Principal>,
    Path(id): Path<Uuid>,
    kernel::ApiJson(payload): kernel::ApiJson<DecideRequest>,
) -> Response {
    let approve = match payload.decision.as_str() {
        "approve" => true,
        "deny" => false,
        _ => {
            return ApiError::unprocessable_entity("decision must be 'approve' or 'deny'")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let membership_id = match sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM tenant_memberships \
         WHERE tenant_id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(ctx.tenant_id)
    .bind(principal.user_id)
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(mid)) => mid,
        Ok(None) => {
            return ApiError::not_found("Membership not found in this tenant")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "decide: resolve membership failed");
            return ApiError::internal_error("Failed to resolve membership")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    match crate::approval::decide(&pool, ctx.tenant_id, id, membership_id, approve).await {
        Ok(crate::approval::DecideOutcome::Applied(row)) => {
            Json(serde_json::json!(row)).into_response()
        }
        Ok(crate::approval::DecideOutcome::AlreadySettled(row)) => {
            ApiError::conflict("Tool request has already been decided")
                .with_details(vec![serde_json::json!(row)])
                .with_request_id(&ctx.request_id)
                .into_response()
        }
        Err(e) => {
            tracing::error!(%e, "decide_tool_request failed");
            ApiError::internal_error("Failed to decide tool request")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/tenant/tool-requests",
    tag = "tools",
    operation_id = "list_pending_tool_requests",
    summary = "List pending tool requests",
    description = "Returns paged tool requests with status=awaiting_approval.",
    params(
        ("cursor" = Option<Uuid>, Query, description = "Pagination cursor"),
        ("limit" = Option<i64>, Query, description = "Page size"),
    ),
    responses(
        (status = 200, description = "Pending tool requests", body = serde_json::Value),
        (status = 500, description = "Internal error", body = kernel::ErrorEnvelope),
    ),
)]
pub async fn list_pending_tool_requests(
    State(pool): State<PgPool>,
    ctx: tenancy::TenantContext,
    Query(params): Query<PageParams>,
) -> Response {
    let limit = params.limit.unwrap_or(50).min(100);

    let result = match params.cursor {
        Some(cursor) => {
            sqlx::query_as::<_, ToolRequestRow>(
                "SELECT id, tenant_id, conversation_id, generation_id, tool_name, \
                 tool_source, tenant_tool_id, arguments, status, approval_required, \
                 expires_at, chain_index, started_at, finished_at, result, error, \
                 created_at, decided_by_membership_id, decided_at \
                 FROM tool_requests \
                 WHERE tenant_id = $1 AND status = 'awaiting_approval' AND created_at < ( \
                     SELECT created_at FROM tool_requests WHERE id = $2 AND tenant_id = $1 \
                 ) \
                 ORDER BY created_at DESC \
                 LIMIT $3",
            )
            .bind(ctx.tenant_id)
            .bind(cursor)
            .bind(limit)
            .fetch_all(&pool)
            .await
        }
        None => {
            sqlx::query_as::<_, ToolRequestRow>(
                "SELECT id, tenant_id, conversation_id, generation_id, tool_name, \
                 tool_source, tenant_tool_id, arguments, status, approval_required, \
                 expires_at, chain_index, started_at, finished_at, result, error, \
                 created_at, decided_by_membership_id, decided_at \
                 FROM tool_requests \
                 WHERE tenant_id = $1 AND status = 'awaiting_approval' \
                 ORDER BY created_at DESC \
                 LIMIT $2",
            )
            .bind(ctx.tenant_id)
            .bind(limit)
            .fetch_all(&pool)
            .await
        }
    };

    match result {
        Ok(items) => Json(serde_json::json!({ "items": items })).into_response(),
        Err(e) => {
            tracing::error!(%e, "list_pending_tool_requests query failed");
            ApiError::internal_error("Failed to list pending tool requests")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// T057: Tool settings routes
// ---------------------------------------------------------------------------

/// Shared helper: resolve membership_id from Principal + TenantContext
async fn resolve_membership(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
    request_id: &str,
) -> Result<Uuid, Response> {
    match sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM tenant_memberships \
         WHERE tenant_id = $1 AND user_id = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    {
        Ok(Some(mid)) => Ok(mid),
        Ok(None) => Err(ApiError::not_found("Membership not found in this tenant")
            .with_request_id(request_id)
            .into_response()),
        Err(e) => {
            tracing::error!(%e, "resolve_membership failed");
            Err(ApiError::internal_error("Failed to resolve membership")
                .with_request_id(request_id)
                .into_response())
        }
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ListToolsItem {
    name: String,
    description: String,
    source: String,
    has_credential: Option<bool>,
    classification: String,
    enabled: bool,
    require_approval: bool,
    effective_approval: bool,
}

#[utoipa::path(
    get,
    path = "/api/v1/tenant/tools",
    tag = "tools",
    operation_id = "list_tools",
    summary = "List all available tools",
    description = "Returns combined built-in and tenant-defined tools for the current tenant, with effective approval settings.",
    responses(
        (status = 200, description = "Tool list", body = serde_json::Value),
        (status = 500, description = "Internal error", body = kernel::ErrorEnvelope),
    ),
)]
pub async fn list_tools(State(pool): State<PgPool>, ctx: tenancy::TenantContext) -> Response {
    let resolved = match crate::policy::resolve_available(&pool, ctx.tenant_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(%e, "list_tools: resolve_available failed");
            return ApiError::internal_error("Failed to list tools")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let tenant_tools = match crate::queries::list_tenant_tools(&pool, ctx.tenant_id).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(%e, "list_tools: list_tenant_tools failed");
            return ApiError::internal_error("Failed to list tools")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let mut items: Vec<ListToolsItem> = Vec::new();

    for bt in &resolved {
        let classification = if bt.approval_required {
            "approval"
        } else {
            "auto"
        };
        items.push(ListToolsItem {
            name: bt.spec.name.clone(),
            description: bt.spec.description.clone(),
            source: bt.source.as_str().to_string(),
            has_credential: None,
            classification: classification.to_string(),
            enabled: true,
            require_approval: bt.approval_required,
            effective_approval: bt.approval_required,
        });
    }

    for tt in &tenant_tools {
        items.push(ListToolsItem {
            name: tt.name.clone(),
            description: tt.description.clone(),
            source: "tenant".to_string(),
            has_credential: None,
            classification: tt.classification.clone(),
            enabled: tt.enabled,
            require_approval: tt.classification == "approval",
            effective_approval: tt.classification == "approval",
        });
    }

    items.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.name.cmp(&b.name)));

    Json(serde_json::json!({ "items": items })).into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateBuiltinPolicyBody {
    pub enabled: bool,
    pub require_approval: bool,
}

#[utoipa::path(
    put,
    path = "/api/v1/tenant/tools/builtin/{name}/policy",
    tag = "tools",
    operation_id = "update_builtin_policy",
    summary = "Update policy for a built-in tool",
    description = "Enable/disable a built-in tool and set require_approval (tighten-only: cannot relax platform-mandated approval).",
    params(
        ("name" = String, Path, description = "Built-in tool name"),
    ),
    responses(
        (status = 200, description = "Policy updated"),
        (status = 500, description = "Internal error", body = kernel::ErrorEnvelope),
    ),
)]
pub async fn update_builtin_policy(
    State(pool): State<PgPool>,
    ctx: tenancy::TenantContext,
    Extension(principal): Extension<identity::Principal>,
    Path(name): Path<String>,
    kernel::ApiJson(payload): kernel::ApiJson<UpdateBuiltinPolicyBody>,
) -> Response {
    let membership_id =
        match resolve_membership(&pool, ctx.tenant_id, principal.user_id, &ctx.request_id).await {
            Ok(mid) => mid,
            Err(resp) => return resp,
        };

    // Tighten-only: if the catalog classifies as Approval, require_approval=false has no effect
    let catalog = crate::registry::catalog();
    let catalog_classification = catalog
        .iter()
        .find(|t| t.name() == name.as_str())
        .map(|t| t.classification());

    let effective_require = match catalog_classification {
        Some(crate::model::Classification::Approval) => true,
        _ => payload.require_approval,
    };

    if let Err(e) = crate::queries::upsert_policy(
        &pool,
        ctx.tenant_id,
        &name,
        payload.enabled,
        effective_require,
        membership_id,
    )
    .await
    {
        tracing::error!(%e, "update_builtin_policy: upsert_policy failed");
        return ApiError::internal_error("Failed to update tool policy")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    crate::audit::record_config_change(
        &pool,
        ctx.tenant_id,
        membership_id,
        "tool.policy.updated",
        serde_json::json!({
            "toolName": name,
            "enabled": payload.enabled,
            "requireApproval": effective_require,
        }),
    )
    .await;

    Json(serde_json::json!({ "status": "updated" })).into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTenantToolBody {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub endpoint_url: String,
    pub credential: Option<String>,
    #[serde(default = "default_classification")]
    pub classification: String,
}

fn default_classification() -> String {
    "approval".into()
}

/// T073: Validate that a tenant-tool name does not collide with built-in or existing tenant tools.
async fn validate_tool_name_unique(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    name: &str,
    exclude_id: Option<Uuid>,
) -> Result<(), String> {
    let catalog = crate::registry::catalog();
    if catalog.iter().any(|t| t.name() == name) {
        return Err(format!(
            "tool name '{}' conflicts with a built-in tool",
            name
        ));
    }

    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM tenant_tools \
         WHERE tenant_id = $1 AND name = $2 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("database error: {e}"))?;

    if let Some(existing_id) = existing {
        if exclude_id != Some(existing_id) {
            return Err(format!(
                "tool name '{}' already exists in this tenant",
                name
            ));
        }
    }

    Ok(())
}

#[utoipa::path(
    post,
    path = "/api/v1/tenant/tools",
    tag = "tools",
    operation_id = "create_tenant_tool",
    summary = "Create a tenant-defined tool",
    description = "Register a new external tool with endpoint URL and optional credential. The credential is write-only and never returned.",
    responses(
        (status = 200, description = "Tool created", body = serde_json::Value),
        (status = 422, description = "Validation failed", body = kernel::ErrorEnvelope),
        (status = 500, description = "Internal error", body = kernel::ErrorEnvelope),
    ),
)]
pub async fn create_tenant_tool(
    State(pool): State<PgPool>,
    ctx: tenancy::TenantContext,
    Extension(principal): Extension<identity::Principal>,
    kernel::ApiJson(payload): kernel::ApiJson<CreateTenantToolBody>,
) -> Response {
    let membership_id =
        match resolve_membership(&pool, ctx.tenant_id, principal.user_id, &ctx.request_id).await {
            Ok(mid) => mid,
            Err(resp) => return resp,
        };

    if !payload.input_schema.is_object() || payload.input_schema.get("type").is_none() {
        return ApiError::unprocessable_entity(
            "inputSchema must be a JSON object with a 'type' field",
        )
        .with_request_id(&ctx.request_id)
        .into_response();
    }

    if !payload.endpoint_url.starts_with("https://") {
        return ApiError::unprocessable_entity("endpointUrl must use https scheme")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    // T071: Hardened SSRF guard at registration time
    if let Err(e) = crate::executor::url_parse_and_validate(&payload.endpoint_url).await {
        return ApiError::unprocessable_entity(&e)
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    // T073: Name-collision validation
    if let Err(msg) = validate_tool_name_unique(&pool, ctx.tenant_id, &payload.name, None).await {
        return ApiError::unprocessable_entity(&msg)
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let credential_ciphertext = seal_credential_if_provided(
        &payload.credential,
        ctx.tenant_id,
        &payload.name,
        &ctx.request_id,
    )
    .await;

    let credential_ciphertext = match credential_ciphertext {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let new = crate::queries::NewTenantTool {
        tenant_id: ctx.tenant_id,
        name: payload.name,
        description: payload.description,
        input_schema: payload.input_schema,
        endpoint_url: payload.endpoint_url,
        credential_ciphertext,
        classification: payload.classification,
        created_by_membership_id: membership_id,
    };

    let id = match crate::queries::insert_tenant_tool(&pool, new).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!(%e, "create_tenant_tool: insert failed");
            return ApiError::internal_error("Failed to create tool")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    crate::audit::record_config_change(
        &pool,
        ctx.tenant_id,
        membership_id,
        "tool.created",
        serde_json::json!({
            "toolId": id,
        }),
    )
    .await;

    Json(serde_json::json!({ "id": id })).into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTenantToolBody {
    pub name: Option<String>,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
    pub endpoint_url: Option<String>,
    pub credential: Option<String>,
    pub classification: Option<String>,
    pub enabled: Option<bool>,
}

#[utoipa::path(
    put,
    path = "/api/v1/tenant/tools/{id}",
    tag = "tools",
    operation_id = "update_tenant_tool",
    summary = "Update a tenant-defined tool",
    description = "Update properties of an existing tenant-defined tool. Credential is write-only.",
    params(
        ("id" = Uuid, Path, description = "Tool ID"),
    ),
    responses(
        (status = 200, description = "Tool updated"),
        (status = 404, description = "Tool not found", body = kernel::ErrorEnvelope),
        (status = 422, description = "Validation failed", body = kernel::ErrorEnvelope),
        (status = 500, description = "Internal error", body = kernel::ErrorEnvelope),
    ),
)]
pub async fn update_tenant_tool_route(
    State(pool): State<PgPool>,
    ctx: tenancy::TenantContext,
    Extension(principal): Extension<identity::Principal>,
    Path(id): Path<Uuid>,
    kernel::ApiJson(payload): kernel::ApiJson<UpdateTenantToolBody>,
) -> Response {
    let membership_id =
        match resolve_membership(&pool, ctx.tenant_id, principal.user_id, &ctx.request_id).await {
            Ok(mid) => mid,
            Err(resp) => return resp,
        };

    let credential_ciphertext = seal_credential_if_provided(
        &payload.credential,
        ctx.tenant_id,
        payload.name.as_deref().unwrap_or("unknown"),
        &ctx.request_id,
    )
    .await;

    let credential_ciphertext = match credential_ciphertext {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    if let Some(ref schema) = payload.input_schema {
        if !schema.is_object() || schema.get("type").is_none() {
            return ApiError::unprocessable_entity(
                "inputSchema must be a JSON object with a 'type' field",
            )
            .with_request_id(&ctx.request_id)
            .into_response();
        }
    }

    if let Some(ref url) = payload.endpoint_url {
        if !url.starts_with("https://") {
            return ApiError::unprocessable_entity("endpointUrl must use https scheme")
                .with_request_id(&ctx.request_id)
                .into_response();
        }

        // T071: Hardened SSRF guard at registration time
        if let Err(e) = crate::executor::url_parse_and_validate(url).await {
            return ApiError::unprocessable_entity(&e)
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    // T073: Name-collision validation (if name is being updated)
    if let Some(ref name) = payload.name {
        if let Err(msg) = validate_tool_name_unique(&pool, ctx.tenant_id, name, Some(id)).await {
            return ApiError::unprocessable_entity(&msg)
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    let patch = crate::queries::TenantToolPatch {
        name: payload.name,
        description: payload.description,
        input_schema: payload.input_schema,
        endpoint_url: payload.endpoint_url,
        credential_ciphertext,
        classification: payload.classification,
        enabled: payload.enabled,
    };

    match crate::queries::update_tenant_tool(&pool, ctx.tenant_id, id, patch).await {
        Ok(true) => {
            crate::audit::record_config_change(
                &pool,
                ctx.tenant_id,
                membership_id,
                "tool.updated",
                serde_json::json!({
                    "toolId": id,
                }),
            )
            .await;

            Json(serde_json::json!({ "status": "updated" })).into_response()
        }
        Ok(false) => ApiError::not_found("Tool not found")
            .with_request_id(&ctx.request_id)
            .into_response(),
        Err(e) => {
            tracing::error!(%e, "update_tenant_tool failed");
            ApiError::internal_error("Failed to update tool")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/tenant/tools/{id}",
    tag = "tools",
    operation_id = "delete_tenant_tool",
    summary = "Soft-delete a tenant-defined tool",
    description = "Soft-deletes a tenant-defined tool. Existing tool_requests are unaffected.",
    params(
        ("id" = Uuid, Path, description = "Tool ID"),
    ),
    responses(
        (status = 200, description = "Tool deleted"),
        (status = 404, description = "Tool not found", body = kernel::ErrorEnvelope),
        (status = 500, description = "Internal error", body = kernel::ErrorEnvelope),
    ),
)]
pub async fn delete_tenant_tool_route(
    State(pool): State<PgPool>,
    ctx: tenancy::TenantContext,
    Extension(principal): Extension<identity::Principal>,
    Path(id): Path<Uuid>,
) -> Response {
    let membership_id =
        match resolve_membership(&pool, ctx.tenant_id, principal.user_id, &ctx.request_id).await {
            Ok(mid) => mid,
            Err(resp) => return resp,
        };

    match crate::queries::soft_delete_tenant_tool(&pool, ctx.tenant_id, id).await {
        Ok(true) => {
            crate::audit::record_config_change(
                &pool,
                ctx.tenant_id,
                membership_id,
                "tool.deleted",
                serde_json::json!({
                    "toolId": id,
                }),
            )
            .await;

            Json(serde_json::json!({ "status": "deleted" })).into_response()
        }
        Ok(false) => ApiError::not_found("Tool not found")
            .with_request_id(&ctx.request_id)
            .into_response(),
        Err(e) => {
            tracing::error!(%e, "delete_tenant_tool failed");
            ApiError::internal_error("Failed to delete tool")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
    }
}

/// Shared helper: seal credential if provided, return Ok(None) if not provided.
/// Stubbed until US4 activation — the encryption key is not available in
/// the tools route handler scope.
async fn seal_credential_if_provided(
    _credential: &Option<String>,
    _tenant_id: Uuid,
    _tool_name: &str,
    request_id: &str,
) -> Result<Option<String>, Response> {
    if _credential.is_some() {
        Err(
            ApiError::internal_error("Credential sealing not available in route handler")
                .with_request_id(request_id)
                .into_response(),
        )
    } else {
        Ok(None)
    }
}
