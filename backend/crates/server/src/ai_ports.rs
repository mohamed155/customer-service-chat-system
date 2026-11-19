//! Wires the ports `ai` declares (`ai::ports`) to the modules that implement them (ADR-001 M3,
//! ADR-017, MOH-195). This is the one place that knows both sides: `ai` does not depend on
//! `knowledge`, and `knowledge` does not depend on `ai`.
//!
//! # Public Interfaces
//! - [`AppState::ai_ports`] builds the [`ai::ports::AiPorts`] the engine runs on
//! - [`AiEmbedder`] lets the knowledge indexer embed through the `ai` service

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use ai::ports::{AiPorts, KnowledgeRetrieval, RetrievedChunk};

use crate::state::AppState;

/// `ai::ports::KnowledgeRetrieval` on top of `knowledge::retrieval`.
struct KnowledgeAdapter;

#[async_trait]
impl KnowledgeRetrieval for KnowledgeAdapter {
    async fn search(
        &self,
        pool: &PgPool,
        tenant_id: Uuid,
        query_embedding: &[f32],
        top_k: i32,
        threshold: f32,
    ) -> sqlx::Result<Vec<RetrievedChunk>> {
        let chunks =
            knowledge::retrieval::search(pool, tenant_id, query_embedding, top_k, threshold)
                .await?;
        Ok(chunks
            .into_iter()
            .map(|chunk| RetrievedChunk {
                chunk_id: chunk.chunk_id,
                item_id: chunk.item_id,
                tenant_id: chunk.tenant_id,
                content: chunk.content,
                content_hash: chunk.content_hash,
                similarity: chunk.similarity,
                item_title: chunk.item_title,
            })
            .collect())
    }
}

/// The ports the AI engine runs on, wired to the modules behind them.
pub fn ai_ports() -> AiPorts {
    AiPorts {
        knowledge: std::sync::Arc::new(KnowledgeAdapter),
    }
}

impl AppState {
    /// The ports the AI engine runs on (see [`ai_ports()`]).
    pub fn ai_ports(&self) -> AiPorts {
        ai_ports()
    }
}

/// The knowledge indexer's `Embedder`, answered by the `ai` service. A newtype because the server
/// owns neither the trait (`knowledge`) nor the type (`ai`), and `ai` may not implement a
/// `knowledge` trait without depending on it.
pub struct AiEmbedder(pub ai::AiService);

#[async_trait]
impl knowledge::indexer::Embedder for AiEmbedder {
    async fn embed(
        &self,
        tenant_id: Uuid,
        texts: Vec<String>,
        request_id: String,
    ) -> Result<Vec<Vec<f32>>, knowledge::indexer::EmbedError> {
        use knowledge::indexer::EmbedError;

        let ctx = ai::AiCallContext {
            tenant_id,
            request_id: Some(request_id),
        };
        match self.0.embed_platform(ctx, texts).await {
            Ok(embeddings) => Ok(embeddings),
            Err(ai::AiCallError::Provider { category, .. }) if category.retriable() => {
                Err(EmbedError::Retriable(category.as_str().to_string()))
            }
            Err(ai::AiCallError::Provider { category, .. }) => {
                Err(EmbedError::Permanent(category.as_str().to_string()))
            }
            Err(ai::AiCallError::NotConfigured) => Err(EmbedError::NotConfigured),
            Err(ai::AiCallError::Internal(e)) => Err(EmbedError::Permanent(e)),
        }
    }
}
