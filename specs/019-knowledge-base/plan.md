# Implementation Plan: Knowledge Base

**Branch**: `019-knowledge-base` | **Date**: 2026-07-17 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/019-knowledge-base/spec.md`

## Summary

Give tenants a real knowledge base: authored articles/FAQs with a rich-text (WYSIWYG) editor, uploaded documents stored in S3-compatible object storage, flat categories plus free-form tags, and a strict draft → published → archived lifecycle where only published items count as AI-available (ingestion/RAG itself is out of scope). Mechanically: the M0 placeholder crate `modules/knowledge` comes alive with four tables (migration `0046_knowledge_base.sql`: `knowledge_categories`, `knowledge_items`, `knowledge_documents`, `knowledge_item_tags`) and eleven `/tenant/knowledge/*` endpoints behind the **existing** `knowledge_base.view/manage` permission codes (the role matrix already matches clarification #1 — zero RBAC changes). A new `shared/storage` crate wraps `aws-sdk-s3` behind a 3-method `ObjectStorage` trait (MinIO-compatible, in-memory impl for tests), threaded through the router as an optional param and layered as an `Extension` following the existing `email_sender` precedent — deliberately *not* an `AppState` field (R2). Uploads are multipart proxied through the API (20 MB cap, pdf/docx/txt/md allowlist, object-before-row orphan safety); downloads stream back through the same authz stack. Article HTML is sanitized server-side with `ammonia` on every write. The dashboard's fixture-backed knowledge-base page becomes a real feature: list with filters, article editor (new `@taiga-ui/editor` wrapped per design-system rules), detail page, upload + category dialogs, and permission-gated publish/archive/restore actions.

## Technical Context

**Language/Version**: Backend Rust (Cargo workspace, edition 2021); frontend Angular 22 standalone (TypeScript, Signals, RxJS-first, Taiga UI 5) in `frontend/apps/dashboard`

**Primary Dependencies**: Backend — Axum (workspace features gain `"multipart"`), SQLx/PostgreSQL, existing `tenancy` (`audit::record_in_tx`), `authz` (existing `knowledge_base.view/manage` codes — no matrix or catalog change), `kernel` (envelope, `PageParams`), utoipa. **New workspace deps**: `aws-sdk-s3` + `aws-config` (confined to new `shared/storage` crate, research R2) and `ammonia` (HTML sanitization, R10). **Activated crate**: `modules/knowledge` (M0 placeholder → real module, R1). Frontend — existing typed `ApiResponse<T>` HTTP layer, NgRx SignalStore, `APP_PATHS` routing, shared Taiga-wrapped components; **new dep** `@taiga-ui/editor` ^5 wrapped in a feature component (R11)

**Storage**: PostgreSQL — migration `0046_knowledge_base.sql`: `knowledge_categories` (flat, per-tenant case-insensitive unique name, hard-delete), `knowledge_items` (type/status CHECKs, title 1–200, body ≤ 100k & NULL for documents, `category_id … ON DELETE SET NULL`, author snapshot columns, list/status/category indexes), `knowledge_documents` (1:1, unique `storage_key = {tenant_id}/knowledge/{item_id}`, size CHECK ≤ 20 MB), `knowledge_item_tags` (tag-as-value rows, PK `(item_id, tag)`, `(tenant_id, tag)` index) — see [data-model.md](./data-model.md). S3-compatible object storage (MinIO already in `infra/docker-compose.yml`) for document bytes; `AppConfig` gains one grouped `s3: Option<S3Config>` field with secret redaction (R2 — one mechanical line added to the 17 existing `AppConfig` test literals)

**Testing**: `cargo test` — unit: transition rules, upload validation (type/size matrix), tag normalization, filename sanitization, sanitizer behavior pinning; integration `server/tests/knowledge_base.rs` (DB-gated per `REQUIRE_DB_TESTS`, storage faked via `InMemoryStorage`): CRUD + transition matrix (incl. no-op, illegal edges, empty-body publish block), upload happy/reject/orphan-safety, download incl. missing-object → `not_found`, category CRUD + `SET NULL`, filter/pagination behavior, per-route RBAC (view vs manage), cross-tenant `not_found`, audit rows for all four actions (attribution, no content in details); `openapi_contract.rs`/`openapi_coverage.rs` for 11 new paths + DTOs (multipart body documented); `shared/db/tests/schema.rs` 0046 assertions. Frontend: `pnpm ng test dashboard` — store specs (list/filter/create/edit/transition/upload flows incl. rejection), component specs (list gating by permission, editor validation, upload dialog errors, detail file-unavailable state); lint/format/build gates. Full commands in [quickstart.md](./quickstart.md)

**Target Platform**: Linux server (backend) + evergreen-browser dashboard; MinIO/S3-compatible endpoint reachable from the backend only

**Project Type**: Web application — existing Cargo workspace backend + Angular dashboard frontend

**Performance Goals**: List = one indexed cursor-paginated scan + one `ANY($ids)` tags query (no N+1, no offsets); writes = one short transaction (+ audit row in-tx); upload/download proxy a ≤ 20 MB body without ever touching disk; publish/archive = single-row update + audit. All item queries ride the `(tenant_id, updated_at DESC, id DESC)` / `(tenant_id, status)` indexes (Constitution X)

**Constraints**: Tenant isolation on every table incl. children, cross-tenant answers `not_found` (Constitution II); all routes behind `require_permission`, writes audited in-transaction, file bytes/body content never in logs or audit details (Constitution III + 015 invariant); object keys tenant-prefixed so cross-tenant collisions are structurally impossible; lifecycle transitions exactly FR-003's three edges, publish requires non-empty body for authored types (FR-004); server-side HTML sanitization on every write — client sanitization is defense-in-depth only (R10); incomplete uploads leave no rows and no permanently orphaned objects (FR-016, object-before-row + compensating delete, R3); schema via migration 0046 only; published edits go live in place (clarification #2)

**Scale/Scope**: 1 migration, 4 new tables; 11 new tenant endpoints, 0 new permission codes, 0 matrix changes; audit vocabulary +4 actions; 1 new shared crate (`storage`), 1 activated module crate (`knowledge`: ~5 files — routes, store, validate, upload, lib), router + config edits (plus a one-line `s3: None,` sweep across 17 existing test literals; `state.rs` untouched — R2); frontend: 1 feature area rebuilt (~12 files under `features/tenant/knowledge-base/`: models, api service, store, list/detail/editor pages, upload + category dialogs, editor wrapper), `APP_PATHS`/`PAGE_TITLES` child-route additions; 1 new integration suite + 3 extended test files

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment | Status |
|-----------|------------|--------|
| I. Enterprise Modular Monolith | Feature lives in the purpose-built `modules/knowledge` placeholder (R1) with zero inbound module edges (AI ingestion explicitly out of scope) and one infra dependency on the new `shared/storage` trait crate — vendor SDK types never cross the boundary, so extraction stays a table+bucket move | ✅ Pass |
| II. Multi-Tenant Isolation | `tenant_id NOT NULL` on all four tables including children (no-join enforcement, 018 precedent); every query tenant-scoped; object keys tenant-prefixed; downloads proxied through the same tenancy stack (R4); cross-tenant → `not_found`; isolation cases per resource in `knowledge_base.rs` | ✅ Pass |
| III. Zero-Trust Security & RBAC | All 11 routes behind `require_permission` with the pre-existing `knowledge_base.*` codes (matrix already Owner/Admin/Manager for manage — clarification #1 satisfied by verified existing code, pinned by new RBAC tests); create/publish/archive/restore audited in-transaction with actor attribution; stored-XSS blocked server-side via `ammonia` (R10); upload validation server-side regardless of client checks; no secrets in code — S3 creds via env config with Debug redaction | ✅ Pass |
| IV. AI Provider Independence & Tool-Mediated Access | No LLM involvement anywhere in this feature; the published set is exposed as plain queryable state (FR-015) for a future ingestion feature to consume through the module boundary — no direct AI-to-data path is introduced | ✅ Pass |
| V. API-First & Contract Consistency | Eleven REST endpoints in [contracts/rest-api.md](./contracts/rest-api.md) with the standard envelope/error vocabulary and existing cursor-pagination shape; status transitions replay-safe via structured no-op (`changed: false`), category delete replay answers `not_found`; OpenAPI covers all paths incl. the multipart body | ✅ Pass |
| VI. Observability by Default | Handlers emit structured events (item id, action, latency) under the existing request-id propagation; upload/download log key + size, never bytes; audit trail for every lifecycle mutation | ✅ Pass |
| VII. Test-First & Regression Discipline | Unit (transitions, validation matrices, normalization), integration (full endpoint/RBAC/isolation/audit matrix, DB-gated), schema (0046), OpenAPI contract/coverage, frontend store + component specs — obligations enumerated in the contract and quickstart | ✅ Pass |
| VIII. Database Integrity & Migration Discipline | Migration 0046 only; 005 conventions on items (UUID PK, timestamps); two justified deviations recorded in R6 (category hard-delete implementing FR-008's exact semantics; tag-as-value rows over a lifecycle-free tag entity); all production query paths indexed; tags fetched set-wise (no N+1) | ✅ Pass |
| IX. Design System Discipline | Rebuilt pages compose existing shared components (`toolbar`, `search-input`, `status-badge`, `empty-state`, `dialog-shell`) and `--app-*` tokens; `@taiga-ui/editor` is wrapped in a feature component so no raw Taiga leaks into pages (R11); no duplicated UI logic — validation mirrors live once per side | ✅ Pass |
| X. Performance & Efficiency | Cursor pagination over a covering index; one extra set-query for tags; single-transaction writes; 20 MB cap keeps proxying allocation-bounded; no offsets, no N+1 | ✅ Pass |

**Initial gate**: PASS — no deviations; Complexity Tracking intentionally empty.

**Post-design re-check (after Phase 1)**: PASS. Three calls worth surfacing: (1) two new backend workspace dependencies (`aws-sdk-s3`/`aws-config`, `ammonia`) are stack-conformant — the constitution names S3-compatible storage as the platform storage tier, and server-side sanitization is a zero-trust requirement, with the SDK quarantined behind `shared/storage`; (2) category hard-delete and tag-as-value rows deviate from habit, not from the constitution — both are justified in R6 against actual requirements; (3) proxied (not presigned) transfer is a deliberate v1 tradeoff at the 20 MB cap, and the `ObjectStorage` trait keeps presigned upload as a non-breaking future upgrade (R3/R4).

## Project Structure

### Documentation (this feature)

```text
specs/019-knowledge-base/
├── plan.md                  # This file
├── spec.md                  # Feature spec (clarified 2026-07-17)
├── research.md              # Phase 0 — R1–R12 decisions
├── data-model.md            # Phase 1 — four tables, migration 0046, state machine
├── quickstart.md            # Phase 1 — gates + manual validation scenarios
├── contracts/
│   └── rest-api.md          # Phase 1 — 11 endpoints, DTOs, error vocabulary, test obligations
├── checklists/
│   └── requirements.md      # Spec quality checklist (16/16)
└── tasks.md                 # Phase 2 (/speckit-tasks — not created here)
```

### Source Code (repository root)

```text
backend/
├── Cargo.toml                                  # + aws-sdk-s3, aws-config, ammonia; axum "multipart" feature
├── migrations/
│   └── 0046_knowledge_base.sql                 # four tables per data-model.md
└── crates/
    ├── shared/
    │   ├── storage/                            # NEW crate: ObjectStorage trait, S3Storage, InMemoryStorage
    │   │   ├── Cargo.toml
    │   │   └── src/lib.rs
    │   ├── config/src/lib.rs                   # + grouped `s3: Option<S3Config>` (Debug-redacted secret)
    │   └── db/tests/schema.rs                  # + 0046 assertions
    ├── modules/knowledge/                      # ACTIVATED placeholder crate
    │   ├── Cargo.toml                          # + axum, sqlx, serde, utoipa, kernel, tenancy, authz, identity, storage, ammonia…
    │   └── src/
    │       ├── lib.rs                          # module doc (purpose/interfaces/extension points) + re-exports
    │       ├── store.rs                        # tenant-scoped queries, transitions, tag set-replace, audit in-tx
    │       ├── validate.rs                     # title/body/tag/category rules, transition legality, sanitization
    │       ├── upload.rs                       # multipart parsing, type/size allowlist, object-before-row flow
    │       └── routes.rs                       # 11 handlers + DTOs (utoipa)
    └── server/
        ├── src/router.rs                       # wire /tenant/knowledge/* (require_permission); thread
        │                                       #   Option<Arc<dyn ObjectStorage>> + Extension + app_with_storage
        │                                       #   constructors (email_sender precedent); upload body limit
        └── tests/
            ├── knowledge_base.rs               # NEW integration suite (DB-gated; InMemoryStorage)
            ├── openapi_contract.rs             # + new paths/DTOs
            ├── openapi_coverage.rs             # + new routes
            └── *.rs (17 files)                 # mechanical `s3: None,` in each AppConfig literal
                                                #   (AppState literals untouched by design — R2)

frontend/apps/dashboard/src/app/
├── core/
│   ├── api/knowledge.models.ts                 # NEW: DTOs mirroring contracts/rest-api.md
│   └── router/app-paths.ts, page-title.ts      # + knowledge-base child paths/titles (new, :id, :id/edit)
├── features/tenant/
│   ├── tenant.routes.ts                        # knowledge-base → child routes (list/new/detail/edit)
│   └── knowledge-base/
│       ├── knowledge-api.service.ts(+spec)     # NEW: typed HTTP layer incl. multipart upload
│       ├── knowledge.store.ts(+spec)           # NEW: SignalStore (list+filters+detail+mutations)
│       ├── knowledge-base.component.ts(+spec)  # REBUILT: real list (filters, pagination, gated actions)
│       ├── article-detail.component.ts(+spec)  # NEW: detail incl. document metadata / file-unavailable state
│       ├── article-editor.component.ts(+spec)  # NEW: create/edit page (title, type, category, tags, body)
│       ├── rich-text-editor.component.ts(+spec)# NEW: wrapped @taiga-ui/editor (headings/lists/links/emphasis)
│       ├── upload-document.component.ts(+spec) # NEW: dialog (file pick, status choice, client-side pre-validation)
│       └── category-manager.component.ts(+spec)# NEW: dialog (CRUD, delete → uncategorized notice)
└── package.json                                # + @taiga-ui/editor
```

**Structure Decision**: Web application split along existing lines. Backend follows the module-crate anatomy 017/018 established (store/validate/routes separation, server-side wiring only in `router.rs`), with the two genuinely shared concerns (object storage, config) in `shared/`. Frontend rebuilds the existing `features/tenant/knowledge-base/` area in place — same route entry, now with child routes — reusing the shared component library per Constitution IX.

## Complexity Tracking

*No constitutional violations — table intentionally empty. Judgment calls (new deps, category hard-delete, tag-as-value, proxied transfer) are justified in research.md R2/R3/R6/R10 and surfaced in the post-design gate above.*
