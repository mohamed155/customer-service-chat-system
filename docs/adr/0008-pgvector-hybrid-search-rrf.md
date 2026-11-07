# ADR-008: Retrieval — pgvector HNSW plus full-text hybrid search fused with RRF

- **Status:** Accepted (MOH-17)
- **Date:** 2026-11-07
- **Deciders:** Platform engineering
- **Related:** [System architecture §7.2 and §10.3](../architecture/system-architecture.md#103-retrieval-rag),
  ADR-004 (tenant isolation, RLS on vector tables), ADR-007 (ingestion jobs)

## Context

The AI agent answers from each tenant's published knowledge (constitution III: grounded answers
with citations, no fabrication). Retrieval quality directly decides answer quality.

The decision has to balance these forces:

- **Exact terms vs. meaning.** Customers ask with SKUs, order numbers, error codes and product
  names, which pure vector search often misses. They also paraphrase, which pure keyword search
  misses.
- **Tenant isolation.** Every query must be filtered to one tenant, and that filter must not
  wreck recall or latency of approximate nearest-neighbour search.
- **Operations.** A separate vector database is another system to secure, back up and keep in sync
  with PostgreSQL, including deletes and unpublished documents.
- **Scale.** Most tenants have thousands to tens of thousands of chunks; a few may have millions.
- **Model churn.** Embedding models will change. Re-embedding must be gradual.

## Decision

### 1. Storage: pgvector in PostgreSQL

- `knowledge_chunks(tenant_id, document_id, chunk_index, content, tsv tsvector, embedding vector(N),
  embedding_model, content_hash, …)`, with composite FK to the document and RLS (ADR-004).
- **HNSW** index with cosine distance (`vector_cosine_ops`). Start at `m = 16`,
  `ef_construction = 64`; `hnsw.ef_search` set per query (default 100), tuned against the eval set.
- **Tenant filtering:** use pgvector **iterative index scans** (`hnsw.iterative_scan =
  relaxed_order`) so the tenant filter doesn't starve results. For very large tenants, add a
  partial HNSW index per tenant or move them to their own cell (ADR-004 §5).
- Query-time settings are applied with `SET LOCAL` **inside the query's transaction**, never as
  session settings on a pooled connection.
- `embedding_model` is stored per row. A model change re-embeds in the background (ADR-007 jobs)
  and queries filter by the active model, so both models can coexist during migration.

### 2. Lexical search: PostgreSQL full-text

- `tsv` is a generated `tsvector` column with a **GIN** index, built with the tenant's language
  configuration (e.g. `english`, `arabic`, or `simple` as fallback).
- Queries use `websearch_to_tsquery` and rank with `ts_rank_cd`. `pg_trgm` covers fuzzy matching
  of names and codes where needed.

### 3. Fusion: Reciprocal Rank Fusion

- Run both searches for the tenant (each top 50), then fuse:
  `score(d) = Σ 1 / (k + rank_i(d))` over the lists where `d` appears, with **k = 60**.
- RRF needs no score normalisation between cosine similarity and `ts_rank_cd`, which aren't
  comparable, and it's robust to either list being empty.
- After fusion: **MMR** removes near-duplicates and increases coverage; an optional re-ranker
  (cross-encoder or provider API) reorders the top N.
- **Relevance threshold:** if nothing clears the threshold, the agent says it doesn't know or
  escalates. It never answers without grounding.

### 4. The vector-store escape hatch

- Retrieval sits behind a `KnowledgeRetriever` port owned by `ai_runtime` (ADR-001). Callers pass
  tenant, query text, query embedding and limits, and get ranked chunks with citations back.
- If an ADR shows that pgvector can't meet the budget — for example retrieval p95 above 150 ms at
  the target corpus size after tuning, index builds or memory measurably hurting the primary
  database, or a tenant above ~10 M chunks — the semantic half can move to a dedicated vector
  store (Qdrant, etc.) behind the same port. Lexical search and fusion stay in the application.
- A move like that must keep tenant isolation (per-tenant collections or mandatory filters) and
  delete propagation through the outbox.

### 5. Evaluation

Retrieval changes (chunking, `ef_search`, k, re-ranker, models) run the retrieval eval set
(recall@k, MRR, groundedness) and must not regress agreed metrics (constitution VI).

## Alternatives considered

| Option | Why rejected |
| -- | -- |
| **Vector search only** | Misses exact identifiers (SKUs, error codes) and rare terms. Hybrid search consistently does better on support content. |
| **Dedicated vector DB from day one** (Qdrant, Pinecone, Weaviate) | A second store to secure and keep consistent with PostgreSQL (publish/unpublish, deletes, RLS). No measured need at current corpus sizes. Kept as the escape hatch. |
| **IVFFlat index** | Needs a training step and re-indexing as data grows; worse recall/latency trade-off than HNSW for this workload. |
| **Weighted score blending** (`α·cosine + (1−α)·ts_rank`) | The scores are on different scales and distributions, so α needs constant re-tuning. RRF uses ranks and avoids that. |
| **Elasticsearch / OpenSearch for lexical** | Better BM25, but a heavy extra cluster. PostgreSQL full-text is good enough for support corpora, and can be revisited with the escape hatch. |

## Consequences

**Positive**

- One database: transactions, backups, RLS and deletes cover knowledge too.
- Hybrid search handles both exact identifiers and paraphrases.
- The port keeps a dedicated vector store a contained change.

**Negative and costs**

- HNSW indexes use memory and slow down bulk ingestion; builds must be scheduled and watched.
- Two searches per query plus fusion code, and a language configuration per tenant.
- Tuning (`ef_search`, k, thresholds) needs a maintained evaluation set.

## Current state and migration

These gaps between the current code and this decision are follow-up work. No code changes in
this ADR.

| # | Gap | Where | Follow-up |
| -- | -- | -- | -- |
| R1 | **Vector search only.** There's no `tsvector` column, no GIN index, no full-text query and no RRF. | `backend/migrations/0047_embeddings_rag.sql`, `backend/crates/modules/knowledge/src/retrieval.rs` | Add the generated `tsv` column and GIN index, the lexical query and RRF fusion. Measure against the eval set. |
| R2 | **Query-time settings are applied to the pool, not the query's connection.** `SET hnsw.iterative_scan` and `SET hnsw.ef_search` run as separate `execute(pool)` calls, so they can land on a different pooled connection than the search, and they stay set on whatever connection they ran on. | `knowledge/src/retrieval.rs` (`search`) | **Bug-level.** Run both as `SET LOCAL` in one transaction with the search query. Add a test. |
| R3 | **The HNSW index is probably not used.** The query orders by `1 - (embedding <=> v)` descending and filters on that expression. pgvector only uses the index for `ORDER BY embedding <=> v` (ascending) with a `LIMIT`. Needs `EXPLAIN` on a realistic dataset to confirm. | `knowledge/src/retrieval.rs` | Order by the distance operator, apply the similarity threshold after the index scan, and confirm with `EXPLAIN (ANALYZE)` in an integration test. |
| R4 | The query vector is formatted into the SQL string (`AssertSqlSafe(format!(…))`) instead of being bound as a parameter. Floats can't inject SQL, but it bypasses the query plan cache and the "no SQL string building" rule. | `knowledge/src/retrieval.rs` | Bind the vector as a parameter (`pgvector` crate type). |
| R5 | `embedding_model` exists only on `ai_configurations`, not per chunk; the dimension is fixed at `vector(1536)`. | `0047_embeddings_rag.sql` | Add `embedding_model` per chunk before the first model change. |
| R6 | No MMR, re-ranker or retrieval eval set. | — | Add with R1; the eval set comes first so the other changes can be measured. |
