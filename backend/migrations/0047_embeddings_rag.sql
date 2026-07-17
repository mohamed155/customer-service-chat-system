-- Migration 0047: Embeddings & RAG — knowledge chunks, index state, message
-- citations, and embedding model column for AI configurations.
-- See specs/020-embeddings-rag/ for the full design.

-- Table: knowledge_chunks — chunked content with vector embeddings for
-- semantic search. Each knowledge item is split into ordered chunks of
-- up to 8000 characters with a 1536-dimension OpenAI embedding.
CREATE TABLE knowledge_chunks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    item_id UUID NOT NULL REFERENCES knowledge_items(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    content TEXT NOT NULL CHECK (char_length(content) BETWEEN 1 AND 8000),
    embedding vector(1536) NOT NULL,
    content_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (item_id, ordinal),
    FOREIGN KEY (tenant_id, item_id) REFERENCES knowledge_items(tenant_id, id)
);

-- HNSW index for fast approximate nearest-neighbour vector search.
CREATE INDEX knowledge_chunks_embedding_idx
    ON knowledge_chunks USING hnsw (embedding vector_cosine_ops);

-- B-tree index for tenant-scoped chunk lookups.
CREATE INDEX knowledge_chunks_tenant_item_idx
    ON knowledge_chunks (tenant_id, item_id);

-- Table: knowledge_index_state — tracks per-item embedding-indexing status
-- with retry tracking and content-hash change detection.
CREATE TABLE knowledge_index_state (
    item_id UUID PRIMARY KEY REFERENCES knowledge_items(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    status TEXT NOT NULL CHECK (status IN ('not_indexed','pending','indexing','indexed','failed','not_indexable')),
    failure_reason TEXT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    indexed_content_hash TEXT NULL,
    chunk_count INTEGER NOT NULL DEFAULT 0,
    last_indexed_at TIMESTAMPTZ NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (tenant_id, item_id) REFERENCES knowledge_items(tenant_id, id)
);

CREATE INDEX knowledge_index_state_tenant_status_idx
    ON knowledge_index_state (tenant_id, status);

-- Table: message_citations — records which knowledge items were cited in an
-- AI response. knowledge_item_id intentionally has no FK so citations survive
-- knowledge-item deletion (the title and passage text are denormalised).
CREATE TABLE message_citations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    message_id UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    knowledge_item_id UUID NOT NULL,
    item_title TEXT NOT NULL,
    passage_text TEXT NOT NULL,
    relevance_score REAL NOT NULL CHECK (relevance_score >= 0),
    ordinal INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX message_citations_message_idx
    ON message_citations (message_id);

-- Column: ai_configurations.embedding_model — identifies which embedding
-- model (e.g. 'text-embedding-3-small') this configuration uses. Null
-- means embeddings are disabled for this configuration.
ALTER TABLE ai_configurations
    ADD COLUMN embedding_model TEXT NULL;
