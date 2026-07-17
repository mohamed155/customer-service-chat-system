# Phase 1 Data Model: Embeddings & RAG

Single migration: `backend/migrations/0047_embeddings_rag.sql`. All tenant-owned tables carry `tenant_id` (Principle II). Conventions follow existing migrations: UUID PKs (`gen_random_uuid()`), `timestamptz` timestamps, FKs to `tenants(id)`.

The vector dimension `N` below is the v1 platform embedding model's dimension (OpenAI `text-embedding-3-small` = 1536); it is fixed in the migration because embeddings use a single platform-wide configuration. The configured `embedding_model` MUST produce exactly `N`-dim vectors — a startup/insert-time assertion fails fast on mismatch (see research §1; Gemini models have different dims and are not valid v1 without changing `N`). Retrieval must configure `hnsw.iterative_scan` + a raised `hnsw.ef_search` so the tenant filter does not starve recall (research §1).

## Entities

### 1. `knowledge_chunks` — Knowledge Passage + Passage Embedding

A contiguous slice of a published knowledge item's text, with its embedding. Maps the spec's **Knowledge Passage (Chunk)** and **Passage Embedding** entities (co-located: one row = one passage and its vector).

| Column | Type | Notes |
|---|---|---|
| `id` | `UUID` PK | |
| `tenant_id` | `UUID NOT NULL` | FK `tenants(id)` — isolation key |
| `item_id` | `UUID NOT NULL` | FK `knowledge_items(id)` ON DELETE CASCADE |
| `ordinal` | `INTEGER NOT NULL` | position within source (0-based), `CHECK (ordinal >= 0)` |
| `content` | `TEXT NOT NULL` | passage text (display/cite), `CHECK (char_length(content) BETWEEN 1 AND 8000)` |
| `embedding` | `vector(N) NOT NULL` | passage vector |
| `content_hash` | `TEXT NOT NULL` | hash of source content this chunk set was derived from (coalescing/idempotency) |
| `created_at` | `timestamptz NOT NULL DEFAULT now()` | |

**Constraints & indexes**:
- `UNIQUE (item_id, ordinal)` — stable ordering per item.
- Composite FK `(tenant_id, item_id)` → `knowledge_items(tenant_id, id)` so a chunk's tenant must match its item's tenant (defense in depth; mirrors migration 0027 pattern).
- HNSW: `CREATE INDEX ON knowledge_chunks USING hnsw (embedding vector_cosine_ops);`
- B-tree: `(tenant_id, item_id)` for delete/replace by item.

**Lifecycle**: Inserted/replaced atomically by the indexer per item; deleted by cascade when the item is deleted, or explicitly when the item is unpublished (archive/revert to draft) so unpublished content is not retrievable (FR-004, FR-005).

**Validation rules**: Only chunks whose `item_id` refers to a `published` item are eligible for retrieval — enforced in the retrieval query via join to `knowledge_items` on `status = 'published'` (belt-and-suspenders with the unpublish-delete behavior).

### 2. `knowledge_index_state` — Index State

Per-item searchability lifecycle. Maps the spec's **Index State** entity.

| Column | Type | Notes |
|---|---|---|
| `item_id` | `UUID PK` | FK `knowledge_items(id)` ON DELETE CASCADE (1:1 with item) |
| `tenant_id` | `UUID NOT NULL` | FK `tenants(id)` |
| `status` | `TEXT NOT NULL` | `CHECK (status IN ('not_indexed','pending','indexing','indexed','failed','not_indexable'))` |
| `failure_reason` | `TEXT NULL` | present when `status='failed'` or `'not_indexable'` |
| `attempts` | `INTEGER NOT NULL DEFAULT 0` | retry counter for bounded auto-retry (FR-016) |
| `indexed_content_hash` | `TEXT NULL` | content hash of the last successful index (coalescing) |
| `chunk_count` | `INTEGER NOT NULL DEFAULT 0` | |
| `last_indexed_at` | `timestamptz NULL` | last successful index time |
| `updated_at` | `timestamptz NOT NULL DEFAULT now()` | |

**State transitions**:
```
(item published)        → pending
pending    → indexing    (worker claims)
indexing   → indexed     (chunks written, content_hash recorded)
indexing   → failed      (retries exhausted; reason set)
indexing   → not_indexable (no extractable text; reason set)
failed     → pending     (manual reindex, or auto-retry before exhaustion)
indexed    → pending     (published item edited → re-index)
(item unpublished/deleted) → chunks removed; row → not_indexed or row removed on delete
```
Composite FK `(tenant_id, item_id)` → `knowledge_items(tenant_id, id)`. Index `(tenant_id, status)` for the list view.

### 3. `message_citations` — Citation (snapshot)

Point-in-time record linking one AI `messages` row to a source knowledge item + passage. Maps the spec's **Citation** entity.

| Column | Type | Notes |
|---|---|---|
| `id` | `UUID` PK | |
| `tenant_id` | `UUID NOT NULL` | FK `tenants(id)` |
| `message_id` | `UUID NOT NULL` | FK `messages(id)` ON DELETE CASCADE — the AI reply |
| `knowledge_item_id` | `UUID NOT NULL` | source item id (NOT a FK / no cascade — must survive item deletion) |
| `item_title` | `TEXT NOT NULL` | snapshot of item title at response time |
| `passage_text` | `TEXT NOT NULL` | snapshot of the cited passage text at response time |
| `relevance_score` | `REAL NOT NULL` | cosine similarity used for ordering, `CHECK (relevance_score >= 0)` |
| `ordinal` | `INTEGER NOT NULL` | display order (0-based) |
| `created_at` | `timestamptz NOT NULL DEFAULT now()` | |

**Design notes**:
- `knowledge_item_id` is intentionally **not** a foreign key so a citation survives item deletion (FR-009; Story 2 acceptance #4). Whether the current item still exists is resolved at read time by a live lookup; the snapshot always renders.
- Index `(message_id)` for batch timeline loads (no N+1). One AI message → 0..k citation rows; an ungrounded reply has zero rows.
- Tenant isolation: `tenant_id` on every row; timeline queries already scope by tenant + conversation.

## Relationships

```
tenants (1) ──< knowledge_items (1) ──< knowledge_chunks (N)
                       │  (1:1)
                       └──< knowledge_index_state (1)

messages (AI reply, 1) ──< message_citations (N) ┄┄> knowledge_items (soft ref, no FK)
knowledge_chunks (retrieved) ──snapshot──> message_citations (passage_text, item_title)
```

## Platform embedding configuration (existing table, additive)

The single platform-wide embedding model is recorded on the platform-scope `ai_configurations` row (`tenant_id IS NULL`). Add a nullable `embedding_model TEXT` column (and reuse the existing `provider` semantics / platform `ai_credentials` for the key). This keeps embedding-model selection auditable and consistent with how chat models are configured. (Final placement — column vs. app-config — confirmed at task time per research §3; the migration is the default.)

## Contract field additions (no new persistence)

- Knowledge item list/detail views gain `index_status: { status, failure_reason?, last_indexed_at?, chunk_count }` sourced from `knowledge_index_state`.
- Conversation `Message` view gains `citations: [{ knowledge_item_id, item_title, passage_text, relevance_score, item_available: bool }]` (empty for non-AI or ungrounded messages).
