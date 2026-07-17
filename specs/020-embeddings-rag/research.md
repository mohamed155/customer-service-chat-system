# Phase 0 Research: Embeddings & RAG

All Technical Context unknowns are resolved below. Each entry records the decision, rationale, and alternatives considered.

## 1. Vector storage & index type (pgvector)

**Decision**: Store embeddings in a `vector(N)` column on `knowledge_chunks` where `N` is the platform embedding model's dimension (1536 for OpenAI `text-embedding-3-small`). Use an HNSW index with cosine distance (`vector_cosine_ops`) for approximate nearest-neighbour search. Query with the `<=>` cosine-distance operator and convert to a similarity score `1 - distance` for thresholding.

**Rationale**: The `vector` extension is already enabled (migration `0001_init.sql`). HNSW gives better recall/latency than IVFFlat at this scale (thousands of items/tenant) and needs no training step or `lists` tuning. Cosine distance is the standard metric for both OpenAI and Gemini embeddings (they are normalized/ recommended for cosine). Fixing the dimension in the column is safe because the clarification mandates a single platform-wide embedding configuration — all vectors share one dimension.

**Alternatives considered**:
- *IVFFlat* — rejected: requires `lists` tuning and a populated table to build a good index; recall degrades on small/growing per-tenant sets.
- *Separate vector database (Qdrant/Pinecone)* — rejected: violates the constitution's mandated stack ("no separate search infrastructure").
- *Per-tenant partial HNSW indexes* — rejected: unnecessary at this scale; a single HNSW index plus a `tenant_id` + `published` filter in the query is sufficient and simpler. (Filtering happens as a post-filter/pre-filter around the ANN scan; correctness of isolation is guaranteed by the WHERE clause, not the index.)

**Filtered-ANN recall (must address before implementation)**: The retrieval query is `WHERE tenant_id = $t [AND published] ORDER BY embedding <=> $q LIMIT k` against one shared HNSW index. This is **correct for isolation** (the WHERE clause can never admit another tenant's rows) but has a **recall risk**: pgvector applies the tenant filter around an ANN candidate set of size `hnsw.ef_search` (default 40). In a large multi-tenant table where any one tenant is a small slice, the global nearest-40 can be dominated by *other* tenants' chunks and then filtered away, leaving the target tenant with few or zero passages even when good matches exist for it. This would silently break Story 1 acceptance #1 and SC-006.

Critically, the naive 2-tenant isolation test (identical content) will **pass anyway** — with a tiny corpus the target tenant's chunks are in the global top-k — so it hides the defect. The recall gap must be tested separately (see §9) with a realistic distribution.

**Resolution**: The dev/prod Postgres image is `pgvector/pgvector:pg16`, which ships pgvector ≥ 0.8.0 (verify at deploy; **pin the image tag** so this isn't a moving target). Use pgvector 0.8's **iterative index scan** for filtered queries: set `hnsw.iterative_scan = 'relaxed_order'` (session/statement scope) and a raised `hnsw.ef_search` (e.g. 100) for the retrieval query, so the scan keeps pulling candidates until `k` tenant-matching rows are found (or a `max_scan_tuples` cap is hit). This preserves recall under filtering without per-tenant indexes.

- *Rejected alternative — per-tenant partial HNSW indexes*: does not scale to many tenants (index proliferation); iterative scan is the cleaner default.
- *Rejected alternative — pre-filter by copying tenant chunks into a temp set*: defeats the point of ANN at scale.

**Dimension-change & initial-mismatch note**: The `vector(N)` column is fixed at the v1 platform model's dimension. v1 **pins OpenAI `text-embedding-3-small` (1536 dims)**; the configured `embedding_model` MUST produce vectors of exactly that length. Gemini embedding models have different dimensions (e.g. 768 / 3072) and are therefore **not** valid v1 platform embedding models without also changing the migration's `N`. This is an *initial-configuration* hazard (silently inserting a mismatched-dimension vector), not only a later change: add a startup assertion (or an insert-time length check) that the resolved embedding dimension equals the column dimension, failing fast on mismatch. Changing `N` later (different model) is the out-of-v1-scope operational migration (drop/rebuild column + re-embed); a migration comment documents the procedure. (1536 < pgvector's 2000-dim HNSW limit, so the index is unaffected.)

## 2. Embedding provider abstraction

**Decision**: Add an `EmbeddingProvider` trait to the `ai-providers` crate, parallel to the existing `ChatProvider` trait, with `async fn embed(&self, key: &SecretKey, req: &EmbeddingRequest) -> Result<EmbeddingResponse, ProviderError>`. Implement it for OpenAI (`POST /v1/embeddings`, batch input array) and Gemini (`batchEmbedContents`). Anthropic does **not** implement it (no embeddings API). The `Registry` exposes `embedding_provider(name) -> Option<&dyn EmbeddingProvider>` so callers discover capability explicitly.

**Rationale**: Keeps LLM access tool-mediated and provider-independent (Principle IV) while honestly modelling that embeddings are an *optional* provider capability. The platform embedding configuration selects a provider that supports embeddings; capability is checked at resolution time, not assumed. Reuses existing `SecretKey`, `ProviderError`, `ErrorCategory` (retriable classification already exists — drives the retry policy).

**v1 provider scope**: v1 wires **only OpenAI** as the platform embedding provider (dimension 1536, matching the fixed `vector(1536)` column — research §1). The Gemini `EmbeddingProvider` implementation exists to satisfy provider-independence (Principle IV) and is *not* selectable as the v1 platform `embedding_model`, because Gemini's embedding dimensions differ from the pinned column width; configuring it would trip the fail-fast dimension guard in `embed_platform`. Making Gemini a valid platform embedding provider is an out-of-v1 change (new `vector(N)` + re-embed, same operational migration as any model change). The Gemini impl may be deferred out of v1 entirely without affecting any FR/SC.

**Alternatives considered**:
- *Force all three providers to implement embeddings* — rejected: Anthropic has no embeddings endpoint; a stub returning `InvalidRequest` would be a lie in the type system.
- *Put embeddings entirely inside the `knowledge` module with its own HTTP client* — rejected: duplicates credential handling, usage recording, and error taxonomy; violates provider-independence being centralized in one crate.

## 3. Platform-wide embedding configuration & credentials

**Decision**: Reuse the existing `ai_configurations` / `ai_credentials` platform-scope rows (`tenant_id IS NULL`). Add `AiService::embed_platform(...)` that resolves the **platform** config + credential (via `resolution::Scope::Platform`), calls the embedding provider, and appends an `ai_usage_records` row. The embedding model/provider is read from platform config (a dedicated embedding-model field, see data-model) rather than per-tenant config.

**Rationale**: Clarification fixes embeddings to one platform-wide configuration; the platform-scope credential/config machinery already exists and is encrypted (AES-256-GCM). Usage recording gives cost attribution and observability for free. Tenant AI config continues to govern chat/completions only.

**Alternatives considered**:
- *New `embedding_credentials` table* — rejected: `ai_credentials` already supports platform scope + provider; a `provider` value + platform scope is enough.
- *Per-tenant embedding credentials* — rejected by clarification (config-change re-embedding problem; mixed dimensions).

**Open sub-decision deferred to tasks**: whether the embedding model name is a new column on `ai_configurations` or an app-config/env value. Leaning to a nullable `embedding_model` column on the platform `ai_configurations` row for auditability and consistency with how chat models are stored; final call at task time.

## 4. Text chunking strategy

**Decision**: Deterministic, structure-aware chunker in `knowledge/src/chunking.rs`:
1. Extract plain text — authored articles/FAQs: strip HTML to text via existing `ammonia`-based sanitization → text; documents: extract by content type (plain/markdown verbatim; PDF via `pdf-extract` text layer). No extractable text → return the "not indexable" signal (FR: not-indexable status, no error).
2. Split into chunks targeting ~500–800 tokens with ~15% overlap, breaking on paragraph/sentence boundaries so each chunk is independently meaningful (FR-001). Approximate tokens by characters (~4 chars/token) to avoid a tokenizer dependency; cap at ≤ 500 chunks per item (bounded per edge case).
3. Chunking is pure/deterministic (same input → same chunks) to support stable re-index and testability.

**Rationale**: Character-based approximation keeps the chunker dependency-free and deterministic; exact token counting isn't required because the embedding models accept generous input sizes and retrieval quality is robust to modest chunk-size variance. Overlap preserves context across boundaries.

**Alternatives considered**:
- *Fixed-size character windows with no boundary awareness* — rejected: splits sentences mid-thought, hurting passage meaningfulness (FR-001).
- *Real tokenizer (tiktoken)* — rejected for v1: adds a heavy dependency for marginal benefit; revisit if retrieval quality needs it.
- *OCR for scanned PDFs* — rejected: explicitly out of scope (spec Assumptions); such files are marked not-indexable.

**PDF library**: `pdf-extract` (pure-Rust text-layer extraction). If a PDF yields empty/whitespace text → not indexable.

## 5. Indexing trigger & worker (outbox-driven)

**Decision**: Reuse the existing `outbox_events` + single-row-claim worker pattern (as used by the agent responder and escalation workers). On publish, published-edit, unpublish (archive/revert-to-draft), and delete, the knowledge `store` enqueues a `knowledge.index_requested` (or `.remove_requested`) outbox event in the same transaction as the status/content change. A new `run_knowledge_indexer_worker` claims events with `FOR UPDATE SKIP LOCKED`, chunks + embeds + atomically replaces chunks, and updates `knowledge_index_state`.

**Rationale**: Transactional enqueue guarantees an index request is never lost if the write commits (SC-001 within 2 min). `SKIP LOCKED` single-row claim gives per-item isolation so one tenant's large document can't block another's indexing (edge case). The pattern is already established and tested in this codebase — no new infrastructure.

**Coalescing (rapid edits)**: The worker, when claiming an item, reads the item's *current* content version and computes chunks from that; superseded events for the same item become no-ops (the index-state records the content hash/version it indexed; a claim whose target hash already matches the indexed state is skipped). This makes repeated re-index of the same content converge to the latest (edge case).

**Atomic replace (FR-015)**: New chunks are inserted and old chunks deleted in a single transaction keyed by item; a failed embed aborts the transaction, leaving prior chunks intact and retrievable.

**Retry policy (FR-016, clarification)**: On a retriable `ProviderError` (rate_limited/unavailable/timeout) or transient DB error, the worker leaves the event unclaimed / re-schedules with backoff up to a bounded attempt count (reuse the outbox `available_at` backoff column). After exhaustion, index-state → `failed` with reason; recovery is manual via reindex.

**Alternatives considered**:
- *Synchronous indexing on the publish request* — rejected: large documents would block the API request and couple request latency to embedding-provider latency.
- *Redis queue* — rejected: the Postgres outbox is the established, transactional mechanism; adding Redis queues here is redundant.

## 6. Retrieval query construction (context-aware) & pipeline insertion

**Decision**: Insert a retrieval step into `agent_responder.rs` Phase B (after gates, before/around the vendor call). Build the search query from the latest customer message plus recent conversation context by concatenating the last few customer turns (bounded window, e.g. last 1–3 customer messages) into one query string, embed it once via `AiService::embed_platform`, and run tenant-scoped similarity search (`knowledge::retrieval::search`). Apply a cosine-similarity threshold and cap top-k ≤ 5 (FR-008). Inject retrieved passages as a deterministic, clearly delimited block appended to the system message (Principle IV: deterministic prompt construction). Record which chunks were supplied for citation.

**Rationale**: Concatenating recent customer turns resolves elliptical follow-ups ("what about enterprise plans?") without an extra LLM rewrite call, keeping within the 1-second budget (SC-004). The exact window size is a tuning constant set at task time. Deterministic block placement keeps prompt construction reproducible.

**Time budget & graceful degradation (FR-017 / SC-004)**: Wrap embed+search in a timeout (~800 ms). On timeout, embedding-provider error, or empty result, proceed with the existing ungrounded flow — no citations, no failure (edge cases: provider unavailable during conversation; zero published knowledge; no relevant passages).

**SC-004 "streaming" caveat**: SC-004 inherited the spec's streaming language, but the current `agent_responder.rs` uses `AiService::complete()` (not `stream()`) and inserts a whole reply — nothing streams to the customer today. Validate SC-004 as *retrieval adds ≤ ~1 s before the reply is produced* (the retrieval latency budget in front of the existing `complete()` call), not as time-to-first-token. If/when the responder moves to streaming, the same budget applies before the first token. No architecture change is introduced by this feature to satisfy SC-004.

**Query-rewrite alternative**: An LLM call to rewrite the conversation into a standalone query (spec option C) — rejected for v1: adds latency + cost per turn; the concatenation window is sufficient for the acceptance scenarios and the mechanism is swappable later.

## 7. Citation persistence & exposure (snapshot)

**Decision**: New `message_citations` table (tenant-scoped, FK to the AI `messages` row). On inserting a grounded AI reply (Phase C), write one citation row per source item used, snapshotting `knowledge_item_id`, `item_title`, and the cited `passage_text` (clarification: passage snapshot) plus a relevance score and ordinal. Expose citations as an additive `citations: []` array on the conversation `Message` view; batch-load citations for all messages in a timeline in one query (no N+1). Frontend renders a reusable citation-list component under AI messages; a citation links to the current item when it still exists (looked up live) and shows a "no longer available" state otherwise (the snapshot title/text always render).

**Rationale**: Snapshotting title + passage text makes historical conversations auditable even after the source is edited, archived, or deleted (FR-009, Story 2 acceptance #4). Writing citations in the same transaction as the AI reply keeps them consistent with the message. Additive contract fields avoid breaking existing consumers; uncited responses carry an empty array and render no affordance (FR-011).

**Alternatives considered**:
- *Identity-only citation (link to live item)* — rejected by clarification (loses grounding text on edit/delete).
- *Full item-version snapshot* — rejected by clarification (heavy; effectively version-controls knowledge, out of scope).
- *Storing citations in the knowledge schema* — rejected: citations belong to the conversation/message aggregate; keeping them in the conversations schema respects module boundaries (the `ai` module writes them through a `conversations::queries` helper).

## 8. Observability (FR-014, internal-only)

**Decision**: Emit tracing spans `rag.index` (fields: tenant_id, item_id, chunk_count, outcome, attempt) and `rag.retrieve` (fields: tenant_id, conversation_id, message_id, query_len, candidates, returned, top_score, elapsed_ms, degraded) correlated to the request/conversation. Embedding calls append to `ai_usage_records` (existing ledger). No user-facing retrieval-inspection surface in v1 (clarification).

**Rationale**: Satisfies the constitution's RAG-operation observability requirement and FR-014's "inspectable record" via the platform's existing structured-logging/tracing tooling, at zero new UI cost.

## 9. Isolation testing (FR-007, SC-003)

**Decision**: Dedicated integration test `rag_isolation.rs`: seed tenant A and tenant B with *deliberately identical* published knowledge, run retrieval for a tenant B conversation, assert zero tenant A chunks appear regardless of similarity; assert the retrieval SQL filters `tenant_id` at the data-access layer (not in application code after the fact). Add a variant asserting draft/archived items in the same tenant are never returned (FR-005).

**Rationale**: Cross-tenant leakage is an existential failure (Principle II); the isolation guarantee must be proven by a test that would fail if the `tenant_id` filter were removed.

**Companion recall test (guards the §1 filtered-ANN trap)**: A separate `rag_recall.rs` integration test seeds one **target** tenant plus **many noise tenants** (realistic distribution — target tenant is a small fraction of a large chunk table), inserts a known-good passage for the target tenant, and asserts that passage is returned in the top-k for a target-tenant query. This test would fail under a naive single-`ef_search` HNSW scan and pass once iterative scan / raised `ef_search` is configured — closing the gap that the 2-tenant isolation test cannot detect.
