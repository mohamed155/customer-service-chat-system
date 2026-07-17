use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Extension;
use axum::Json;
use chrono::{DateTime, Utc};
use identity::Principal;
use kernel::{ApiError, ApiJson, ErrorEnvelope};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, ToSchema};

use std::time::Instant;

use tenancy::TenantContext;

use crate::agent_audit;
use crate::prompt_store::{self, SaveError, SaveOutcome};
use crate::prompt_validate;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptSummaryDto {
    pub exists: bool,
    pub active_version: i32,
    pub content: String,
    pub updated_at: Option<DateTime<Utc>>,
    pub updated_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VariableDto {
    pub name: String,
    pub description: String,
    pub sample: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LimitsDto {
    pub max_content_length: u32,
    pub max_change_note_length: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptBootstrapResponse {
    pub prompt: PromptSummaryDto,
    pub variables: Vec<VariableDto>,
    pub limits: LimitsDto,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptSavePayload {
    pub content: String,
    pub change_note: Option<String>,
    pub base_version: i32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptSaveResponse {
    pub version: i32,
    pub created: bool,
    pub restored_from: Option<i32>,
    pub updated_at: Option<DateTime<Utc>>,
    pub updated_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptVersionListItemDto {
    pub version_number: i32,
    pub content_preview: String,
    pub change_note: Option<String>,
    pub restored_from: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptVersionListResponse {
    pub items: Vec<PromptVersionListItemDto>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptVersionDetailResponse {
    pub version_number: i32,
    pub content: String,
    pub change_note: Option<String>,
    pub restored_from: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestorePayload {
    pub base_version: i32,
}

#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(default)]
pub struct ListVersionsQuery {
    pub limit: Option<i64>,
    pub before: Option<i32>,
}

/// `GET /tenant/ai/agent/prompt` — editor bootstrap: active prompt,
/// variables catalog, limits.
#[utoipa::path(
    get,
    path = "/tenant/ai/agent/prompt",
    tag = "tenant-ai",
    operation_id = "get_prompt_bootstrap",
    summary = "Get prompt editor bootstrap data",
    description = "Return the active prompt (or the starter default), the \
                   variables catalog, and validation limits for the prompt \
                   editor. Requires permission: ai_agent.view",
    responses(
        (status = 200, description = "Prompt bootstrap data.", body = PromptBootstrapResponse),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_prompt_bootstrap(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
) -> Response {
    match prompt_store::load_bootstrap(&pool, ctx.tenant_id).await {
        Ok(Some((prompt_row, version_row))) => {
            let response = PromptBootstrapResponse {
                prompt: PromptSummaryDto {
                    exists: true,
                    active_version: prompt_row.active_version,
                    content: version_row.content,
                    updated_at: Some(version_row.created_at),
                    updated_by: Some(version_row.created_by_display),
                },
                variables: build_variables(),
                limits: build_limits(),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Ok(None) => {
            let response = PromptBootstrapResponse {
                prompt: PromptSummaryDto {
                    exists: false,
                    active_version: 0,
                    content: prompt_validate::STARTER_PROMPT.to_string(),
                    updated_at: None,
                    updated_by: None,
                },
                variables: build_variables(),
                limits: build_limits(),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            tracing::error!(%e, "get_prompt_bootstrap: load failed");
            ApiError::internal_error("Failed to load prompt data")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
    }
}

fn build_variables() -> Vec<VariableDto> {
    prompt_validate::VARIABLES
        .iter()
        .map(|v| VariableDto {
            name: v.name.to_string(),
            description: v.description.to_string(),
            sample: v.sample.to_string(),
        })
        .collect()
}

fn build_limits() -> LimitsDto {
    LimitsDto {
        max_content_length: prompt_validate::MAX_CONTENT_LENGTH as u32,
        max_change_note_length: prompt_validate::MAX_CHANGE_NOTE_LENGTH as u32,
    }
}

/// `PUT /tenant/ai/agent/prompt` — save a new prompt version.
#[utoipa::path(
    put,
    path = "/tenant/ai/agent/prompt",
    tag = "tenant-ai",
    operation_id = "put_prompt",
    summary = "Save a new prompt version",
    description = "Validate and save a new system prompt version. If the \
                   content is byte-equal to the current active version the \
                   response carries `created: false` and no new version is \
                   created. Stale `baseVersion` produces 409 conflict. \
                   Validation failures produce 422. \
                   Requires permission: ai_agent.manage",
    request_body = PromptSavePayload,
    responses(
        (status = 200, description = "Prompt version created or no-op.", body = PromptSaveResponse),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 409, description = "Version conflict.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn put_prompt(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    ApiJson(payload): ApiJson<PromptSavePayload>,
) -> Response {
    let start = Instant::now();
    if let Err(issues) = prompt_validate::validate_prompt(&payload.content) {
        let details: Vec<serde_json::Value> = issues
            .iter()
            .map(|i| serde_json::to_value(i).unwrap())
            .collect();
        return ApiError::unprocessable_entity("Validation failed")
            .with_details(details)
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Some(ref note) = payload.change_note {
        if let Err(issues) = prompt_validate::validate_change_note(note) {
            let details: Vec<serde_json::Value> = issues
                .iter()
                .map(|i| serde_json::to_value(i).unwrap())
                .collect();
            return ApiError::unprocessable_entity("Validation failed")
                .with_details(details)
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "put_prompt: begin tx failed");
            return ApiError::internal_error("Failed to save prompt")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    match prompt_store::save_version_in_tx(
        &mut tx,
        ctx.tenant_id,
        payload.base_version,
        &payload.content,
        payload.change_note.as_deref(),
        Some(principal.user_id),
        &principal.display_name,
        None,
    )
    .await
    {
        Ok(SaveOutcome::Created { version, prompt_id }) => {
            if let Err(e) = agent_audit::record_agent_prompt_version_created(
                &mut tx,
                Some(principal.user_id),
                ctx.tenant_id,
                prompt_id,
                version,
                payload.content.chars().count(),
                payload.change_note.is_some(),
            )
            .await
            {
                tracing::error!(%e, "put_prompt: audit failed");
                let _ = tx.rollback().await;
                return ApiError::internal_error("Failed to save prompt")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }

            if let Err(e) = tx.commit().await {
                tracing::error!(%e, "put_prompt: commit failed");
                return ApiError::internal_error("Failed to save prompt")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }

            tracing::info!(
                action = "agent_prompt.version_created",
                version = version,
                content_length = payload.content.chars().count(),
                has_change_note = payload.change_note.is_some(),
                elapsed_ms = start.elapsed().as_millis() as u64,
                request_id = %ctx.request_id,
                "put_prompt: saved"
            );

            let response = PromptSaveResponse {
                version,
                created: true,
                restored_from: None,
                updated_at: Some(Utc::now()),
                updated_by: Some(principal.display_name.clone()),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Ok(SaveOutcome::NoOp { version }) => {
            let _ = tx.rollback().await;
            let response = PromptSaveResponse {
                version,
                created: false,
                restored_from: None,
                updated_at: None,
                updated_by: None,
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(SaveError::Conflict { active_version }) => {
            let _ = tx.rollback().await;
            ApiError::conflict("Prompt changed since it was loaded")
                .with_details(vec![serde_json::json!({"activeVersion": active_version})])
                .with_request_id(&ctx.request_id)
                .into_response()
        }
        Err(SaveError::Db(e)) => {
            tracing::error!(%e, "put_prompt: save failed");
            let _ = tx.rollback().await;
            ApiError::internal_error("Failed to save prompt")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
    }
}

/// `GET /tenant/ai/agent/prompt/versions` — list prompt version history.
#[utoipa::path(
    get,
    path = "/tenant/ai/agent/prompt/versions",
    tag = "tenant-ai",
    operation_id = "list_prompt_versions",
    summary = "List prompt version history",
    description = "Return a paginated list of prompt versions for the \
                   active tenant, newest first. `limit` (1–100, default 25) \
                   and `before` (exclusive version_number cursor) control \
                   pagination. Requires permission: ai_agent.view",
    params(ListVersionsQuery),
    responses(
        (status = 200, description = "Page of prompt versions.", body = PromptVersionListResponse),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn list_prompt_versions(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
    Query(params): Query<ListVersionsQuery>,
) -> Response {
    let limit = params.limit.map(|l| l.clamp(1, 100)).unwrap_or(25);

    let (rows, has_more) =
        match prompt_store::list_versions(&pool, ctx.tenant_id, limit, params.before).await {
            Ok(result) => result,
            Err(e) => {
                tracing::error!(%e, "list_prompt_versions: list failed");
                return ApiError::internal_error("Failed to list versions")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        };

    let active_ver = match prompt_store::active_version_number(&pool, ctx.tenant_id).await {
        Ok(v) => v.unwrap_or(0),
        Err(e) => {
            tracing::error!(%e, "list_prompt_versions: active_version query failed");
            return ApiError::internal_error("Failed to list versions")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let items: Vec<PromptVersionListItemDto> = rows
        .into_iter()
        .map(|row| {
            let preview: String = row
                .content
                .replace('\r', "")
                .replace('\n', " ")
                .chars()
                .take(160)
                .collect();

            PromptVersionListItemDto {
                version_number: row.version_number,
                content_preview: preview,
                change_note: row.change_note,
                restored_from: row.restored_from,
                created_at: row.created_at,
                created_by: row.created_by_display,
                is_active: row.version_number == active_ver,
            }
        })
        .collect();

    (
        StatusCode::OK,
        Json(PromptVersionListResponse { items, has_more }),
    )
        .into_response()
}

/// `GET /tenant/ai/agent/prompt/versions/{number}` — get a specific version.
#[utoipa::path(
    get,
    path = "/tenant/ai/agent/prompt/versions/{number}",
    tag = "tenant-ai",
    operation_id = "get_prompt_version",
    summary = "Get a specific prompt version",
    description = "Return the full content and metadata of a single prompt \
                   version. 404 if the version does not exist or belongs to \
                   a different tenant. Requires permission: ai_agent.view",
    params(
        ("number" = i32, Path, description = "Prompt version number"),
    ),
    responses(
        (status = 200, description = "Prompt version detail.", body = PromptVersionDetailResponse),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Version not found.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_prompt_version(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
    Path(version_number): Path<i32>,
) -> Response {
    match prompt_store::get_version(&pool, ctx.tenant_id, version_number).await {
        Ok(Some((row, is_active))) => {
            let response = PromptVersionDetailResponse {
                version_number: row.version_number,
                content: row.content,
                change_note: row.change_note,
                restored_from: row.restored_from,
                created_at: row.created_at,
                created_by: row.created_by_display,
                is_active,
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Ok(None) => ApiError::not_found("Version not found")
            .with_request_id(&ctx.request_id)
            .into_response(),
        Err(e) => {
            tracing::error!(%e, "get_prompt_version: get_version failed");
            ApiError::internal_error("Failed to load version")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
    }
}

/// `POST /tenant/ai/agent/prompt/versions/{number}/restore` — roll-forward restore.
#[utoipa::path(
    post,
    path = "/tenant/ai/agent/prompt/versions/{number}/restore",
    tag = "tenant-ai",
    operation_id = "restore_prompt_version",
    summary = "Restore a historical prompt version",
    description = "Re-validate a historical version's content and create a \
                   new version with `restoredFrom` pointing to the source. \
                   Same conflict/no-op rules as PUT apply. \
                   Requires permission: ai_agent.manage",
    params(
        ("number" = i32, Path, description = "Source version number to restore from"),
    ),
    request_body = RestorePayload,
    responses(
        (status = 200, description = "Version restored or no-op.", body = PromptSaveResponse),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Version not found.", body = ErrorEnvelope),
        (status = 409, description = "Version conflict.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn restore_prompt_version(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Path(version_number): Path<i32>,
    ApiJson(payload): ApiJson<RestorePayload>,
) -> Response {
    let start = Instant::now();
    let (source_row, _is_active) =
        match prompt_store::get_version(&pool, ctx.tenant_id, version_number).await {
            Ok(Some(row)) => row,
            Ok(None) => {
                return ApiError::not_found("Version not found")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
            Err(e) => {
                tracing::error!(%e, "restore_prompt_version: get_version failed");
                return ApiError::internal_error("Failed to restore prompt")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        };

    if let Err(issues) = prompt_validate::validate_prompt(&source_row.content) {
        let details: Vec<serde_json::Value> = issues
            .iter()
            .map(|i| serde_json::to_value(i).unwrap())
            .collect();
        return ApiError::unprocessable_entity("Validation failed")
            .with_details(details)
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "restore_prompt_version: begin tx failed");
            return ApiError::internal_error("Failed to restore prompt")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let change_note = format!("Restored from v{}", version_number);

    match prompt_store::save_version_in_tx(
        &mut tx,
        ctx.tenant_id,
        payload.base_version,
        &source_row.content,
        Some(&change_note),
        Some(principal.user_id),
        &principal.display_name,
        Some(version_number),
    )
    .await
    {
        Ok(SaveOutcome::Created { version, prompt_id }) => {
            if let Err(e) = agent_audit::record_agent_prompt_version_restored(
                &mut tx,
                Some(principal.user_id),
                ctx.tenant_id,
                prompt_id,
                version,
                version_number,
                source_row.content.chars().count(),
            )
            .await
            {
                tracing::error!(%e, "restore_prompt_version: audit failed");
                let _ = tx.rollback().await;
                return ApiError::internal_error("Failed to restore prompt")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }

            if let Err(e) = tx.commit().await {
                tracing::error!(%e, "restore_prompt_version: commit failed");
                return ApiError::internal_error("Failed to restore prompt")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }

            tracing::info!(
                action = "agent_prompt.version_restored",
                version = version,
                restored_from = version_number,
                content_length = source_row.content.chars().count(),
                elapsed_ms = start.elapsed().as_millis() as u64,
                request_id = %ctx.request_id,
                "restore_prompt_version: saved"
            );

            let response = PromptSaveResponse {
                version,
                created: true,
                restored_from: Some(version_number),
                updated_at: Some(Utc::now()),
                updated_by: Some(principal.display_name.clone()),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Ok(SaveOutcome::NoOp { version }) => {
            let _ = tx.rollback().await;
            let response = PromptSaveResponse {
                version,
                created: false,
                restored_from: None,
                updated_at: None,
                updated_by: None,
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(SaveError::Conflict { active_version }) => {
            let _ = tx.rollback().await;
            ApiError::conflict("Prompt changed since it was loaded")
                .with_details(vec![serde_json::json!({"activeVersion": active_version})])
                .with_request_id(&ctx.request_id)
                .into_response()
        }
        Err(SaveError::Db(e)) => {
            tracing::error!(%e, "restore_prompt_version: save failed");
            let _ = tx.rollback().await;
            ApiError::internal_error("Failed to restore prompt")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
    }
}
