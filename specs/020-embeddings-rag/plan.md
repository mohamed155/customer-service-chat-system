# Implementation Plan: Embeddings & RAG

**Branch**: `020-embeddings-rag` | **Date**: 2026-07-17 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/020-embeddings-rag/spec.md`

## Summary

Give the AI agent grounded answers by indexing published knowledge into pgvector and retrieving tenant-scoped passages during conversation turns. Backend: a new `EmbeddingProvider` capability in the `ai-providers` crate (OpenAI + Gemini; platform-wide embedding configuration per clarification), an outbox-driven indexing worker in the `knowledge` module (chunk → embed → atomic replace, bounded retries), a retrieval step inserted into the existing agent responder (context-aware query, threshold + cap, hard tenant scoping, time-budgeted with graceful degradation), and snapshot citations persisted with each grounded AI reply. Frontend: per-item index-status indicator + re-index action in the knowledge base pages, and a reusable citation component on AI messages in the conversation view.

## Technical Context

**Language/Version**: Backend Rust (2021 edition, workspace in `backend/`); Frontend TypeScript / Angular 22 (standalone components, Signals, RxJS-first)

**Primary Dependencies**: Axum, Tokio, SQLx, `pgvector` crate (new, sqlx feature), `pdf-extract` (new, PDF text layer), `ammonia` (existing, HTML→text), `reqwest` (existing, provider HTTP); Taiga UI, NgRx SignalStore

**Storage**: PostgreSQL with pgvector (`CREATE EXTENSION vector` already applied in migration 0001); S3-compatible object storage for uploaded documents (existing `shared/storage`)

**Testing**: `cargo test` (unit + Postgres-backed integration tests in `backend/crates/server/tests`, run via `backend/run-db-tests.cmd` conventions); `pnpm ng test dashboard`, `pnpm lint`, `pnpm format:check` in `frontend/`

**Target Platform**: Linux server (backend), evergreen browsers (dashboard)

**Project Type**: Web application — modular-monolith Rust backend + Angular dashboard

**Performance Goals**: SC-001 publish→retrievable ≤ 2 min; SC-004 retrieval adds ≤ 1 s before response streaming starts (internal budget: ~800 ms for embed+search, then proceed ungrounded); indexing must not block other tenants (single-row claim worker, batched embedding calls)

**Constraints**: Tenant isolation enforced in every chunk/citation query (Principle II); atomic chunk replacement (FR-015); deterministic prompt composition when injecting knowledge context (Principle IV); no new search infrastructure (constitution stack); embeddings use one platform-wide configuration (clarification 2026-07-17)

**Scale/Scope**: Up to ~thousands of knowledge items per tenant, ≤ 500 chunks per item (bounded), 20 MB max uploaded document (existing limit); retrieval top-k ≤ 5 passages per turn

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment |
|---|---|
| I. Modular Monolith | PASS — indexing lives in the `knowledge` module; retrieval is exposed to the `ai` module as a narrow public function (`knowledge::retrieval`); communication with the responder is a plain in-process service call plus existing outbox events; no cross-module table access (citations live in the `conversations` schema written through a conversations query helper). |
| II. Multi-Tenant Isolation | PASS — `knowledge_chunks`, `knowledge_index_state`, `message_citations` all carry `tenant_id`; every retrieval SQL statement filters by `tenant_id` (plus join to published items as defense in depth); FR-007 isolation tests are mandatory integration tests. |
| III. Zero-Trust & RBAC | PASS — re-index endpoint requires knowledge manage permission (Owner/Admin/Manager) via existing `authz`; status is readable by all tenant roles; no new secrets (platform embedding credential reuses encrypted `ai_credentials`). |
| IV. AI Provider Independence & Tool-Mediated Access | PASS — embeddings go through a new `EmbeddingProvider` trait in `ai-providers`; the LLM never queries the DB (retrieval is performed by backend code and injected as deterministic prompt context); Anthropic offers no embeddings API — the trait makes embedding support an optional provider capability (recorded in research, not a violation: the abstraction stays uniform, capability discovery is explicit). |
| V. API-First & Contract Consistency | PASS — index status and citations are additive fields on existing REST contracts; new `POST /tenant/knowledge/items/{id}/reindex` is idempotent (re-request while pending is a no-op); documented in `contracts/`. |
| VI. Observability | PASS — `rag.retrieve` and `rag.index` tracing spans with tenant/conversation/message correlation satisfy FR-014 (internal observability only, per clarification); embedding calls append to `ai_usage_records`. |
| VII. Test-First & Regression | PASS — unit (chunker, threshold, prompt block), integration (indexing lifecycle, isolation, citation persistence), API (contract fields, RBAC on reindex), frontend specs. |
| VIII. DB Integrity & Migrations | PASS — single migration `0047_embeddings_rag.sql`; normalized tables; HNSW + b-tree indexes on production query paths; citation snapshot denormalization is deliberate and justified (point-in-time record, per clarification). |
| IX. Design System Discipline | PASS — citation UI is a reusable `shared/components` citation-list component consumed by the conversation thread; status chip reuses existing badge patterns; tokens only. |
| X. Performance & Efficiency | PASS — time-budgeted retrieval, batched embedding requests, batch citation loading for timelines (no N+1), HNSW index for search. **Note**: filtered-ANN recall on a shared HNSW index requires pgvector ≥ 0.8 iterative scan + raised `ef_search` so the `tenant_id` filter does not starve recall; guarded by a realistic-distribution recall test (research §1, §9). |

**Post-design re-check (after Phase 1)**: PASS — no violations introduced by the data model or contracts; Complexity Tracking left empty.

## Project Structure

### Documentation (this feature)

```text
specs/020-embeddings-rag/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   ├── embedding-provider.md      # internal trait contract
│   ├── knowledge-indexing.md      # index status + reindex REST contract
│   └── conversation-citations.md  # message citations REST contract
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   └── 0047_embeddings_rag.sql          # knowledge_chunks, knowledge_index_state, message_citations
└── crates/
    ├── ai-providers/src/
    │   ├── contract.rs                  # + EmbeddingProvider trait, EmbeddingRequest/Response
    │   ├── openai.rs                    # + /v1/embeddings implementation
    │   ├── gemini.rs                    # + batchEmbedContents implementation
    │   └── registry.rs                  # + embedding capability lookup
    ├── modules/ai/src/
    │   ├── service.rs                   # + AiService::embed_platform (platform config + credential + usage)
    │   └── agent_responder.rs           # + retrieval step (Phase B) + citation write (Phase C)
    ├── modules/knowledge/src/
    │   ├── chunking.rs                  # NEW — text extraction + deterministic chunker
    │   ├── indexer.rs                   # NEW — outbox-driven indexing worker (claim, retry, atomic replace)
    │   ├── index_state.rs               # NEW — knowledge_index_state persistence
    │   ├── retrieval.rs                 # NEW — tenant-scoped similarity search (public interface for ai module)
    │   ├── store.rs                     # + chunk persistence, + outbox enqueue on publish/edit/unpublish/delete
    │   └── routes.rs                    # + index_status in item payloads, + POST /items/{id}/reindex
    ├── modules/conversations/src/
    │   ├── model.rs                     # + citations on Message view
    │   └── queries.rs                   # + insert_citations_in_tx, batch load citations for timeline
    └── server/
        ├── src/main.rs                  # + spawn knowledge indexer worker
        └── tests/                       # + rag_indexing.rs, rag_isolation.rs, rag_recall.rs, rag_citations.rs

frontend/apps/dashboard/src/app/
├── shared/components/citation-list/     # NEW — reusable citation chips + preview dialog
└── features/tenant/
    ├── knowledge-base/                  # index-status chip (list + detail), re-index action, status polling
    │   ├── knowledge-api.service.ts     # + indexStatus field, + reindex()
    │   └── knowledge.store.ts           # + status refresh while non-terminal
    └── conversations/
        ├── conversations-api.service.ts # + citations on message model
        └── conversation-thread.component.ts  # renders citation-list under AI messages
```

**Structure Decision**: Follows the existing modular-monolith layout — indexing/retrieval concerns live in `backend/crates/modules/knowledge`, provider capability in `backend/crates/ai-providers`, pipeline insertion in `backend/crates/modules/ai`, citation persistence/exposure in `backend/crates/modules/conversations`. Frontend work stays inside the two existing tenant feature areas plus one new reusable shared component, per Principle IX.

## Complexity Tracking

> No constitution violations — table intentionally empty.
