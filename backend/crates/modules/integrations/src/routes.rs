use std::sync::Arc;

use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use identity::Principal;
use kernel::{ApiError, ApiJson, ErrorEnvelope};
use serde_json::Map;
use sqlx::PgPool;
use tenancy::TenantContext;
use uuid::Uuid;

use crate::crypto::{self, MasterKey};
use crate::model::{
    self, ConfigFieldDto, IntegrationConnectionDto, IntegrationDetailDto, IntegrationEventDto,
    IntegrationEventListResponse, IntegrationListItemDto, IntegrationListResponse,
    IntegrationSecretRefDto, PaginationInfo,
};
use crate::queries;
use crate::status;
use crate::webhook;

const FIELD_WEBHOOK_TOKEN: &str = "__webhook_token";

fn build_event_response(
    rows: &[queries::EventRow],
    limit: i64,
) -> IntegrationEventListResponse {
    let has_more = rows.len() > limit as usize;
    let rows = if has_more {
        &rows[..limit as usize]
    } else {
        rows
    };
    let data: Vec<IntegrationEventDto> = rows
        .iter()
        .map(|row| IntegrationEventDto {
            id: row.id,
            event_type: match row.event_type.as_str() {
                "connected" => model::EventType::Connected,
                "config_updated" => model::EventType::ConfigUpdated,
                "secret_rotated" => model::EventType::SecretRotated,
                "disconnected" => model::EventType::Disconnected,
                "delivery_accepted" => model::EventType::DeliveryAccepted,
                "delivery_rejected" => model::EventType::DeliveryRejected,
                other => panic!("unexpected event_type in DB: {other}"),
            },
            outcome: row.outcome.clone(),
            reason: row.reason.as_deref().map(|r| match r {
                "invalid_signature" => model::RejectionReason::InvalidSignature,
                "inactive_connection" => model::RejectionReason::InactiveConnection,
                "payload_too_large" => model::RejectionReason::PayloadTooLarge,
                "rate_limited" => model::RejectionReason::RateLimited,
                "malformed_payload" => model::RejectionReason::MalformedPayload,
                other => panic!("unexpected rejection reason in DB: {other}"),
            }),
            actor_membership_id: row.actor_membership_id,
            created_at: row.created_at,
        })
        .collect();
    let next_cursor = has_more.then(|| {
        let last = rows.last().expect("rows must be non-empty when has_more is true");
        queries::encode_cursor(last.created_at, last.id)
    });
    IntegrationEventListResponse {
        data,
        pagination: PaginationInfo {
            next_cursor,
            has_more,
        },
    }
}

async fn active_membership_id(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM tenant_memberships \
         WHERE tenant_id = $1 AND user_id = $2 AND status = 'active' AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id,)| id))
}

fn load_master_key(config: &config::AppConfig) -> Result<MasterKey, Response> {
    let raw = config
        .integration_secrets_key
        .as_deref()
        .ok_or_else(|| {
            ApiError::internal_error("Integration secrets key is not configured")
                .into_response()
        })?;
    MasterKey::from_base64(raw).map_err(|e| {
        tracing::error!(error = %e, "invalid integration_secrets_key");
        ApiError::internal_error("Integration secrets key is invalid").into_response()
    })
}

fn build_webhook_url(config: &config::AppConfig, token: &str) -> String {
    let host = config.public_dashboard_url.trim_end_matches('/');
    format!("{}/hooks/v1/{}", host, token)
}

#[utoipa::path(
    get,
    path = "/tenant/integrations",
    tag = "integrations",
    operation_id = "list_integrations",
    summary = "List integration catalog with tenant status",
    responses(
        (status = 200, description = "Catalog entries with per-tenant status.", body = IntegrationListResponse),
        (status = 403, description = "Forbidden — missing integrations.view permission."),
    ),
)]
pub async fn list_integrations(
    State(pool): State<PgPool>,
    ctx: TenantContext,
) -> Response {
    let rows = match queries::list_catalog_with_status(&pool, ctx.tenant_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(%e, "list_integrations: list_catalog_with_status failed");
            return ApiError::internal_error("Failed to load integrations")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let items: Vec<IntegrationListItemDto> = rows
        .iter()
        .map(|row| {
            let outcomes_vec: Vec<String> = row.outcomes.clone().unwrap_or_default();
            let outcomes_refs: Vec<&str> = outcomes_vec.iter().map(String::as_str).collect();
            let status = status::derive_status(row.is_active, &outcomes_refs);
            IntegrationListItemDto {
                slug: row.slug.clone(),
                name: row.name.clone(),
                description: row.description.clone(),
                category: row.category.clone(),
                is_available: row.is_available,
                status,
            }
        })
        .collect();

    (StatusCode::OK, Json(IntegrationListResponse { data: items })).into_response()
}

#[utoipa::path(
    get,
    path = "/tenant/integrations/{slug}",
    tag = "integrations",
    operation_id = "get_integration",
    summary = "Get a single integration catalog entry with connection detail",
    params(
        ("slug" = String, Path, description = "Catalog entry slug"),
    ),
    responses(
        (status = 200, description = "Integration detail.", body = IntegrationDetailDto),
        (status = 403, description = "Forbidden — missing integrations.view permission."),
        (status = 404, description = "Unknown integration slug."),
    ),
)]
pub async fn get_integration(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(config): Extension<Arc<config::AppConfig>>,
    Path(slug): Path<String>,
) -> Response {
    let catalog = match queries::find_catalog_by_slug(&pool, &slug).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return ApiError::not_found("Integration not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "get_integration: find_catalog_by_slug failed");
            return ApiError::internal_error("Failed to load integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let connection = queries::find_connection(&pool, ctx.tenant_id, catalog.id)
        .await
        .ok()
        .flatten();

    let schema: Vec<ConfigFieldDto> = serde_json::from_value(catalog.config_schema.clone())
        .unwrap_or_default();

    let master = match load_master_key(&config) {
        Ok(m) => Some(m),
        Err(resp) => return resp,
    };

    let detail = build_detail_dto(&pool, &catalog, connection, schema, master.as_ref(), &config).await;
    (StatusCode::OK, Json(detail)).into_response()
}

async fn build_detail_dto(
    pool: &PgPool,
    catalog: &queries::CatalogRow,
    connection: Option<queries::ConnectionRow>,
    schema: Vec<ConfigFieldDto>,
    master: Option<&MasterKey>,
    config: &config::AppConfig,
) -> IntegrationDetailDto {
    match connection {
        None => IntegrationDetailDto {
            slug: catalog.slug.clone(),
            name: catalog.name.clone(),
            description: catalog.description.clone(),
            category: catalog.category.clone(),
            is_available: catalog.is_available,
            status: model::ConnectionStatus::NotConnected,
            config_schema: schema,
            connection: None,
        },
        Some(c) => {
            let outcomes = queries::recent_event_outcomes(pool, c.id)
                .await
                .unwrap_or_default();
            let outcome_refs: Vec<&str> = outcomes.iter().map(String::as_str).collect();
            let derived = status::derive_status(Some(c.is_active), &outcome_refs);

            let secrets: Vec<IntegrationSecretRefDto> = if c.is_active {
                queries::list_secret_refs(pool, c.id)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|r| IntegrationSecretRefDto {
                        field_key: r.field_key,
                        hint: r.hint,
                    })
                    .collect()
            } else {
                Vec::new()
            };

            let config_map: Map<String, serde_json::Value> = match c.config {
                serde_json::Value::Object(m) => m,
                _ => Map::new(),
            };

            let webhook_url = if c.is_active {
                master.and_then(|m| {
                    let aad = crypto::aad(c.tenant_id, &catalog.slug, FIELD_WEBHOOK_TOKEN);
                    let token = crypto::open(m, &aad, &c.webhook_token_ciphertext, &c.webhook_token_nonce).ok()?;
                    Some(build_webhook_url(config, &token))
                })
            } else {
                None
            };

            IntegrationDetailDto {
                slug: catalog.slug.clone(),
                name: catalog.name.clone(),
                description: catalog.description.clone(),
                category: catalog.category.clone(),
                is_available: catalog.is_available,
                status: derived,
                config_schema: schema,
                connection: Some(IntegrationConnectionDto {
                    config: config_map,
                    secrets,
                    webhook_url,
                    connected_at: c.connected_at,
                    disconnected_at: c.disconnected_at,
                }),
            }
        }
    }
}

async fn build_detail_dto_for_connection(
    pool: &PgPool,
    config: &config::AppConfig,
    master: &MasterKey,
    tenant_id: Uuid,
    catalog: &queries::CatalogRow,
    connection_id: Uuid,
    webhook_token_plain: Option<&str>,
) -> IntegrationDetailDto {
    let connection = queries::find_connection_by_id(pool, connection_id)
        .await
        .ok()
        .flatten();
    let connection = match connection {
        Some(c) if c.tenant_id == tenant_id => c,
        _ => {
            return IntegrationDetailDto {
                slug: catalog.slug.clone(),
                name: catalog.name.clone(),
                description: catalog.description.clone(),
                category: catalog.category.clone(),
                is_available: catalog.is_available,
                status: model::ConnectionStatus::NotConnected,
                config_schema: Vec::new(),
                connection: None,
            };
        }
    };

    let schema: Vec<ConfigFieldDto> = serde_json::from_value(catalog.config_schema.clone())
        .unwrap_or_default();

    let outcomes = queries::recent_event_outcomes(pool, connection.id)
        .await
        .unwrap_or_default();
    let outcome_refs: Vec<&str> = outcomes.iter().map(String::as_str).collect();
    let derived = status::derive_status(Some(connection.is_active), &outcome_refs);

    let secrets: Vec<IntegrationSecretRefDto> = if connection.is_active {
        queries::list_secret_refs(pool, connection.id)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|r| IntegrationSecretRefDto {
                field_key: r.field_key,
                hint: r.hint,
            })
            .collect()
    } else {
        Vec::new()
    };

    let config_map: Map<String, serde_json::Value> = match connection.config {
        serde_json::Value::Object(m) => m,
        _ => Map::new(),
    };

    let webhook_url = if connection.is_active {
        if let Some(token) = webhook_token_plain {
            Some(build_webhook_url(config, token))
        } else {
            let aad = crypto::aad(connection.tenant_id, &catalog.slug, FIELD_WEBHOOK_TOKEN);
            crypto::open(
                master,
                &aad,
                &connection.webhook_token_ciphertext,
                &connection.webhook_token_nonce,
            )
            .ok()
            .map(|t| build_webhook_url(config, &t))
        }
    } else {
        None
    };

    IntegrationDetailDto {
        slug: catalog.slug.clone(),
        name: catalog.name.clone(),
        description: catalog.description.clone(),
        category: catalog.category.clone(),
        is_available: catalog.is_available,
        status: derived,
        config_schema: schema,
        connection: Some(IntegrationConnectionDto {
            config: config_map,
            secrets,
            webhook_url,
            connected_at: connection.connected_at,
            disconnected_at: connection.disconnected_at,
        }),
    }
}

#[utoipa::path(
    post,
    path = "/tenant/integrations/{slug}/connect",
    tag = "integrations",
    operation_id = "connect_integration",
    summary = "Connect (or reconnect) an integration for the active tenant",
    params(
        ("slug" = String, Path, description = "Catalog entry slug"),
    ),
    request_body = model::ConnectPayload,
    responses(
        (status = 201, description = "Integration connected.", body = IntegrationDetailDto),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Unknown integration slug.", body = ErrorEnvelope),
        (status = 409, description = "Integration is already actively connected.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed or catalog entry not available.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn connect_integration(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Extension(config): Extension<Arc<config::AppConfig>>,
    Path(slug): Path<String>,
    ApiJson(payload): ApiJson<model::ConnectPayload>,
) -> Response {
    let master = match load_master_key(&config) {
        Ok(m) => m,
        Err(resp) => return resp,
    };

    let catalog = match queries::find_catalog_by_slug(&pool, &slug).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return ApiError::not_found("Integration not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "connect_integration: find_catalog_by_slug failed");
            return ApiError::internal_error("Failed to load integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if !catalog.is_available {
        return ApiError::unprocessable_entity("Integration is not available for connect")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let schema: Vec<ConfigFieldDto> = serde_json::from_value(catalog.config_schema.clone())
        .unwrap_or_default();
    if let Err(errors) = model::validate_against_schema(
        &schema,
        &payload.config,
        &payload.secrets,
        true,
    ) {
        return ApiError::unprocessable_entity("Validation failed")
            .with_details(errors)
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let existing = queries::find_connection(&pool, ctx.tenant_id, catalog.id)
        .await
        .ok()
        .flatten();
    if let Some(c) = &existing {
        if c.is_active {
            return ApiError::new_with_code(
                    StatusCode::CONFLICT,
                    "integration_already_connected",
                    "Integration is already actively connected",
                )
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    let membership_id = match active_membership_id(&pool, ctx.tenant_id, principal.user_id).await {
        Ok(Some(m)) => Some(m),
        Ok(None) => None,
        Err(e) => {
            tracing::error!(%e, "connect_integration: membership lookup failed");
            return ApiError::internal_error("Failed to connect integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "connect_integration: begin tx failed");
            return ApiError::internal_error("Failed to connect integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let token = webhook::generate_token();
    let token_hash = webhook::hash_token(&token);
    let token_aad = crypto::aad(ctx.tenant_id, &catalog.slug, FIELD_WEBHOOK_TOKEN);
    let (token_ct, token_nonce) = match crypto::seal(&master, &token_aad, &token) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "connect_integration: seal token failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to connect integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let connection_id = match queries::upsert_connection(
        &mut tx,
        ctx.tenant_id,
        catalog.id,
        &token_hash,
        &token_ct,
        &token_nonce,
        membership_id,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            tracing::error!(%e, "connect_integration: upsert_connection failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to connect integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if let Err(e) =
        queries::delete_secrets_for_connection(&mut tx, connection_id).await
    {
        tracing::error!(%e, "connect_integration: delete_secrets_for_connection failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to connect integration")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    for (field_key, plaintext) in &payload.secrets {
        let aad = crypto::aad(ctx.tenant_id, &catalog.slug, field_key);
        let (ct, nonce) = match crypto::seal(&master, &aad, plaintext) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, "connect_integration: seal secret failed");
                let _ = tx.rollback().await;
                return ApiError::internal_error("Failed to connect integration")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        };
        let hint = crypto::hint(plaintext);
        if let Err(e) = queries::upsert_secret(
            &mut tx,
            ctx.tenant_id,
            connection_id,
            field_key,
            &ct,
            &nonce,
            &hint,
        )
        .await
        {
            tracing::error!(%e, "connect_integration: upsert_secret failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to connect integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    let config_value = serde_json::Value::Object(payload.config.clone());
    if let Err(e) =
        queries::update_connection_config(&mut tx, connection_id, &config_value).await
    {
        tracing::error!(%e, "connect_integration: update_connection_config failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to connect integration")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(e) = queries::insert_event(
        &mut tx,
        ctx.tenant_id,
        connection_id,
        model::EventType::Connected.as_str(),
        "success",
        None,
        membership_id,
    )
    .await
    {
        tracing::error!(%e, "connect_integration: insert_event failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to connect integration")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(e) = crate::audit::record_connected_in_tx(
        &mut tx,
        principal.user_id,
        ctx.tenant_id,
        connection_id,
        &catalog.slug,
    )
    .await
    {
        tracing::error!(%e, "connect_integration: audit failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to connect integration")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "connect_integration: commit failed");
        return ApiError::internal_error("Failed to connect integration")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let detail = build_detail_dto_for_connection(
        &pool,
        &config,
        &master,
        ctx.tenant_id,
        &catalog,
        connection_id,
        None,
    )
    .await;
    (StatusCode::OK, Json(detail)).into_response()
}

#[utoipa::path(
    get,
    path = "/tenant/integrations/{slug}/events",
    tag = "integrations",
    operation_id = "list_integration_events",
    summary = "List the event log for a connection (newest first, cursor-paginated)",
    params(
        ("slug" = String, Path, description = "Catalog entry slug"),
        ("cursor" = Option<String>, Query, description = "Opaque pagination cursor"),
        ("limit" = Option<i64>, Query, description = "Items per page, clamped 1..=100"),
    ),
    responses(
        (status = 200, description = "Event log page.", body = IntegrationEventListResponse),
        (status = 403, description = "Forbidden — missing integrations.view permission."),
        (status = 404, description = "Unknown integration slug."),
        (status = 422, description = "Invalid cursor."),
    ),
)]
pub async fn list_integration_events(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Path(slug): Path<String>,
    Query(query): Query<model::EventsQuery>,
) -> Response {
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let cursor = match query.cursor.as_deref() {
        Some(c) => match queries::decode_cursor(c) {
            Some(v) => Some(v),
            None => {
                return ApiError::unprocessable_entity("Invalid cursor")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        },
        None => None,
    };

    let catalog = match queries::find_catalog_by_slug(&pool, &slug).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return ApiError::not_found("Integration not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "list_integration_events: find_catalog_by_slug failed");
            return ApiError::internal_error("Failed to load integration events")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let connection = match queries::find_connection(&pool, ctx.tenant_id, catalog.id).await {
        Ok(Some(c)) => c,
        // No connection yet → empty log, not 404 (the integration exists).
        Ok(None) => {
            return (
                StatusCode::OK,
                Json(IntegrationEventListResponse {
                    data: Vec::new(),
                    pagination: PaginationInfo {
                        next_cursor: None,
                        has_more: false,
                    },
                }),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "list_integration_events: find_connection failed");
            return ApiError::internal_error("Failed to load integration events")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let fetch_limit = limit + 1;
    let rows = match queries::list_events(&pool, connection.id, cursor, fetch_limit).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(%e, "list_integration_events: list_events failed");
            return ApiError::internal_error("Failed to load integration events")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let response = build_event_response(&rows, limit);
    (StatusCode::OK, Json(response)).into_response()
}

#[utoipa::path(
    put,
    path = "/tenant/integrations/{slug}/config",
    tag = "integrations",
    operation_id = "update_integration_config",
    summary = "Update an active integration's configuration and/or rotate secrets",
    params(
        ("slug" = String, Path, description = "Catalog entry slug"),
    ),
    request_body = model::UpdateConfigPayload,
    responses(
        (status = 200, description = "Integration updated.", body = IntegrationDetailDto),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Unknown integration slug.", body = ErrorEnvelope),
        (status = 409, description = "Integration is not actively connected.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn update_integration_config(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Extension(config): Extension<Arc<config::AppConfig>>,
    Path(slug): Path<String>,
    ApiJson(payload): ApiJson<model::UpdateConfigPayload>,
) -> Response {
    let master = match load_master_key(&config) {
        Ok(m) => m,
        Err(resp) => return resp,
    };

    let catalog = match queries::find_catalog_by_slug(&pool, &slug).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return ApiError::not_found("Integration not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "update_integration_config: find_catalog_by_slug failed");
            return ApiError::internal_error("Failed to update integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let connection = match queries::find_connection(&pool, ctx.tenant_id, catalog.id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return ApiError::new_with_code(
                StatusCode::CONFLICT,
                "integration_not_connected",
                "Integration is not actively connected",
            )
            .with_request_id(&ctx.request_id)
            .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "update_integration_config: find_connection failed");
            return ApiError::internal_error("Failed to update integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if !connection.is_active {
        return ApiError::new_with_code(
            StatusCode::CONFLICT,
            "integration_not_connected",
            "Integration is not actively connected",
        )
        .with_request_id(&ctx.request_id)
        .into_response();
    }

    let schema: Vec<ConfigFieldDto> = serde_json::from_value(catalog.config_schema.clone())
        .unwrap_or_default();
    let secrets_ref = payload.secrets.clone().unwrap_or_default();
    if let Err(errors) = model::validate_against_schema(
        &schema,
        &payload.config,
        &secrets_ref,
        false,
    ) {
        return ApiError::unprocessable_entity("Validation failed")
            .with_details(errors)
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let membership_id = match active_membership_id(&pool, ctx.tenant_id, principal.user_id).await {
        Ok(Some(m)) => Some(m),
        Ok(None) => None,
        Err(e) => {
            tracing::error!(%e, "update_integration_config: membership lookup failed");
            return ApiError::internal_error("Failed to update integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let config_changed = !payload.config.is_empty();
    let secrets_changed = !secrets_ref.is_empty();

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "update_integration_config: begin tx failed");
            return ApiError::internal_error("Failed to update integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if config_changed {
        let config_value = serde_json::Value::Object(payload.config.clone());
        if let Err(e) =
            queries::update_connection_config(&mut tx, connection.id, &config_value).await
        {
            tracing::error!(%e, "update_integration_config: update_connection_config failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to update integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    for (field_key, plaintext) in &secrets_ref {
        let aad = crypto::aad(ctx.tenant_id, &catalog.slug, field_key);
        let (ct, nonce) = match crypto::seal(&master, &aad, plaintext) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, "update_integration_config: seal failed");
                let _ = tx.rollback().await;
                return ApiError::internal_error("Failed to update integration")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        };
        let hint = crypto::hint(plaintext);
        if let Err(e) = queries::upsert_secret(
            &mut tx,
            ctx.tenant_id,
            connection.id,
            field_key,
            &ct,
            &nonce,
            &hint,
        )
        .await
        {
            tracing::error!(%e, "update_integration_config: upsert_secret failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to update integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    if config_changed {
        if let Err(e) = queries::insert_event(
            &mut tx,
            ctx.tenant_id,
            connection.id,
            model::EventType::ConfigUpdated.as_str(),
            "success",
            None,
            membership_id,
        )
        .await
        {
            tracing::error!(%e, "update_integration_config: insert_event config failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to update integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        if let Err(e) = crate::audit::record_config_updated_in_tx(
            &mut tx,
            principal.user_id,
            ctx.tenant_id,
            connection.id,
            &catalog.slug,
        )
        .await
        {
            tracing::error!(%e, "update_integration_config: audit config failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to update integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    if secrets_changed {
        if let Err(e) = queries::insert_event(
            &mut tx,
            ctx.tenant_id,
            connection.id,
            model::EventType::SecretRotated.as_str(),
            "success",
            None,
            membership_id,
        )
        .await
        {
            tracing::error!(%e, "update_integration_config: insert_event secret failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to update integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        if let Err(e) = crate::audit::record_secret_rotated_in_tx(
            &mut tx,
            principal.user_id,
            ctx.tenant_id,
            connection.id,
            &catalog.slug,
        )
        .await
        {
            tracing::error!(%e, "update_integration_config: audit secret failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to update integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "update_integration_config: commit failed");
        return ApiError::internal_error("Failed to update integration")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let detail = build_detail_dto_for_connection(
        &pool,
        &config,
        &master,
        ctx.tenant_id,
        &catalog,
        connection.id,
        None,
    )
    .await;
    (StatusCode::OK, Json(detail)).into_response()
}

#[utoipa::path(
    post,
    path = "/tenant/integrations/{slug}/disconnect",
    tag = "integrations",
    operation_id = "disconnect_integration",
    summary = "Disconnect an active integration",
    params(
        ("slug" = String, Path, description = "Catalog entry slug"),
    ),
    responses(
        (status = 200, description = "Integration disconnected.", body = IntegrationDetailDto),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Unknown integration slug.", body = ErrorEnvelope),
        (status = 409, description = "Integration is not actively connected.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn disconnect_integration(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Extension(config): Extension<Arc<config::AppConfig>>,
    Path(slug): Path<String>,
) -> Response {
    let master = match load_master_key(&config) {
        Ok(m) => m,
        Err(resp) => return resp,
    };

    let catalog = match queries::find_catalog_by_slug(&pool, &slug).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return ApiError::not_found("Integration not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "disconnect_integration: find_catalog_by_slug failed");
            return ApiError::internal_error("Failed to disconnect integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let connection = match queries::find_connection(&pool, ctx.tenant_id, catalog.id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return ApiError::new_with_code(
                StatusCode::CONFLICT,
                "integration_not_connected",
                "Integration is not actively connected",
            )
            .with_request_id(&ctx.request_id)
            .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "disconnect_integration: find_connection failed");
            return ApiError::internal_error("Failed to disconnect integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if !connection.is_active {
        return ApiError::new_with_code(
            StatusCode::CONFLICT,
            "integration_not_connected",
            "Integration is not actively connected",
        )
        .with_request_id(&ctx.request_id)
        .into_response();
    }

    let membership_id = match active_membership_id(&pool, ctx.tenant_id, principal.user_id).await {
        Ok(Some(m)) => Some(m),
        Ok(None) => None,
        Err(e) => {
            tracing::error!(%e, "disconnect_integration: membership lookup failed");
            return ApiError::internal_error("Failed to disconnect integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "disconnect_integration: begin tx failed");
            return ApiError::internal_error("Failed to disconnect integration")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if let Err(e) = queries::delete_secrets_for_connection(&mut tx, connection.id).await {
        tracing::error!(%e, "disconnect_integration: delete_secrets_for_connection failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to disconnect integration")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(e) =
        queries::deactivate_connection(&mut tx, connection.id, membership_id).await
    {
        tracing::error!(%e, "disconnect_integration: deactivate_connection failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to disconnect integration")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(e) = queries::insert_event(
        &mut tx,
        ctx.tenant_id,
        connection.id,
        model::EventType::Disconnected.as_str(),
        "success",
        None,
        membership_id,
    )
    .await
    {
        tracing::error!(%e, "disconnect_integration: insert_event failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to disconnect integration")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(e) = crate::audit::record_disconnected_in_tx(
        &mut tx,
        principal.user_id,
        ctx.tenant_id,
        connection.id,
        &catalog.slug,
    )
    .await
    {
        tracing::error!(%e, "disconnect_integration: audit failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to disconnect integration")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "disconnect_integration: commit failed");
        return ApiError::internal_error("Failed to disconnect integration")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let detail = build_detail_dto_for_connection(
        &pool,
        &config,
        &master,
        ctx.tenant_id,
        &catalog,
        connection.id,
        None,
    )
    .await;
    (StatusCode::OK, Json(detail)).into_response()
}
