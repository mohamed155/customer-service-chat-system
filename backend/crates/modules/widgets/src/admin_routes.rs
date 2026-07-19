use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Extension;
use axum::Json;
use identity::Principal;
use kernel::{ApiError, ApiJson, ErrorEnvelope};
use serde_json::json;
use sqlx::PgPool;
use tenancy::TenantContext;
use uuid::Uuid;

use crate::audit;
use crate::model::{
    CreateWidgetInstancePayload, UpdateWidgetInstancePayload, WidgetInstanceDto, WidgetInstanceRow,
    WidgetSnippetResponse,
};
use crate::queries;

fn row_to_dto(row: WidgetInstanceRow) -> WidgetInstanceDto {
    WidgetInstanceDto {
        id: row.id,
        public_id: row.public_id,
        name: row.name,
        display_name: row.display_name,
        primary_color: row.primary_color,
        welcome_message: row.welcome_message,
        position: row.position,
        theme: row.theme,
        enabled: row.enabled,
        allowed_domains: row.allowed_domains,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn validate_instance_fields(
    name: &str,
    display_name: &str,
    primary_color: Option<&str>,
    welcome_message: Option<&str>,
    position: Option<&str>,
    theme: Option<&str>,
    allowed_domains: &[String],
) -> Result<(), Vec<serde_json::Value>> {
    let mut errors: Vec<serde_json::Value> = Vec::new();

    let name = name.trim();
    if name.is_empty() || name.len() > 80 {
        errors.push(json!({
            "field": "name",
            "code": "invalid_length",
            "message": "Name must be between 1 and 80 characters"
        }));
    }

    let display_name = display_name.trim();
    if display_name.is_empty() || display_name.len() > 80 {
        errors.push(json!({
            "field": "displayName",
            "code": "invalid_length",
            "message": "Display name must be between 1 and 80 characters"
        }));
    }

    if let Some(color) = primary_color {
        if !color.starts_with('#') || color.len() != 7 {
            errors.push(json!({
                "field": "primaryColor",
                "code": "invalid_format",
                "message": "Primary color must be a hex color like #4F46E5"
            }));
        }
    }

    if let Some(msg) = welcome_message {
        if msg.len() > 500 {
            errors.push(json!({
                "field": "welcomeMessage",
                "code": "too_long",
                "message": "Welcome message must be at most 500 characters"
            }));
        }
    }

    if let Some(pos) = position {
        if pos != "bottom-right" && pos != "bottom-left" {
            errors.push(json!({
                "field": "position",
                "code": "invalid_value",
                "message": "Position must be 'bottom-right' or 'bottom-left'"
            }));
        }
    }

    if let Some(th) = theme {
        if th != "light" && th != "dark" {
            errors.push(json!({
                "field": "theme",
                "code": "invalid_value",
                "message": "Theme must be 'light' or 'dark'"
            }));
        }
    }

    if allowed_domains.len() > 20 {
        errors.push(json!({
            "field": "allowedDomains",
            "code": "too_many",
            "message": "At most 20 allowed domains"
        }));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// `GET /tenant/widgets` — list widget instances
#[utoipa::path(
    get,
    path = "/tenant/widgets",
    tag = "widgets",
    operation_id = "list_widget_instances",
    summary = "List widget instances",
    responses(
        (status = 200, description = "List of widget instances.", body = Vec<WidgetInstanceDto>),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn list_instances(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
) -> Response {
    let rows = match queries::list_instances(&pool, ctx.tenant_id).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(%e, "list_instances: db error");
            return ApiError::internal_error("Failed to list widget instances")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };
    let dtos: Vec<WidgetInstanceDto> = rows.into_iter().map(row_to_dto).collect();
    (StatusCode::OK, Json(json!({ "data": dtos }))).into_response()
}

/// `POST /tenant/widgets` — create a widget instance
#[utoipa::path(
    post,
    path = "/tenant/widgets",
    tag = "widgets",
    operation_id = "create_widget_instance",
    summary = "Create a widget instance",
    request_body = CreateWidgetInstancePayload,
    responses(
        (status = 201, description = "Widget instance created.", body = WidgetInstanceDto),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn create_instance(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    ApiJson(payload): ApiJson<CreateWidgetInstancePayload>,
) -> Response {
    let name = payload.name.trim().to_string();
    let display_name = payload
        .display_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Support".into());
    let primary_color = payload.primary_color.as_deref();
    let welcome_message = payload.welcome_message.as_deref();
    let position = payload.position.as_deref();
    let theme = payload.theme.as_deref();
    let _enabled = payload.enabled.unwrap_or(true);
    let allowed_domains = payload.allowed_domains.unwrap_or_default();

    if let Err(errors) = validate_instance_fields(
        &name,
        &display_name,
        primary_color,
        welcome_message,
        position,
        theme,
        &allowed_domains,
    ) {
        return ApiError::unprocessable_entity("Validation failed")
            .with_details(errors)
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let public_id = queries::generate_public_id();
    let primary_color = primary_color.filter(|s| !s.is_empty()).unwrap_or("#4F46E5");
    let welcome_message = welcome_message
        .filter(|s| !s.is_empty())
        .unwrap_or("Hi! How can we help?");
    let position = position.filter(|s| !s.is_empty()).unwrap_or("bottom-right");
    let theme = theme.filter(|s| !s.is_empty()).unwrap_or("light");

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "create_instance: begin tx failed");
            return ApiError::internal_error("Failed to create widget instance")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let instance = match queries::insert_instance(
        &pool,
        ctx.tenant_id,
        &public_id,
        &name,
        &display_name,
        Some(primary_color),
        Some(welcome_message),
        Some(position),
        Some(theme),
        &allowed_domains,
    )
    .await
    {
        Ok(i) => i,
        Err(e) => {
            tracing::error!(%e, "create_instance: insert failed");
            return ApiError::internal_error("Failed to create widget instance")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if let Err(e) = audit::record_instance_created_in_tx(
        &mut tx,
        principal.user_id,
        ctx.tenant_id,
        instance.id,
        &instance.name,
    )
    .await
    {
        tracing::error!(%e, "create_instance: audit failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to create widget instance")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "create_instance: commit failed");
        return ApiError::internal_error("Failed to create widget instance")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    (
        StatusCode::CREATED,
        Json(json!({ "data": row_to_dto(instance) })),
    )
        .into_response()
}

/// `GET /tenant/widgets/{id}` — get a widget instance
#[utoipa::path(
    get,
    path = "/tenant/widgets/{id}",
    tag = "widgets",
    operation_id = "get_widget_instance",
    summary = "Get a widget instance",
    params(
        ("id" = Uuid, Path, description = "Widget instance ID"),
    ),
    responses(
        (status = 200, description = "Widget instance.", body = WidgetInstanceDto),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Widget instance not found.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_instance(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
    Path(instance_id): Path<Uuid>,
) -> Response {
    let instance = match queries::find_instance_by_id(&pool, ctx.tenant_id, instance_id).await {
        Ok(Some(i)) => i,
        Ok(None) => {
            return ApiError::not_found("Widget instance not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "get_instance: db error");
            return ApiError::internal_error("Failed to get widget instance")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    (
        StatusCode::OK,
        Json(json!({ "data": row_to_dto(instance) })),
    )
        .into_response()
}

/// `PUT /tenant/widgets/{id}` — update a widget instance
#[utoipa::path(
    put,
    path = "/tenant/widgets/{id}",
    tag = "widgets",
    operation_id = "update_widget_instance",
    summary = "Update a widget instance",
    params(
        ("id" = Uuid, Path, description = "Widget instance ID"),
    ),
    request_body = UpdateWidgetInstancePayload,
    responses(
        (status = 200, description = "Widget instance updated.", body = WidgetInstanceDto),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Widget instance not found.", body = ErrorEnvelope),
        (status = 422, description = "Validation failed.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn update_instance(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Path(instance_id): Path<Uuid>,
    ApiJson(payload): ApiJson<UpdateWidgetInstancePayload>,
) -> Response {
    let name = payload.name.trim().to_string();
    let display_name = payload.display_name.trim().to_string();
    let primary_color = payload.primary_color.as_deref();
    let welcome_message = payload.welcome_message.as_deref();
    let position = payload.position.as_deref();
    let theme = payload.theme.as_deref();
    let allowed_domains = payload.allowed_domains;

    if let Err(errors) = validate_instance_fields(
        &name,
        &display_name,
        primary_color,
        welcome_message,
        position,
        theme,
        &allowed_domains,
    ) {
        return ApiError::unprocessable_entity("Validation failed")
            .with_details(errors)
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "update_instance: begin tx failed");
            return ApiError::internal_error("Failed to update widget instance")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let updated = match queries::update_instance(
        &pool,
        ctx.tenant_id,
        instance_id,
        &name,
        &display_name,
        primary_color,
        welcome_message,
        position,
        theme,
        payload.enabled,
        &allowed_domains,
    )
    .await
    {
        Ok(Some(i)) => i,
        Ok(None) => {
            let _ = tx.rollback().await;
            return ApiError::not_found("Widget instance not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "update_instance: update failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to update widget instance")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if let Err(e) = audit::record_instance_updated_in_tx(
        &mut tx,
        principal.user_id,
        ctx.tenant_id,
        instance_id,
        &updated.name,
    )
    .await
    {
        tracing::error!(%e, "update_instance: audit failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to update widget instance")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "update_instance: commit failed");
        return ApiError::internal_error("Failed to update widget instance")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    (StatusCode::OK, Json(json!({ "data": row_to_dto(updated) }))).into_response()
}

/// `DELETE /tenant/widgets/{id}` — soft-delete a widget instance
#[utoipa::path(
    delete,
    path = "/tenant/widgets/{id}",
    tag = "widgets",
    operation_id = "delete_widget_instance",
    summary = "Soft-delete a widget instance",
    params(
        ("id" = Uuid, Path, description = "Widget instance ID"),
    ),
    responses(
        (status = 204, description = "Widget instance deleted."),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Widget instance not found.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn delete_instance(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Path(instance_id): Path<Uuid>,
) -> Response {
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "delete_instance: begin tx failed");
            return ApiError::internal_error("Failed to delete widget instance")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let instance = match sqlx::query_as::<_, WidgetInstanceRow>(
        "SELECT id, tenant_id, public_id, name, display_name, primary_color, \
                welcome_message, position, theme, enabled, allowed_domains, \
                created_at, updated_at \
         FROM widget_instances \
         WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL \
         FOR UPDATE",
    )
    .bind(instance_id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(Some(i)) => i,
        Ok(None) => {
            let _ = tx.rollback().await;
            return ApiError::not_found("Widget instance not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "delete_instance: fetch existing failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to delete widget instance")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    match queries::soft_delete_instance(&pool, ctx.tenant_id, instance_id).await {
        Ok(true) => {}
        Ok(false) => {
            return ApiError::not_found("Widget instance not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "delete_instance: soft delete failed");
            return ApiError::internal_error("Failed to delete widget instance")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    if let Err(e) = audit::record_instance_deleted_in_tx(
        &mut tx,
        principal.user_id,
        ctx.tenant_id,
        instance_id,
        &instance.name,
    )
    .await
    {
        tracing::error!(%e, "delete_instance: audit failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to delete widget instance")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "delete_instance: commit failed");
        return ApiError::internal_error("Failed to delete widget instance")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

/// `GET /tenant/widgets/{id}/snippet` — get embed snippet
#[utoipa::path(
    get,
    path = "/tenant/widgets/{id}/snippet",
    tag = "widgets",
    operation_id = "get_widget_snippet",
    summary = "Get widget embed snippet",
    params(
        ("id" = Uuid, Path, description = "Widget instance ID"),
    ),
    responses(
        (status = 200, description = "Embed snippet.", body = WidgetSnippetResponse),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Widget instance not found.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_snippet(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
    Extension(config): Extension<std::sync::Arc<config::AppConfig>>,
    Path(instance_id): Path<Uuid>,
) -> Response {
    let instance = match queries::find_instance_by_id(&pool, ctx.tenant_id, instance_id).await {
        Ok(Some(i)) => i,
        Ok(None) => {
            return ApiError::not_found("Widget instance not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "get_snippet: db error");
            return ApiError::internal_error("Failed to get widget snippet")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let host = config.public_dashboard_url.trim_end_matches('/');
    let snippet = format!(
        r#"<script src="{host}/widget.js" data-widget-id="{}" async></script>"#,
        instance.public_id
    );

    (
        StatusCode::OK,
        Json(json!({ "data": { "snippet": snippet } })),
    )
        .into_response()
}
