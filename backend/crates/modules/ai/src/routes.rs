use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::Extension;
use identity::Principal;
use kernel::{ApiError, ApiJson, ErrorEnvelope};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use utoipa::IntoParams;
use uuid::Uuid;

use tenancy::TenantContext;

use crate::audit;
use crate::crypto;
use crate::model::{
    AiConfigurationView, ConfigPayload, CredentialPayload, CredentialView, FallbackEntry,
    TestConfigResult, UsageDetailResponse,
};
use crate::resolution::{resolve_config, resolve_credential, resolve_credential_view, Scope};
use crate::usage::{self, PaginatedResponse, UsageListItem, UsageSummary};
use std::time::Instant;

use ai_providers::{ChatRequest, Message, ProviderKind, Role};

use crate::AiService;

#[allow(clippy::too_many_arguments)]
async fn build_config_view(
    pool: &PgPool,
    scope: Scope,
    _config_id: Uuid,
    provider: &str,
    model: &str,
    max_output_tokens: Option<i32>,
    temperature: Option<f32>,
    fallbacks: serde_json::Value,
    capture_content: Option<bool>,
    updated_at: chrono::DateTime<chrono::Utc>,
    scope_is_tenant: bool,
) -> AiConfigurationView {
    let scope_label = if scope_is_tenant {
        "tenant"
    } else {
        "platform_default"
    };

    let credential = resolve_credential_view(pool, scope, provider).await;

    let fallbacks: Vec<FallbackEntry> = serde_json::from_value(fallbacks).unwrap_or_default();

    let effective_capture = if scope_is_tenant {
        capture_content
    } else {
        None
    };

    AiConfigurationView {
        scope: scope_label.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        max_output_tokens,
        temperature,
        fallbacks,
        capture_content: effective_capture,
        credential,
        updated_at,
    }
}

fn view_response(view: AiConfigurationView) -> Response {
    (StatusCode::OK, Json(json!(view))).into_response()
}

// ── Tenant Config ──────────────────────────────────────────────────────────

/// `GET /tenant/ai/config` — effective AI configuration for the current tenant.
///
/// Returns the tenant's own configuration when present, otherwise the
/// platform-wide default. The `scope` field on the response distinguishes the
/// two cases (`tenant` vs `platform_default`).
#[utoipa::path(
    get,
    path = "/tenant/ai/config",
    tag = "tenant-ai",
    operation_id = "get_tenant_ai_config",
    summary = "Get the effective tenant AI configuration",
    description = "Return the AI configuration in effect for the current tenant. When the \
                  tenant has its own row, the response's `scope` is `tenant` and all fields \
                  reflect the tenant row; otherwise the platform default is returned with \
                  `scope` set to `platform_default`. The associated `credential` view (if any) \
                  indicates the source — `tenant`, `platform`, or `none`. Requires permission: \
                  ai_agent.view",
    responses(
        (status = 200, description = "Effective AI configuration for this tenant.", body = AiConfigurationView),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "AI is not configured at any level.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_tenant_config(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
) -> Response {
    let scope = Scope::Tenant(ctx.tenant_id);

    match resolve_config(&pool, scope).await {
        Ok(Some(resolved)) => {
            let row = &resolved.row;
            let scope_is_tenant = resolved.scope_is_tenant;
            let view = build_config_view(
                &pool,
                scope,
                row.id,
                &row.provider,
                &row.model,
                row.max_output_tokens,
                row.temperature,
                row.fallbacks.clone(),
                Some(row.capture_content),
                row.updated_at,
                scope_is_tenant,
            )
            .await;
            view_response(view)
        }
        Ok(None) => ApiError::not_found("AI is not configured")
            .with_request_id(&ctx.request_id)
            .into_response(),
        Err(e) => {
            tracing::error!(%e, "get_tenant_config: resolve failed");
            ApiError::internal_error("Failed to read AI config")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
    }
}

/// `PUT /tenant/ai/config` — upsert the tenant's AI configuration.
#[utoipa::path(
    put,
    path = "/tenant/ai/config",
    tag = "tenant-ai",
    operation_id = "put_tenant_ai_config",
    summary = "Upsert the tenant AI configuration",
    description = "Create or update the AI configuration for the current tenant. The `provider` \
                  must be a known provider; `model` must be non-empty; `max_output_tokens` must \
                  be positive; `temperature` must be in the 0..=2 range. At most three fallbacks \
                  are allowed and no fallback may duplicate the primary provider/model or any \
                  other fallback. The `capture_content` flag is a tenant-level toggle and is \
                  audited when changed. Requires permission: ai_agent.manage",
    request_body = ConfigPayload,
    responses(
        (status = 200, description = "Updated AI configuration.", body = AiConfigurationView),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (e.g. unknown provider, bad temperature).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn put_tenant_config(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    ApiJson(payload): ApiJson<ConfigPayload>,
) -> Response {
    if let Err(e) = payload.validate() {
        return e.with_request_id(&ctx.request_id).into_response();
    }

    let fallbacks_vec = payload.fallbacks.clone().unwrap_or_default();
    let fallbacks_json = serde_json::to_value(&fallbacks_vec).unwrap();
    let capture = payload.capture_content.unwrap_or(false);

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "put_tenant_config: begin tx failed");
            return ApiError::internal_error("Failed to update AI config")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    // SELECT ... FOR UPDATE then UPDATE-or-INSERT per decision 10
    let existing: Option<(Uuid, bool)> = sqlx::query_as(
        "SELECT id, capture_content FROM ai_configurations \
         WHERE tenant_id = $1 AND deleted_at IS NULL \
         FOR UPDATE",
    )
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(%e, "put_tenant_config: select for update failed");
        e
    })
    .unwrap_or(None);

    let (old_capture, _config_id) = match &existing {
        Some((id, cap)) => (*cap, *id),
        None => (false, Uuid::nil()),
    };

    let config_id = if let Some((id, _)) = existing {
        // UPDATE existing row
        sqlx::query(
            "UPDATE ai_configurations SET provider = $1, model = $2, \
             max_output_tokens = $3, temperature = $4, fallbacks = $5, \
             capture_content = $6, deleted_at = NULL, updated_at = now() \
             WHERE id = $7",
        )
        .bind(&payload.provider)
        .bind(&payload.model)
        .bind(payload.max_output_tokens)
        .bind(payload.temperature)
        .bind(&fallbacks_json)
        .bind(capture)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(%e, "put_tenant_config: update failed");
        })
        .ok();
        id
    } else {
        // INSERT new row
        match sqlx::query_scalar(
            "INSERT INTO ai_configurations \
             (tenant_id, provider, model, max_output_tokens, temperature, fallbacks, capture_content) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
        )
        .bind(ctx.tenant_id)
        .bind(&payload.provider)
        .bind(&payload.model)
        .bind(payload.max_output_tokens)
        .bind(payload.temperature)
        .bind(&fallbacks_json)
        .bind(capture)
        .fetch_one(&mut *tx)
        .await
        {
            Ok(id) => id,
            Err(e) => {
                tracing::error!(%e, "put_tenant_config: insert failed");
                let _ = tx.rollback().await;
                return ApiError::internal_error("Failed to save AI config")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        }
    };

    let updated_at = chrono::Utc::now();

    let details = json!({
        "provider": payload.provider,
        "model": payload.model,
        "max_output_tokens": payload.max_output_tokens,
        "temperature": payload.temperature,
        "fallbacks": fallbacks_vec,
        "capture_content": capture,
    });

    if let Err(e) = audit::config_updated(
        &mut tx,
        principal.user_id,
        Some(ctx.tenant_id),
        config_id,
        &details,
    )
    .await
    {
        tracing::error!(%e, "put_tenant_config: audit failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to save AI config")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if old_capture != capture {
        if let Err(e) = audit::capture_content_changed(
            &mut tx,
            principal.user_id,
            ctx.tenant_id,
            config_id,
            old_capture,
            capture,
        )
        .await
        {
            tracing::error!(%e, "put_tenant_config: capture_content audit failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to save AI config")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "put_tenant_config: commit failed");
        return ApiError::internal_error("Failed to save AI config")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let view = build_config_view(
        &pool,
        Scope::Tenant(ctx.tenant_id),
        config_id,
        &payload.provider,
        &payload.model,
        payload.max_output_tokens,
        payload.temperature,
        fallbacks_json.clone(),
        Some(capture),
        updated_at,
        true,
    )
    .await;

    view_response(view)
}

/// `DELETE /tenant/ai/config` — soft-delete the tenant's AI configuration.
///
/// After this call the effective configuration for the tenant falls back to
/// the platform default.
#[utoipa::path(
    delete,
    path = "/tenant/ai/config",
    tag = "tenant-ai",
    operation_id = "delete_tenant_ai_config",
    summary = "Delete the tenant AI configuration",
    description = "Soft-delete the AI configuration for the current tenant. The tenant then \
                  inherits the platform default configuration (if any). Requires permission: \
                  ai_agent.manage",
    responses(
        (status = 204, description = "Configuration deleted."),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "AI config not found for this tenant.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn delete_tenant_config(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
) -> Response {
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "delete_tenant_config: begin tx failed");
            return ApiError::internal_error("Failed to delete AI config")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let row: Option<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, provider, model FROM ai_configurations \
         WHERE tenant_id = $1 AND deleted_at IS NULL \
         FOR UPDATE",
    )
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *tx)
    .await
    .unwrap_or(None);

    let (config_id, provider, model) = match row {
        Some(r) => r,
        None => {
            return ApiError::not_found("AI config not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    sqlx::query("UPDATE ai_configurations SET deleted_at = now() WHERE id = $1")
        .bind(config_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(%e, "delete_tenant_config: soft delete failed");
        })
        .unwrap_or_default();

    let details = json!({"provider": provider, "model": model});

    if let Err(e) = audit::config_deleted(
        &mut tx,
        principal.user_id,
        Some(ctx.tenant_id),
        config_id,
        &details,
    )
    .await
    {
        tracing::error!(%e, "delete_tenant_config: audit failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to delete AI config")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "delete_tenant_config: commit failed");
        return ApiError::internal_error("Failed to delete AI config")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

// ── Platform Config ────────────────────────────────────────────────────────

/// `GET /platform/ai/config` — platform-wide default AI configuration.
#[utoipa::path(
    get,
    path = "/platform/ai/config",
    tag = "platform-ai",
    operation_id = "get_platform_ai_config",
    summary = "Get the platform-wide AI configuration",
    description = "Return the platform's default AI configuration. The response's `scope` is \
                  always `platform_default`. `capture_content` is not surfaced here because it \
                  is a tenant-level setting. Requires permission: platform.admin",
    responses(
        (status = 200, description = "Platform AI configuration.", body = AiConfigurationView),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "AI is not configured.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_platform_config(
    State(pool): State<PgPool>,
    Extension(_principal): Extension<Principal>,
) -> Response {
    let scope = Scope::Platform;

    match resolve_config(&pool, scope).await {
        Ok(Some(resolved)) => {
            let row = &resolved.row;
            let view = build_config_view(
                &pool,
                scope,
                row.id,
                &row.provider,
                &row.model,
                row.max_output_tokens,
                row.temperature,
                row.fallbacks.clone(),
                None,
                row.updated_at,
                false,
            )
            .await;
            view_response(view)
        }
        Ok(None) => ApiError::not_found("AI is not configured").into_response(),
        Err(e) => {
            tracing::error!(%e, "get_platform_config: resolve failed");
            ApiError::internal_error("Failed to read AI config").into_response()
        }
    }
}

/// `PUT /platform/ai/config` — upsert the platform's default AI configuration.
///
/// `capture_content` is rejected here; it is a tenant-only field.
#[utoipa::path(
    put,
    path = "/platform/ai/config",
    tag = "platform-ai",
    operation_id = "put_platform_ai_config",
    summary = "Upsert the platform-wide AI configuration",
    description = "Create or update the AI configuration used as the default for every tenant \
                  that has not set its own override. `capture_content` is rejected with 400 \
                  because it is a tenant-level setting. Requires permission: platform.admin",
    request_body = ConfigPayload,
    responses(
        (status = 200, description = "Updated platform AI configuration.", body = AiConfigurationView),
        (status = 400, description = "Validation failed (e.g. `capture_content` is a tenant-level setting).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (e.g. unknown provider, bad temperature).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn put_platform_config(
    State(pool): State<PgPool>,
    Extension(principal): Extension<Principal>,
    ApiJson(payload): ApiJson<ConfigPayload>,
) -> Response {
    if let Err(e) = payload.validate() {
        return e.into_response();
    }

    if payload.capture_content.is_some() {
        return ApiError::validation_failed("capture_content is a tenant-level setting")
            .into_response();
    }

    let fallbacks_vec = payload.fallbacks.clone().unwrap_or_default();
    let fallbacks_json = serde_json::to_value(&fallbacks_vec).unwrap();

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "put_platform_config: begin tx failed");
            return ApiError::internal_error("Failed to update platform AI config").into_response();
        }
    };

    // SELECT ... FOR UPDATE then UPDATE-or-INSERT
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM ai_configurations \
         WHERE tenant_id IS NULL AND deleted_at IS NULL \
         FOR UPDATE",
    )
    .fetch_optional(&mut *tx)
    .await
    .unwrap_or(None);

    let config_id = if let Some(id) = existing {
        sqlx::query(
            "UPDATE ai_configurations SET provider = $1, model = $2, \
             max_output_tokens = $3, temperature = $4, fallbacks = $5, \
             updated_at = now() WHERE id = $6",
        )
        .bind(&payload.provider)
        .bind(&payload.model)
        .bind(payload.max_output_tokens)
        .bind(payload.temperature)
        .bind(&fallbacks_json)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| tracing::error!(%e, "put_platform_config: update failed"))
        .ok();
        id
    } else {
        match sqlx::query_scalar(
            "INSERT INTO ai_configurations (provider, model, max_output_tokens, temperature, fallbacks) \
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(&payload.provider)
        .bind(&payload.model)
        .bind(payload.max_output_tokens)
        .bind(payload.temperature)
        .bind(&fallbacks_json)
        .fetch_one(&mut *tx)
        .await
        {
            Ok(id) => id,
            Err(e) => {
                tracing::error!(%e, "put_platform_config: insert failed");
                let _ = tx.rollback().await;
                return ApiError::internal_error("Failed to save platform AI config").into_response();
            }
        }
    };

    let updated_at = chrono::Utc::now();
    let details = json!({
        "provider": payload.provider,
        "model": payload.model,
        "max_output_tokens": payload.max_output_tokens,
        "temperature": payload.temperature,
        "fallbacks": fallbacks_vec,
    });

    if let Err(e) =
        audit::config_updated(&mut tx, principal.user_id, None, config_id, &details).await
    {
        tracing::error!(%e, "put_platform_config: audit failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to save platform AI config").into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "put_platform_config: commit failed");
        return ApiError::internal_error("Failed to save platform AI config").into_response();
    }

    let view = build_config_view(
        &pool,
        Scope::Platform,
        config_id,
        &payload.provider,
        &payload.model,
        payload.max_output_tokens,
        payload.temperature,
        fallbacks_json,
        None,
        updated_at,
        false,
    )
    .await;

    view_response(view)
}

// ── Tenant Credentials ─────────────────────────────────────────────────────

/// `PUT /tenant/ai/credentials/{provider}` — set the tenant's API key for `provider`.
#[utoipa::path(
    put,
    path = "/tenant/ai/credentials/{provider}",
    tag = "tenant-ai",
    operation_id = "put_tenant_ai_credential",
    summary = "Set the tenant AI credential for a provider",
    description = "Encrypt and store the API key for the given provider at the tenant scope. \
                  If a credential already exists for `(tenant, provider)`, it is rotated in \
                  place. The plaintext key is never echoed back — the response only includes \
                  a `key_hint` derived from the plaintext. Requires permission: ai_agent.manage",
    params(("provider" = String, Path, description = "Provider identifier (e.g. `openai`, `anthropic`, `gemini`).")),
    request_body = CredentialPayload,
    responses(
        (status = 200, description = "Credential stored.", body = CredentialView),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (e.g. unknown provider or empty/oversized key).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error (e.g. encryption not configured).", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn put_tenant_credential(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Extension(ai): Extension<AiService>,
    Path(provider): Path<String>,
    ApiJson(payload): ApiJson<CredentialPayload>,
) -> Response {
    if ProviderKind::from_str(&provider).is_none() {
        return ApiError::validation_failed("unknown provider")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(msg) = payload.validate() {
        return ApiError::validation_failed(msg)
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let plaintext_key = payload.api_key;
    let key_hint = crypto::hint(&plaintext_key);
    let aad = crypto::aad(Some(ctx.tenant_id), &provider);

    let master = match ai.master_key() {
        Some(k) => k,
        None => {
            return ApiError::internal_error("Encryption is not configured")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let (ciphertext, nonce) = match crypto::seal(master, &aad, &plaintext_key) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(%e, "put_tenant_credential: seal failed");
            return ApiError::internal_error("Failed to encrypt credential")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "put_tenant_credential: begin tx failed");
            return ApiError::internal_error("Failed to save credential")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let existing: Option<Uuid> = match sqlx::query_scalar(
        "SELECT id FROM ai_credentials \
         WHERE tenant_id = $1 AND provider = $2 AND deleted_at IS NULL \
         FOR UPDATE",
    )
    .bind(ctx.tenant_id)
    .bind(&provider)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(row) => row,
        Err(e) => {
            tracing::error!(%e, "put_tenant_credential: select for update failed");
            return ApiError::internal_error("Failed to save credential")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let (credential_id, rotated) = match existing {
        Some(id) => {
            if let Err(e) = sqlx::query(
                "UPDATE ai_credentials SET ciphertext = $1, nonce = $2, key_hint = $3, \
                 deleted_at = NULL WHERE id = $4",
            )
            .bind(&ciphertext)
            .bind(&nonce)
            .bind(&key_hint)
            .bind(id)
            .execute(&mut *tx)
            .await
            {
                tracing::error!(%e, "put_tenant_credential: update failed");
                return ApiError::internal_error("Failed to save credential")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
            (id, true)
        }
        None => {
            let id = match sqlx::query_scalar(
                "INSERT INTO ai_credentials (tenant_id, provider, ciphertext, nonce, key_hint) \
                 VALUES ($1, $2, $3, $4, $5) RETURNING id",
            )
            .bind(ctx.tenant_id)
            .bind(&provider)
            .bind(&ciphertext)
            .bind(&nonce)
            .bind(&key_hint)
            .fetch_one(&mut *tx)
            .await
            {
                Ok(id) => id,
                Err(e) => {
                    tracing::error!(%e, "put_tenant_credential: insert failed");
                    return ApiError::internal_error("Failed to save credential")
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
            };
            (id, false)
        }
    };

    if let Err(e) = audit::credential_set(
        &mut tx,
        principal.user_id,
        Some(ctx.tenant_id),
        credential_id,
        &provider,
        &key_hint,
        rotated,
    )
    .await
    {
        tracing::error!(%e, "put_tenant_credential: audit failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to save credential")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "put_tenant_credential: commit failed");
        return ApiError::internal_error("Failed to save credential")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    (
        StatusCode::OK,
        Json(json!({
            "provider": provider,
            "source": "tenant",
            "key_hint": key_hint,
        })),
    )
        .into_response()
}

/// `DELETE /tenant/ai/credentials/{provider}` — remove the tenant's API key for `provider`.
#[utoipa::path(
    delete,
    path = "/tenant/ai/credentials/{provider}",
    tag = "tenant-ai",
    operation_id = "delete_tenant_ai_credential",
    summary = "Delete the tenant AI credential for a provider",
    description = "Soft-delete the tenant's API key for `provider`. After this call the tenant \
                  resolves credentials at the platform scope (if present) or has no credential \
                  for that provider. Requires permission: ai_agent.manage",
    params(("provider" = String, Path, description = "Provider identifier (e.g. `openai`, `anthropic`, `gemini`).")),
    responses(
        (status = 204, description = "Credential deleted."),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "No credential found for this provider.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn delete_tenant_credential(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Path(provider): Path<String>,
) -> Response {
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "delete_tenant_credential: begin tx failed");
            return ApiError::internal_error("Failed to delete credential")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let row: Option<Uuid> = match sqlx::query_scalar(
        "SELECT id FROM ai_credentials \
         WHERE tenant_id = $1 AND provider = $2 AND deleted_at IS NULL \
         FOR UPDATE",
    )
    .bind(ctx.tenant_id)
    .bind(&provider)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(row) => row,
        Err(e) => {
            tracing::error!(%e, "delete_tenant_credential: find failed");
            return ApiError::internal_error("Failed to delete credential")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let credential_id = match row {
        Some(id) => id,
        None => {
            return ApiError::not_found("Credential not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if let Err(e) = sqlx::query("UPDATE ai_credentials SET deleted_at = now() WHERE id = $1")
        .bind(credential_id)
        .execute(&mut *tx)
        .await
    {
        tracing::error!(%e, "delete_tenant_credential: soft delete failed");
        return ApiError::internal_error("Failed to delete credential")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(e) = audit::credential_deleted(
        &mut tx,
        principal.user_id,
        Some(ctx.tenant_id),
        credential_id,
        &provider,
    )
    .await
    {
        tracing::error!(%e, "delete_tenant_credential: audit failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to delete credential")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "delete_tenant_credential: commit failed");
        return ApiError::internal_error("Failed to delete credential")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

// ── Platform Credentials ────────────────────────────────────────────────────

/// `PUT /platform/ai/credentials/{provider}` — set the platform's API key for `provider`.
#[utoipa::path(
    put,
    path = "/platform/ai/credentials/{provider}",
    tag = "platform-ai",
    operation_id = "put_platform_ai_credential",
    summary = "Set the platform AI credential for a provider",
    description = "Encrypt and store the API key for the given provider at the platform scope. \
                  If a credential already exists for `(null tenant, provider)`, it is rotated \
                  in place. The plaintext key is never echoed back — the response only includes \
                  a `key_hint` derived from the plaintext. Requires permission: platform.admin",
    params(("provider" = String, Path, description = "Provider identifier (e.g. `openai`, `anthropic`, `gemini`).")),
    request_body = CredentialPayload,
    responses(
        (status = 200, description = "Credential stored.", body = CredentialView),
        (status = 400, description = "Validation failed (request body is not valid JSON).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed (e.g. unknown provider or empty/oversized key).", body = ErrorEnvelope),
        (status = 500, description = "Internal server error (e.g. encryption not configured).", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn put_platform_credential(
    State(pool): State<PgPool>,
    Extension(principal): Extension<Principal>,
    Extension(ai): Extension<AiService>,
    Path(provider): Path<String>,
    ApiJson(payload): ApiJson<CredentialPayload>,
) -> Response {
    if ProviderKind::from_str(&provider).is_none() {
        return ApiError::validation_failed("unknown provider").into_response();
    }

    if let Err(msg) = payload.validate() {
        return ApiError::validation_failed(msg).into_response();
    }

    let plaintext_key = payload.api_key;
    let key_hint = crypto::hint(&plaintext_key);
    let aad = crypto::aad(None, &provider);

    let master = match ai.master_key() {
        Some(k) => k,
        None => {
            return ApiError::internal_error("Encryption is not configured").into_response();
        }
    };

    let (ciphertext, nonce) = match crypto::seal(master, &aad, &plaintext_key) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(%e, "put_platform_credential: seal failed");
            return ApiError::internal_error("Failed to encrypt credential").into_response();
        }
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "put_platform_credential: begin tx failed");
            return ApiError::internal_error("Failed to save credential").into_response();
        }
    };

    let existing: Option<Uuid> = match sqlx::query_scalar(
        "SELECT id FROM ai_credentials \
         WHERE tenant_id IS NULL AND provider = $1 AND deleted_at IS NULL \
         FOR UPDATE",
    )
    .bind(&provider)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(row) => row,
        Err(e) => {
            tracing::error!(%e, "put_platform_credential: select for update failed");
            return ApiError::internal_error("Failed to save credential").into_response();
        }
    };

    let (credential_id, rotated) = match existing {
        Some(id) => {
            if let Err(e) = sqlx::query(
                "UPDATE ai_credentials SET ciphertext = $1, nonce = $2, key_hint = $3, \
                 deleted_at = NULL WHERE id = $4",
            )
            .bind(&ciphertext)
            .bind(&nonce)
            .bind(&key_hint)
            .bind(id)
            .execute(&mut *tx)
            .await
            {
                tracing::error!(%e, "put_platform_credential: update failed");
                return ApiError::internal_error("Failed to save credential").into_response();
            }
            (id, true)
        }
        None => {
            let id = match sqlx::query_scalar(
                "INSERT INTO ai_credentials (provider, ciphertext, nonce, key_hint) \
                 VALUES ($1, $2, $3, $4) RETURNING id",
            )
            .bind(&provider)
            .bind(&ciphertext)
            .bind(&nonce)
            .bind(&key_hint)
            .fetch_one(&mut *tx)
            .await
            {
                Ok(id) => id,
                Err(e) => {
                    tracing::error!(%e, "put_platform_credential: insert failed");
                    return ApiError::internal_error("Failed to save credential").into_response();
                }
            };
            (id, false)
        }
    };

    if let Err(e) = audit::credential_set(
        &mut tx,
        principal.user_id,
        None,
        credential_id,
        &provider,
        &key_hint,
        rotated,
    )
    .await
    {
        tracing::error!(%e, "put_platform_credential: audit failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to save credential").into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "put_platform_credential: commit failed");
        return ApiError::internal_error("Failed to save credential").into_response();
    }

    (
        StatusCode::OK,
        Json(json!({
            "provider": provider,
            "source": "platform",
            "key_hint": key_hint,
        })),
    )
        .into_response()
}

/// `DELETE /platform/ai/credentials/{provider}` — remove the platform's API key for `provider`.
#[utoipa::path(
    delete,
    path = "/platform/ai/credentials/{provider}",
    tag = "platform-ai",
    operation_id = "delete_platform_ai_credential",
    summary = "Delete the platform AI credential for a provider",
    description = "Soft-delete the platform's API key for `provider`. After this call, no \
                  platform-level credential exists for that provider, and any tenant that \
                  relied on the platform fallback will have no resolved credential until it \
                  sets one of its own. Requires permission: platform.admin",
    params(("provider" = String, Path, description = "Provider identifier (e.g. `openai`, `anthropic`, `gemini`).")),
    responses(
        (status = 204, description = "Credential deleted."),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "No credential found for this provider.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn delete_platform_credential(
    State(pool): State<PgPool>,
    Extension(principal): Extension<Principal>,
    Path(provider): Path<String>,
) -> Response {
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "delete_platform_credential: begin tx failed");
            return ApiError::internal_error("Failed to delete credential").into_response();
        }
    };

    let row: Option<Uuid> = match sqlx::query_scalar(
        "SELECT id FROM ai_credentials \
         WHERE tenant_id IS NULL AND provider = $1 AND deleted_at IS NULL \
         FOR UPDATE",
    )
    .bind(&provider)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(row) => row,
        Err(e) => {
            tracing::error!(%e, "delete_platform_credential: find failed");
            return ApiError::internal_error("Failed to delete credential").into_response();
        }
    };

    let credential_id = match row {
        Some(id) => id,
        None => {
            return ApiError::not_found("Credential not found").into_response();
        }
    };

    if let Err(e) = sqlx::query("UPDATE ai_credentials SET deleted_at = now() WHERE id = $1")
        .bind(credential_id)
        .execute(&mut *tx)
        .await
    {
        tracing::error!(%e, "delete_platform_credential: soft delete failed");
        return ApiError::internal_error("Failed to delete credential").into_response();
    }

    if let Err(e) =
        audit::credential_deleted(&mut tx, principal.user_id, None, credential_id, &provider).await
    {
        tracing::error!(%e, "delete_platform_credential: audit failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to delete credential").into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "delete_platform_credential: commit failed");
        return ApiError::internal_error("Failed to delete credential").into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

// ── Config Test ────────────────────────────────────────────────────────────

/// `POST /tenant/ai/config/test` — resolve the tenant's effective
/// configuration + credential and make a low-cost chat completion against
/// the provider to verify connectivity.
#[utoipa::path(
    post,
    path = "/tenant/ai/config/test",
    tag = "tenant-ai",
    operation_id = "test_tenant_ai_config",
    summary = "Test the tenant AI configuration end-to-end",
    description = "Resolve the tenant's effective AI configuration and credential, then issue \
                  a one-token `ping` chat completion against the provider. On success the \
                  response is `TestConfigResult` with `ok: true` plus `provider`, `model`, and \
                  `latency_ms`. On failure the response is 422 with `ok: false`, \
                  `error_category`, and a sanitized `detail`. The `ping` request is not \
                  recorded in the usage ledger. Requires permission: ai_agent.manage",
    responses(
        (status = 200, description = "Test succeeded.", body = TestConfigResult),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "AI is not configured or no credential resolves.", body = ErrorEnvelope),
        (status = 422, description = "Provider rejected the test call.", body = TestConfigResult),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn test_tenant_config(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(ai): Extension<AiService>,
) -> Response {
    let scope = Scope::Tenant(ctx.tenant_id);

    let resolved = match resolve_config(&pool, scope).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            return ApiError::not_found("AI is not configured")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "test_tenant_config: resolve_config failed");
            return ApiError::internal_error("Failed to resolve AI config")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let provider_name = &resolved.row.provider;
    let model = &resolved.row.model;

    let master = match ai.master_key() {
        Some(k) => k,
        None => {
            return ApiError::not_found("AI is not configured")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let (key, _) = match resolve_credential(&pool, master, scope, provider_name).await {
        Ok(Some(k)) => k,
        Ok(None) => {
            return ApiError::not_found("AI is not configured")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "test_tenant_config: resolve_credential failed");
            return ApiError::internal_error("Failed to resolve credential")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let provider = match ai.registry().resolve(provider_name) {
        Some(p) => p,
        None => {
            return ApiError::not_found("AI is not configured")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let req = ChatRequest {
        system: None,
        messages: vec![Message {
            role: Role::User,
            content: "ping".into(),
            tool_calls: vec![],
            tool_call_id: None,
        }],
        model: model.clone(),
        max_output_tokens: Some(16),
        temperature: None,
        request_id: Some(ctx.request_id.clone()),
        tools: vec![],
    };

    let start = Instant::now();
    let result = provider.complete(&key, &req).await;
    let latency_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(_completion) => (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "provider": provider_name,
                "model": model,
                "latency_ms": latency_ms,
            })),
        )
            .into_response(),
        Err(err) => {
            let sanitized: String = err
                .detail
                .chars()
                .filter(|c| c.is_alphanumeric() || c.is_whitespace() || "._-/:".contains(*c))
                .take(200)
                .collect();
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "ok": false,
                    "error_category": err.category.as_str(),
                    "detail": sanitized,
                })),
            )
                .into_response()
        }
    }
}

/// `POST /platform/ai/config/test` — verify the platform's AI configuration
/// end-to-end with a low-cost chat completion.
#[utoipa::path(
    post,
    path = "/platform/ai/config/test",
    tag = "platform-ai",
    operation_id = "test_platform_ai_config",
    summary = "Test the platform AI configuration end-to-end",
    description = "Resolve the platform's AI configuration and credential, then issue a \
                  one-token `ping` chat completion against the provider. On success the \
                  response is `TestConfigResult` with `ok: true` plus `provider`, `model`, and \
                  `latency_ms`. On failure the response is 422 with `ok: false`, \
                  `error_category`, and a sanitized `detail`. The `ping` request is not \
                  recorded in the usage ledger. Requires permission: platform.admin",
    responses(
        (status = 200, description = "Test succeeded.", body = TestConfigResult),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "AI is not configured or no credential resolves.", body = ErrorEnvelope),
        (status = 422, description = "Provider rejected the test call.", body = TestConfigResult),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn test_platform_config(
    State(pool): State<PgPool>,
    Extension(ai): Extension<AiService>,
) -> Response {
    let scope = Scope::Platform;

    let resolved = match resolve_config(&pool, scope).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            return ApiError::not_found("AI is not configured").into_response();
        }
        Err(e) => {
            tracing::error!(%e, "test_platform_config: resolve_config failed");
            return ApiError::internal_error("Failed to resolve AI config").into_response();
        }
    };

    let provider_name = &resolved.row.provider;
    let model = &resolved.row.model;

    let master = match ai.master_key() {
        Some(k) => k,
        None => {
            return ApiError::not_found("AI is not configured").into_response();
        }
    };

    let (key, _) = match resolve_credential(&pool, master, scope, provider_name).await {
        Ok(Some(k)) => k,
        Ok(None) => {
            return ApiError::not_found("AI is not configured").into_response();
        }
        Err(e) => {
            tracing::error!(%e, "test_platform_config: resolve_credential failed");
            return ApiError::internal_error("Failed to resolve credential").into_response();
        }
    };

    let provider = match ai.registry().resolve(provider_name) {
        Some(p) => p,
        None => {
            return ApiError::not_found("AI is not configured").into_response();
        }
    };

    let req = ChatRequest {
        system: None,
        messages: vec![Message {
            role: Role::User,
            content: "ping".into(),
            tool_calls: vec![],
            tool_call_id: None,
        }],
        model: model.clone(),
        max_output_tokens: Some(16),
        temperature: None,
        request_id: None,
        tools: vec![],
    };

    let start = Instant::now();
    let result = provider.complete(&key, &req).await;
    let latency_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(_completion) => (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "provider": provider_name,
                "model": model,
                "latency_ms": latency_ms,
            })),
        )
            .into_response(),
        Err(err) => {
            let sanitized: String = err
                .detail
                .chars()
                .filter(|c| c.is_alphanumeric() || c.is_whitespace() || "._-/:".contains(*c))
                .take(200)
                .collect();
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "ok": false,
                    "error_category": err.category.as_str(),
                    "detail": sanitized,
                })),
            )
                .into_response()
        }
    }
}

// ── Usage ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(default, deny_unknown_fields)]
pub struct UsageQueryParams {
    pub from: Option<String>,
    pub to: Option<String>,
    pub cursor: Option<String>,
    pub limit: u32,
}

impl Default for UsageQueryParams {
    fn default() -> Self {
        Self {
            from: None,
            to: None,
            cursor: None,
            limit: 25,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(default, deny_unknown_fields)]
pub struct UsageRangeParams {
    pub from: Option<String>,
    pub to: Option<String>,
}

#[allow(clippy::result_large_err)]
fn parse_rfc3339_opt(s: Option<String>) -> Result<Option<chrono::DateTime<chrono::Utc>>, Response> {
    match s {
        None => Ok(None),
        Some(v) => match chrono::DateTime::parse_from_rfc3339(&v) {
            Ok(dt) => Ok(Some(dt.with_timezone(&chrono::Utc))),
            Err(_) => Err(ApiError::unprocessable_entity(
                "Invalid datetime format, expected RFC 3339",
            )
            .into_response()),
        },
    }
}

/// `GET /tenant/ai/usage` — cursor-paginated usage ledger for the current tenant.
#[utoipa::path(
    get,
    path = "/tenant/ai/usage",
    tag = "tenant-ai",
    operation_id = "list_tenant_ai_usage",
    summary = "List tenant AI usage records",
    description = "Return one page of AI usage records for the current tenant, ordered by \
                  `created_at DESC, id DESC`. The list contains metadata only — request and \
                  response content are exposed only via `GET /tenant/ai/usage/{id}`. The \
                  `next_cursor` from a previous page is opaque; pass it back verbatim to fetch \
                  the next page. Requires permission: ai_agent.view",
    params(UsageQueryParams),
    responses(
        (status = 200, description = "Page of usage records (data + pagination).", body = PaginatedResponse<UsageListItem>),
        (status = 400, description = "Validation failed (e.g. invalid `from`/`to` format).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn list_tenant_usage(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
    Query(params): Query<UsageQueryParams>,
) -> Response {
    let from = match parse_rfc3339_opt(params.from) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let to = match parse_rfc3339_opt(params.to) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let limit = (params.limit.clamp(1, 100)) as i64;

    match usage::list(&pool, ctx.tenant_id, from, to, params.cursor, limit).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => {
            tracing::error!(%e, "list_tenant_usage failed");
            ApiError::internal_error("Failed to load usage records")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
    }
}

/// `GET /tenant/ai/usage/summary` — aggregate usage counts for the tenant.
#[utoipa::path(
    get,
    path = "/tenant/ai/usage/summary",
    tag = "tenant-ai",
    operation_id = "tenant_ai_usage_summary",
    summary = "Summarize tenant AI usage",
    description = "Return aggregate call/token counts for the current tenant over an optional \
                  `[from, to)` RFC 3339 time window. `unreported_calls` counts rows where the \
                  provider did not report token usage. Requires permission: ai_agent.view",
    params(UsageRangeParams),
    responses(
        (status = 200, description = "Aggregate usage summary.", body = UsageSummary),
        (status = 400, description = "Validation failed (e.g. invalid `from`/`to` format).", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn tenant_usage_summary(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
    Query(params): Query<UsageRangeParams>,
) -> Response {
    let from = match parse_rfc3339_opt(params.from) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let to = match parse_rfc3339_opt(params.to) {
        Ok(v) => v,
        Err(r) => return r,
    };

    match usage::summary(&pool, ctx.tenant_id, from, to).await {
        Ok(response) => Json(response).into_response(),
        Err(e) => {
            tracing::error!(%e, "tenant_usage_summary failed");
            ApiError::internal_error("Failed to load usage summary")
                .with_request_id(&ctx.request_id)
                .into_response()
        }
    }
}

/// `GET /tenant/ai/usage/{id}` — full detail (including request/response
/// content) for a single usage record belonging to the tenant.
#[utoipa::path(
    get,
    path = "/tenant/ai/usage/{id}",
    tag = "tenant-ai",
    operation_id = "get_tenant_ai_usage_detail",
    summary = "Get a single tenant AI usage record",
    description = "Return the full record for a single usage row, including captured request \
                  and response content (when `capture_content` is enabled for the tenant at the \
                  time the call was made). Cross-tenant and missing ids both return 404. \
                  Requires permission: ai_agent.manage",
    params(("id" = Uuid, Path, description = "Usage record identifier.")),
    responses(
        (status = 200, description = "Usage record detail.", body = UsageDetailResponse),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Usage record not found.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_tenant_usage_detail(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Response {
    match usage::detail(&pool, ctx.tenant_id, id).await {
        Some(row) => Json(json!({ "data": row })).into_response(),
        None => ApiError::not_found("Usage record not found")
            .with_request_id(&ctx.request_id)
            .into_response(),
    }
}
