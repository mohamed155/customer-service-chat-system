//! The ports `ai` depends on instead of the modules that implement them (ADR-001 M3, ADR-017,
//! MOH-195). `ai` says what it needs and owns the DTOs; the server implements each port on top of
//! the module that has the data and passes the result in as [`AiPorts`]. This keeps `ai` free of
//! the `knowledge`, `tools` and `escalations` crates, so the generation engine can be read, and
//! tested, without them.
//!
//! A port that touches tenant data takes the caller's pool or connection and the `tenant_id`, the
//! same as the function behind it: the adapter must not open a transaction of its own where the
//! caller passes one.

use std::sync::Arc;

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

/// A passage of the tenant's knowledge base that matched a customer's question.
#[derive(Debug, Clone)]
pub struct RetrievedChunk {
    pub chunk_id: Uuid,
    pub item_id: Uuid,
    pub tenant_id: Uuid,
    pub content: String,
    pub content_hash: String,
    /// Cosine similarity to the question, `0.0..=1.0`; higher is closer.
    pub similarity: f64,
    pub item_title: String,
}

/// Finds the passages of a tenant's knowledge base closest to an embedded question.
#[async_trait]
pub trait KnowledgeRetrieval: Send + Sync {
    /// The up-to-`top_k` passages of `tenant_id`'s knowledge base whose similarity to
    /// `query_embedding` is at least `threshold`, closest first. Only that tenant's passages.
    async fn search(
        &self,
        pool: &PgPool,
        tenant_id: Uuid,
        query_embedding: &[f32],
        top_k: i32,
        threshold: f32,
    ) -> sqlx::Result<Vec<RetrievedChunk>>;
}

/// Everything `ai` needs from other modules, built once by the server and passed to the engine.
#[derive(Clone)]
pub struct AiPorts {
    pub knowledge: Arc<dyn KnowledgeRetrieval>,
}
