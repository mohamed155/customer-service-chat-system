use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::validate::{ItemStatus, ItemType};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KnowledgeItemRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub item_type: String,
    pub title: String,
    pub body: Option<String>,
    pub status: String,
    pub category_id: Option<Uuid>,
    pub source: String,
    pub created_by_user_id: Option<Uuid>,
    pub created_by_display: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KnowledgeDocumentRow {
    pub item_id: Uuid,
    pub tenant_id: Uuid,
    pub storage_key: String,
    pub original_filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KnowledgeCategoryRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KnowledgeItemTagRow {
    pub item_id: Uuid,
    pub tenant_id: Uuid,
    pub tag: String,
}

#[derive(Debug, Clone)]
pub struct ItemFilters {
    pub item_type: Option<ItemType>,
    pub status: Option<ItemStatus>,
    pub category_id: Option<Uuid>,
    pub tag: Option<String>,
    pub q: Option<String>,
}

#[derive(Debug)]
pub enum CategoryError {
    Duplicate,
    NotFound,
    Db(sqlx::Error),
}

impl From<sqlx::Error> for CategoryError {
    fn from(e: sqlx::Error) -> Self {
        if let sqlx::Error::Database(ref db) = e {
            if db.is_unique_violation() {
                return CategoryError::Duplicate;
            }
        }
        CategoryError::Db(e)
    }
}

pub async fn list_items(
    pool: &PgPool,
    tenant_id: Uuid,
    filters: ItemFilters,
    limit: i64,
    before: Option<(DateTime<Utc>, Uuid)>,
) -> sqlx::Result<(Vec<KnowledgeItemRow>, Vec<KnowledgeItemTagRow>, bool)> {
    let fetch_limit = limit + 1;

    let sql = if let Some((_, _)) = before {
        "SELECT i.* FROM knowledge_items i \
         WHERE i.tenant_id = $1 \
           AND ($2::text IS NULL OR i.item_type = $2) \
           AND ($3::text IS NULL OR i.status = $3) \
           AND ($4::uuid IS NULL OR i.category_id = $4) \
           AND ($5::text IS NULL OR i.id IN (SELECT item_id FROM knowledge_item_tags WHERE tenant_id = $1 AND tag = $5)) \
           AND ($6::text IS NULL OR LOWER(i.title) LIKE LOWER($6)) \
           AND (i.updated_at, i.id) < ($7::timestamptz, $8::uuid) \
         ORDER BY i.updated_at DESC, i.id DESC \
         LIMIT $9"
    } else {
        "SELECT i.* FROM knowledge_items i \
         WHERE i.tenant_id = $1 \
           AND ($2::text IS NULL OR i.item_type = $2) \
           AND ($3::text IS NULL OR i.status = $3) \
           AND ($4::uuid IS NULL OR i.category_id = $4) \
           AND ($5::text IS NULL OR i.id IN (SELECT item_id FROM knowledge_item_tags WHERE tenant_id = $1 AND tag = $5)) \
           AND ($6::text IS NULL OR LOWER(i.title) LIKE LOWER($6)) \
         ORDER BY i.updated_at DESC, i.id DESC \
         LIMIT $7"
    };

    let q_val = filters.q.as_ref().map(|q| format!("%{}%", q));
    let tag_val = filters.tag.as_deref();
    let type_val = filters.item_type.as_ref().map(|t| t.as_str());
    let status_val = filters.status.as_ref().map(|s| s.as_str());

    let mut query = sqlx::query_as::<_, KnowledgeItemRow>(sql)
        .bind(tenant_id)
        .bind(type_val)
        .bind(status_val)
        .bind(filters.category_id)
        .bind(tag_val)
        .bind(q_val);

    if let Some((cursor_ts, cursor_id)) = before {
        query = query.bind(cursor_ts).bind(cursor_id).bind(fetch_limit);
    } else {
        query = query.bind(fetch_limit);
    }

    let items = query.fetch_all(pool).await?;
    let has_more = items.len() > limit as usize;
    let items = items.into_iter().take(limit as usize).collect::<Vec<_>>();
    let tags = load_tags_for_items(pool, &items).await?;
    Ok((items, tags, has_more))
}

async fn load_tags_for_items(
    pool: &PgPool,
    items: &[KnowledgeItemRow],
) -> sqlx::Result<Vec<KnowledgeItemTagRow>> {
    if items.is_empty() {
        return Ok(vec![]);
    }
    let ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    sqlx::query_as::<_, KnowledgeItemTagRow>(
        "SELECT * FROM knowledge_item_tags WHERE item_id = ANY($1) ORDER BY item_id, tag",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await
}

pub async fn get_item(
    pool: &PgPool,
    tenant_id: Uuid,
    item_id: Uuid,
) -> sqlx::Result<Option<KnowledgeItemRow>> {
    sqlx::query_as::<_, KnowledgeItemRow>(
        "SELECT * FROM knowledge_items WHERE id = $1 AND tenant_id = $2",
    )
    .bind(item_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn create_item_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    item_type: &str,
    title: &str,
    body: Option<&str>,
    source: &str,
    category_id: Option<Uuid>,
    created_by_user_id: Option<Uuid>,
    created_by_display: &str,
) -> sqlx::Result<KnowledgeItemRow> {
    sqlx::query_as::<_, KnowledgeItemRow>(
        "INSERT INTO knowledge_items (tenant_id, item_type, title, body, source, category_id, created_by_user_id, created_by_display) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING *",
    )
    .bind(tenant_id)
    .bind(item_type)
    .bind(title)
    .bind(body)
    .bind(source)
    .bind(category_id)
    .bind(created_by_user_id)
    .bind(created_by_display)
    .fetch_one(&mut **tx)
    .await
}

pub async fn update_item_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    item_id: Uuid,
    title: Option<&str>,
    body: Option<&str>,
    item_type: Option<&str>,
    category_id: Option<Option<Uuid>>,
) -> sqlx::Result<Option<KnowledgeItemRow>> {
    let existing = sqlx::query_as::<_, KnowledgeItemRow>(
        "SELECT * FROM knowledge_items WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
    )
    .bind(item_id)
    .bind(tenant_id)
    .fetch_optional(&mut **tx)
    .await?;

    let existing = match existing {
        Some(r) => r,
        None => return Ok(None),
    };

    let new_title = title.map(|s| s.to_string()).unwrap_or(existing.title);
    let new_body = body.map(|s| Some(s.to_string())).unwrap_or(existing.body);
    let new_item_type = item_type
        .map(|s| s.to_string())
        .unwrap_or(existing.item_type);
    let new_category_id = match category_id {
        Some(Some(id)) => Some(id),
        Some(None) => None,
        None => existing.category_id,
    };

    sqlx::query_as::<_, KnowledgeItemRow>(
        "UPDATE knowledge_items SET title = $1, body = $2, item_type = $3, category_id = $4, updated_at = now() \
         WHERE id = $5 AND tenant_id = $6 RETURNING *",
    )
    .bind(&new_title)
    .bind(new_body)
    .bind(&new_item_type)
    .bind(new_category_id)
    .bind(item_id)
    .bind(tenant_id)
    .fetch_optional(&mut **tx)
    .await
}

pub async fn set_status_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    item_id: Uuid,
    status: &str,
) -> sqlx::Result<Option<KnowledgeItemRow>> {
    sqlx::query_as::<_, KnowledgeItemRow>(
        "UPDATE knowledge_items SET status = $1, updated_at = now() WHERE id = $2 AND tenant_id = $3 RETURNING *",
    )
    .bind(status)
    .bind(item_id)
    .bind(tenant_id)
    .fetch_optional(&mut **tx)
    .await
}

pub async fn replace_tags_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    item_id: Uuid,
    tenant_id: Uuid,
    tags: &[String],
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM knowledge_item_tags WHERE item_id = $1")
        .bind(item_id)
        .execute(&mut **tx)
        .await?;

    for tag in tags {
        sqlx::query(
            "INSERT INTO knowledge_item_tags (item_id, tenant_id, tag) VALUES ($1, $2, $3)",
        )
        .bind(item_id)
        .bind(tenant_id)
        .bind(tag)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CategoryWithCount {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub item_count: i64,
}

pub async fn list_categories(
    pool: &PgPool,
    tenant_id: Uuid,
) -> sqlx::Result<Vec<CategoryWithCount>> {
    sqlx::query_as::<_, CategoryWithCount>(
        "SELECT c.*, COUNT(i.id)::bigint AS item_count \
         FROM knowledge_categories c \
         LEFT JOIN knowledge_items i ON i.category_id = c.id \
         WHERE c.tenant_id = $1 \
         GROUP BY c.id \
         ORDER BY c.name",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
}

pub async fn create_category_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    name: &str,
) -> Result<KnowledgeCategoryRow, CategoryError> {
    Ok(sqlx::query_as::<_, KnowledgeCategoryRow>(
        "INSERT INTO knowledge_categories (tenant_id, name) VALUES ($1, $2) RETURNING *",
    )
    .bind(tenant_id)
    .bind(name)
    .fetch_one(&mut **tx)
    .await?)
}

pub async fn rename_category_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    category_id: Uuid,
    name: &str,
) -> Result<KnowledgeCategoryRow, CategoryError> {
    let row = sqlx::query_as::<_, KnowledgeCategoryRow>(
        "UPDATE knowledge_categories SET name = $1, updated_at = now() WHERE id = $2 AND tenant_id = $3 RETURNING *",
    )
    .bind(name)
    .bind(category_id)
    .bind(tenant_id)
    .fetch_optional(&mut **tx)
    .await?;

    match row {
        Some(r) => Ok(r),
        None => Err(CategoryError::NotFound),
    }
}

pub async fn delete_category_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    category_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM knowledge_categories WHERE id = $1 AND tenant_id = $2")
        .bind(category_id)
        .bind(tenant_id)
        .execute(&mut **tx)
        .await?;

    Ok(result.rows_affected() > 0)
}

#[allow(clippy::too_many_arguments)]
pub async fn create_document_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: Uuid,
    title: &str,
    status: &str,
    category_id: Option<Uuid>,
    created_by_user_id: Option<Uuid>,
    created_by_display: &str,
    storage_key: &str,
    original_filename: &str,
    content_type: &str,
    size_bytes: i64,
) -> sqlx::Result<KnowledgeItemRow> {
    let item = sqlx::query_as::<_, KnowledgeItemRow>(
        "INSERT INTO knowledge_items (tenant_id, item_type, title, body, status, source, category_id, created_by_user_id, created_by_display) \
         VALUES ($1, 'document', $2, NULL, $3, 'uploaded', $4, $5, $6) RETURNING *",
    )
    .bind(tenant_id)
    .bind(title)
    .bind(status)
    .bind(category_id)
    .bind(created_by_user_id)
    .bind(created_by_display)
    .fetch_one(&mut **tx)
    .await?;

    sqlx::query(
        "INSERT INTO knowledge_documents (item_id, tenant_id, storage_key, original_filename, content_type, size_bytes) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(item.id)
    .bind(tenant_id)
    .bind(storage_key)
    .bind(original_filename)
    .bind(content_type)
    .bind(size_bytes)
    .execute(&mut **tx)
    .await?;

    Ok(item)
}

pub async fn get_document(
    pool: &PgPool,
    tenant_id: Uuid,
    item_id: Uuid,
) -> sqlx::Result<Option<KnowledgeDocumentRow>> {
    sqlx::query_as::<_, KnowledgeDocumentRow>(
        "SELECT d.* FROM knowledge_documents d \
         JOIN knowledge_items i ON i.id = d.item_id \
         WHERE d.item_id = $1 AND d.tenant_id = $2 AND i.tenant_id = $2",
    )
    .bind(item_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
}
