---

description: "Task list for feature 019-knowledge-base"

---

# Tasks: Knowledge Base

**Input**: Design documents from `/specs/019-knowledge-base/`
**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/rest-api.md](./contracts/rest-api.md), [quickstart.md](./quickstart.md)

**Audience note**: These tasks assume no prior context beyond what's written in each task. Every task names its exact file(s) and, where the codebase already has an equivalent pattern, points at the exact file/line to copy the style from. When a task says "mirror X", open X first and match its structure — do not invent a different structure.

**New dependencies** (unlike 018, this feature does add some — all justified in research.md): backend `aws-sdk-s3`, `aws-config`, `ammonia`, plus the `axum` `"multipart"` feature; frontend `@taiga-ui/editor`.

**Tests**: Backend integration/unit tests and frontend spec files ARE requested (see plan.md Testing section) — included inline with each story's implementation tasks, not as a separate optional block.

**Organization**: Tasks are grouped by user story (from spec.md) so each story is independently implementable and testable.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel with other [P] tasks in the same phase (different files, no ordering dependency)
- **[Story]**: US1–US4, matching spec.md's priorities (US1 = P1, US2 = P2, US3 = P3, US4 = P4)
- Tasks with no [Story] label are Setup, Foundational, or Polish

---

## Phase 1: Setup

- [X] T001 Confirm a clean baseline before touching anything: `cd backend && cargo check --workspace` and `cd frontend && pnpm ng build dashboard`. Both must succeed with the *current* (pre-019) code. If either fails, stop and fix the pre-existing issue first — do not build 019 on top of a broken baseline. Note: the working tree currently carries uncommitted 018 work; that is expected.

- [X] T002 Add the backend workspace dependencies in `backend/Cargo.toml` under `[workspace.dependencies]` (the list starting at line 12, alphabetical-ish but append is fine — match surrounding style): `aws-config = { version = "1", default-features = false, features = ["behavior-version-latest", "rustls"] }`, `aws-sdk-s3 = { version = "1", default-features = false, features = ["rustls"] }`, `ammonia = "4"`. Also change the existing `axum = "0.8"` line to `axum = { version = "0.8", features = ["multipart"] }` (upload transport, research R3). Then run `cargo check --workspace` — it must still compile with no code changes yet.

- [X] T003 [P] Add the frontend dependency: in `frontend/package.json`, add `"@taiga-ui/editor": "^5.13.0"` to `dependencies` next to the other `@taiga-ui/*` entries (lines 27-32 — match the installed 5.13 line exactly; a major-version mismatch with `@taiga-ui/core` will fail at runtime). Run `pnpm install` from `frontend/`, then `pnpm ng build dashboard` to confirm the workspace still builds.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Migration + object-storage crate + config + module skeleton + router seam. After this phase there are still **no user-facing endpoints** — those are built story-by-story from Phase 3.

**⚠️ CRITICAL**: Nothing in Phase 3+ compiles until T004–T012 are done.

- [X] T004 Create `backend/migrations/0046_knowledge_base.sql`. Follow the exact conventions of `backend/migrations/0045_agent_prompts.sql` (header comment naming the spec, `UUID PRIMARY KEY DEFAULT gen_random_uuid()`, `tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT`, a `set_<table>_updated_at()` function + `BEFORE UPDATE` trigger per mutable table — copy the pattern at `0045_agent_prompts.sql:23-34`). Create, in this order, exactly the four tables specified in [data-model.md](./data-model.md) — read it first, it is authoritative for every column, CHECK, and index:
  1. `knowledge_categories` (+ `UNIQUE INDEX knowledge_categories_tenant_name_uq ON knowledge_categories (tenant_id, lower(name))` — case-insensitive per tenant; + updated_at trigger). Note: **no `deleted_at`** — categories hard-delete (research R6).
  2. `knowledge_items` (+ CHECKs on `item_type`, `status`, `char_length(title) BETWEEN 1 AND 200`, `char_length(body) <= 100000`, and `item_type <> 'document' OR body IS NULL`; `category_id UUID NULL REFERENCES knowledge_categories(id) ON DELETE SET NULL` — this FK action *is* the FR-008 implementation, do not replace it with application logic; `created_by_user_id UUID NULL REFERENCES users(id) ON DELETE SET NULL`; `created_by_display TEXT NOT NULL`; + updated_at trigger). **No `deleted_at`** — `archived` is the terminal state, there is no delete.
  3. `knowledge_documents` (PK is `item_id UUID REFERENCES knowledge_items(id) ON DELETE CASCADE`; `storage_key TEXT NOT NULL UNIQUE`; `size_bytes BIGINT NOT NULL CHECK (size_bytes > 0 AND size_bytes <= 20971520)`; no updated_at/trigger — the row is write-once).
  4. `knowledge_item_tags` (`PRIMARY KEY (item_id, tag)`, `item_id … ON DELETE CASCADE`, `CHECK (char_length(tag) BETWEEN 1 AND 40)`; no timestamps).
  Indexes: `knowledge_items_tenant_updated_idx ON knowledge_items (tenant_id, updated_at DESC, id DESC)` (list cursor path), `knowledge_items_tenant_status_idx ON knowledge_items (tenant_id, status)`, `knowledge_items_category_idx ON knowledge_items (category_id)`, `knowledge_item_tags_tenant_tag_idx ON knowledge_item_tags (tenant_id, tag)`.

- [X] T005 [P] In `backend/crates/shared/db/tests/schema.rs`, add a `migration_0046_*` test block. Mirror the style of the existing `migration_0045_*` tests (same file — find them by searching `0045`; `#[tokio::test]` per assertion, `db::run_migrations` + raw-SQL-insert-expect-error pattern). Cover: (a) `knowledge_items` CHECK rejects an `item_type` outside `article|faq|document`, a `status` outside `draft|published|archived`, an empty title, a >200-char title, a >100000-char body, and a `document`-type row with a non-NULL body; (b) `knowledge_categories` unique index rejects a second row whose name differs only in case for the same tenant, but *allows* the same name in a different tenant; (c) deleting a category sets `category_id` to NULL on its items and leaves the item rows present (FR-008 — assert both); (d) `knowledge_documents` CHECK rejects `size_bytes = 0` and `size_bytes = 20971521`, and the UNIQUE on `storage_key` rejects a duplicate; (e) deleting a `knowledge_items` row cascades away its `knowledge_documents` and `knowledge_item_tags` rows; (f) `knowledge_item_tags` PK rejects a duplicate `(item_id, tag)`. Depends on T004.

- [X] T006 Create the new shared crate `backend/crates/shared/storage` (it is picked up automatically by the `crates/shared/*` glob in `backend/Cargo.toml:4` — no members edit needed). Two files:
  - `Cargo.toml`: package name `storage`, `version.workspace = true`, `edition.workspace = true`; dependencies `async-trait.workspace = true`, `aws-config.workspace = true`, `aws-sdk-s3.workspace = true`, `tokio.workspace = true`, `tracing.workspace = true`. Mirror the shape of `backend/crates/modules/ai/Cargo.toml:1-4`.
  - `src/lib.rs` with:
    - `#[derive(Debug)] pub enum StorageError { NotFound, Other(String) }` + `impl std::fmt::Display` + `impl std::error::Error`.
    - `#[async_trait::async_trait] pub trait ObjectStorage: Send + Sync + 'static { async fn put(&self, key: &str, content_type: &str, bytes: Vec<u8>) -> Result<(), StorageError>; async fn get(&self, key: &str) -> Result<(Vec<u8>, String), StorageError>; async fn delete(&self, key: &str) -> Result<(), StorageError>; }` (`get` returns bytes + content type).
    - `pub struct S3Storage` built from an `aws_sdk_s3::Client` + bucket name, constructed via `pub async fn new(cfg: &config::S3Config) -> Result<Self, StorageError>` using `aws_config::defaults(BehaviorVersion::latest())` with `.endpoint_url(&cfg.endpoint)`, `.region(Region::new(cfg.region.clone()))`, static credentials from `cfg.access_key_id`/`cfg.secret_access_key`, and `force_path_style(cfg.force_path_style)` on the S3 config builder (**required for MinIO** — without it the SDK uses virtual-host addressing and every call 404s). Map a `NoSuchKey`/404 GET to `StorageError::NotFound`, everything else to `Other`. **The `aws_sdk_s3` types must not appear in the public API** — only the trait, `S3Storage`, `InMemoryStorage`, `StorageError` are `pub` (research R2: SDK never crosses the module boundary).
    - `#[derive(Default)] pub struct InMemoryStorage(std::sync::Mutex<std::collections::HashMap<String, (Vec<u8>, String)>>)` implementing the same trait, for tests/fallback; `get` on a missing key → `StorageError::NotFound`.
    - Unit tests in the same file: `InMemoryStorage` put→get round-trips bytes+content-type, `get` of an unknown key is `NotFound`, `delete` then `get` is `NotFound`, and `delete` of an unknown key is `Ok` (idempotent — the upload compensation path in T024 depends on this).
  This crate depends on `config` for `S3Config` (add `config = { path = "../config" }`), which T007 creates — write T007 first if you prefer a compiling checkpoint.

- [X] T007 Edit `backend/crates/shared/config/src/lib.rs` to add the S3 config, following the **grouped-field** decision in research.md R2 (do **not** add six flat fields):
  - Add `#[derive(Clone, PartialEq, Eq)] pub struct S3Config { pub endpoint: String, pub region: String, pub bucket: String, pub access_key_id: String, pub secret_access_key: String, pub force_path_style: bool }` with a **hand-written `Debug` impl** that renders `secret_access_key` as `"[REDACTED]"` and `access_key_id` as `"[REDACTED]"`, mirroring the redaction style of `AppConfig`'s Debug impl at lines 110-137.
  - Add one field to `AppConfig` (after `ai_gemini_base_url`, line 97): `pub s3: Option<S3Config>,`.
  - Add `.field("s3", &self.s3)` to the `AppConfig` Debug impl (line 136, before `.finish()`) — safe because `S3Config`'s own Debug redacts.
  - In `from_env` (after the `ai_*` parsing at lines 247-249), parse all-or-nothing: read `APP_S3_ENDPOINT`, `APP_S3_REGION`, `APP_S3_BUCKET`, `APP_S3_ACCESS_KEY_ID`, `APP_S3_SECRET_ACCESS_KEY` via `env::var(...).ok()`; if **all five** are `Some`, build `Some(S3Config { … , force_path_style: env::var("APP_S3_FORCE_PATH_STYLE").map(|v| v == "true").unwrap_or(true) })`; if **none** are set, `None`; if **some but not all** are set, return `Err(ConfigError("APP_S3_* configuration is incomplete: set all of APP_S3_ENDPOINT, APP_S3_REGION, APP_S3_BUCKET, APP_S3_ACCESS_KEY_ID, APP_S3_SECRET_ACCESS_KEY, or none".into()))` — half-configured storage must fail at boot, not at first upload. Add unit tests in the existing `mod tests` (near the `ai_openai_base_url` assertion at line 434) covering: all-unset → `None`, all-set → populated `Some`, partial → `Err`, and that `format!("{:?}", config)` never contains the secret value.
  - Note `APP_S3_*` naming matches the existing `APP_AI_*` convention (line 225/247), **not** the bare `S3_*` names shown in quickstart.md — update `specs/019-knowledge-base/quickstart.md`'s env block to the `APP_S3_*` names in the same commit so the guide stays runnable.

- [X] T008 Mechanical compile-fix sweep for T007's new `AppConfig` field. Every file that constructs an `AppConfig` **struct literal** must add one line `s3: None,`. The 17 files are: `backend/crates/server/tests/{ai,ai_agent,ai_agent_prompt,auth,conversations,cors,customers,errors,escalations,health,live_deps,openapi_contract,platform_tenants,rbac,team_members,tenancy,tracing}.rs` plus the literal inside `backend/crates/server/src/router.rs`'s `mod tests`. Find them all with `grep -rln 'ai_gemini_base_url' backend/crates/server backend/crates/shared/config`; in each, add `s3: None,` immediately after the `ai_gemini_base_url` line (see `ai_agent_prompt.rs:38-40` for the exact shape). **Do not touch `AppState` literals** — by design, storage is not an `AppState` field (research R2). Verify with `cargo check --workspace --all-targets`. Depends on T007.

- [X] T009 Wire the storage seam into `backend/crates/server/src/router.rs`, copying the **`email_sender` pattern exactly** — read `router.rs:686-691` (`api_routes` signature), `:703-715` (fallback construction), `:798-804` (`build_app` signature), `:741-745` (Extension layering), and `:853-871` (public constructors) before editing:
  - Add `storage: Option<Arc<dyn ObjectStorage>>` as a parameter to `api_routes` (line 686-690) and `build_app` (line 798-802), threaded through the `build_app` → `api_routes` call at line 804.
  - In `api_routes`, next to the `email_sender` fallback at line 703, resolve: `let storage: Arc<dyn ObjectStorage> = match storage { Some(s) => s, None => match &state.config.s3 { Some(cfg) => match S3Storage::new(cfg).await { … } , None => { tracing::warn!("no S3 configuration; falling back to in-memory object storage"); Arc::new(InMemoryStorage::default()) } } };`. **`api_routes` is currently sync** — since `S3Storage::new` is async, either make `S3Storage::new` sync (preferred: `aws_sdk_s3::Client::from_conf` needs no `.await` when credentials are static and no profile/IMDS lookup happens — build the `Config` directly rather than via `aws_config::defaults().load().await`) or make the `api_routes`/`build_app`/`app*` chain async. Prefer the sync client build; it keeps all four public constructors sync and avoids touching every test call site.
  - Add `.layer(Extension(storage))` in the layer stack at line 741-745 (next to `.layer(Extension(state.ai.clone()))`).
  - Add public constructors mirroring lines 853-871: `pub fn app_with_storage(state: AppState, storage: Arc<dyn ObjectStorage>) -> Router { build_app(state, false, None, Some(storage)) }` and `pub fn app_with_test_routes_and_storage(state: AppState, storage: Arc<dyn ObjectStorage>) -> Router { build_app(state, true, None, Some(storage)) }`; update the existing `app`, `app_with_email_sender`, `app_with_test_routes`, `app_with_test_routes_and_email_sender` to pass `None` for the new param. Add `storage = { path = "../shared/storage" }` to `backend/crates/server/Cargo.toml`. Depends on T006, T007.

- [X] T010 Activate the `modules/knowledge` crate. Edit `backend/crates/modules/knowledge/Cargo.toml` — currently a bare 4-line stub with no `[dependencies]` section. Add dependencies mirroring `backend/crates/modules/ai/Cargo.toml:6-31`: `ammonia.workspace = true`, `async-trait.workspace = true`, `authz = { path = "../authz" }`, `axum.workspace = true`, `chrono = { workspace = true, features = ["serde"] }`, `identity = { path = "../identity" }`, `kernel = { path = "../../shared/kernel" }`, `serde.workspace = true`, `serde_json.workspace = true`, `sqlx = { workspace = true, features = ["postgres", "uuid", "chrono"] }`, `storage = { path = "../../shared/storage" }`, `tenancy = { path = "../tenancy" }`, `tracing.workspace = true`, `utoipa.workspace = true`, `uuid.workspace = true`. Replace `src/lib.rs`'s single placeholder doc-comment with a module doc block documenting **Purpose, Responsibilities, Public Interfaces, Dependencies, Data Model, Extension Points** (mandated by the constitution's Documentation section — copy the depth/shape of `backend/crates/modules/ai/src/lib.rs`'s header) plus `pub mod routes; pub mod store; pub mod upload; pub mod validate;`. Add `knowledge = { path = "../modules/knowledge" }` to `backend/crates/server/Cargo.toml`. Depends on T006.

- [X] T011 Create `backend/crates/modules/knowledge/src/validate.rs` (NEW) — pure, no DB, no I/O. Contents:
  - `pub const MAX_TITLE_LENGTH: usize = 200; pub const MAX_BODY_LENGTH: usize = 100_000; pub const MAX_TAG_LENGTH: usize = 40; pub const MAX_TAGS_PER_ITEM: usize = 20; pub const MAX_DOCUMENT_BYTES: u64 = 20 * 1024 * 1024;`
  - `pub struct ValidationIssue { pub field: String, pub code: String, pub message: String }` — mirror the field names of the existing `ValidationIssue` in `backend/crates/modules/ai/src/agent_config.rs` so error envelopes look identical across features (do not import it — `ai` is not a dependency of `knowledge`; a parallel local type keeps the module boundary clean per Constitution I).
  - `pub fn validate_title(title: &str) -> Option<ValidationIssue>` — `required` when trimmed-empty, `too_long` past `MAX_TITLE_LENGTH` (US1 scenario 4).
  - `pub fn sanitize_body(body: &str) -> String` — `ammonia::clean(body)` with the crate default allowlist (research R10: covers headings/lists/links/emphasis, strips scripts/handlers/iframes). Every write path calls this; never store raw input.
  - `pub fn validate_body(body: &str) -> Option<ValidationIssue>` — `too_long` past `MAX_BODY_LENGTH` (measure **after** sanitization).
  - `pub fn normalize_tags(raw: &[String]) -> Result<Vec<String>, ValidationIssue>` — trim, lowercase, drop empties, dedupe preserving first-seen order; `too_many` past `MAX_TAGS_PER_ITEM`; `too_long` if any tag exceeds `MAX_TAG_LENGTH`.
  - `pub enum ItemType { Article, Faq, Document }` and `pub enum ItemStatus { Draft, Published, Archived }` with `as_str`/`FromStr` matching the DB CHECK strings exactly.
  - `pub enum TransitionError { Illegal { from: ItemStatus, to: ItemStatus }, BodyRequired }` and `pub fn check_transition(from: ItemStatus, to: ItemStatus, item_type: ItemType, body: Option<&str>) -> Result<bool, TransitionError>` — returns `Ok(true)` for a real transition, **`Ok(false)` for a same-status no-op** (the replay-safe `changed: false` contract), `Err(Illegal)` for anything outside the three FR-003 edges (`draft→published`, `published→archived`, `archived→draft`), and `Err(BodyRequired)` when publishing an `Article`/`Faq` whose body is `None`/whitespace (FR-004; documents are exempt).
  - `pub fn sanitize_filename(name: &str) -> String` — strip path separators and control chars, cap at 255, fall back to `"download"` when empty (used by the `Content-Disposition` header in T033; prevents header injection).
  - Unit tests in-file for **every** rule above, with an explicit table for `check_transition` covering all 9 `(from,to)` pairs × both item-type classes, and a sanitizer test asserting `<script>alert(1)</script>` and `<img onerror=…>` are stripped while `<h2>`/`<ul>`/`<a href>`/`<strong>` survive.

- [X] T012 Create `backend/crates/modules/knowledge/src/store.rs` (NEW) — all DB access, every query tenant-scoped. Mirror the shape and error style of `backend/crates/modules/ai/src/prompt_store.rs` (row structs at the top, `pub async fn` per operation, `sqlx::Result` or a local error enum, `_in_tx` suffix for transaction participants). Provide:
  - Row structs `KnowledgeItemRow`, `KnowledgeDocumentRow`, `KnowledgeCategoryRow` matching [data-model.md](./data-model.md) columns exactly.
  - `pub struct ItemFilters { pub item_type: Option<ItemType>, pub status: Option<ItemStatus>, pub category_id: Option<Uuid>, pub tag: Option<String>, pub q: Option<String> }` and `pub async fn list_items(pool, tenant_id, filters, limit, before: Option<(DateTime<Utc>, Uuid)>) -> sqlx::Result<(Vec<ItemRowWithJoins>, bool)>` — keyset pagination via `WHERE (updated_at, id) < ($cursor_ts, $cursor_id)` ordered `updated_at DESC, id DESC` riding `knowledge_items_tenant_updated_idx`; fetch `limit + 1` to compute `hasMore`. **Tags for the page load in ONE follow-up query** `WHERE item_id = ANY($1)` — never per-item (Constitution X forbids N+1).
  - `pub async fn get_item(pool, tenant_id, item_id)`, `create_item_in_tx`, `update_item_in_tx`, `set_status_in_tx`, `replace_tags_in_tx` (delete-then-insert the item's full tag set inside the caller's transaction), and category CRUD `list_categories` (with `itemCount` via one `LEFT JOIN … GROUP BY` — not per-category counts), `create_category`, `rename_category`, `delete_category`.
  - Every function takes `tenant_id` and includes `AND tenant_id = $n` in its `WHERE`; a miss returns `Ok(None)` so handlers can answer `not_found` uniformly for both missing and cross-tenant rows (Constitution II).
  - `pub async fn create_document_in_tx(...)` inserting the `knowledge_items` + `knowledge_documents` pair.
  - A duplicate category name must surface distinguishably: map the unique-violation to a `CategoryError::Duplicate` (check `sqlx::Error::Database(e) if e.is_unique_violation()`) so T038 can return `conflict`.
  Depends on T004, T010, T011.

**Checkpoint**: `cargo check --workspace --all-targets` passes; migration applies; no endpoints exist yet.

---

## Phase 3: User Story 1 — Author and edit knowledge articles (Priority: P1) 🎯 MVP

**Goal**: A tenant manager can create an article/FAQ, save it as a draft, edit it, see it in a real (non-fixture) list, and open its detail page. Tenant-scoped throughout.

**Independent Test**: Sign in as an Admin, create an article, edit it, confirm it appears in the list and detail view; confirm a user of another tenant sees none of it and direct links answer not-found. No publishing, documents, or categories/tags needed.

### Backend

- [X] T013 [US1] In `backend/crates/modules/knowledge/src/routes.rs` (NEW), define the DTOs from [contracts/rest-api.md](./contracts/rest-api.md) with `#[derive(Serialize/Deserialize, ToSchema)]` + `#[serde(rename_all = "camelCase")]` — mirror `backend/crates/modules/ai/src/prompt_routes.rs:19-80` exactly for derive/attribute style: `KnowledgeItemSummaryDto`, `KnowledgeItemDetailDto`, `CategoryRefDto`, `DocumentMetaDto`, `ItemListResponse { items, has_more, next_cursor }`, `CreateItemPayload`, `UpdateItemPayload` (all fields `Option`, `#[serde(default)]`), `SetStatusPayload`, `SetStatusResponse { id, status, changed, updated_at }`, `CategoryDto`, `CreateCategoryPayload`, `RenameCategoryPayload`, `ItemListQuery` (`#[derive(IntoParams)]`, fields `limit`, `before`, `type`→`item_type` via `#[serde(rename = "type")]`, `status`, `category_id`, `tag`, `q`).

- [X] T014 [US1] In `routes.rs`, implement `pub async fn list_items` (`GET /tenant/knowledge/items`) — `#[utoipa::path]` annotated (copy the annotation shape from `prompt_routes.rs`'s `list_prompt_versions`), extracting `Extension<TenantContext>` for `tenant_id` and `Principal` for the actor, calling `store::list_items`. Cursor encoding: base64 of `{updated_at_rfc3339}|{id}`, decoded defensively (a malformed cursor → `validation_failed`, never a panic). Clamp `limit` to 1..=50, default 20.

- [X] T015 [US1] In `routes.rs`, implement `pub async fn create_item` (`POST /tenant/knowledge/items`, 201) — validate title (T011), reject `itemType: "document"` with `validation_failed` (documents are created only by upload, T024), sanitize body, normalize tags, verify any `categoryId` belongs to the tenant (else `validation_failed`), then in **one transaction**: insert item (`status='draft'`, `source='authored'`, `created_by_user_id` + `created_by_display` from the `Principal`) → replace tags → `tenancy::audit::record_in_tx` with action `knowledge_item.created` and details `{ itemId, itemType, source }` — **never the body content** (015/018 invariant: user content stays out of audit details and logs). For the audit call signature and the actor-attribution shape (including the platform-actor case), copy `backend/crates/modules/ai/src/agent_audit.rs`.

- [X] T016 [US1] In `routes.rs`, implement `pub async fn get_item` (`GET /tenant/knowledge/items/{id}`) returning `KnowledgeItemDetailDto`, and `pub async fn update_item` (`PATCH /tenant/knowledge/items/{id}`) applying only the provided fields (FR-002). Rules: `body` on a document item → `validation_failed`; `itemType` may only move between `article` and `faq` (never to/from `document`); **status is never touched** (clarification #2 — a published item stays published and the edit is live immediately); tags replaced only when the field is present. No version guard — last-save-wins is the specified concurrency semantic (spec Edge Cases). Cross-tenant/missing → `not_found`.

- [X] T017 [US1] Wire the US1 routes in `backend/crates/server/src/router.rs`'s `tenant_routes` (the fn at line 314). Copy the co-registration idiom at lines 549-560 verbatim — `routes!(a, b).map(|_| { let get = routing::get(a).route_layer(require_permission(Permission::KnowledgeBaseView)); let post = routing::post(b).route_layer(require_permission(Permission::KnowledgeBaseManage)); get.merge(post) })` for the `/tenant/knowledge/items` pair, and the same shape for the `{id}` GET/PATCH pair. **Use the existing `Permission::KnowledgeBaseView` / `KnowledgeBaseManage` variants** (`backend/crates/modules/authz/src/permission.rs:21-24`) — do **not** add permission codes and do **not** edit `matrix.rs`: the matrix already grants manage to Owner/Admin/Manager and view to Agent/Viewer (`matrix.rs:12-13,28-29,42,48`), which is exactly clarification #1. T019 proves it.

- [X] T018 [US1] Create `backend/crates/server/tests/knowledge_base.rs` (NEW) — the integration suite. Copy the entire harness preamble from `backend/crates/server/tests/ai_agent_prompt.rs:1-90` (the `test_config()` literal, `plain_state()`, the `REQUIRE_DB_TESTS`/`DATABASE_URL` gate at lines 68-90, and the seeding helpers) — do not invent a new harness. Build the app under test with `router::app_with_storage(state, Arc::new(InMemoryStorage::default()))` (T009) so no MinIO is needed. US1 cases: create → 201 + draft status + list contains it; create with empty title → `validation_failed`, nothing persisted; create with `itemType: "document"` → `validation_failed`; PATCH title/body/type persists and detail reflects it; PATCH of a published item leaves `status: "published"` (clarification #2); GET/PATCH of another tenant's item id → `not_found` (not 403); list is tenant-scoped; cursor pagination returns each item exactly once across pages with `hasMore` correct; a malformed `before` cursor → `validation_failed`; audit row `knowledge_item.created` exists with the actor and **no body content in `details`**; a PATCH body containing `source`, `createdBy`, or `createdAt` leaves the item's attribution unchanged (FR-002 — attribution is immutable; unknown fields are ignored by serde, this pins that behavior).

- [X] T019 [US1] In `knowledge_base.rs`, add the RBAC block for the US1 routes, following 017/018's precedent of testing RBAC in the feature's own suite (`rbac.rs` stays untouched): for each of `GET /items`, `POST /items`, `GET /items/{id}`, `PATCH /items/{id}`, assert Owner/Admin/Manager succeed on writes; Agent and Viewer get `unauthorized` on writes but succeed on reads (clarification #1 / FR-014). Depends on T017, T018.

- [X] T020 [P] [US1] Register the US1 paths/DTOs in `backend/crates/server/tests/openapi_contract.rs` and `backend/crates/server/tests/openapi_coverage.rs` — find the existing prompt-route assertions (grep `prompt`) and add the parallel `/tenant/knowledge/*` entries in the same style. The coverage gate fails the build if a live route is undocumented, so this must land with T017.

### Frontend

- [X] T021 [P] [US1] Create `frontend/apps/dashboard/src/app/core/api/knowledge.models.ts` (NEW) — TypeScript mirrors of the T013 DTOs, exactly matching [contracts/rest-api.md](./contracts/rest-api.md)'s camelCase wire shape. Mirror the file style of the existing `core/api/ai-agent.models.ts`. Include `KnowledgeItemSummary`, `KnowledgeItemDetail`, `CategoryRef`, `DocumentMeta`, `ItemListResponse`, `KnowledgeItemType = 'article' | 'faq' | 'document'`, `KnowledgeItemStatus = 'draft' | 'published' | 'archived'`, `ItemFilters`.

- [X] T022 [US1] Create `frontend/apps/dashboard/src/app/features/tenant/knowledge-base/knowledge-api.service.ts` + `.spec.ts` (NEW) — typed HTTP over the shared `ApiResponse<T>` layer. Mirror `features/tenant/ai-agent/ai-agent-api.service.ts` for structure, and **RxJS-first per the constitution's Technology Stack section**: return `Observable<T>` and compose with operators; no `firstValueFrom`/`async-await`. US1 methods only: `listItems(filters, cursor?)`, `getItem(id)`, `createItem(payload)`, `updateItem(id, payload)`. Spec file mirrors `ai-agent-api.service.spec.ts` (HttpTestingController; assert URL, method, body, and typed unwrap).

- [X] T023 [US1] Create the store + pages and retire the fixture page, all under `frontend/apps/dashboard/src/app/` (paths below are relative to that root):
  - `knowledge.store.ts` + `.spec.ts` (NEW): one NgRx SignalStore for the feature (mirror `features/tenant/ai-agent/ai-agent.store.ts` and its spec). State: list items, filters, cursor/hasMore, loading/error, selected detail. Methods: `loadList`, `loadMore`, `setFilter`, `loadItem`, `createItem`, `updateItem`.
  - `rich-text-editor.component.ts` + `.spec.ts` (NEW): wraps `@taiga-ui/editor` (T003) with the toolbar restricted to headings, bold/italic, bullet/ordered lists, and links (clarification #5). **The Taiga import must not appear in any page component** — the frontend rule is that Taiga is wrapped in project components (`frontend/CLAUDE.md`, spec 003 section; Constitution IX).
  - `article-editor.component.ts` + `.spec.ts` (NEW): the create/edit page (title, type article|faq, body via the wrapper, category + tags fields wired in US4). Inline validation mirroring T011's rules; empty title blocks save (US1 scenario 4); content is never lost on a rejected save.
  - `article-detail.component.ts` + `.spec.ts` (NEW): detail page rendering title/status/metadata and body via `[innerHTML]` (Angular's sanitizer is the second layer — the server sanitizes first, T011/R10).
  - **Rewrite** `knowledge-base.component.ts` + `.spec.ts`: replace the fixture list (the whole `RoutedPageStore`/`PAGE_ROUTE` wiring at lines 27 and 185-215, and the `fixture.models` import at line 12) with the real store. Keep the existing Helix layout and shared components (`app-page-container`, `app-page-header`, `app-toolbar`, `app-search-input`, `app-empty-state`, `app-dashboard-card`, `app-status-badge`) — this is a data-source swap, not a redesign. Manage-only affordances ("New article") are permission-gated in the UI the way `ai-agent` gates its manage actions; the backend remains the real enforcement.
  - Routing: in `core/router/app-paths.ts` (line 26) add `knowledgeBaseNew: 'knowledge-base/new'`, `knowledgeBaseDetail: (id: string) => \`knowledge-base/${id}\``, `knowledgeBaseEdit: (id: string) => \`knowledge-base/${id}/edit\`` — mirroring the existing `conversationDetail`/`aiAgentPrompt` entries. In `features/tenant/tenant.routes.ts`, convert the flat `knowledge-base` route (lines 94-104) into a parent with children (`''`, `new`, `:id`, `:id/edit`), copying the parent/children shape the `ai-agent` route already uses (lines ~80-93). Add titles for the new pages in `core/router/page-title.ts` next to `knowledgeBase` (line 66). Page permission stays `knowledge_base.view` (`core/authz/permissions.ts:39`) for all four.
  - Fixture cleanup: remove the `knowledge-base` case from `features/tenant/routed-page-data.service.ts` (lines 87, 148-149, 212-213, 270-271) and its `KNOWLEDGE_FIXTURES` import (line 31); delete `shared/fixtures/knowledge.fixtures.ts` and the `KnowledgeArticleFixture`/`ArticleStatus`/`ArticleSource` types in `shared/fixtures/fixture.models.ts` (lines 5-6, 80-86) **only if nothing else imports them** (`grep -rn 'ArticleStatus\|ArticleSource\|KNOWLEDGE_FIXTURES' frontend/apps/dashboard/src` first); update `shared/fixtures/fixtures.spec.ts` (lines 6, 73) accordingly.

**Checkpoint**: US1 is independently demoable — authored drafts, list, detail, edit, tenant isolation. Run `cd frontend && pnpm ng test dashboard` and `cd backend && REQUIRE_DB_TESTS=1 cargo test --workspace`.

---

## Phase 4: User Story 2 — Publish and archive knowledge (Priority: P2)

**Goal**: The draft → published → archived → draft lifecycle, with publish gated on non-empty content, no-op replay safety, and an audit record per transition.

**Independent Test**: Take an existing draft, publish it (status flips, it joins the AI-available set), archive it (leaves the set, still listed), restore it (back to draft); confirm audit rows exist for each.

- [X] T024 [US2] In `backend/crates/modules/knowledge/src/routes.rs`, implement `pub async fn set_status` (`POST /tenant/knowledge/items/{id}/status`). Load the item tenant-scoped (miss → `not_found`), call `validate::check_transition` (T011), then: `Ok(false)` → return `changed: false` with the current status and **write no audit row** (replay-safe, Constitution V); `Err(Illegal)` → `validation_failed` naming the allowed transitions; `Err(BodyRequired)` → `validation_failed` ("publishing requires content", FR-004); `Ok(true)` → in **one transaction** update status + `tenancy::audit::record_in_tx` with `knowledge_item.published` / `.archived` / `.restored` per the target state (R8), details `{ itemId, itemType }`, no content.

- [X] T025 [US2] Wire `POST /tenant/knowledge/items/{id}/status` in `backend/crates/server/src/router.rs` behind `require_permission(Permission::KnowledgeBaseManage)` — single-verb route, so copy the simpler idiom at `router.rs:570-572` (`routes!(...).layer(require_permission(...))`). Add the path to `openapi_contract.rs`/`openapi_coverage.rs` (same files as T020).

- [X] T026 [US2] In `backend/crates/server/tests/knowledge_base.rs`, add the US2 matrix: each legal transition succeeds and is reflected in list+detail; **all six illegal `(from,to)` pairs** → `validation_failed`; same-status → 200 with `changed: false` and no new audit row; publishing an article with an empty/whitespace body → `validation_failed` and status unchanged; publishing a *document* with no body succeeds (documents are exempt); each successful transition writes exactly one correctly-named audit row with actor attribution and no content in details; `GET /items?status=published` returns exactly the published set and no drafts/archived (FR-015 / SC-006); Agent/Viewer → `unauthorized` on the status route, Manager/Admin/Owner succeed.

- [X] T027 [US2] Frontend: add `setStatus(id, status)` to `knowledge-api.service.ts` (+ spec) and a `setStatus` method to `knowledge.store.ts` (+ spec, covering the `changed: false` no-op and the validation-rejection path). Surface Publish/Archive/Restore actions on `article-detail.component.ts` and in the list rows, gated to manage-permission holders, with the status reflected via the existing `app-status-badge`. Add a status filter to the list toolbar (the type/category/tag filters land in US4). Component specs assert the actions appear for a manage user, are absent for a view-only user, and that a rejected publish surfaces the server's message.

**Checkpoint**: US1 + US2 both work independently. Full lifecycle demoable.

---

## Phase 5: User Story 3 — Upload knowledge documents (Priority: P3)

**Goal**: Upload a supported document ≤ 20 MB into object storage with metadata as a knowledge item, choosing draft or publish-immediately; download it back; reject bad files without orphaning anything.

**Independent Test**: Upload a PDF, confirm the object exists under `{tenant_id}/knowledge/{item_id}` and the item shows correct metadata; download returns the original bytes; a `.exe` or oversized file is rejected with no item and no object created.

- [X] T028 [US3] Create `backend/crates/modules/knowledge/src/upload.rs` (NEW) — multipart parsing + validation, no DB. Contents:
  - `pub struct ParsedUpload { pub filename: String, pub content_type: String, pub bytes: Vec<u8>, pub title: Option<String>, pub status: ItemStatus, pub category_id: Option<Uuid>, pub tags: Vec<String> }`.
  - `pub async fn parse(multipart: axum::extract::Multipart) -> Result<ParsedUpload, ValidationIssue>` reading the fields defined in [contracts/rest-api.md](./contracts/rest-api.md) § Documents (`file`, `title`, `status`, `categoryId`, `tags`). While reading the `file` part, **abort past `MAX_DOCUMENT_BYTES`** rather than buffering unbounded (T011's constant).
  - `pub fn validate_file(filename: &str, declared_content_type: &str, size: u64) -> Result<(), ValidationIssue>` — the R5 allowlist checked on **both** extension and declared MIME: `.pdf`→`application/pdf`; `.docx`→`application/vnd.openxmlformats-officedocument.wordprocessingml.document`; `.txt`→`text/plain`; `.md`→`text/markdown`|`text/x-markdown`|`text/plain`. Rejection messages must name the allowed types and the size limit (US3 scenario 2).
  - `status` defaults to `Draft`, accepts only `draft`|`published` (clarification #4 — `archived` on upload is `validation_failed`); `title` defaults to the filename stem.
  - In-file unit tests: the full accept/reject matrix (each allowed extension+MIME pair; extension/MIME mismatch such as `evil.exe` declared `application/pdf` **and** `evil.pdf` declared `application/x-msdownload` both rejected; at-limit accepted, one-byte-over rejected), title defaulting, status parsing.

- [X] T029 [US3] In `routes.rs`, implement `pub async fn upload_document` (`POST /tenant/knowledge/documents`, 201) taking `Extension<Arc<dyn ObjectStorage>>` (layered in T009) and `axum::extract::Multipart`. Order is load-bearing (research R3, FR-016): parse+validate (T028) → generate `item_id` → `storage.put("{tenant_id}/knowledge/{item_id}", content_type, bytes)` → **then** one transaction inserting `knowledge_items` (`item_type='document'`, `source='uploaded'`, chosen status, actor attribution) + `knowledge_documents` + tags + the `knowledge_item.created` audit row → **on transaction failure, best-effort `storage.delete(key)` before returning the error** (compensating action; `InMemoryStorage::delete` is idempotent per T006). Object-before-row means an interrupted request can never leave a metadata row without a file. The tenant-prefixed key makes cross-tenant key collisions structurally impossible (Constitution II). Log key + size, **never bytes**.
  Structure the put→persist→compensate sequence as an extractable helper in `upload.rs` so the compensation is testable without a database:
  ```rust
  pub async fn put_then_persist<F, Fut, T, E>(
      storage: &dyn ObjectStorage, key: &str, content_type: &str, bytes: Vec<u8>, persist: F,
  ) -> Result<T, UploadFailure<E>>
  where F: FnOnce() -> Fut, Fut: std::future::Future<Output = Result<T, E>> {
      storage.put(key, content_type, bytes).await.map_err(UploadFailure::Storage)?;
      match persist().await {
          Ok(v) => Ok(v),
          Err(e) => { let _ = storage.delete(key).await; Err(UploadFailure::Persist(e)) }
      }
  }
  ```
  The handler passes a closure that runs the metadata transaction. In-file unit tests against `InMemoryStorage` (no DB): (a) `persist` returns `Ok` → the object remains stored; (b) `persist` returns `Err` → **the object is gone** and the error propagates (US3 scenario 3, FR-016's compensating-delete clause); (c) `put` itself fails → `persist` is never called (assert via a closure that flips a flag). Test (b) is the only proof the compensation actually runs — without it, deleting the `storage.delete` line would pass every other test in the suite. The one case this does **not** cover is a process crash between `put` and commit; that residual orphan is an accepted v1 scope boundary (research R3, spec Assumptions) — do not add a sweeper without reopening that decision.

- [X] T030 [US3] In `routes.rs`, implement `pub async fn download_file` (`GET /tenant/knowledge/items/{id}/file`) — load the document row tenant-scoped (miss/cross-tenant/non-document → `not_found` / `validation_failed` per the contract), `storage.get(key)`, and respond with the stored content type plus `Content-Disposition: attachment; filename="{sanitize_filename(original_filename)}"` (T011 — prevents header injection). `StorageError::NotFound` (object deleted out-of-band) → `not_found`, while `GET /items/{id}` keeps serving the metadata so the UI can render "file unavailable" (spec Edge Cases).

- [X] T031 [US3] Wire both document routes in `backend/crates/server/src/router.rs`: `POST /tenant/knowledge/documents` behind `require_permission(Permission::KnowledgeBaseManage)` **with a route-scoped `.layer(DefaultBodyLimit::max(25 * 1024 * 1024))`** (axum's default 2 MB limit would otherwise reject every real upload; 25 MB = 20 MB file + form envelope), and `GET /tenant/knowledge/items/{id}/file` behind `KnowledgeBaseView`. Add both to `openapi_contract.rs`/`openapi_coverage.rs`, documenting the upload's `multipart/form-data` request body via `#[utoipa::path(request_body(content = …, content_type = "multipart/form-data"))]`.

- [X] T032 [US3] In `backend/crates/server/tests/knowledge_base.rs`, add US3 cases against `InMemoryStorage` (a handle kept from T018's `app_with_storage` call so the test can assert storage contents directly): upload → 201, object present at `{tenant_id}/knowledge/{item_id}`, metadata (filename/type/size) correct, `source: "uploaded"`; `status=published` at upload yields a published item, default yields draft, `status=archived` → `validation_failed` (clarification #4); rejected type/oversize → `validation_failed` naming allowed types **and neither an item row nor an object created**; download returns the original bytes + content type + `Content-Disposition`; download after the object is deleted out-of-band → `not_found` while `GET /items/{id}` still returns metadata; download of a non-document item → `validation_failed`; cross-tenant download → `not_found`; Agent/Viewer → `unauthorized` on upload, allowed on download.

- [X] T033 [US3] Frontend: add `uploadDocument(formData)` and `fileDownloadUrl(id)` to `knowledge-api.service.ts` (+ spec — assert it posts `FormData` and does **not** set a `Content-Type` header manually, which would break the multipart boundary). Create `upload-document.component.ts` + `.spec.ts` (NEW): a dialog built on the shared `app-dialog-shell` (`shared/components/dialog-shell/dialog-shell.component.ts` — `open`/`variant`/`dismiss` API at lines 86-94) with file picker, title, a draft-vs-publish-immediately choice (clarification #4), and client-side pre-validation mirroring T028's allowlist and 20 MB cap (fail fast; the server re-validates regardless — Constitution II). Wire it into the list page's toolbar, manage-gated. Render documents in the list/detail with their metadata, a download action, and the "file unavailable" state when the file endpoint 404s. Store methods + spec cover upload success and rejection.

**Checkpoint**: US1–US3 independently functional. Object storage exercised end-to-end (via MinIO manually per quickstart.md, via `InMemoryStorage` in CI).

---

## Phase 6: User Story 4 — Organize knowledge with categories and tags (Priority: P4)

**Goal**: Flat per-tenant categories and free-form tags, assignable to items, with list filtering by category, tag, type, and status.

**Independent Test**: Create categories and tags, assign them, verify each filter returns exactly the matching items, and that deleting an assigned category leaves its items intact but uncategorized.

- [X] T034 [US4] In `backend/crates/modules/knowledge/src/routes.rs`, implement the four category handlers per [contracts/rest-api.md](./contracts/rest-api.md) § Categories: `list_categories` (with `itemCount`, ordered by name), `create_category` (201; duplicate name case-insensitively → **`conflict`** via T012's `CategoryError::Duplicate`), `rename_category` (same duplicate rule), `delete_category` (204; `ON DELETE SET NULL` from T004 does the un-assignment — do **not** hand-write an `UPDATE … SET category_id = NULL`; second delete → `not_found`). Categories are flat: there is no parent field anywhere (clarification #3).

- [X] T035 [US4] Wire the four category routes in `backend/crates/server/src/router.rs` — GET behind `KnowledgeBaseView`, POST/PATCH/DELETE behind `KnowledgeBaseManage`, using the same idioms as T017/T025. Add all four to `openapi_contract.rs`/`openapi_coverage.rs`.

- [X] T036 [US4] Backend filter completion in `backend/crates/modules/knowledge/src/store.rs` (with the query params already defined in `routes.rs`'s `ItemListQuery`, T013): ensure `store::list_items` (T012) honors `category_id`, `tag`, `item_type`, `status`, and `q` (case-insensitive title contains) in any combination, and that the tag filter joins `knowledge_item_tags` without duplicating item rows. Confirm the tags-for-page query is still the single `ANY($1)` batch (no N+1) once the tag filter is present.

- [X] T037 [US4] In `backend/crates/server/tests/knowledge_base.rs`, add US4 cases: category CRUD happy paths; duplicate name (differing only in case) → `conflict`; the same name in a different tenant succeeds (isolation); rename to an existing name → `conflict`; **delete an assigned category → items survive with `category: null`** (FR-008 / US4 scenario 4); cross-tenant category id → `not_found`; assigning another tenant's category to an item → `validation_failed`; tags normalize (trim/lowercase/dedupe) and cap at 20 with `validation_failed` at 21; each filter (`type`, `status`, `categoryId`, `tag`, `q`) and a multi-filter combination return exactly the expected items; Agent/Viewer → `unauthorized` on category writes, allowed on the list.

- [X] T038 [US4] Frontend: add the category methods to `knowledge-api.service.ts` (+ spec) and the store (+ spec). Create `category-manager.component.ts` + `.spec.ts` (NEW) — an `app-dialog-shell` dialog for category CRUD, with the delete confirmation stating that affected items become uncategorized rather than deleted (matching real backend behavior). Add category/tag pickers to `article-editor.component.ts` and category/tag/type filters to the list toolbar alongside US2's status filter — note the existing fixture page already renders a category `<select>` (`knowledge-base.component.ts:51-61`); replace its fixture-derived options with store-derived ones rather than adding a second control. Specs assert filter changes trigger the expected store calls and that manage-gating hides category management from view-only users.

**Checkpoint**: All four user stories independently functional.

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T039 [P] Update `AGENTS.md` and `CLAUDE.md`'s "Recent Changes" section with a `019-knowledge-base` entry, following the existing one-line-per-feature style (see the `014-human-handoff-routing` entry in `CLAUDE.md`). Mention: knowledge module activation, S3-compatible document storage, draft/published/archived lifecycle, and that only published items are the AI-available set. Prefer running `/speckit-agent-context-update` for the managed section.

- [X] T040 [P] Document the new env vars: add the `APP_S3_*` block (T007's names) to the backend env documentation/example alongside the existing `APP_AI_*` entries, and note in `infra/docker-compose.yml` (or its README, if the compose file has no comment convention) that the MinIO bucket named by `APP_S3_BUCKET` must exist before first upload — the app does not create it.

- [X] T041 Constitution VI check: confirm the handlers emit structured `tracing` events carrying item id/action/latency under the existing request-id propagation, and **grep the diff for content leaks** — `rg 'body|content|bytes' backend/crates/modules/knowledge/src --type rust` and verify no `tracing::` call and no audit `details` payload interpolates article body text or file bytes (015/018 invariant; T015/T024/T029 each assert this, this is the belt-and-braces sweep).

- [X] T042 Run the full gate set from [quickstart.md](./quickstart.md) and fix anything red: `cd backend && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && REQUIRE_DB_TESTS=1 cargo test --workspace`; `cd frontend && pnpm ng build dashboard && pnpm ng test dashboard && pnpm lint && pnpm format:check`.

- [ ] T043 Walk the six manual validation scenarios in [quickstart.md](./quickstart.md) against a real stack (`docker compose -f infra/docker-compose.yml up -d`, `APP_S3_*` exported, MinIO bucket created) — especially scenario 3 (the file really lands in MinIO and downloads back byte-identical) and scenario 5 (cross-tenant + Agent/Viewer read-only), which the `InMemoryStorage`-backed suite cannot prove.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: T001 first; T002/T003 are independent of each other.
- **Foundational (Phase 2)**: depends on Setup. **Blocks every user story.** Internal order: T004 → T005; T007 → T008; T006+T007 → T009; T006 → T010 → T011 → T012.
- **User Stories (Phases 3–6)**: all depend on Phase 2 completion. US1 → US2 → US3 → US4 in priority order, or in parallel across developers (see below).
- **Polish (Phase 7)**: after the stories you intend to ship.

### User Story Dependencies

- **US1 (P1)**: needs only Phase 2. The MVP.
- **US2 (P2)**: needs Phase 2. Independently testable by seeding a draft directly; in practice authored via US1's UI.
- **US3 (P3)**: needs Phase 2 (specifically T006/T009's storage seam). Independent of US1/US2 — a document item exercises the same store/list code but no article authoring.
- **US4 (P4)**: needs Phase 2. Touches `store::list_items` (T012) and the editor page (T023) — if US1 and US4 run concurrently, T036/T038 conflict with T014/T023 in the same files; sequence those two tasks or accept a merge.

### Parallel Opportunities

- Phase 1: T002 and T003 (backend vs frontend).
- Phase 2: T005 [P] runs alongside T006/T007 (different files). T009 and T010 are independent once T006/T007 land.
- Phase 3: T020 and T021 [P] with each other and with backend work (different files). Within US1, backend (T013–T019) and frontend (T021–T023) are independently developable against the contract once T013's DTOs are agreed.
- Cross-story: with multiple developers, US2/US3/US4 proceed in parallel after Phase 2, with the T036/T038-vs-US1 file overlap noted above as the one real conflict.

## Parallel Example: User Story 1

```bash
# After Phase 2, launch the independent US1 tracks together:
Task: "Backend DTOs + list/create/detail/edit handlers in backend/crates/modules/knowledge/src/routes.rs"   # T013-T016
Task: "Frontend models in frontend/apps/dashboard/src/app/core/api/knowledge.models.ts"                     # T021 [P]
Task: "OpenAPI registration in backend/crates/server/tests/openapi_{contract,coverage}.rs"                  # T020 [P]
```

## Implementation Strategy

### MVP First (User Story 1 only)

1. Phase 1 (Setup) → Phase 2 (Foundational — the big one: migration, storage crate, config, module skeleton).
2. Phase 3 (US1).
3. **STOP and VALIDATE**: authored articles, list, detail, edit, tenant isolation, RBAC — all real, no fixtures.
4. Demo-ready: tenants can manage authored knowledge.

### Incremental Delivery

1. Setup + Foundational → foundation ready.
2. + US1 → **MVP**: author/edit/list/detail.
3. + US2 → the draft/published/archived lifecycle and the AI-available set (FR-015 satisfied — the payoff for a future ingestion feature).
4. + US3 → document upload/download in object storage (satisfies the spec's "files are stored in object storage" acceptance criterion).
5. + US4 → categories/tags/filters for scale.
6. Polish → docs, env, observability sweep, full gates, manual validation.

## Notes

- [P] = different files, no ordering dependency.
- Commit after each task or logical group; keep migration 0046 and its compile-fix cascade (T004/T005) in one commit so the tree never sits un-buildable.
- **Zero permission-code or role-matrix changes** — `knowledge_base.view/manage` already exist and already match clarification #1 (`authz/src/permission.rs:21-24`, `authz/src/matrix.rs:12-13,28-29,42,48`). If you find yourself editing `matrix.rs`, stop: the requirement is already met and the change is a regression.
- Cross-tenant access answers `not_found`, never `unauthorized` — no existence oracle (Constitution II).
- Article body and file bytes never enter logs, traces, or audit details (015/018 invariant, re-swept in T041).

---

## Phase 8: Convergence

Appended by `/speckit-converge`. Only genuine gaps are listed — the rest of T001–T042 was verified as actually implemented, not merely checked off. T043 (manual real-stack validation) remains open and is deliberately not duplicated here.

- [ ] T044 Stop `loadList`/`loadMore` from permanently bricking on a failed request in `frontend/apps/dashboard/src/app/features/tenant/knowledge-base/knowledge.store.ts` (lines 59-103) per FR-010, SC-005 (partial). Both `rxMethod` pipes handle failure with `tap({ error: … })` inside `switchMap`, which *observes* the error but does not swallow it: the error propagates out of the pipe, terminating the rxMethod's single long-lived subscription. After one transient list failure, every later `loadList()`/`loadMore()` call is a silent no-op until a full page reload. Proof it escapes today: `pnpm ng test dashboard` reports 2 uncaught "Network error" exceptions from `knowledge.store.spec.ts` even though all 858 tests pass. Fix by adding `catchError(() => EMPTY)` to the **inner** pipe (after the `tap`) in both methods — keep the `tap` so the `error` state still populates — or switch to `tapResponse` from `@ngrx/operators`. `EMPTY` is already imported; `catchError` is not. Scope strictly to these two methods: `setFilter`, `loadItem`, `createItem`, `updateItem`, `setStatus`, and the category methods use imperative `.subscribe({ error })`, whose subscriber-level handler recovers correctly and must not be changed. Extend `knowledge.store.spec.ts`'s "handles error on loadList" test to assert **recovery** — after the error, a second `loadList()` against a fresh subject must repopulate `items()` — since the current test only asserts `error()` is set and therefore passes while the stream is dead. The suite must finish with 0 uncaught errors.

- [ ] T045 Surface the document read path in the UI per FR-011, US3/AC1 and the spec's "file unavailable" edge case (partial). The backend is complete and the frontend API layer is ready — `knowledge-api.service.ts:75`'s `fileDownloadUrl(id)` is defined and unit-tested but **called by no component**, so uploaded documents currently cannot be downloaded or inspected from the dashboard at all. Only component wiring is missing: (a) in `article-detail.component.ts` (277 lines — currently renders status/type/category/tags/author/updated and nothing file-related), render the `document: DocumentMeta` fields already present on `KnowledgeItemDetail` (`fileName`, `contentType`, `sizeBytes`, uploader/upload time) for `itemType === 'document'`, with a download action using `fileDownloadUrl`; (b) render a clear "file unavailable" indication when `GET /tenant/knowledge/items/{id}/file` answers 404 (the object was deleted out-of-band) while still showing the metadata rather than failing the page — the backend already serves metadata in exactly this case; (c) add a download affordance to the document rows in `knowledge-base.component.ts`, which today only maps the type to a "Document" label (line 388). Cover all three in `article-detail.component.spec.ts`, whose fixtures currently only ever set `document: null` — add a document-item case plus the file-unavailable branch.

- [ ] T046 Restore the `cargo fmt --check` gate that T042 claims is green per T042 / quickstart gate set (partial). `cd backend && cargo fmt --check` currently exits 1 on `crates/server/tests/ai_agent_prompt.rs:996` (a mis-indented `send(` block inside a `for` loop). **Provenance: this is 018-prompt-management's in-flight test code, not a 019 regression** — the line is unrelated to T008's `s3: None,` sweep, which touched only that file's `AppConfig` literal near line 38. It is listed here solely because T042 is marked complete while asserting the full gate set passes, and 019 cannot merge behind a red fmt gate. Run `cargo fmt` to fix, and confirm `cargo fmt --check` exits 0. Do not treat this as license to reformat anything else in 018's files.
