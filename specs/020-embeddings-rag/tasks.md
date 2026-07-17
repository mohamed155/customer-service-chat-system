---

description: "Task list for Embeddings & RAG (020-embeddings-rag)"
---

# Tasks: Embeddings & RAG

**Input**: Design documents from `/specs/020-embeddings-rag/`
**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: Included — the constitution (Principle VII: Test-First & Regression Discipline) mandates unit/integration/API coverage as a required category for shipped functionality, and FR-007/SC-003 explicitly require automated isolation tests.

**Organization**: Tasks are grouped by user story (US1 = Story 1 "AI answers grounded in tenant knowledge" P1, US2 = Story 2 "Citations on AI responses" P2, US3 = Story 3 "Indexing status and re-indexing" P3) so each can be implemented, tested, and delivered independently.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1 / US2 / US3 — omitted for Setup, Foundational, and Polish tasks
- File paths are exact, relative to the repository root

## Path Conventions (from plan.md)

- Backend: `backend/crates/ai-providers/src/`, `backend/crates/modules/{ai,knowledge,conversations}/src/`, `backend/crates/server/{src,tests}/`, `backend/migrations/`
- Frontend: `frontend/apps/dashboard/src/app/{shared/components,features/tenant/{knowledge-base,conversations}}/`

---

## Phase 1: Setup

**Purpose**: Add new dependencies and confirm the environment can support pgvector's filtered-ANN requirements before any code is written.

- [x] T001 Add `pdf-extract` dependency to `backend/crates/modules/knowledge/Cargo.toml` (PDF text-layer extraction, research.md §4)
- [x] T002 Pin the Postgres image in `infra/docker-compose.yml` to a `pgvector/pgvector:pg16` tag known to ship pgvector ≥ 0.8.0 (required for `hnsw.iterative_scan`, research.md §1); record the pinned tag in a comment
- [x] T003 [P] Verify/document the local dev DB's pgvector version (`SELECT extversion FROM pg_extension WHERE extname = 'vector';`) meets ≥ 0.8.0 in `backend/migrations/README.md`

**Checkpoint**: Dependencies declared; pgvector version confirmed to support the filtered-recall strategy the retrieval design depends on.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Schema, provider capability, and platform embedding plumbing that every user story's retrieval/indexing/citation code depends on. No user-story work can start until this phase is done.

**⚠️ CRITICAL**: US1, US2, and US3 all read/write these tables and call these functions.

- [x] T004 Write migration `backend/migrations/0047_embeddings_rag.sql`: `knowledge_chunks` (with `vector(1536)` column, `content_hash`, composite FK to `knowledge_items(tenant_id, id)`, `UNIQUE(item_id, ordinal)`), `knowledge_index_state` (1:1 with item, status enum incl. `not_indexable`, `attempts`, `indexed_content_hash`), `message_citations` (tenant-scoped, FK to `messages(id)` ON DELETE CASCADE, `knowledge_item_id` with **no** FK, snapshot `item_title`/`passage_text`), per data-model.md
- [x] T005 In the same migration, add HNSW index `CREATE INDEX ON knowledge_chunks USING hnsw (embedding vector_cosine_ops)`, b-tree `(tenant_id, item_id)` on `knowledge_chunks`, b-tree `(tenant_id, status)` on `knowledge_index_state`, b-tree `(message_id)` on `message_citations`
- [x] T006 In the same migration, add nullable `embedding_model TEXT` column to `ai_configurations` (platform-wide embedding model selection, research.md §3)
- [x] T007 [P] Add `EmbeddingRequest`, `EmbeddingResponse`, `EmbeddingProvider` trait to `backend/crates/ai-providers/src/contract.rs` per contracts/embedding-provider.md
- [x] T008 [P] Implement `EmbeddingProvider` for OpenAI (`POST /v1/embeddings`, batch input) in `backend/crates/ai-providers/src/openai.rs`
- [x] T009 [P] Implement `EmbeddingProvider` for Gemini (`batchEmbedContents`) in `backend/crates/ai-providers/src/gemini.rs` — **forward-looking capability for provider-independence (Principle IV), NOT the wired v1 platform embedding provider.** v1 pins OpenAI/1536 (research.md §1); Gemini's differing dimensions make it invalid as the configured platform model until the migration's `vector(N)` is changed. Implement the trait + a unit test; do not point the platform `embedding_model` at Gemini in v1. If deferring Gemini embeddings entirely is preferred, this task may be dropped from v1 without affecting any FR/SC.
- [x] T010 Add `Registry::embedding_provider(name) -> Option<&dyn EmbeddingProvider>` capability lookup to `backend/crates/ai-providers/src/registry.rs` (returns `None` for `"anthropic"`)
- [x] T011 Implement `AiService::embed_platform(ctx, inputs) -> Result<Vec<Vec<f32>>, AiCallError>` in `backend/crates/modules/ai/src/service.rs`: resolve platform config/credential via `resolution::Scope::Platform`, look up `embedding_model`, call the embedding provider, assert returned vector length == 1536 (fail fast on dimension mismatch, research.md §1), append an `ai_usage_records` row (depends on T004, T006, T007–T010)
- [x] T012 [P] Create `backend/crates/modules/knowledge/src/chunking.rs`: text extraction (HTML→text for authored articles/FAQs via existing `ammonia` sanitizer, plain/markdown verbatim, PDF via `pdf-extract`) + deterministic paragraph/sentence-aware chunker (~500–800 tokens, ~15% overlap, ≤ 500 chunks/item cap, content hash); returns a "not indexable" signal for empty extraction (depends on T001)
- [x] T013 [P] Create `backend/crates/modules/knowledge/src/index_state.rs`: CRUD for `knowledge_index_state` (get/upsert status, set failed with reason, set indexed with content hash + chunk count, increment attempts) (depends on T004)
- [x] T014 Create `backend/crates/modules/knowledge/src/retrieval.rs`: public `search(pool, tenant_id, query_embedding, top_k, threshold) -> Vec<RetrievedChunk>` — sets `hnsw.iterative_scan = 'relaxed_order'` and a raised `hnsw.ef_search` per-query (research.md §1), joins to `knowledge_items` filtering `status = 'published'`, filters `tenant_id`, applies the relevance threshold and `LIMIT top_k` (depends on T004, T005)
- [x] T015 [P] [Foundational] Unit tests for the chunker in `backend/crates/modules/knowledge/src/chunking.rs` (`#[cfg(test)] mod tests`): deterministic output for identical input, paragraph-boundary splitting, overlap, chunk-count cap, not-indexable on empty text (depends on T012)

**Checkpoint**: Schema exists, embeddings can be generated through the provider abstraction with a dimension guard, chunking is deterministic and testable, and tenant-scoped similarity search with recall-safe filtering is available for US1 to consume.

---

## Phase 3: User Story 1 - AI answers grounded in tenant knowledge (Priority: P1) 🎯 MVP

**Goal**: Published knowledge is automatically indexed; the AI agent retrieves tenant-scoped relevant passages during a conversation turn and uses them to ground its reply; retrieval is provably tenant-isolated and degrades gracefully when unavailable.

**Independent Test**: Publish a knowledge article with a distinctive fact, ask the AI about it in a conversation, and confirm the response reflects the article's content (quickstart.md Scenario 1) — no citation UI required.

### Tests for User Story 1

> Write these first; they must fail before the corresponding implementation lands.

- [x] T016 [P] [US1] Integration test `backend/crates/server/tests/rag_indexing.rs`: publishing an item enqueues an outbox event and progresses `index_status` `pending → indexing → indexed` with `chunk_count > 0`; editing a published item re-triggers indexing; archiving/reverting to draft removes chunks and resets status; a failed embed leaves prior chunks intact (atomic replace, FR-015)
- [x] T017 [P] [US1] Integration test `backend/crates/server/tests/rag_isolation.rs`: tenant A and tenant B seeded with *identical* published content; retrieval for a tenant B conversation returns zero tenant-A chunks regardless of similarity (FR-007, SC-003); a second case asserts draft/archived items in the same tenant are never returned (FR-005)
- [x] T018 [P] [US1] Integration test `backend/crates/server/tests/rag_recall.rs`: seed one target tenant + many noise tenants in a large `knowledge_chunks` table, assert a known-good passage for the target tenant is returned in the top-k for a matching query (guards the filtered-ANN recall gap in research.md §1/§9 that `rag_isolation.rs` cannot detect)
- [x] T019 [P] [US1] Integration test in `backend/crates/server/tests/rag_indexing.rs` (or a dedicated `rag_degradation.rs`): embedding-provider unavailable/timeout during a conversation → AI still replies ungrounded, conversation does not fail, no citations recorded (FR-017, edge case)

### Implementation for User Story 1

- [x] T020 [US1] Add outbox enqueue of `knowledge.index_requested` (transactional, same tx as the write) to `create_document_in_tx`/publish/update/`set_status_in_tx` paths in `backend/crates/modules/knowledge/src/store.rs`: on publish and on published-content edit → enqueue index; on unpublish (archive/revert-to-draft) → delete the item's `knowledge_chunks` rows and reset `knowledge_index_state` to `not_indexed` (FR-004) (depends on T004, T013)
- [x] T021 [US1] Create `backend/crates/modules/knowledge/src/indexer.rs`: `run_knowledge_indexer_worker` — single-row `FOR UPDATE SKIP LOCKED` claim of `knowledge.index_requested` outbox events, skip if the item's current content hash already matches `indexed_content_hash` (coalescing rapid edits), extract+chunk via `chunking.rs`, embed via `AiService::embed_platform`, atomically replace `knowledge_chunks` for the item in one transaction, update `knowledge_index_state` to `indexed`/`failed`/`not_indexable`; on retriable `ProviderError` re-schedule with backoff up to a bounded attempt count, then mark `failed` with reason (FR-016) (depends on T011, T012, T013, T014, T020)
- [x] T022 [US1] Emit `rag.index` tracing span (tenant_id, item_id, chunk_count, attempt, outcome) in `indexer.rs` (FR-014, Principle VI) (depends on T021)
- [x] T023 [US1] Spawn the indexer worker alongside the existing outbox workers in `backend/crates/server/src/main.rs` (`tokio::spawn(knowledge::indexer::run_knowledge_indexer_worker(...))`) (depends on T021)
- [x] T024 [US1] Build the context-aware search query in `backend/crates/modules/ai/src/agent_responder.rs` Phase B: concatenate the latest customer message plus the last 1–3 customer turns from `recent_history` into one query string (FR-006, research.md §6) (depends on existing `agent_responder.rs`)
- [x] T025 [US1] Insert the retrieval step into `agent_responder.rs`: embed the query via `AiService::embed_platform`, call `knowledge::retrieval::search` with an ~800ms timeout, apply relevance threshold + top-k ≤ 5 cap (FR-008); on timeout/error/empty result proceed ungrounded (FR-017, SC-004) (depends on T011, T014, T024)
- [x] T026 [US1] Inject retrieved passages as a deterministic, clearly delimited block appended to the composed system message before the vendor call (Principle IV: deterministic prompt construction) (depends on T025)
- [x] T027 [US1] Emit `rag.retrieve` tracing span (tenant_id, conversation_id, message_id, query_len, candidates, returned, top_score, elapsed_ms, degraded) in `agent_responder.rs` (FR-014) (depends on T025)

**Checkpoint**: User Story 1 is fully functional and independently testable — publishing grounds AI answers, isolation and recall are proven by tests, and provider outages degrade gracefully. This is the MVP.

---

## Phase 4: User Story 2 - Citations on AI responses (Priority: P2)

**Goal**: Every AI reply that used retrieved knowledge carries citations identifying the source items and cited passages, captured as a durable snapshot so historical conversations remain auditable even if the source is later changed, archived, or deleted; uncited replies show no citation affordance.

**Independent Test**: Trigger a knowledge-grounded AI response and confirm it carries citations naming the source items and linking to their detail views (quickstart.md Scenario 4).

### Tests for User Story 2

- [x] T028 [P] [US2] Integration test `backend/crates/server/tests/rag_citations.rs`: a grounded AI reply persists one `message_citations` row per source chunk with snapshotted `item_title`/`passage_text`; an ungrounded reply persists zero rows; the timeline endpoint returns `citations: []` for non-AI and ungrounded messages (FR-009, FR-011)
- [x] T029 [P] [US2] Integration test in `rag_citations.rs`: deleting/archiving a cited knowledge item after the reply was sent leaves the citation's snapshot (`item_title`, `passage_text`) unchanged and flips `item_available` to `false` on read (Story 2 acceptance #4)
- [x] T030 [P] [US2] Contract/API test asserting the timeline handler batch-loads citations for all returned messages in a single query (no N+1), consistent with contracts/conversation-citations.md rule 5

### Implementation for User Story 2

- [x] T031 [US2] Add `insert_citations_in_tx(tx, tenant_id, message_id, citations: &[RetrievedChunk])` to `backend/crates/modules/conversations/src/queries.rs`: writes one `message_citations` row per supplied chunk with snapshot `item_title`/`passage_text`, `relevance_score`, `ordinal` (depends on T004)
- [x] T032 [US2] Call `insert_citations_in_tx` from `agent_responder.rs` Phase C, in the same transaction as `insert_ai_reply_in_tx`, passing the chunks retrieved in T025 (depends on T025, T031)
- [x] T033 [US2] Add batch citation loading (single query keyed by the timeline's message id list) to `backend/crates/modules/conversations/src/queries.rs`, resolving `item_available` via a live lookup against `knowledge_items` (depends on T004)
- [x] T034 [US2] Add `citations: Vec<CitationView>` field to the `Message` struct in `backend/crates/modules/conversations/src/model.rs` and populate it in the timeline/`AddMessageResponse` handlers in `backend/crates/modules/conversations/src/routes.rs` (depends on T033)
- [x] T035 [P] [US2] Create reusable `frontend/apps/dashboard/src/app/shared/components/citation-list/citation-list.component.ts`: renders citation chips, opens a preview/navigates to the item detail when `item_available`, shows a "no longer available" badge with the snapshot otherwise (FR-011, Story 2 acceptance #2/#4)
- [x] T036 [US2] Add `citations` to the message model in `frontend/apps/dashboard/src/app/features/tenant/conversations/conversations-api.service.ts` and render `citation-list` under AI messages in `frontend/apps/dashboard/src/app/features/tenant/conversations/conversation-thread.component.ts` (empty array → no affordance) (depends on T034, T035)
- [x] T037 [P] [US2] Frontend spec for `citation-list.component.ts` covering available/unavailable citation states and empty-array rendering (`citation-list.component.spec.ts`)

**Checkpoint**: User Stories 1 AND 2 both work independently — grounded replies are now auditable via durable citation snapshots in the dashboard.

---

## Phase 5: User Story 3 - Indexing status and re-indexing (Priority: P3)

**Goal**: Knowledge managers see per-item index status (not indexed, pending, indexing, indexed, failed-with-reason, not-indexable) in the knowledge base UI and can trigger re-indexing; Agent/Viewer roles see status read-only.

**Independent Test**: Publish an item, observe status progress to "indexed," force a failure or staleness, trigger re-index, confirm status reflects each transition (quickstart.md Scenario 5).

### Tests for User Story 3

- [x] T038 [P] [US3] API test: `POST /tenant/knowledge/items/{id}/reindex` returns `202` and resets status to `pending` for a published item; returns `409 not_publishable` for a draft/archived item; returns `403` for Agent/Viewer roles (RBAC, contracts/knowledge-indexing.md)
- [x] T039 [P] [US3] API test: re-invoking reindex while already `pending`/`indexing` is a no-op (idempotent, Principle V) and does not enqueue a duplicate outbox event
- [x] T040 [P] [US3] Integration test: uploading a document with no extractable text (e.g., scanned-image PDF) resolves to `not_indexable` with a reason, not an error (edge case)

### Implementation for User Story 3

- [x] T041 [US3] Add `index_status` (status, failure_reason, last_indexed_at, chunk_count) to knowledge item list/detail response payloads in `backend/crates/modules/knowledge/src/routes.rs`, sourced from `knowledge_index_state` via `index_state.rs` (depends on T013)
- [x] T042 [US3] Add `POST /tenant/knowledge/items/{id}/reindex` route in `backend/crates/modules/knowledge/src/routes.rs`: require knowledge-manage permission (Owner/Admin/Manager) via existing `authz`, validate item is `published` (else `409`), reset `attempts`, enqueue `knowledge.index_requested`, set status `pending` idempotently (depends on T020, T041)
- [x] T043 [US3] Handle not-indexable resolution in `indexer.rs`: when `chunking.rs` signals no extractable text, set `knowledge_index_state.status = 'not_indexable'` with a clear reason instead of treating it as a failure (depends on T012, T021)
- [x] T044 [P] [US3] Add `index_status` field + `reindex()` call to `frontend/apps/dashboard/src/app/features/tenant/knowledge-base/knowledge-api.service.ts`
- [x] T045 [US3] Add a status chip (not indexed/pending/indexing/indexed/failed/not indexable) to the knowledge list and detail views, plus a re-index action (visible/enabled only for Owner/Admin/Manager; read-only for Agent/Viewer) in `frontend/apps/dashboard/src/app/features/tenant/knowledge-base/knowledge-base.component.ts` and `article-detail.component.ts` (depends on T044)
- [x] T046 [US3] Add a short-interval status refresh (poll or re-fetch) in `frontend/apps/dashboard/src/app/features/tenant/knowledge-base/knowledge.store.ts` while any item's `index_status.status` is non-terminal (`pending`/`indexing`), so users can watch progress without leaving the page (SC-005) (depends on T044)
- [x] T047 [P] [US3] Frontend specs for the status chip and re-index action (role-gating, all six status states) in `knowledge-base.component.spec.ts` / `article-detail.component.spec.ts`

**Checkpoint**: All three user stories are independently functional — indexing is observable and operable end-to-end by tenant knowledge managers.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Validation, documentation, and quality-gate confirmation across all three stories.

- [x] T048 [P] Add/update OpenAPI schema entries for `index_status`, `POST /tenant/knowledge/items/{id}/reindex`, and `citations` in the swagger doc generation covered by `backend/crates/server/tests/openapi_contract.rs` / `openapi_coverage.rs`
- [x] T049 [P] Update module doc comments (`Purpose`/`Responsibilities`/`Data Model`/`Extension Points`) in `backend/crates/modules/knowledge/src/lib.rs` and `backend/crates/modules/ai/src/lib.rs` to reflect indexing/retrieval/embedding additions (constitution: Documentation & Future Readiness)
- [x] T050 Run `cargo clippy --all-targets` and `cargo test` across the workspace; fix any warnings/failures introduced by this feature
- [x] T051 Run `pnpm ng build dashboard`, `pnpm ng test dashboard`, `pnpm lint`, `pnpm format:check` in `frontend/`; fix any failures
- [x] T052 Execute all six quickstart.md scenarios end-to-end (publish→index→grounded answer, isolation, recall, published-only, citations & durability, status & re-index, graceful degradation) and record results

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately
- **Foundational (Phase 2)**: Depends on Setup (T001 for T012) — BLOCKS all user stories (US1 retrieval needs T007–T014; US2/US3 tables need T004–T006)
- **User Story 1 (Phase 3)**: Depends on Foundational completion. No dependency on US2/US3.
- **User Story 2 (Phase 4)**: Depends on Foundational completion **and** on US1's retrieval step (T025) to have chunks to cite — citations without retrieval have nothing to record. Not independently buildable before US1, but independently *testable* once US1 exists (a grounded reply is the precondition, not a shared implementation task).
- **User Story 3 (Phase 5)**: Depends on Foundational completion **and** on US1's indexing pipeline (T020, T021) to have status to report and re-index to trigger.
- **Polish (Phase 6)**: Depends on all desired user stories being complete.

### User Story Dependencies

- **US1 (P1)**: Foundational only. Fully independent — this is the MVP.
- **US2 (P2)**: Foundational + US1's retrieval output (chunks retrieved per turn). Independently testable once US1 ships.
- **US3 (P3)**: Foundational + US1's indexing pipeline (outbox enqueue, indexer worker, index_state). Independently testable once US1 ships.

### Within Each User Story

- Tests written first, confirmed failing, then implementation
- Backend before frontend within a story (frontend consumes the contract fields backend adds)
- Story checkpoint validated via its quickstart.md scenario before moving to the next priority

### Parallel Opportunities

- T003, T007–T010, T012, T013, T015 (Phase 2) are marked [P] — different files, no cross-dependency
- T016–T019 (US1 tests) are marked [P] — independent test files
- T028–T030 (US2 tests) are marked [P]; T035/T037 (US2 frontend) are marked [P]
- T038–T040 (US3 tests) are marked [P]; T044/T047 (US3 frontend) are marked [P]
- T048/T049 (Polish) are marked [P]

---

## Parallel Example: Foundational Phase

```bash
# After T004–T006 (migration) land, launch the provider-capability and chunker work together:
Task: "Add EmbeddingRequest/EmbeddingResponse/EmbeddingProvider trait to backend/crates/ai-providers/src/contract.rs"
Task: "Implement EmbeddingProvider for OpenAI in backend/crates/ai-providers/src/openai.rs"
Task: "Implement EmbeddingProvider for Gemini in backend/crates/ai-providers/src/gemini.rs"
Task: "Create backend/crates/modules/knowledge/src/chunking.rs"
Task: "Create backend/crates/modules/knowledge/src/index_state.rs"
```

## Parallel Example: User Story 1 Tests

```bash
Task: "Integration test backend/crates/server/tests/rag_indexing.rs"
Task: "Integration test backend/crates/server/tests/rag_isolation.rs"
Task: "Integration test backend/crates/server/tests/rag_recall.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001–T003)
2. Complete Phase 2: Foundational (T004–T015) — CRITICAL, blocks everything
3. Complete Phase 3: User Story 1 (T016–T027)
4. **STOP and VALIDATE**: run quickstart.md Scenarios 1, 2, 2b, 3, 6 independently
5. Deploy/demo — grounded AI answers with proven tenant isolation and graceful degradation is a viable MVP on its own (per spec.md Story 1 rationale)

### Incremental Delivery

1. Setup + Foundational → foundation ready
2. Add US1 → validate independently → deploy/demo (MVP!)
3. Add US2 → validate independently (quickstart Scenario 4) → deploy/demo
4. Add US3 → validate independently (quickstart Scenario 5) → deploy/demo
5. Polish (Phase 6) → final quality gates + full quickstart pass

### Parallel Team Strategy

With multiple developers, after Foundational (Phase 2) completes:
- Developer A: US1 backend (indexer + retrieval, T020–T027)
- Developer B: starts US2/US3 test scaffolding and frontend shells in parallel, wiring them to US1's outputs as T025/T020–T021 land

---

## Notes

- [P] tasks touch different files with no dependency on an incomplete task
- [Story] label maps every user-story-phase task to US1/US2/US3 for traceability
- Foundational-phase task T015 has no [Story] label (shared infrastructure test), consistent with template rules
- Commit after each task or logical group; verify tests fail before implementing them
- Stop at each phase checkpoint and run the matching quickstart.md scenario before proceeding
- Avoid cross-story file conflicts: US2 and US3 touch disjoint files in `conversations` vs `knowledge-base` respectively, aside from both reading `agent_responder.rs`'s retrieval output (US2) and `store.rs`'s outbox enqueue (US3) — neither writes to the other's files

---

## Phase 7: Convergence

- [x] T053 Make `reindex_item` idempotent so a re-request while the item's index state is already `pending` or `indexing` is a true no-op that enqueues **no** duplicate `knowledge.index_requested` outbox event, while still returning `202` per `US3/AC` (`backend/crates/modules/knowledge/src/routes.rs:1279` currently calls `store::enqueue_index_requested_in_tx` unconditionally after the `published` check, and `store::enqueue_index_requested_in_tx` at `store.rs:422` always INSERTs a fresh outbox event; guard the enqueue on current status). This satisfies the plan's Constitution Check V commitment ("re-request while pending is a no-op") and makes the DB-gated test `reindex_while_pending_is_noop` in `backend/crates/server/tests/rag_reindex.rs:492` (asserting `events_after == events_before`) pass against a real database rather than only when skipped. per plan: Constitution V (idempotent reindex)
