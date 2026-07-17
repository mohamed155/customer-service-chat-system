use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RetrievedChunk {
    pub chunk_id: Uuid,
    pub item_id: Uuid,
    pub tenant_id: Uuid,
    pub content: String,
    pub content_hash: String,
    pub similarity: f64,
    pub item_title: String,
}

pub async fn search(
    pool: &PgPool,
    tenant_id: Uuid,
    query_embedding: &[f32],
    top_k: i32,
    threshold: f32,
) -> Result<Vec<RetrievedChunk>, sqlx::Error> {
    sqlx::query("SET hnsw.iterative_scan = 'relaxed_order'")
        .execute(pool)
        .await?;

    sqlx::query("SET hnsw.ef_search = 100")
        .execute(pool)
        .await?;

    let vec_str = query_embedding
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",");

    sqlx::query_as::<_, RetrievedChunk>(
        &format!(
            "SELECT kc.id AS chunk_id, \
                    kc.item_id, \
                    kc.tenant_id, \
                    kc.content, \
                    kc.content_hash, \
                    (1 - (kc.embedding <=> '[{v}]'::vector)) AS similarity, \
                    ki.title AS item_title \
             FROM knowledge_chunks kc \
             JOIN knowledge_items ki \
               ON ki.id = kc.item_id \
              AND ki.tenant_id = kc.tenant_id \
             WHERE kc.tenant_id = $1 \
               AND ki.status = 'published' \
               AND (1 - (kc.embedding <=> '[{v}]'::vector)) >= $2 \
             ORDER BY similarity DESC \
             LIMIT $3",
            v = vec_str
        ),
    )
    .bind(tenant_id)
    .bind(threshold)
    .bind(top_k)
    .fetch_all(pool)
    .await
}
