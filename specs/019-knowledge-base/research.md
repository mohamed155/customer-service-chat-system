# Research: Knowledge Base

**Feature**: 019-knowledge-base | **Date**: 2026-07-17

Decisions R1–R12 resolve every open technical choice for the plan. No NEEDS CLARIFICATION remain.

## R1 — Module ownership: activate `modules/knowledge`

**Decision**: Implement the entire feature in the existing M0 placeholder crate `backend/crates/modules/knowledge` (currently one doc-comment line). It owns the tables, validation, store, and routes; `server` wires routes and layers dependencies.

**Rationale**: Unlike 018 (prompts stayed in `modules/ai` because prompt content was agent-config-owned), the knowledge base is its own bounded domain with its own pre-existing permission codes (`knowledge_base.view/manage` already in `authz::Permission` and the role matrix). Activating the placeholder is exactly what the M0 skeleton anticipated. No other module needs to call into `knowledge` in this feature (AI ingestion/RAG is explicitly out of scope), so the crate starts with zero inbound edges — the cleanest possible Constitution I posture.

**Alternatives considered**: `modules/ai` (rejected: knowledge is not agent configuration; would bloat the crate 018 just carefully scoped); a new crate name (rejected: placeholder already exists with the right name).

## R2 — Object storage client: `aws-sdk-s3` behind a `shared/storage` trait

**Decision**: New shared crate `backend/crates/shared/storage` exposing a narrow `ObjectStorage` trait (`put`, `get`, `delete`) with two implementations: `S3Storage` (new workspace deps `aws-sdk-s3` + `aws-config`, custom `endpoint_url` + `force_path_style(true)` for MinIO) and `InMemoryStorage` (tests). Modules depend only on the trait; the SDK types never leak.

**Config shape** — `AppConfig` gains **one grouped field**, `pub s3: Option<S3Config>` (fields: `endpoint`, `region`, `bucket`, `access_key_id`, `secret_access_key`, `force_path_style`), not six flat `Option`s. Grouping makes the config all-or-nothing by construction (six independent Options admit half-configured nonsense that would fail at first upload instead of at boot), and costs each existing `AppConfig` literal exactly one added line. `S3Config` carries its own `Debug` impl redacting `secret_access_key`, mirroring `AppConfig`'s existing redaction of `smtp_url`/`ai_key_encryption_key` (`shared/config/src/lib.rs:110-137`).

**Wiring** — storage is threaded as `Option<Arc<dyn ObjectStorage>>` through `api_routes` / `build_app` and layered as `Extension`, following the **`email_sender` precedent exactly** (`server/src/router.rs:690`, `:801`, `:853`), with new public constructors `app_with_storage` / `app_with_test_routes_and_storage`. When the param is `None`, the router falls back on `AppConfig::s3` → `S3Storage`, and failing that to `InMemoryStorage` with a `tracing::warn!` — precisely the `SmtpEmailSender` → `LogEmailSender` degradation at `router.rs:703-715`.

**Explicitly rejected: adding `storage` to `AppState`.** `AppState` is constructed with full field literals in **17 server test files**; a new field breaks every one of them for zero benefit, whereas the email_sender param path leaves them untouched and gives tests a first-class seam to inject `InMemoryStorage`. (`AppConfig`'s one grouped field still touches those 17 literals with a mechanical `s3: None,` — unavoidable if config stays centralized in `AppConfig`, which convention and central validation/redaction both require. Task T00X does that sweep once.)

**Rationale**: The constitution mandates S3-compatible object storage and MinIO is already in `infra/docker-compose.yml`. `aws-sdk-s3` is the maintained, officially supported client, works against MinIO with path-style addressing, and gives us presigning later if we outgrow proxied transfers. The trait boundary keeps handler unit tests storage-free, keeps Constitution I extraction cheap, and lets integration tests swap MinIO in via env.

**Alternatives considered**: `rust-s3` (rejected: smaller maintenance surface/community, historically spotty edge-case behavior); `object_store` (rejected: excellent crate but presigning for S3-compatible endpoints is awkward and its generic abstraction buys nothing over our own 3-method trait); direct SDK use inside `modules/knowledge` (rejected: leaks vendor types across a module boundary and makes future reuse — attachments, exports — a refactor).

## R3 — Upload transport: multipart proxy through the API

**Decision**: `POST /tenant/knowledge/documents` accepts `multipart/form-data` (axum workspace features gain `"multipart"`), with a route-scoped `DefaultBodyLimit` of 25 MB (20 MB file cap + form-field envelope). The handler buffers the file part up to the cap, validates (R5), then executes: (1) `put` object under key `{tenant_id}/knowledge/{item_id}` → (2) insert `knowledge_items` + `knowledge_documents` (+ tags) + audit row in one transaction → (3) on transaction failure, best-effort `delete` of the just-written object. Object-before-row ordering means an interrupted request can never produce a metadata row without a file (FR-016); at worst it leaves an unreferenced object that the failure path deletes.

**Rationale**: A single server-side enforcement point for authn/tenancy/type/size/audit (Constitution II/III) with zero client-side S3 coupling; at a 20 MB cap, buffering and proxying is well within normal request handling and avoids MinIO CORS + presign-policy complexity entirely in v1.

**Residual risk**: the compensating delete covers validation failure, storage failure, and transaction failure — but not a process crash between `put` and commit. Accepted for v1: the window is milliseconds, the orphan is unreferenced and invisible to users, and the cost is storage only. Closing it needs a sweeper, whose operational surface (scheduling, prefix listing at scale, a grace period to avoid racing in-flight uploads) is not worth it at this stage. The deterministic key scheme (`{tenant_id}/knowledge/{item_id}`) makes a later sweeper a straightforward set-difference against `knowledge_documents.storage_key`, and would extend the R2 trait with a `list(prefix)` method. FR-016 and the spec's Assumptions record this scope boundary explicitly.

**Alternatives considered**: Presigned PUT URLs (rejected for v1: moves validation client-side, needs bucket CORS + object-verification callback to avoid orphans; the right upgrade path if limits grow, and R2's trait doesn't preclude it); streaming multipart directly to S3 multipart-upload (rejected: complexity only pays above ~100 MB).

## R4 — Download: proxied stream, not presigned GET

**Decision**: `GET /tenant/knowledge/items/{id}/file` (view permission) loads the document row tenant-scoped, fetches bytes via `ObjectStorage::get`, and responds with the stored `content_type`, `Content-Disposition: attachment; filename="…"` (sanitized), and no caching of cross-tenant significance. If the object is missing out-of-band, the endpoint returns `not_found` while the item detail endpoint still serves metadata — the frontend renders the spec's "file unavailable" state.

**Rationale**: Tenant isolation stays enforced by the same middleware/permission stack as every other byte the API serves (Constitution II); the MinIO endpoint never needs public exposure; 20 MB proxied downloads are trivial.

**Alternatives considered**: Presigned GET redirect (rejected: bearer-style URLs leak-prone in logs/history, requires public object endpoint, breaks the "cross-tenant answers `not_found`" uniformity).

## R5 — File validation rules

**Decision**: Allowlist enforced on both the original filename extension and the declared part content type: `.pdf` (`application/pdf`), `.docx` (`application/vnd.openxmlformats-officedocument.wordprocessingml.document`), `.txt` (`text/plain`), `.md` (`text/markdown`, `text/x-markdown`, or `text/plain`). Max size 20 MB (constant `MAX_DOCUMENT_BYTES`), enforced while reading the part (abort past the cap, don't buffer unbounded). Violations → `validation_failed` whose message lists the allowed types and limit (US3 scenario 2). Content sniffing (magic bytes) is out of scope v1 and noted as a hardening follow-up.

**Rationale**: Matches the spec's assumption verbatim; extension+MIME double check is cheap and blocks accidental wrong files, which is the v1 threat model (uploaders are trusted tenant staff — Owner/Admin/Manager only).

## R6 — Schema shape (migration `0046_knowledge_base.sql`)

**Decision**: Four tables owned by `modules/knowledge` — `knowledge_categories`, `knowledge_items`, `knowledge_documents` (1:1 with document-type items), `knowledge_item_tags` (tag-as-value join). Details in [data-model.md](./data-model.md). Two deliberate convention calls:

1. **Categories hard-delete with `ON DELETE SET NULL`** instead of the 005 soft-delete convention: FR-008 defines delete semantics as "items become uncategorized" — a soft-deleted category would still need the same un-assignment, so the tombstone buys nothing; categories are pure labels with no audit or recovery requirement. Items themselves have **no delete at all** (archived is the terminal state per spec assumption), which is stricter than soft delete.
2. **Tags are value rows (`item_id, tag`), not a `knowledge_tags` entity**: tags are free-form labels with no attributes and no lifecycle of their own (spec Key Entities); a separate entity table would force orphan-cleanup logic and an extra join on every list query for zero requirements gained. The `(tenant_id, tag)` index serves the tag filter directly. Recorded here because it trades a little normalization for materially less machinery — not carried to Complexity Tracking since Constitution VIII allows justified deviations and this is the justification.

**Alternatives considered**: document columns inlined on `knowledge_items` (rejected: nulls for two of three item types; 1:1 table keeps the CHECK story clean); `text[]` tags column (rejected: un-indexable per-tenant tag filtering without GIN gymnastics, and harder CHECK constraints).

## R7 — Lifecycle enforcement

**Decision**: Transitions validated in the store layer against the exact FR-003 edges: `draft→published`, `published→archived`, `archived→draft`. Publishing an article/FAQ with an empty/whitespace body → `validation_failed` (FR-004); documents publish without body. A transition to the item's current status is a structured no-op (`changed: false`, HTTP 200) making the endpoint replay-safe (Constitution V idempotency). Illegal edges (e.g., `draft→archived`) → `validation_failed` naming the allowed transitions. Published-item edits save in place with no status side effects (clarification #2).

## R8 — Audit vocabulary

**Decision**: Four actions via the existing `tenancy::audit::record_in_tx`, written in the same transaction as the state change: `knowledge_item.created` (covers both authored creation and document upload; details carry `itemType` and `source`), `knowledge_item.published`, `knowledge_item.archived`, `knowledge_item.restored`. Details include item id, type, and title — never body content or file bytes (015/018 invariant: user content stays out of logs and audit details).

**Rationale**: FR-013 names exactly create/publish/archive/restore; edits are not audited in v1 (matches spec — content versioning was explicitly scoped out; adding `updated` audit noise without version storage would imply recoverability we don't have).

## R9 — API surface & pagination

**Decision**: Eleven tenant endpoints under `/tenant/knowledge/…` (contract in [contracts/rest-api.md](./contracts/rest-api.md)): items list/create/detail/patch/status, document upload, file download, categories list/create/rename/delete. List pagination is cursor-based (`limit` + `before` over `(updated_at, id)` descending) consistent with 018's history contract; filters `type`, `status`, `categoryId`, `tag`, plus `q` (title contains, case-insensitive) because the existing Helix list UI ships a search box. Item tags for a page load via one `WHERE item_id = ANY($1)` query — no N+1 (Constitution X). GETs require `knowledge_base.view`, writes `knowledge_base.manage`, using the established `require_permission`/`merge_with_permissions` router pattern; cross-tenant access answers `not_found`.

**Zero RBAC changes needed**: the matrix already grants manage to Owner/Admin/Manager and view to Agent/Viewer — clarification #1 is satisfied by existing code, verified by `matrix.rs` and to be pinned by RBAC cases in the new integration suite.

## R10 — Rich text storage & sanitization

**Decision**: Article/FAQ body is stored as HTML (what the WYSIWYG editor produces), hard-capped at 100,000 characters (DB CHECK + validation error). Server sanitizes on every write with the `ammonia` crate (new workspace dep) using its conservative default allowlist (covers headings, lists, links, emphasis — exactly the clarified formatting set; strips scripts/handlers/iframes). Angular's `[innerHTML]` built-in sanitization remains as the second, defense-in-depth layer on render.

**Rationale**: Stored XSS is the one real security risk of a rich-text feature; zero-trust (Constitution III) means the server cannot rely on client-side sanitization alone. `ammonia` is the standard, maintained Rust HTML sanitizer.

**Alternatives considered**: store Markdown + client render (rejected: clarification #5 chose WYSIWYG rich text; Markdown round-tripping through a WYSIWYG loses fidelity); trust Angular sanitization only (rejected: any non-Angular consumer — future widget, email — would inherit unsanitized HTML).

## R11 — WYSIWYG editor component

**Decision**: Add `@taiga-ui/editor` (^5, matching the installed Taiga UI 5.13 line) and wrap it in a feature-owned `article-editor` component so no raw Taiga usage leaks into pages (design-system rule). Toolbar restricted to the clarified formatting set: headings, bold/italic, bullet/ordered lists, links.

**Rationale**: The workspace is Taiga-only by rule (frontend CLAUDE.md); Taiga's editor is the first-party rich-text option and produces HTML compatible with R10's sanitizer.

**Alternatives considered**: ngx-quill (rejected: second design system's worth of styling to reconcile); hand-rolled `contenteditable` (rejected: enormous edge-case surface for zero differentiation).

## R12 — Frontend architecture

**Decision**: The fixture-backed `knowledge-base.component.ts` (003 Helix visuals) is replaced by a real feature area following the 017/018 anatomy: typed models in `core/api/knowledge.models.ts`, `knowledge-api.service.ts` on the shared `ApiResponse<T>` layer, one NgRx SignalStore for the feature, and child routes under the existing `knowledge-base` path — list (``), `new`, `:id` (detail), `:id/edit` — registered via `APP_PATHS`/`PAGE_TITLES` with the existing `knowledge_base.view` page permission. Manage-only affordances (New article, Upload, Publish/Archive, category management) are permission-gated in the UI the same way ai-agent gates its manage actions, with the backend as the real enforcement (Constitution II). Upload and category management are dialogs from the list page (using the shared `dialog-shell`), not separate routes. `RoutedPageStore`/fixtures usage for this page is removed.

**Rationale**: Mirrors the proven 017/018 feature anatomy; keeps the route surface small; reuses shared components (toolbar, search-input, status-badge, empty-state, dialog-shell) per Constitution IX.
