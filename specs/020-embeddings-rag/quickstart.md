# Quickstart: Embeddings & RAG — Validation Guide

Runnable scenarios proving the feature end-to-end. Assumes the modular-monolith backend and Angular dashboard from prior specs are running, a tenant exists, and a **platform** embedding provider credential is configured (platform-scope `ai_credentials` + `ai_configurations` with an `embedding_model`).

References: [data-model.md](data-model.md), contracts in [contracts/](contracts/). Do not treat the commands as literal endpoints if a path differs in code — they mirror the contracts.

## Prerequisites

```bash
# Backend (from backend/): apply migrations incl. 0047, run server + workers
#   (the knowledge indexer worker is spawned in server main.rs alongside the agent responder)
cargo run -p server

# Frontend (from frontend/):
pnpm ng serve dashboard
```

Platform embedding config check: the platform `ai_configurations` row has a non-null `embedding_model` and a matching platform `ai_credentials` row for a provider that supports embeddings (OpenAI or Gemini — **not** Anthropic). **v1 pins OpenAI `text-embedding-3-small` (1536 dims)** to match the `vector(1536)` column; a mismatched-dimension model must fail fast at startup/insert (research §1). Pin the Postgres image to a pgvector ≥ 0.8 tag so iterative index scan is available.

## Scenario 1 — Publish → index → grounded answer (Story 1, P1)

1. Create and **publish** a knowledge article containing a distinctive fact (e.g. "The Acme Widget ships in Marmalade Orange only.").
2. Poll the item: `GET /tenant/knowledge/items/{id}` → `index_status.status` progresses `pending → indexing → indexed` within ~2 min (SC-001), `chunk_count > 0`.
3. In a conversation handled by the AI agent, send a customer message: "What colour does the widget come in?"
4. **Expected**: the AI reply reflects "Marmalade Orange" (grounded, SC-006), and the reply's `citations` array names the article (SC-002).

## Scenario 2 — Tenant isolation (FR-007, SC-003) — the critical test

1. Seed tenant **A** and tenant **B** with an *identical* published article containing the same distinctive fact.
2. Trigger retrieval in a **tenant B** conversation.
3. **Expected**: only tenant B's chunks are retrieved; no tenant A chunk appears regardless of identical similarity. This is asserted by integration test `backend/crates/server/tests/rag_isolation.rs`, which fails if the `tenant_id` filter is removed from the retrieval query.

## Scenario 2b — Recall under a realistic multi-tenant corpus (guards research §1)

1. Seed one **target** tenant plus **many noise tenants** so the target is a small fraction of a large `knowledge_chunks` table; insert a known-good published passage for the target tenant.
2. Run retrieval for a target-tenant query that matches the known-good passage.
3. **Expected**: the known-good passage is in the top-k. This is asserted by `backend/crates/server/tests/rag_recall.rs`. It fails under a naive single-`ef_search` HNSW scan and passes with `hnsw.iterative_scan` + raised `hnsw.ef_search` configured — a gap the 2-tenant isolation test (Scenario 2) cannot detect.

## Scenario 3 — Only published is retrievable (FR-005)

1. Take an indexed published item and **archive** it (or revert to draft).
2. **Expected**: its chunks are removed; a new conversation asking about its content retrieves nothing and the AI answers ungrounded (no citation). `index_status.status` → `not_indexed`.

## Scenario 4 — Citations & durability (Story 2, P2)

1. Open a conversation with a grounded AI reply in the dashboard.
2. **Expected**: citation chips appear under the AI message; selecting one navigates to / previews the source item (`item_available: true`).
3. **Delete** the cited knowledge item, then re-open the historical conversation.
4. **Expected**: the citation still shows the snapshot `item_title` + `passage_text` with a "no longer available" indicator (`item_available: false`) — the answer remains auditable (FR-009).
5. An AI reply produced with **zero** retrieved knowledge shows **no** citation affordance (FR-011).

## Scenario 5 — Index status & manual re-index (Story 3, P3)

1. In the knowledge base UI, confirm each item shows an index-status chip (pending/indexing/indexed/failed/not_indexable).
2. Upload a scanned-image PDF (no text layer) and publish it → **Expected**: `index_status.status = not_indexable` with a clear reason (not an error).
3. For a `failed` item, click **Re-index** (Owner/Admin/Manager). `POST /tenant/knowledge/items/{id}/reindex` → `202`, status returns to `pending` and progresses. Agent/Viewer see status read-only (no re-index control; API returns `403`).

## Scenario 6 — Graceful degradation (FR-017, edge cases)

1. Simulate the embedding provider being unavailable during a conversation (e.g. invalid platform credential / forced timeout).
2. **Expected**: the AI still replies, ungrounded, with no citations — the conversation does not fail. Retrieval adds no user-perceptible delay (SC-004); `rag.retrieve` span shows `degraded=true`.

## Quality gates

```bash
# Backend (from backend/)
cargo test                     # unit + integration (rag_indexing, rag_isolation, rag_citations)
cargo clippy --all-targets

# Frontend (from frontend/)
pnpm ng test dashboard
pnpm lint
pnpm format:check
pnpm ng build dashboard
```

All gates must pass. The isolation test (Scenario 2) and graceful-degradation test (Scenario 6) are the highest-value regression guards.
