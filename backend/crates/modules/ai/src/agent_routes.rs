use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Extension;
use axum::Json;
use chrono::{DateTime, Utc};
use identity::Principal;
use kernel::{ApiError, ApiJson, ErrorEnvelope};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

use tenancy::TenantContext;

use crate::agent_audit;
use crate::agent_config;
use crate::agent_config::AgentConfigurationRow;
use crate::agent_config::{
    validate_payload, AgentConfigPayload, EscalationRule, EscalationTrigger,
};
use crate::prompt_store;

// ── Agent Options types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentOptionsResponse {
    pub tones: Vec<&'static str>,
    pub channels: Vec<&'static str>,
    pub avatar_presets: &'static [&'static str],
    pub providers: Vec<ProviderOption>,
    pub ai_layer_default: Option<AiLayerDefaultInfo>,
    pub prompt_max_length: u32,
    pub limits: LimitsInfo,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderOption {
    pub provider: &'static str,
    pub credential_available: bool,
    pub models: &'static [&'static str],
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiLayerDefaultInfo {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LimitsInfo {
    pub business_rules_max: u32,
    pub escalation_rules_max: u32,
}

// ── Response types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigResponse {
    pub configured: bool,
    pub agent: AgentDetail,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActivePromptSummary {
    pub version: i32,
    pub updated_at: DateTime<Utc>,
    pub updated_by: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentDetail {
    pub id: Option<Uuid>,
    pub name: String,
    pub is_default: bool,
    pub avatar: AvatarInfo,
    pub tone: String,
    pub business_rules: Vec<String>,
    pub escalation_rules: Vec<EscalationRuleDetail>,
    pub enabled_channels: Vec<String>,
    pub provider_selection: ProviderSelectionInfo,
    pub version: Option<i32>,
    pub updated_at: Option<DateTime<Utc>>,
    pub active_prompt: Option<ActivePromptSummary>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AvatarInfo {
    pub kind: String,
    pub preset: Option<String>,
    pub upload_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EscalationRuleDetail {
    pub id: Uuid,
    pub name: String,
    pub trigger: EscalationTrigger,
    pub keywords: Vec<String>,
    pub required_skill_ids: Vec<Uuid>,
    pub broken_skill_refs: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSelectionInfo {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AvatarUpdateResponse {
    pub avatar: AvatarInfo,
    pub version: i32,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn default_agent_config() -> AgentConfigResponse {
    AgentConfigResponse {
        configured: false,
        agent: AgentDetail {
            id: None,
            name: "AI Assistant".into(),
            is_default: true,
            avatar: AvatarInfo {
                kind: "preset".into(),
                preset: Some("spark".into()),
                upload_url: None,
            },
            tone: "professional".into(),
            business_rules: Vec::new(),
            escalation_rules: Vec::new(),
            enabled_channels: vec!["web_chat".into()],
            provider_selection: ProviderSelectionInfo {
                provider: None,
                model: None,
                stale: false,
            },
            version: None,
            updated_at: None,
            active_prompt: None,
        },
    }
}

async fn build_agent_response(
    pool: &PgPool,
    tenant_id: Uuid,
    row: &AgentConfigurationRow,
) -> AgentConfigResponse {
    let business_rules: Vec<String> =
        serde_json::from_value(row.business_rules.clone()).unwrap_or_default();
    let escalation_rules: Vec<EscalationRule> =
        serde_json::from_value(row.escalation_rules.clone()).unwrap_or_default();
    let enabled_channels: Vec<String> =
        serde_json::from_value(row.enabled_channels.clone()).unwrap_or_default();

    // Compute broken_skill_refs for each escalation rule
    let all_skill_ids: Vec<Uuid> = escalation_rules
        .iter()
        .flat_map(|r| r.required_skill_ids.iter())
        .copied()
        .collect();
    let live_ids = agent_config::live_skill_ids(pool, tenant_id, &all_skill_ids)
        .await
        .unwrap_or_default();

    let escalation_rule_details: Vec<EscalationRuleDetail> = escalation_rules
        .into_iter()
        .map(|rule| {
            let broken_skill_refs: Vec<Uuid> = rule
                .required_skill_ids
                .iter()
                .filter(|id| !live_ids.contains(id))
                .copied()
                .collect();
            EscalationRuleDetail {
                id: rule.id,
                name: rule.name,
                trigger: rule.trigger,
                keywords: rule.keywords,
                required_skill_ids: rule.required_skill_ids,
                broken_skill_refs,
            }
        })
        .collect();

    // Compute provider staleness
    let (provider, model, stale) = match (&row.provider, &row.model) {
        (Some(prov), Some(modl)) => {
            let is_stale = !agent_config::credential_resolves(pool, tenant_id, prov).await;
            (Some(prov.clone()), Some(modl.clone()), is_stale)
        }
        _ => (None, None, false),
    };

    let upload_url = if row.avatar_kind == "upload" {
        Some("/tenant/ai/agent/avatar".into())
    } else {
        None
    };

    let active_prompt = match prompt_store::load_bootstrap(pool, tenant_id).await {
        Ok(Some((p_row, v_row))) => {
            let excerpt: String = v_row
                .content
                .chars()
                .take(120)
                .collect::<String>()
                .lines()
                .next()
                .unwrap_or("")
                .to_string()
                .replace('\n', " ");
            Some(ActivePromptSummary {
                version: p_row.active_version,
                updated_at: v_row.created_at,
                updated_by: v_row.created_by_display,
                excerpt,
            })
        }
        _ => None,
    };

    AgentConfigResponse {
        configured: true,
        agent: AgentDetail {
            id: Some(row.id),
            name: row.name.clone(),
            is_default: row.is_default,
            avatar: AvatarInfo {
                kind: row.avatar_kind.clone(),
                preset: row.avatar_preset.clone(),
                upload_url,
            },
            tone: row.tone.clone(),
            business_rules,
            escalation_rules: escalation_rule_details,
            enabled_channels,
            provider_selection: ProviderSelectionInfo {
                provider,
                model,
                stale,
            },
            version: Some(row.version),
            updated_at: Some(row.updated_at),
            active_prompt,
        },
    }
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// `GET /tenant/ai/agent` — returns the tenant's agent configuration or
/// platform defaults when never configured.
#[utoipa::path(
    get,
    path = "/tenant/ai/agent",
    tag = "tenant-ai",
    operation_id = "get_agent_config",
    summary = "Get tenant agent configuration",
    description = "Return the tenant's AI agent configuration with computed fields \
                  (broken_skill_refs per escalation rule, provider staleness). When \
                  the tenant has never saved a configuration the response carries \
                  configured=false with editable platform defaults. \
                  Requires permission: ai_agent.view",
    responses(
        (status = 200, description = "Agent configuration.", body = AgentConfigResponse),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_agent_config(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
) -> Response {
    match agent_config::load_live(&pool, ctx.tenant_id).await {
        Ok(Some(row)) => {
            let response = build_agent_response(&pool, ctx.tenant_id, &row).await;
            (StatusCode::OK, Json(response)).into_response()
        }
        Ok(None) => {
            let response = default_agent_config();
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            tracing::error!(%e, "get_agent_config: load failed");
            ApiError::internal_error("Failed to load agent configuration")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
    }
}

/// `GET /tenant/ai/agent/options` — return available tones, channels, avatar
/// presets, and provider options with credential availability.
#[utoipa::path(
    get,
    path = "/tenant/ai/agent/options",
    tag = "tenant-ai",
    operation_id = "get_agent_options",
    summary = "Get agent configuration options",
    description = "Return available tones, channels, avatar presets, provider \
                   options with credential availability, and AI layer defaults. \
                   Requires permission: ai_agent.view",
    responses(
        (status = 200, description = "Agent options.", body = AgentOptionsResponse),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_agent_options(State(pool): State<PgPool>, ctx: TenantContext) -> Response {
    use crate::agent_config::CURATED_MODELS;
    use crate::agent_config::PROVIDER_CATALOG;
    use crate::resolution::{resolve_config, Scope};

    let tones: Vec<&'static str> = agent_config::TONES.to_vec();
    let channels: Vec<&'static str> = agent_config::CATALOG_CHANNELS.to_vec();

    let mut providers = Vec::with_capacity(PROVIDER_CATALOG.len());
    for prov in &PROVIDER_CATALOG {
        let credential_available =
            agent_config::credential_resolves(&pool, ctx.tenant_id, prov).await;
        let models = CURATED_MODELS
            .iter()
            .find(|(name, _)| name == prov)
            .map(|(_, models)| *models)
            .unwrap_or(&[]);
        providers.push(ProviderOption {
            provider: prov,
            credential_available,
            models,
        });
    }

    let ai_layer_default = match resolve_config(&pool, Scope::Tenant(ctx.tenant_id)).await {
        Ok(Some(resolved)) => Some(AiLayerDefaultInfo {
            provider: resolved.row.provider,
            model: resolved.row.model,
        }),
        _ => None,
    };

    let response = AgentOptionsResponse {
        tones,
        channels,
        avatar_presets: agent_config::AVATAR_PRESETS,
        providers,
        ai_layer_default,
        prompt_max_length: 8000,
        limits: LimitsInfo {
            business_rules_max: 20,
            escalation_rules_max: 20,
        },
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// `PUT /tenant/ai/agent` — create or update the tenant's agent configuration.
#[utoipa::path(
    put,
    path = "/tenant/ai/agent",
    tag = "tenant-ai",
    operation_id = "put_agent_config",
    summary = "Create or update tenant agent configuration",
    description = "Full-replace upsert of the tenant's agent configuration. \
                  First save creates the row (201); later saves update (200). \
                  Version must match the live row when the agent already exists, \
                  else 409. Validation failures produce 422 with per-field details. \
                  Requires permission: ai_agent.manage",
    request_body = AgentConfigPayload,
    responses(
        (status = 200, description = "Agent configuration updated.", body = AgentConfigResponse),
        (status = 201, description = "Agent configuration created.", body = AgentConfigResponse),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 409, description = "Version conflict.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (e.g. bad tone, unknown channel).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn put_agent_config(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    ApiJson(payload): ApiJson<AgentConfigPayload>,
) -> Response {
    if let Err(issues) = validate_payload(&payload) {
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
            tracing::error!(%e, "put_agent_config: begin tx failed");
            return ApiError::internal_error("Failed to save agent configuration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let existing = match agent_config::load_live_in_tx(&mut tx, ctx.tenant_id).await {
        Ok(row) => row,
        Err(e) => {
            tracing::error!(%e, "put_agent_config: load failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to save agent configuration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let (created, updated_row) = match existing {
        None => {
            if payload.version.is_some() {
                let _ = tx.rollback().await;
                return ApiError::conflict(
                    "Agent configuration does not exist yet; version must be null",
                )
                .with_request_id(&ctx.request_id)
                .into_response();
            }

            if payload.avatar.kind == "upload" {
                let _ = tx.rollback().await;
                return ApiError::unprocessable_entity(
                    "Avatar kind 'upload' requires an existing agent with an uploaded avatar",
                )
                .with_request_id(&ctx.request_id)
                .into_response();
            }

            let row = match agent_config::create_in_tx(&mut tx, ctx.tenant_id, &payload).await {
                Ok(r) => r,
                Err(e) => {
                    if let Some(db_err) = e.as_database_error() {
                        if db_err.is_unique_violation() {
                            let _ = tx.rollback().await;
                            return ApiError::conflict(
                                "Agent configuration was created concurrently",
                            )
                            .with_request_id(&ctx.request_id)
                            .into_response();
                        }
                    }
                    tracing::error!(%e, "put_agent_config: create failed");
                    let _ = tx.rollback().await;
                    return ApiError::internal_error("Failed to save agent configuration")
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
            };

            let details = serde_json::to_value(&payload).unwrap_or_default();
            if let Err(e) = agent_audit::record_agent_config_created(
                &mut tx,
                Some(principal.user_id),
                ctx.tenant_id,
                row.id,
                &details,
            )
            .await
            {
                tracing::error!(%e, "put_agent_config: audit created failed");
                let _ = tx.rollback().await;
                return ApiError::internal_error("Failed to save agent configuration")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }

            (true, row)
        }
        Some(existing_row) => {
            let expected_version = match payload.version {
                Some(v) => v,
                None => {
                    let _ = tx.rollback().await;
                    return ApiError::conflict(
                        "Version is required when updating an existing agent configuration",
                    )
                    .with_request_id(&ctx.request_id)
                    .into_response();
                }
            };

            if expected_version != existing_row.version {
                let _ = tx.rollback().await;
                return ApiError::conflict("Configuration changed since it was loaded")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }

            // Validate avatar kind
            if payload.avatar.kind == "upload" {
                let has_upload = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM agent_avatar_uploads WHERE agent_id = $1 AND deleted_at IS NULL)",
                )
                .bind(existing_row.id)
                .fetch_one(&mut *tx)
                .await
                .unwrap_or(false);
                if !has_upload {
                    let _ = tx.rollback().await;
                    return ApiError::unprocessable_entity(
                        "No uploaded avatar exists; set avatar.kind to 'preset' or upload an avatar first",
                    )
                    .with_request_id(&ctx.request_id)
                    .into_response();
                }
            }

            // Validate skill IDs exist
            if !payload.escalation_rules.is_empty() {
                let all_skill_ids: Vec<Uuid> = payload
                    .escalation_rules
                    .iter()
                    .flat_map(|r| r.required_skill_ids.iter())
                    .copied()
                    .collect();
                let live_ids = agent_config::live_skill_ids(&pool, ctx.tenant_id, &all_skill_ids)
                    .await
                    .unwrap_or_default();
                for rule in &payload.escalation_rules {
                    let missing: Vec<&Uuid> = rule
                        .required_skill_ids
                        .iter()
                        .filter(|id| !live_ids.contains(id))
                        .collect();
                    if !missing.is_empty() {
                        let _ = tx.rollback().await;
                        return ApiError::unprocessable_entity(format!(
                            "Rule '{}' references non-existent skill IDs",
                            rule.name
                        ))
                        .with_request_id(&ctx.request_id)
                        .into_response();
                    }
                }
            }

            // Validate provider credential resolves
            if let Some(ref sel) = payload.provider_selection {
                if !agent_config::credential_resolves(&pool, ctx.tenant_id, &sel.provider).await {
                    let _ = tx.rollback().await;
                    return ApiError::unprocessable_entity(format!(
                        "Provider '{}' has no resolvable credential",
                        sel.provider
                    ))
                    .with_request_id(&ctx.request_id)
                    .into_response();
                }
            }

            let row = match agent_config::update_in_tx(
                &mut tx,
                ctx.tenant_id,
                existing_row.id,
                expected_version,
                &payload,
            )
            .await
            {
                Ok(Some(r)) => r,
                Ok(None) => {
                    let _ = tx.rollback().await;
                    return ApiError::conflict("Configuration changed since it was loaded")
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
                Err(e) => {
                    tracing::error!(%e, "put_agent_config: update failed");
                    let _ = tx.rollback().await;
                    return ApiError::internal_error("Failed to save agent configuration")
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
            };

            let changed_fields: &[&str] = &[
                "name",
                "avatar",
                "tone",
                "business_rules",
                "escalation_rules",
                "enabled_channels",
                "provider_selection",
            ];
            if let Err(e) = agent_audit::record_agent_config_updated(
                &mut tx,
                Some(principal.user_id),
                ctx.tenant_id,
                row.id,
                changed_fields,
            )
            .await
            {
                tracing::error!(%e, "put_agent_config: audit updated failed");
                let _ = tx.rollback().await;
                return ApiError::internal_error("Failed to save agent configuration")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }

            (false, row)
        }
    };

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "put_agent_config: commit failed");
        return ApiError::internal_error("Failed to save agent configuration")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let response = build_agent_response(&pool, ctx.tenant_id, &updated_row).await;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    (status, Json(response)).into_response()
}

/// `PUT /tenant/ai/agent/avatar` — upload a custom avatar image.
#[utoipa::path(
    put,
    path = "/tenant/ai/agent/avatar",
    tag = "tenant-ai",
    operation_id = "put_agent_avatar",
    summary = "Upload agent avatar",
    description = "Upload a custom avatar image for the tenant's agent. \
                   Content-Type must be image/png, image/jpeg, or image/webp. \
                   Body must be ≤256 KB. Requires the agent to exist (404 otherwise). \
                   Requires permission: ai_agent.manage",
    request_body(content = Vec::<u8>, description = "Raw image bytes", content_type = "image/png"),
    responses(
        (status = 200, description = "Avatar updated.", body = AvatarUpdateResponse),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Agent configuration not found.", body = ErrorEnvelope),
        (status = 413, description = "Avatar exceeds 256 KB.", body = ErrorEnvelope),
        (status = 422, description = "Invalid content type.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn put_agent_avatar(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let agent_id = match agent_config::load_live(&pool, ctx.tenant_id).await {
        Ok(Some(row)) => row.id,
        Ok(None) => {
            return ApiError::not_found("Agent configuration not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "put_agent_avatar: load failed");
            return ApiError::internal_error("Failed to load agent configuration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    // Validate content type
    let content_type = match headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    {
        Some(ct) if ct == "image/png" || ct == "image/jpeg" || ct == "image/webp" => ct.to_owned(),
        Some(_) => {
            return ApiError::unprocessable_entity(
                "Content-Type must be image/png, image/jpeg, or image/webp",
            )
            .with_request_id(&ctx.request_id)
            .into_response();
        }
        None => {
            return ApiError::unprocessable_entity("Content-Type header is required")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    // Validate size (256 KB = 262144 bytes)
    if body.len() > 262_144 {
        return ApiError::unprocessable_entity("Avatar must not exceed 256 KB")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "put_agent_avatar: begin tx failed");
            return ApiError::internal_error("Failed to upload avatar")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    // Soft-delete prior live upload
    if let Err(e) = sqlx::query(
        "UPDATE agent_avatar_uploads SET deleted_at = now() \
         WHERE agent_id = $1 AND deleted_at IS NULL",
    )
    .bind(agent_id)
    .execute(&mut *tx)
    .await
    {
        tracing::error!(%e, "put_agent_avatar: soft-delete prior failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to upload avatar")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    // Insert new upload
    if let Err(e) = sqlx::query(
        "INSERT INTO agent_avatar_uploads (tenant_id, agent_id, content_type, bytes) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(ctx.tenant_id)
    .bind(agent_id)
    .bind(&content_type)
    .bind(&body[..])
    .execute(&mut *tx)
    .await
    {
        tracing::error!(%e, "put_agent_avatar: insert failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to upload avatar")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    // Bump version and set avatar_kind
    let new_version: i32 = match sqlx::query_scalar(
        "UPDATE agent_configurations \
         SET avatar_kind = 'upload', version = version + 1 \
         WHERE id = $1 AND deleted_at IS NULL \
         RETURNING version",
    )
    .bind(agent_id)
    .fetch_one(&mut *tx)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(%e, "put_agent_avatar: version bump failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to upload avatar")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    // Audit
    if let Err(e) = agent_audit::record_agent_config_avatar_updated(
        &mut tx,
        Some(principal.user_id),
        ctx.tenant_id,
        agent_id,
        "upload",
        &content_type,
    )
    .await
    {
        tracing::error!(%e, "put_agent_avatar: audit failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to upload avatar")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "put_agent_avatar: commit failed");
        return ApiError::internal_error("Failed to upload avatar")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let response = AvatarUpdateResponse {
        avatar: AvatarInfo {
            kind: "upload".into(),
            preset: None,
            upload_url: Some("/tenant/ai/agent/avatar".into()),
        },
        version: new_version,
    };
    (StatusCode::OK, Json(response)).into_response()
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiHandlingPayload {
    pub mode: String,
}

/// `POST /tenant/conversations/{id}/ai-handling` — set AI handling for a
/// conversation that has no configured agent (US6, FR-004c).
///
/// When `mode = "platform_ai"` the conversation is marked for platform AI
/// handling (credential must resolve).  When `mode = "human"` the conversation
/// is marked for human handling and an escalation is created immediately.
/// Returns 409 if an agent configuration exists (configured agent supersedes).
#[utoipa::path(
    post,
    path = "/tenant/conversations/{id}/ai-handling",
    tag = "conversations",
    operation_id = "set_conversation_ai_handling",
    summary = "Set AI handling mode for an unconfigured conversation",
    description = "Set the AI handling mode for a conversation that has no \
                  configured AI agent. Mode 'platform_ai' enables platform AI, \
                  mode 'human' escalates to human agents. \
                  Requires permission: conversations.manage",
    params(("id" = Uuid, Path, description = "Conversation identifier")),
    request_body = AiHandlingPayload,
    responses(
        (status = 200, description = "AI handling mode set.", body = serde_json::Value),
        (status = 400, description = "Validation failed.", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Conversation not found.", body = ErrorEnvelope),
        (status = 409, description = "Conflict (agent configured or conversation resolved/closed).", body = ErrorEnvelope),
        (status = 422, description = "Unprocessable entity (credential not resolvable).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn set_conversation_ai_handling(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Extension(presence): Extension<Arc<escalations::presence::Runtime>>,
    Path(conversation_id): Path<Uuid>,
    ApiJson(payload): ApiJson<AiHandlingPayload>,
) -> Response {
    // 1. Check conversation exists and is in a valid state
    let (status, _) = match conversations::queries::conversation_ai_state(
        &pool,
        ctx.tenant_id,
        conversation_id,
    )
    .await
    {
        Ok(Some(state)) => state,
        Ok(None) => {
            return ApiError::not_found("Conversation not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, conversation_id = %conversation_id, "set_ai_handling: conversation_ai_state failed");
            return ApiError::internal_error("Failed to check conversation state")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if matches!(status.as_str(), "resolved" | "closed") {
        return ApiError::conflict("Conversation is resolved or closed")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    // 2. Check agent does not exist (FR-004c)
    match agent_config::agent_exists(&pool, ctx.tenant_id).await {
        Ok(false) => {}
        Ok(true) => {
            return ApiError::conflict(
                "An AI agent is configured; use the agent instead of manual handling",
            )
            .with_request_id(&ctx.request_id)
            .into_response();
        }
        Err(e) => {
            tracing::error!(%e, tenant_id = %ctx.tenant_id, "set_ai_handling: agent_exists failed");
            return ApiError::internal_error("Failed to check agent configuration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    match payload.mode.as_str() {
        "platform_ai" => {
            // 3a. Check config + credential both resolve
            let resolved = crate::resolution::resolve_config(
                &pool,
                crate::resolution::Scope::Tenant(ctx.tenant_id),
            )
            .await;
            let resolved = match resolved {
                Ok(Some(r)) => r,
                Ok(None) => {
                    return ApiError::unprocessable_entity(
                        "No resolvable platform AI configuration",
                    )
                    .with_request_id(&ctx.request_id)
                    .into_response();
                }
                Err(e) => {
                    tracing::error!(%e, tenant_id = %ctx.tenant_id, "set_ai_handling: resolve_config failed");
                    return ApiError::internal_error("Failed to resolve AI configuration")
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
            };
            if !agent_config::credential_resolves(&pool, ctx.tenant_id, &resolved.row.provider)
                .await
            {
                return ApiError::unprocessable_entity(
                    "Platform AI provider has no resolvable credential",
                )
                .with_request_id(&ctx.request_id)
                .into_response();
            }

            let mut tx = match pool.begin().await {
                Ok(tx) => tx,
                Err(e) => {
                    tracing::error!(%e, "set_ai_handling: begin tx failed");
                    return ApiError::internal_error("Failed to set AI handling mode")
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
            };

            match conversations::queries::set_ai_handling_in_tx(
                &mut tx,
                ctx.tenant_id,
                conversation_id,
                "platform_ai",
            )
            .await
            {
                Ok(true) => {}
                Ok(false) => {
                    let _ = tx.rollback().await;
                    return ApiError::conflict("Conversation is already handled by a human")
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
                Err(e) => {
                    tracing::error!(%e, "set_ai_handling: set_ai_handling_in_tx failed");
                    let _ = tx.rollback().await;
                    return ApiError::internal_error("Failed to set AI handling mode")
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
            }

            if let Err(e) = agent_audit::record_ai_handling_set(
                &mut tx,
                Some(principal.user_id),
                ctx.tenant_id,
                conversation_id,
                "platform_ai",
            )
            .await
            {
                tracing::error!(%e, "set_ai_handling: audit failed");
                let _ = tx.rollback().await;
                return ApiError::internal_error("Failed to record audit event")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }

            if let Err(e) = tx.commit().await {
                tracing::error!(%e, "set_ai_handling: commit failed");
                return ApiError::internal_error("Failed to set AI handling mode")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }

            let detail =
                match fetch_conversation_detail(&pool, ctx.tenant_id, conversation_id).await {
                    Ok(Some(d)) => d,
                    Ok(None) => {
                        return ApiError::not_found("Conversation not found after update")
                            .with_request_id(&ctx.request_id)
                            .into_response();
                    }
                    Err(e) => {
                        tracing::error!(%e, "set_ai_handling: detail query failed");
                        return ApiError::internal_error("Failed to load conversation detail")
                            .with_request_id(&ctx.request_id)
                            .into_response();
                    }
                };

            Json(serde_json::json!({ "data": detail })).into_response()
        }
        "human" => {
            let present_ids = presence.present_membership_ids_async(ctx.tenant_id).await;

            let mut tx = match pool.begin().await {
                Ok(tx) => tx,
                Err(e) => {
                    tracing::error!(%e, "set_ai_handling: begin tx failed");
                    return ApiError::internal_error("Failed to set AI handling mode")
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
            };

            match conversations::queries::set_ai_handling_in_tx(
                &mut tx,
                ctx.tenant_id,
                conversation_id,
                "human",
            )
            .await
            {
                Ok(true) => {}
                Ok(false) => {
                    let _ = tx.rollback().await;
                    return ApiError::conflict("Conversation is already handled by a human")
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
                Err(e) => {
                    tracing::error!(%e, "set_ai_handling: set_ai_handling_in_tx failed");
                    let _ = tx.rollback().await;
                    return ApiError::internal_error("Failed to set AI handling mode")
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
            }

            // T034: Cancel any pending tool requests for this conversation
            let pending_tools = tools::approval::fetch_awaiting_approval_for_conversation(
                &pool,
                ctx.tenant_id,
                conversation_id,
            )
            .await
            .unwrap_or_default();
            let cancelled_ids = tools::approval::cancel_pending_for_conversation(
                &mut tx,
                ctx.tenant_id,
                conversation_id,
            )
            .await
            .unwrap_or_default();

            let escalation = match escalations::routing::route_new_escalation_in_tx(
                &mut tx,
                &pool,
                ctx.tenant_id,
                conversation_id,
                crate::agent_rules::UNCONFIGURED_ESCALATION_REASON,
                &[],
                &[],
                &present_ids,
                principal.user_id,
            )
            .await
            {
                Ok(outcome) => match outcome {
                    escalations::routing::RouteOutcome::Assigned { escalation, .. } => escalation,
                    escalations::routing::RouteOutcome::Queued { escalation } => escalation,
                },
                Err(e) => {
                    tracing::error!(%e, conversation_id = %conversation_id, "set_ai_handling: route_new_escalation failed");
                    let _ = tx.rollback().await;
                    return ApiError::internal_error("Failed to create escalation")
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
            };

            if let Err(e) = agent_audit::record_ai_handling_set(
                &mut tx,
                Some(principal.user_id),
                ctx.tenant_id,
                conversation_id,
                "human",
            )
            .await
            {
                tracing::error!(%e, "set_ai_handling: audit failed");
                let _ = tx.rollback().await;
                return ApiError::internal_error("Failed to record audit event")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }

            if let Err(e) = tx.commit().await {
                tracing::error!(%e, "set_ai_handling: commit failed");
                return ApiError::internal_error("Failed to set AI handling mode")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }

            // Broadcast cancelled tool requests
            if !cancelled_ids.is_empty() {
                for tool in &pending_tools {
                    if cancelled_ids.contains(&tool.id) {
                        let updated_ev = escalations::model::ToolRequestUpdated {
                            id: tool.id,
                            conversation_id: tool.conversation_id,
                            status: "cancelled".into(),
                            decided_by_display_name: None,
                            duration_ms: None,
                            has_result: false,
                            error: None,
                        };
                        presence.broadcast(
                            ctx.tenant_id,
                            escalations::presence::Event::ConversationTool(
                                escalations::presence::ConversationToolEvent::Updated(updated_ev),
                            ),
                        );
                    }
                }
            }

            let detail =
                match fetch_conversation_detail(&pool, ctx.tenant_id, conversation_id).await {
                    Ok(Some(d)) => d,
                    Ok(None) => {
                        return ApiError::not_found("Conversation not found after update")
                            .with_request_id(&ctx.request_id)
                            .into_response();
                    }
                    Err(e) => {
                        tracing::error!(%e, "set_ai_handling: detail query failed");
                        return ApiError::internal_error("Failed to load conversation detail")
                            .with_request_id(&ctx.request_id)
                            .into_response();
                    }
                };

            Json(serde_json::json!({ "data": {
                "conversation": detail,
                "escalation": escalation,
            } }))
            .into_response()
        }
        _ => ApiError::unprocessable_entity("Mode must be 'platform_ai' or 'human'")
            .with_request_id(&ctx.request_id)
            .into_response(),
    }
}

async fn fetch_conversation_detail(
    pool: &PgPool,
    tenant_id: Uuid,
    conversation_id: Uuid,
) -> sqlx::Result<Option<conversations::model::ConversationDetail>> {
    let mut tx = pool.begin().await?;
    let detail =
        conversations::queries::detail_query_in_tx(&mut tx, tenant_id, conversation_id).await?;
    tx.commit().await?;
    Ok(detail)
}

/// `GET /tenant/ai/agent/avatar` — serve the uploaded avatar image.
#[utoipa::path(
    get,
    path = "/tenant/ai/agent/avatar",
    tag = "tenant-ai",
    operation_id = "get_agent_avatar",
    summary = "Get agent avatar image",
    description = "Return the uploaded avatar image bytes with the stored content type. \
                   Cache-Control: private, max-age=300. Requires permission: ai_agent.view",
    responses(
        (status = 200, description = "Avatar image bytes.", content_type = "image/png"),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Agent configuration or avatar not found.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_agent_avatar(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
) -> Response {
    let agent_id = match agent_config::load_live(&pool, ctx.tenant_id).await {
        Ok(Some(row)) => row.id,
        Ok(None) => {
            return ApiError::not_found("Agent configuration not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "get_agent_avatar: load failed");
            return ApiError::internal_error("Failed to load agent configuration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let maybe_upload: Option<(Vec<u8>, String)> = sqlx::query_as(
        "SELECT bytes, content_type FROM agent_avatar_uploads \
         WHERE agent_id = $1 AND deleted_at IS NULL",
    )
    .bind(agent_id)
    .fetch_optional(&pool)
    .await
    .unwrap_or(None);

    let (bytes, content_type) = match maybe_upload {
        Some(row) => row,
        None => {
            return ApiError::not_found("Avatar not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let headers = [
        (header::CONTENT_TYPE, content_type.as_str()),
        (header::CACHE_CONTROL, "private, max-age=300"),
    ];
    (headers, bytes).into_response()
}
