use axum::extract::{Multipart, Path, Query, State};
use axum::http::header;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Extension;
use axum::Json;
use base64::engine::general_purpose;
use base64::Engine;
use chrono::{DateTime, Utc};
use identity::Principal;
use kernel::{ApiError, ApiJson, ErrorEnvelope};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use storage::{ObjectStorage, StorageError};
use tenancy::audit;
use tenancy::TenantContext;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::store;
use crate::upload;
use crate::validate;

// ── DTOs ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeItemSummaryDto {
    pub id: Uuid,
    pub item_type: String,
    pub title: String,
    pub status: String,
    pub category_id: Option<Uuid>,
    pub category_name: Option<String>,
    pub created_by_display: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMetaDto {
    pub original_filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeItemDetailDto {
    pub id: Uuid,
    pub item_type: String,
    pub title: String,
    pub status: String,
    pub category_id: Option<Uuid>,
    pub category_name: Option<String>,
    pub created_by_display: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tags: Vec<String>,
    pub body: Option<String>,
    pub source: String,
    pub created_by_user_id: Option<Uuid>,
    pub document: Option<DocumentMetaDto>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CategoryRefDto {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ItemListResponse {
    pub items: Vec<KnowledgeItemSummaryDto>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateItemPayload {
    pub title: String,
    pub body: Option<String>,
    pub item_type: String,
    pub category_id: Option<Uuid>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateItemPayload {
    pub title: Option<String>,
    pub body: Option<String>,
    pub item_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category_id: Option<serde_json::Value>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetStatusPayload {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetStatusResponse {
    pub id: Uuid,
    pub status: String,
    pub changed: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeCategoryDto {
    pub id: Uuid,
    pub name: String,
    pub item_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateCategoryPayload {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RenameCategoryPayload {
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(default)]
pub struct ItemListQuery {
    pub limit: Option<i64>,
    pub before: Option<String>,
    #[serde(rename = "type")]
    pub item_type: Option<String>,
    pub status: Option<String>,
    pub category_id: Option<Uuid>,
    pub tag: Option<String>,
    pub q: Option<String>,
}

// ── Pagination Cursor ─────────────────────────────────────────────────────

pub struct PaginationCursor {
    pub updated_at: DateTime<Utc>,
    pub id: Uuid,
}

impl PaginationCursor {
    pub fn encode(&self) -> String {
        let data = format!("{}|{}", self.updated_at.format("%+"), self.id);
        general_purpose::STANDARD.encode(data)
    }

    pub fn decode(s: &str) -> Option<Self> {
        let bytes = general_purpose::STANDARD.decode(s.as_bytes()).ok()?;
        let s = std::str::from_utf8(&bytes).ok()?;
        let (ts_str, id_str) = s.split_once('|')?;
        let updated_at = DateTime::parse_from_rfc3339(ts_str)
            .ok()?
            .with_timezone(&Utc);
        let id = Uuid::parse_str(id_str).ok()?;
        Some(Self { updated_at, id })
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn build_tag_map(tags: &[store::KnowledgeItemTagRow]) -> HashMap<Uuid, Vec<String>> {
    let mut map: HashMap<Uuid, Vec<String>> = HashMap::new();
    for tag in tags {
        map.entry(tag.item_id).or_default().push(tag.tag.clone());
    }
    map
}

async fn get_category_name(pool: &PgPool, category_id: Option<Uuid>) -> Option<String> {
    let id = category_id?;
    sqlx::query_scalar::<_, String>("SELECT name FROM knowledge_categories WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()?
}

async fn load_tags_for_item(pool: &PgPool, item_id: Uuid) -> sqlx::Result<Vec<String>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT tag FROM knowledge_item_tags WHERE item_id = $1 ORDER BY tag")
            .bind(item_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

// ── T014: list_items ──────────────────────────────────────────────────────

/// `GET /tenant/knowledge/items` — list knowledge items
#[utoipa::path(
    get,
    path = "/tenant/knowledge/items",
    tag = "tenant-knowledge",
    operation_id = "list_knowledge_items",
    summary = "List knowledge items",
    params(ItemListQuery),
    responses(
        (status = 200, description = "Page of knowledge items.", body = ItemListResponse),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn list_items(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
    Query(query): Query<ItemListQuery>,
) -> Response {
    let limit = query.limit.map(|l| l.clamp(1, 50)).unwrap_or(20);

    let before = match &query.before {
        Some(cursor_str) => match PaginationCursor::decode(cursor_str) {
            Some(c) => Some((c.updated_at, c.id)),
            None => {
                return ApiError::validation_failed("Invalid cursor")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        },
        None => None,
    };

    let item_type_filter = query
        .item_type
        .as_ref()
        .and_then(|s| s.parse::<validate::ItemType>().ok());
    let status_filter = query
        .status
        .as_ref()
        .and_then(|s| s.parse::<validate::ItemStatus>().ok());

    let filters = store::ItemFilters {
        item_type: item_type_filter,
        status: status_filter,
        category_id: query.category_id,
        tag: query.tag.clone(),
        q: query.q.clone(),
    };

    let (items, tags, has_more) =
        match store::list_items(&pool, ctx.tenant_id, filters, limit, before).await {
            Ok(result) => result,
            Err(e) => {
                tracing::error!(%e, "list_items: store::list_items failed");
                return ApiError::internal_error("Failed to list items")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        };

    let tag_map = build_tag_map(&tags);

    let cat_ids: Vec<Uuid> = items.iter().filter_map(|i| i.category_id).collect();
    let cat_names: HashMap<Uuid, String> = if cat_ids.is_empty() {
        HashMap::new()
    } else {
        match sqlx::query_as::<_, (Uuid, String)>(
            "SELECT id, name FROM knowledge_categories WHERE id = ANY($1)",
        )
        .bind(&cat_ids)
        .fetch_all(&pool)
        .await
        {
            Ok(rows) => rows.into_iter().collect(),
            Err(e) => {
                tracing::error!(%e, "list_items: load category names failed");
                return ApiError::internal_error("Failed to list items")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        }
    };

    let next_cursor = if has_more {
        items.last().map(|item| {
            PaginationCursor {
                updated_at: item.updated_at,
                id: item.id,
            }
            .encode()
        })
    } else {
        None
    };

    let dtos: Vec<KnowledgeItemSummaryDto> = items
        .into_iter()
        .map(|item| {
            let tags = tag_map.get(&item.id).cloned().unwrap_or_default();
            KnowledgeItemSummaryDto {
                id: item.id,
                item_type: item.item_type,
                title: item.title,
                status: item.status,
                category_id: item.category_id,
                category_name: item
                    .category_id
                    .and_then(|cid| cat_names.get(&cid).cloned()),
                created_by_display: item.created_by_display,
                created_at: item.created_at,
                updated_at: item.updated_at,
                tags,
            }
        })
        .collect();

    (
        StatusCode::OK,
        Json(ItemListResponse {
            items: dtos,
            has_more,
            next_cursor,
        }),
    )
        .into_response()
}

// ── T015: create_item ─────────────────────────────────────────────────────

/// `POST /tenant/knowledge/items` — create article/FAQ
#[utoipa::path(
    post,
    path = "/tenant/knowledge/items",
    tag = "tenant-knowledge",
    operation_id = "create_knowledge_item",
    summary = "Create a knowledge item",
    request_body = CreateItemPayload,
    responses(
        (status = 201, description = "Item created.", body = KnowledgeItemDetailDto),
        (status = 400, description = "Validation failed.", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 422, description = "Unprocessable entity.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn create_item(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    ApiJson(payload): ApiJson<CreateItemPayload>,
) -> Response {
    let start = std::time::Instant::now();
    if let Some(issue) = validate::validate_title(&payload.title) {
        return ApiError::unprocessable_entity("Validation failed")
            .with_details(vec![json!(issue)])
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if payload.item_type == "document" {
        return ApiError::validation_failed("Documents can only be created via upload")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if payload.item_type != "article" && payload.item_type != "faq" {
        return ApiError::validation_failed("Item type must be 'article' or 'faq'")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let sanitized_body = payload.body.as_ref().map(|b| validate::sanitize_body(b));
    if let Some(ref body) = sanitized_body {
        if let Some(issue) = validate::validate_body(body) {
            return ApiError::unprocessable_entity("Validation failed")
                .with_details(vec![json!(issue)])
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    let tags = match payload.tags {
        Some(ref raw) => match validate::normalize_tags(raw) {
            Ok(t) => t,
            Err(issue) => {
                return ApiError::unprocessable_entity("Validation failed")
                    .with_details(vec![json!(issue)])
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        },
        None => Vec::new(),
    };

    let category_name = if let Some(cid) = payload.category_id {
        match sqlx::query_scalar::<_, String>(
            "SELECT name FROM knowledge_categories WHERE id = $1 AND tenant_id = $2",
        )
        .bind(cid)
        .bind(ctx.tenant_id)
        .fetch_optional(&pool)
        .await
        {
            Ok(Some(name)) => Some(name),
            Ok(None) => {
                return ApiError::validation_failed("Category not found for this tenant")
                    .with_details(vec![json!({
                        "field": "categoryId",
                        "code": "not_found",
                        "message": "Category does not exist"
                    })])
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
            Err(e) => {
                tracing::error!(%e, "create_item: category lookup failed");
                return ApiError::internal_error("Failed to create item")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        }
    } else {
        None
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "create_item: begin tx failed");
            return ApiError::internal_error("Failed to create item")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let item = match store::create_item_in_tx(
        &mut tx,
        ctx.tenant_id,
        &payload.item_type,
        &payload.title,
        sanitized_body.as_deref(),
        "authored",
        payload.category_id,
        Some(principal.user_id),
        &principal.display_name,
    )
    .await
    {
        Ok(item) => item,
        Err(e) => {
            tracing::error!(%e, "create_item: create_item_in_tx failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to create item")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if !tags.is_empty() {
        if let Err(e) = store::replace_tags_in_tx(&mut tx, item.id, ctx.tenant_id, &tags).await {
            tracing::error!(%e, "create_item: replace_tags_in_tx failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to create item")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    if let Err(e) = audit::record_in_tx(
        &mut tx,
        "knowledge_item.created",
        Some(principal.user_id),
        Some(ctx.tenant_id),
        "knowledge_item",
        Some(&item.id.to_string()),
        &json!({
            "itemId": item.id,
            "itemType": item.item_type,
            "source": item.source,
        }),
    )
    .await
    {
        tracing::error!(%e, "create_item: audit failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to create item")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "create_item: commit failed");
        return ApiError::internal_error("Failed to create item")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    tracing::info!(
        item_id = %item.id,
        action = "knowledge_item.created",
        request_id = %ctx.request_id,
        latency_us = %start.elapsed().as_micros(),
        "knowledge item created"
    );

    let dto = KnowledgeItemDetailDto {
        id: item.id,
        item_type: item.item_type,
        title: item.title,
        status: item.status,
        category_id: item.category_id,
        category_name,
        created_by_display: item.created_by_display,
        created_at: item.created_at,
        updated_at: item.updated_at,
        tags,
        body: item.body,
        source: item.source,
        created_by_user_id: item.created_by_user_id,
        document: None,
    };

    (StatusCode::CREATED, Json(dto)).into_response()
}

// ── T016: get_item ────────────────────────────────────────────────────────

/// `GET /tenant/knowledge/items/{id}` — get item detail
#[utoipa::path(
    get,
    path = "/tenant/knowledge/items/{id}",
    tag = "tenant-knowledge",
    operation_id = "get_knowledge_item",
    summary = "Get a knowledge item",
    params(
        ("id" = Uuid, Path, description = "Knowledge item ID"),
    ),
    responses(
        (status = 200, description = "Item detail.", body = KnowledgeItemDetailDto),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Item not found.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn get_item(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
    Path(item_id): Path<Uuid>,
) -> Response {
    let item = match store::get_item(&pool, ctx.tenant_id, item_id).await {
        Ok(Some(item)) => item,
        Ok(None) => {
            return ApiError::not_found("Item not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "get_item: store::get_item failed");
            return ApiError::internal_error("Failed to get item")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let tags = match load_tags_for_item(&pool, item_id).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(%e, "get_item: load_tags_for_item failed");
            return ApiError::internal_error("Failed to get item")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let category_name = get_category_name(&pool, item.category_id).await;

    let document = if item.item_type == "document" {
        match store::get_document(&pool, ctx.tenant_id, item_id).await {
            Ok(Some(doc)) => Some(DocumentMetaDto {
                original_filename: doc.original_filename,
                content_type: doc.content_type,
                size_bytes: doc.size_bytes,
                created_at: doc.created_at,
            }),
            Ok(None) => None,
            Err(e) => {
                tracing::error!(%e, "get_item: get_document failed");
                None
            }
        }
    } else {
        None
    };

    let dto = KnowledgeItemDetailDto {
        id: item.id,
        item_type: item.item_type,
        title: item.title,
        status: item.status,
        category_id: item.category_id,
        category_name,
        created_by_display: item.created_by_display,
        created_at: item.created_at,
        updated_at: item.updated_at,
        tags,
        body: item.body,
        source: item.source,
        created_by_user_id: item.created_by_user_id,
        document,
    };

    (StatusCode::OK, Json(dto)).into_response()
}

// ── T016: update_item ─────────────────────────────────────────────────────

/// `PATCH /tenant/knowledge/items/{id}` — update a knowledge item
#[utoipa::path(
    patch,
    path = "/tenant/knowledge/items/{id}",
    tag = "tenant-knowledge",
    operation_id = "update_knowledge_item",
    summary = "Update a knowledge item",
    params(
        ("id" = Uuid, Path, description = "Knowledge item ID"),
    ),
    request_body = UpdateItemPayload,
    responses(
        (status = 200, description = "Item updated.", body = KnowledgeItemDetailDto),
        (status = 400, description = "Validation failed.", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Item not found.", body = ErrorEnvelope),
        (status = 422, description = "Unprocessable entity.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn update_item(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Path(item_id): Path<Uuid>,
    ApiJson(payload): ApiJson<UpdateItemPayload>,
) -> Response {
    let start = std::time::Instant::now();
    if let Some(ref title) = payload.title {
        if let Some(issue) = validate::validate_title(title) {
            return ApiError::unprocessable_entity("Validation failed")
                .with_details(vec![json!(issue)])
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    let sanitized_body = payload.body.as_ref().map(|b| validate::sanitize_body(b));
    if let Some(ref body) = sanitized_body {
        if let Some(issue) = validate::validate_body(body) {
            return ApiError::unprocessable_entity("Validation failed")
                .with_details(vec![json!(issue)])
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    let tags = match payload.tags {
        Some(ref raw) => match validate::normalize_tags(raw) {
            Ok(t) => Some(t),
            Err(issue) => {
                return ApiError::unprocessable_entity("Validation failed")
                    .with_details(vec![json!(issue)])
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        },
        None => None,
    };

    let category_action: Option<Option<Uuid>> = match payload.category_id {
        None => None,
        Some(v) if v.is_null() => Some(None),
        Some(v) => {
            let s = match v.as_str() {
                Some(s) => s,
                None => {
                    return ApiError::unprocessable_entity("Validation failed")
                        .with_details(vec![json!({
                            "field": "categoryId",
                            "code": "invalid_type",
                            "message": "categoryId must be a UUID string or null"
                        })])
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
            };
            match Uuid::parse_str(s) {
                Ok(id) => Some(Some(id)),
                Err(_) => {
                    return ApiError::unprocessable_entity("Validation failed")
                        .with_details(vec![json!({
                            "field": "categoryId",
                            "code": "invalid_uuid",
                            "message": "Invalid categoryId format"
                        })])
                        .with_request_id(&ctx.request_id)
                        .into_response();
                }
            }
        }
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "update_item: begin tx failed");
            return ApiError::internal_error("Failed to update item")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let existing = match sqlx::query_as::<_, store::KnowledgeItemRow>(
        "SELECT * FROM knowledge_items WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
    )
    .bind(item_id)
    .bind(ctx.tenant_id)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => {
            let _ = tx.rollback().await;
            return ApiError::not_found("Item not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "update_item: fetch existing failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to update item")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if sanitized_body.is_some() && existing.item_type == "document" {
        let _ = tx.rollback().await;
        return ApiError::unprocessable_entity("Validation failed")
            .with_details(vec![json!({
                "field": "body",
                "code": "invalid_for_document",
                "message": "Body cannot be set on document items"
            })])
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Some(ref new_type) = payload.item_type {
        if existing.item_type == "document" && new_type != "document" {
            let _ = tx.rollback().await;
            return ApiError::unprocessable_entity("Validation failed")
                .with_details(vec![json!({
                    "field": "itemType",
                    "code": "invalid_transition",
                    "message": "Cannot change item type from 'document' to a non-document type"
                })])
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        if existing.item_type != "document" && new_type == "document" {
            let _ = tx.rollback().await;
            return ApiError::unprocessable_entity("Validation failed")
                .with_details(vec![json!({
                    "field": "itemType",
                    "code": "invalid_transition",
                    "message": "Cannot change item type to 'document'"
                })])
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    let updated = match store::update_item_in_tx(
        &mut tx,
        ctx.tenant_id,
        item_id,
        payload.title.as_deref(),
        sanitized_body.as_deref(),
        payload.item_type.as_deref(),
        category_action,
    )
    .await
    {
        Ok(Some(item)) => item,
        Ok(None) => {
            let _ = tx.rollback().await;
            return ApiError::not_found("Item not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "update_item: update_item_in_tx failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to update item")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if let Some(ref normalized_tags) = tags {
        if let Err(e) =
            store::replace_tags_in_tx(&mut tx, item_id, ctx.tenant_id, normalized_tags).await
        {
            tracing::error!(%e, "update_item: replace_tags_in_tx failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to update item")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    let details = json!({
        "itemId": updated.id,
        "itemType": updated.item_type,
    });
    if let Err(e) = audit::record_in_tx(
        &mut tx,
        "knowledge_item.updated",
        Some(principal.user_id),
        Some(ctx.tenant_id),
        "knowledge_item",
        Some(&updated.id.to_string()),
        &details,
    )
    .await
    {
        tracing::error!(%e, "update_item: audit failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to update item")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "update_item: commit failed");
        return ApiError::internal_error("Failed to update item")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    tracing::info!(
        item_id = %updated.id,
        action = "knowledge_item.updated",
        request_id = %ctx.request_id,
        latency_us = %start.elapsed().as_micros(),
        "knowledge item updated"
    );

    let updated_tags = match load_tags_for_item(&pool, item_id).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(%e, "update_item: load final tags failed");
            Vec::new()
        }
    };

    let category_name = get_category_name(&pool, updated.category_id).await;

    let dto = KnowledgeItemDetailDto {
        id: updated.id,
        item_type: updated.item_type,
        title: updated.title,
        status: updated.status,
        category_id: updated.category_id,
        category_name,
        created_by_display: updated.created_by_display,
        created_at: updated.created_at,
        updated_at: updated.updated_at,
        tags: updated_tags,
        body: updated.body,
        source: updated.source,
        created_by_user_id: updated.created_by_user_id,
        document: None,
    };

    (StatusCode::OK, Json(dto)).into_response()
}

// ── T024: set_item_status ──────────────────────────────────────────────────

/// `POST /tenant/knowledge/items/{id}/status` — set item status (workflow)
#[utoipa::path(
    post,
    path = "/tenant/knowledge/items/{id}/status",
    tag = "tenant-knowledge",
    operation_id = "set_knowledge_item_status",
    summary = "Set knowledge item status",
    params(
        ("id" = Uuid, Path, description = "Knowledge item ID"),
    ),
    request_body = SetStatusPayload,
    responses(
        (status = 200, description = "Status updated or no-op.", body = SetStatusResponse),
        (status = 400, description = "Validation failed.", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Item not found.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn set_item_status(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    Path(item_id): Path<Uuid>,
    ApiJson(payload): ApiJson<SetStatusPayload>,
) -> Response {
    let start = std::time::Instant::now();
    let item = match store::get_item(&pool, ctx.tenant_id, item_id).await {
        Ok(Some(item)) => item,
        Ok(None) => {
            return ApiError::not_found("Item not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "set_item_status: get_item failed");
            return ApiError::internal_error("Failed to update item status")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let new_status: validate::ItemStatus = match payload.status.parse() {
        Ok(s) => s,
        Err(_) => {
            return ApiError::validation_failed(
                "Invalid status value; allowed: 'draft', 'published', 'archived'",
            )
            .with_request_id(&ctx.request_id)
            .into_response();
        }
    };

    let current_status: validate::ItemStatus = match item.status.parse() {
        Ok(s) => s,
        Err(_) => {
            return ApiError::internal_error("Item has invalid persisted status")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let item_type: validate::ItemType = match item.item_type.parse() {
        Ok(t) => t,
        Err(_) => {
            return ApiError::internal_error("Item has invalid persisted type")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    match validate::check_transition(current_status, new_status, item_type, item.body.as_deref()) {
        Err(validate::TransitionError::Illegal { from, to }) => {
            return ApiError::validation_failed(format!(
                "Cannot transition from '{}' to '{}'; allowed transitions: \
                 draft\u{2192}published, published\u{2192}archived, archived\u{2192}draft",
                from.as_str(),
                to.as_str(),
            ))
            .with_request_id(&ctx.request_id)
            .into_response();
        }
        Err(validate::TransitionError::BodyRequired) => {
            return ApiError::validation_failed("Publishing requires content")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Ok(false) => {
            return (
                StatusCode::OK,
                Json(SetStatusResponse {
                    id: item_id,
                    status: item.status,
                    changed: false,
                    updated_at: item.updated_at,
                }),
            )
                .into_response();
        }
        Ok(true) => {}
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "set_item_status: begin tx failed");
            return ApiError::internal_error("Failed to update item status")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let updated =
        match store::set_status_in_tx(&mut tx, ctx.tenant_id, item_id, new_status.as_str()).await {
            Ok(Some(item)) => item,
            Ok(None) => {
                let _ = tx.rollback().await;
                return ApiError::not_found("Item not found")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
            Err(e) => {
                tracing::error!(%e, "set_item_status: set_status_in_tx failed");
                let _ = tx.rollback().await;
                return ApiError::internal_error("Failed to update item status")
                    .with_request_id(&ctx.request_id)
                    .into_response();
            }
        };

    let audit_action = match new_status {
        validate::ItemStatus::Published => "knowledge_item.published",
        validate::ItemStatus::Archived => "knowledge_item.archived",
        validate::ItemStatus::Draft => "knowledge_item.restored",
    };

    if let Err(e) = audit::record_in_tx(
        &mut tx,
        audit_action,
        Some(principal.user_id),
        Some(ctx.tenant_id),
        "knowledge_item",
        Some(&item_id.to_string()),
        &json!({
            "itemId": item_id,
            "itemType": item.item_type,
        }),
    )
    .await
    {
        tracing::error!(%e, "set_item_status: audit failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to update item status")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "set_item_status: commit failed");
        return ApiError::internal_error("Failed to update item status")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    tracing::info!(
        item_id = %item_id,
        action = %audit_action,
        request_id = %ctx.request_id,
        latency_us = %start.elapsed().as_micros(),
        "knowledge item status changed"
    );

    (
        StatusCode::OK,
        Json(SetStatusResponse {
            id: item_id,
            status: updated.status,
            changed: true,
            updated_at: updated.updated_at,
        }),
    )
        .into_response()
}

// ── T029: upload_document ──────────────────────────────────────────────────

/// `POST /tenant/knowledge/documents` — upload a document
#[utoipa::path(
    post,
    path = "/tenant/knowledge/documents",
    tag = "tenant-knowledge",
    operation_id = "upload_knowledge_document",
    summary = "Upload a knowledge document",
    request_body(content = String, description = "Multipart upload (file + optional fields)"),
    responses(
        (status = 201, description = "Document uploaded.", body = KnowledgeItemDetailDto),
        (status = 400, description = "Validation failed.", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn upload_document(
    State(pool): State<PgPool>,
    Extension(storage): Extension<Arc<dyn ObjectStorage>>,
    ctx: TenantContext,
    Extension(principal): Extension<Principal>,
    multipart: Multipart,
) -> Response {
    let start = std::time::Instant::now();
    let parsed = match upload::parse(multipart).await {
        Ok(p) => p,
        Err(issue) => {
            return ApiError::validation_failed("Upload validation failed")
                .with_details(vec![json!(issue)])
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let upload::ParsedUpload {
        filename,
        content_type,
        bytes,
        title,
        status,
        category_id,
        tags,
    } = parsed;

    let dto_tags = tags.clone();
    let dto_filename = filename.clone();
    let dto_content_type = content_type.clone();
    let file_size = bytes.len() as i64;
    let item_id = Uuid::new_v4();
    let storage_key = format!("{}/knowledge/{}", ctx.tenant_id, item_id);

    let persist_pool = pool.clone();
    let persist_title = title.clone();
    let persist_filename = filename.clone();
    let persist_content_type = content_type.clone();
    let persist_tags = tags.clone();
    let persist_storage_key = storage_key.clone();

    let result: Result<store::KnowledgeItemRow, upload::UploadFailure<sqlx::Error>> =
        upload::put_then_persist(
            &*storage,
            &storage_key,
            &content_type,
            bytes,
            || async move {
                let mut tx = persist_pool.begin().await?;

                let item = store::create_document_in_tx(
                    &mut tx,
                    ctx.tenant_id,
                    persist_title.as_deref().unwrap_or("Untitled"),
                    status.as_str(),
                    category_id,
                    Some(principal.user_id),
                    &principal.display_name,
                    &persist_storage_key,
                    &persist_filename,
                    &persist_content_type,
                    file_size,
                )
                .await?;

                if !persist_tags.is_empty() {
                    store::replace_tags_in_tx(&mut tx, item.id, ctx.tenant_id, &persist_tags)
                        .await?;
                }

                audit::record_in_tx(
                    &mut tx,
                    "knowledge_item.created",
                    Some(principal.user_id),
                    Some(ctx.tenant_id),
                    "knowledge_item",
                    Some(&item.id.to_string()),
                    &json!({
                        "itemId": item.id,
                        "itemType": "document",
                        "source": "uploaded",
                    }),
                )
                .await?;

                tx.commit().await?;
                Ok(item)
            },
        )
        .await;

    let item = match result {
        Ok(item) => item,
        Err(upload::UploadFailure::Storage(e)) => {
            tracing::error!(%e, "upload_document: storage failed");
            return ApiError::internal_error("Failed to upload document")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(upload::UploadFailure::Persist(e)) => {
            tracing::error!(%e, "upload_document: persist failed");
            return ApiError::internal_error("Failed to upload document")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    tracing::info!(
        item_id = %item.id,
        action = "knowledge_document.uploaded",
        request_id = %ctx.request_id,
        latency_us = %start.elapsed().as_micros(),
        "knowledge document uploaded"
    );

    let dto = KnowledgeItemDetailDto {
        id: item.id,
        item_type: item.item_type,
        title: item.title,
        status: item.status,
        category_id: item.category_id,
        category_name: get_category_name(&pool, item.category_id).await,
        created_by_display: item.created_by_display,
        created_at: item.created_at,
        updated_at: item.updated_at,
        tags: dto_tags,
        body: item.body,
        source: item.source,
        created_by_user_id: item.created_by_user_id,
        document: Some(DocumentMetaDto {
            original_filename: dto_filename,
            content_type: dto_content_type,
            size_bytes: file_size,
            created_at: item.created_at,
        }),
    };

    (StatusCode::CREATED, Json(dto)).into_response()
}

// ── T030: download_file ────────────────────────────────────────────────────

/// `GET /tenant/knowledge/items/{id}/file` — download document file
#[utoipa::path(
    get,
    path = "/tenant/knowledge/items/{id}/file",
    tag = "tenant-knowledge",
    operation_id = "download_knowledge_document",
    summary = "Download a knowledge document file",
    params(
        ("id" = Uuid, Path, description = "Knowledge item ID"),
    ),
    responses(
        (status = 200, description = "File content.", content_type = "application/octet-stream"),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Item or file not found.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn download_file(
    State(pool): State<PgPool>,
    Extension(storage): Extension<Arc<dyn ObjectStorage>>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
    Path(item_id): Path<Uuid>,
) -> Response {
    let doc = match store::get_document(&pool, ctx.tenant_id, item_id).await {
        Ok(Some(doc)) => doc,
        Ok(None) => {
            return ApiError::not_found("Document not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "download_file: get_document failed");
            return ApiError::internal_error("Failed to download file")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let item = match store::get_item(&pool, ctx.tenant_id, item_id).await {
        Ok(Some(item)) => item,
        Ok(None) => {
            return ApiError::not_found("Document not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "download_file: get_item failed");
            return ApiError::internal_error("Failed to download file")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if item.item_type != "document" {
        return ApiError::validation_failed("Item is not a document")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let (data, _) = match storage.get(&doc.storage_key).await {
        Ok(result) => result,
        Err(StorageError::NotFound) => {
            return ApiError::not_found("File not found in storage")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "download_file: storage.get failed");
            return ApiError::internal_error("Failed to download file")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let sanitized = validate::sanitize_filename(&doc.original_filename);
    let disposition = format!("attachment; filename=\"{}\"", sanitized);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, doc.content_type)
        .header(header::CONTENT_DISPOSITION, disposition)
        .body(axum::body::Body::from(data))
        .unwrap()
}

// ── T034: Category CRUD ────────────────────────────────────────────────────

/// `GET /tenant/knowledge/categories` — list categories
#[utoipa::path(
    get,
    path = "/tenant/knowledge/categories",
    tag = "tenant-knowledge",
    operation_id = "list_knowledge_categories",
    summary = "List knowledge categories",
    responses(
        (status = 200, description = "List of categories.", body = Vec<KnowledgeCategoryDto>),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn list_knowledge_categories(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
) -> Response {
    let categories = match store::list_categories(&pool, ctx.tenant_id).await {
        Ok(cats) => cats,
        Err(e) => {
            tracing::error!(%e, "list_knowledge_categories: store::list_categories failed");
            return ApiError::internal_error("Failed to list categories")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let dtos: Vec<KnowledgeCategoryDto> = categories
        .into_iter()
        .map(|c| KnowledgeCategoryDto {
            id: c.id,
            name: c.name,
            item_count: c.item_count,
            created_at: c.created_at,
            updated_at: c.updated_at,
        })
        .collect();

    (StatusCode::OK, Json(dtos)).into_response()
}

/// `POST /tenant/knowledge/categories` — create a category
#[utoipa::path(
    post,
    path = "/tenant/knowledge/categories",
    tag = "tenant-knowledge",
    operation_id = "create_knowledge_category",
    summary = "Create a knowledge category",
    request_body = CreateCategoryPayload,
    responses(
        (status = 201, description = "Category created.", body = KnowledgeCategoryDto),
        (status = 400, description = "Validation failed.", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 409, description = "Category already exists.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn create_knowledge_category(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
    ApiJson(payload): ApiJson<CreateCategoryPayload>,
) -> Response {
    let start = std::time::Instant::now();
    if payload.name.trim().is_empty() {
        return ApiError::validation_failed("Category name is required")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "create_knowledge_category: begin tx failed");
            return ApiError::internal_error("Failed to create category")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let category = match store::create_category_in_tx(&mut tx, ctx.tenant_id, &payload.name).await {
        Ok(cat) => cat,
        Err(store::CategoryError::Duplicate) => {
            let _ = tx.rollback().await;
            return ApiError::conflict("Category with this name already exists")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(store::CategoryError::NotFound) => {
            let _ = tx.rollback().await;
            tracing::error!("create_knowledge_category: unexpected NotFound from create");
            return ApiError::internal_error("Failed to create category")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(store::CategoryError::Db(e)) => {
            tracing::error!(%e, "create_knowledge_category: create_category_in_tx failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to create category")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "create_knowledge_category: commit failed");
        return ApiError::internal_error("Failed to create category")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    tracing::info!(
        item_id = %category.id,
        action = "knowledge_category.created",
        request_id = %ctx.request_id,
        latency_us = %start.elapsed().as_micros(),
        "knowledge category created"
    );

    let dto = KnowledgeCategoryDto {
        id: category.id,
        name: category.name,
        item_count: 0,
        created_at: category.created_at,
        updated_at: category.updated_at,
    };

    (StatusCode::CREATED, Json(dto)).into_response()
}

/// `PATCH /tenant/knowledge/categories/{category_id}` — rename a category
#[utoipa::path(
    patch,
    path = "/tenant/knowledge/categories/{category_id}",
    tag = "tenant-knowledge",
    operation_id = "rename_knowledge_category",
    summary = "Rename a knowledge category",
    params(
        ("category_id" = Uuid, Path, description = "Category ID"),
    ),
    request_body = RenameCategoryPayload,
    responses(
        (status = 200, description = "Category renamed.", body = KnowledgeCategoryDto),
        (status = 400, description = "Validation failed.", body = ErrorEnvelope),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Category not found.", body = ErrorEnvelope),
        (status = 409, description = "Duplicate name.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn rename_knowledge_category(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
    Path(category_id): Path<Uuid>,
    ApiJson(payload): ApiJson<RenameCategoryPayload>,
) -> Response {
    let start = std::time::Instant::now();
    if payload.name.trim().is_empty() {
        return ApiError::validation_failed("Category name is required")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "rename_knowledge_category: begin tx failed");
            return ApiError::internal_error("Failed to rename category")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let category = match store::rename_category_in_tx(
        &mut tx,
        ctx.tenant_id,
        category_id,
        &payload.name,
    )
    .await
    {
        Ok(cat) => cat,
        Err(store::CategoryError::Duplicate) => {
            let _ = tx.rollback().await;
            return ApiError::conflict("Category with this name already exists")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(store::CategoryError::NotFound) => {
            let _ = tx.rollback().await;
            return ApiError::not_found("Category not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(store::CategoryError::Db(e)) => {
            tracing::error!(%e, "rename_knowledge_category: rename_category_in_tx failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to rename category")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    let item_count: i64 = match sqlx::query_scalar(
        "SELECT COUNT(*)::bigint FROM knowledge_items WHERE category_id = $1",
    )
    .bind(category_id)
    .fetch_one(&pool)
    .await
    {
        Ok(count) => count,
        Err(e) => {
            tracing::error!(%e, "rename_knowledge_category: count items failed");
            0
        }
    };

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "rename_knowledge_category: commit failed");
        return ApiError::internal_error("Failed to rename category")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    tracing::info!(
        item_id = %category.id,
        action = "knowledge_category.renamed",
        request_id = %ctx.request_id,
        latency_us = %start.elapsed().as_micros(),
        "knowledge category renamed"
    );

    let dto = KnowledgeCategoryDto {
        id: category.id,
        name: category.name,
        item_count,
        created_at: category.created_at,
        updated_at: category.updated_at,
    };

    (StatusCode::OK, Json(dto)).into_response()
}

/// `DELETE /tenant/knowledge/categories/{category_id}` — delete a category
#[utoipa::path(
    delete,
    path = "/tenant/knowledge/categories/{category_id}",
    tag = "tenant-knowledge",
    operation_id = "delete_knowledge_category",
    summary = "Delete a knowledge category",
    params(
        ("category_id" = Uuid, Path, description = "Category ID"),
    ),
    responses(
        (status = 204, description = "Category deleted."),
        (status = 401, description = "Authentication required.", body = ErrorEnvelope),
        (status = 403, description = "Insufficient permissions.", body = ErrorEnvelope),
        (status = 404, description = "Category not found.", body = ErrorEnvelope),
        (status = 500, description = "Internal server error.", body = ErrorEnvelope),
    ),
    security(("session_cookie" = []))
)]
pub async fn delete_knowledge_category(
    State(pool): State<PgPool>,
    ctx: TenantContext,
    Extension(_principal): Extension<Principal>,
    Path(category_id): Path<Uuid>,
) -> Response {
    let start = std::time::Instant::now();
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "delete_knowledge_category: begin tx failed");
            return ApiError::internal_error("Failed to delete category")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    };

    match store::delete_category_in_tx(&mut tx, ctx.tenant_id, category_id).await {
        Ok(true) => {}
        Ok(false) => {
            let _ = tx.rollback().await;
            return ApiError::not_found("Category not found")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
        Err(e) => {
            tracing::error!(%e, "delete_knowledge_category: delete_category_in_tx failed");
            let _ = tx.rollback().await;
            return ApiError::internal_error("Failed to delete category")
                .with_request_id(&ctx.request_id)
                .into_response();
        }
    }

    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "delete_knowledge_category: commit failed");
        return ApiError::internal_error("Failed to delete category")
            .with_request_id(&ctx.request_id)
            .into_response();
    }

    tracing::info!(
        item_id = %category_id,
        action = "knowledge_category.deleted",
        request_id = %ctx.request_id,
        latency_us = %start.elapsed().as_micros(),
        "knowledge category deleted"
    );

    StatusCode::NO_CONTENT.into_response()
}
