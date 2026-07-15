use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::Extension;
use identity::Principal;
use kernel::{ApiError, ApiJson};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use tenancy::TenantContext;

use crate::audit;
use crate::crypto;
use crate::model::{AiConfigurationView, ConfigPayload, CredentialPayload, FallbackEntry};
use crate::resolution::{resolve_config, resolve_credential, resolve_credential_view, Scope};
use crate::usage;
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
        }],
        model: model.clone(),
        max_output_tokens: Some(16),
        temperature: None,
        request_id: Some(ctx.request_id.clone()),
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
        }],
        model: model.clone(),
        max_output_tokens: Some(16),
        temperature: None,
        request_id: None,
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Default, Deserialize)]
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
