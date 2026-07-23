---

description: "Task list for Integrations Foundation (spec 028)"
---

# Tasks: Integrations Foundation

**Input**: Design documents from `/specs/028-integrations-foundation/`

**Prerequisites**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/integrations-api.md](contracts/integrations-api.md), [quickstart.md](quickstart.md)

**Tests**: Included — Constitution Principle VII requires unit + integration + API coverage, and plan.md names the exact test files.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependency on an incomplete task)
- **[Story]**: US1 / US2 / US3 (from spec.md). Setup, Foundational, and Polish tasks have no story label.

## How to work these tasks

Every task names **one file** and **one reference file to copy the pattern from**. Open the reference file first, mirror its structure, then adapt the names given in the task. Do not invent new patterns.

Key facts you need for almost every backend task:

- Crate root: `backend/crates/modules/integrations/` (it already exists as a placeholder containing only `src/lib.rs`; it is already a workspace member via the `crates/modules/*` glob).
- **JSON casing**: DTO structs derive `Serialize, ToSchema` with **NO `#[serde(rename_all)]`** ⇒ snake_case on the wire. List responses use `{ "data": [...], "pagination": { "next_cursor": …, "has_more": … } }`. This matches `backend/crates/modules/audit/src/model.rs`. Do **not** copy the widgets module's camelCase.
- Exact request/response bodies are in [contracts/integrations-api.md](contracts/integrations-api.md). Follow them literally.
- Exact table and column names are in [data-model.md](data-model.md). Follow them literally.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Dependencies, config, and database schema. Nothing compiles against the new module until this is done.

- [x] T001 Add `hmac = "0.12"` to the `[workspace.dependencies]` table in `backend/Cargo.toml` (place it next to the existing `sha2 = "0.10"` line).

- [x] T002 Fill in `backend/crates/modules/integrations/Cargo.toml` dependencies. Copy the dependency list from `backend/crates/modules/widgets/Cargo.toml`, keep only: `axum`, `chrono`, `config`, `hex`, `kernel`, `identity`, `rand`, `serde`, `serde_json`, `sha2`, `sqlx`, `tenancy`, `tokio`, `tracing`, `utoipa`, `uuid`; then add `aes-gcm.workspace = true`, `base64.workspace = true`, `hmac.workspace = true`.

- [x] T003 [P] Add `integrations = { path = "../modules/integrations" }` to `[dependencies]` in `backend/crates/server/Cargo.toml`, keeping the list alphabetically sorted (it goes just after `identity`).

- [x] T004 [P] Add an optional config field `integration_secrets_key: Option<String>` to `backend/crates/shared/config/src/lib.rs`. Copy the existing `ai_key_encryption_key` handling exactly (field declaration ~line 94, `[REDACTED]` Debug entry ~line 114/133, and the `APP_AI_KEY_ENCRYPTION_KEY` env parsing block ~line 250), renaming the env var to `APP_INTEGRATION_SECRETS_KEY`. Same rule: base64, must decode to exactly 32 bytes.

- [x] T005 Create migration `backend/migrations/0056_integrations_foundation.sql`. Create the five tables exactly as specified in [data-model.md](data-model.md): `integration_catalog`, `integration_connections` (note it has three webhook-token columns: `webhook_token_hash`, `webhook_token_ciphertext`, `webhook_token_nonce`), `integration_secrets`, `integration_webhook_deliveries`, `integration_events` — including every listed index and unique constraint. Then seed `integration_catalog` with four rows: `generic-webhook` (name "Generic Webhook", category `automation`, `is_available = true`, `config_schema` = the two-field JSON array from data-model.md) and `slack`, `microsoft-teams`, `crm` (`is_available = false`, `config_schema = '[]'`). Copy the file header/style from `backend/migrations/0054_notifications.sql`.

**Verify Phase 1**: `cd backend && sqlx migrate run && cargo check -p integrations`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: The module's types, crypto, queries, and audit plumbing. Every user story depends on these.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [x] T006 Replace the placeholder comment in `backend/crates/modules/integrations/src/lib.rs` with module documentation and `pub mod` declarations for `model`, `crypto`, `queries`, `routes`, `webhook`, `status`, `audit`, `retention`. Copy the doc-comment structure (Purpose / Responsibilities / Public Interfaces / Dependencies / Data Model / Extension Points) from `backend/crates/modules/audit/src/lib.rs`.

- [x] T007 Create `backend/crates/modules/integrations/src/model.rs` with the DTOs and enums from [contracts/integrations-api.md](contracts/integrations-api.md) and [data-model.md](data-model.md): `IntegrationListItemDto`, `IntegrationListResponse`, `IntegrationDetailDto`, `IntegrationConnectionDto`, `IntegrationSecretRefDto`, `ConfigFieldDto`, `IntegrationEventDto`, `IntegrationEventListResponse`, `PaginationInfo`, `ConnectPayload`, `UpdateConfigPayload`, `EventsQuery`, plus string enums `ConnectionStatus`, `EventType`, `RejectionReason`. Mirror the struct/derive style of `backend/crates/modules/audit/src/model.rs` — remember: no `rename_all`, so snake_case on the wire.

- [x] T008 [P] Create `backend/crates/modules/integrations/src/crypto.rs` by copying `backend/crates/ai-providers/src/crypto.rs` almost verbatim: `MasterKey` (with `[REDACTED]` Debug and `from_base64`), `seal`, `open`, `hint`. Two changes: `open` returns `String` instead of `SecretKey` (do not depend on the `ai-providers` crate), and replace the `aad` function with `pub fn aad(tenant_id: Uuid, slug: &str, field_key: &str) -> String` returning `format!("integration|{tenant_id}|{slug}|{field_key}")`. Keep the existing unit tests, adapted (roundtrip, AAD mismatch fails, tampered ciphertext fails, hint cases).

- [x] T009 [P] Create `backend/crates/modules/integrations/src/status.rs` with a pure function `pub fn derive_status(is_active: Option<bool>, recent_outcomes: &[&str]) -> ConnectionStatus`. Rules: `None` ⇒ `NotConnected`; `Some(false)` ⇒ `Disconnected`; `Some(true)` and `recent_outcomes` has at least 3 entries whose first 3 (newest first) are all `"failure"` ⇒ `Error`; otherwise `Connected`. Add unit tests for all four branches plus the "only 2 failures ⇒ Connected" and "failure,failure,success ⇒ Connected" cases.

- [x] T010 Create `backend/crates/modules/integrations/src/queries.rs` with the read helpers: `find_catalog_by_slug(pool, slug)`, `find_connection(pool, tenant_id, catalog_id)`, `list_secret_refs(pool, connection_id)` (returns `field_key` + `hint` only — never ciphertext), `list_events(pool, connection_id, cursor, limit)`, and `list_catalog_with_status(pool, tenant_id)`. Copy the cursor helpers `encode_cursor` / `decode_cursor` verbatim from `backend/crates/modules/audit/src/queries.rs`.

  **`list_catalog_with_status` must be ONE query** — it returns each catalog row, its optional connection row, **and** that connection's 3 most recent event outcomes, so the caller never queries per row. Use a lateral join:

  ```sql
  SELECT cat.*, c.id AS connection_id, c.is_active, ev.outcomes
  FROM integration_catalog cat
  LEFT JOIN integration_connections c
    ON c.catalog_id = cat.id AND c.tenant_id = $1
  LEFT JOIN LATERAL (
    SELECT array_agg(e.outcome ORDER BY e.created_at DESC) AS outcomes
    FROM (
      SELECT outcome, created_at FROM integration_events
      WHERE connection_id = c.id AND created_at > now() - interval '24 hours'
      ORDER BY created_at DESC LIMIT 3
    ) e
  ) ev ON TRUE
  ORDER BY cat.name;
  ```

  Add a matching single-connection helper `recent_event_outcomes(pool, connection_id)` (newest-first `outcome` strings, last 24 h, limit 3) for the **detail** endpoint only, where exactly one connection is in play.

- [x] T011 [P] Create `backend/crates/modules/integrations/src/audit.rs` with four writers: `record_connected_in_tx`, `record_config_updated_in_tx`, `record_secret_rotated_in_tx`, `record_disconnected_in_tx`. Copy the exact shape of `backend/crates/modules/widgets/src/audit.rs` (each calls `tenancy::audit::record_in_tx`). Use `RESOURCE_TYPE = "integration_connection"` and actions `integration.connected`, `integration.config_updated`, `integration.secret_rotated`, `integration.disconnected`. The JSON details must contain only `connectionId` and `slug` — **never** config values or secrets.

- [x] T012 [P] Add `("integrations", &["integration."])` to the `CATEGORY_PREFIXES` constant in `backend/crates/modules/audit/src/model.rs` (append after the `widgets` entry, ~line 65) so audit rows written by T011 get the `integrations` category.

**Verify Phase 2**: `cd backend && cargo test -p integrations && cargo test -p audit`

---

## Phase 3: User Story 1 - Browse the catalog and see connection state (Priority: P1) 🎯 MVP

**Goal**: A tenant user with `integrations.view` sees the catalog with a per-tenant status badge, and can open a read-only detail page.

**Independent Test**: Sign in as Admin → integrations page lists `generic-webhook` plus the three coming-soon entries with correct badges; detail page opens. Agent gets 403; Viewer sees the page read-only. Statuses for connected/disconnected states are verified by inserting connection rows directly via SQL (the connect endpoint arrives in US2).

### Backend for User Story 1

- [x] T013 [US1] Add the `list_integrations` handler to `backend/crates/modules/integrations/src/routes.rs` for `GET /tenant/integrations`. Extractors (exact form — `TenantContext` is a bare extractor, **not** an `Extension`): `State(pool): State<PgPool>, ctx: tenancy::TenantContext`. Call `queries::list_catalog_with_status(&pool, ctx.tenant_id)` **exactly once**, then derive each row's status with `status::derive_status` using the outcomes already returned by that query. Do **not** call `recent_event_outcomes` (or any other query) inside the loop over catalog rows — a per-row query here is an N+1, which Constitution Principle X treats as a defect. Return `Json(IntegrationListResponse)`. On DB error return `ApiError::internal_error(...).with_request_id(&ctx.request_id)`. Copy the handler signature, `#[utoipa::path(...)]` attribute style, and error handling from `list_tenant_audit_logs` in `backend/crates/modules/audit/src/routes.rs`. Use `tag = "integrations"` and `operation_id = "list_integrations"`.

- [x] T014 [US1] Add the `get_integration` handler to `backend/crates/modules/integrations/src/routes.rs` for `GET /tenant/integrations/{slug}`. Extract the slug with `Path(slug): Path<String>`, return `404` via `ApiError::not_found` when the slug is unknown, and build `IntegrationDetailDto` exactly as shown in the contract — `connection: null` when there is no connection row, `secrets` from `queries::list_secret_refs` (hints only), and `webhook_url: null` for now (T035 in US2 fills it in once tokens exist). `operation_id = "get_integration"`.

- [x] T015 [US1] Mount both handlers in the tenant router in `backend/crates/server/src/router.rs`. Add them next to the audit-logs block (~line 735) using the same shape: `.routes(routes!(integrations::routes::list_integrations).layer(require_permission(Permission::IntegrationsView)))` and the same for `get_integration`. Add a `// Integrations (spec 028)` comment above them.

- [x] T016 [US1] Register the new DTOs in the `components(schemas(...))` list in `backend/crates/server/src/openapi.rs` (follow the existing `widgets::model::…` entries ~line 168): `IntegrationListItemDto`, `IntegrationListResponse`, `IntegrationDetailDto`, `IntegrationConnectionDto`, `IntegrationSecretRefDto`, `ConfigFieldDto`.

### Tests for User Story 1

- [x] T017 [P] [US1] Create `backend/crates/server/tests/integrations_catalog.rs`. Copy the test-harness setup (app spawn, tenant/user seeding, auth headers) from `backend/crates/server/tests/audit_logs.rs`. Assert: list returns all 4 seeded catalog entries; `slack` has `is_available: false`; a tenant with no connections sees `status: "not_connected"` everywhere; after inserting an active connection row via SQL the status is `"connected"` and after `is_active = false` it is `"disconnected"`; detail returns 404 for an unknown slug. Add the retired-entry case (spec FR-001): with an active connection in place, flip that catalog row's `is_available` to false via SQL and assert the connection still reports `"connected"` — availability gates new connections only, never existing ones.

- [x] T018 [P] [US1] Create `backend/crates/server/tests/integrations_rbac.rs`. Assert Admin and Manager get 200 on list and detail; Viewer gets 200 on list and detail; Agent gets 403 on both. Copy the multi-role setup from `backend/crates/server/tests/rbac.rs`.

- [x] T019 [P] [US1] Create `backend/crates/server/tests/integrations_isolation.rs`. Seed two tenants, give tenant A an active connection, and assert tenant B's list shows `not_connected` for that slug and tenant B's detail returns `connection: null` (and therefore no config, no secret hints, and no webhook URL). Copy the two-tenant setup from `backend/crates/server/tests/widget_tenant_isolation.rs`. The events endpoint is covered by the cross-tenant case in T055, once that endpoint exists.

### Frontend for User Story 1

- [x] T020 [US1] Add the wire interfaces and mappers to `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts`: `IntegrationListItemWire`, `IntegrationListWire`, `IntegrationConfigFieldWire`, `IntegrationSecretRefWire`, `IntegrationConnectionWire`, `IntegrationDetailWire` (all snake_case, matching the contract bodies) plus camelCase model types `IntegrationListItem`, `IntegrationDetail`, `IntegrationConnection` and the mappers `integrationListFromWire` and `integrationDetailFromWire`. Copy the style of `AuditListWire` / `auditListFromWire` (~line 997).

- [x] T021 [US1] Create `frontend/apps/dashboard/src/app/features/tenant/integrations/integrations-api.service.ts` with `list()` and `detail(slug)` methods returning `Observable<ApiResponse<…>>`. Copy `frontend/apps/dashboard/src/app/features/tenant/audit-logs/audit-logs-api.service.ts` exactly, swapping the paths to `/tenant/integrations` and `/tenant/integrations/${slug}` and the mappers to those from T020.

- [x] T022 [US1] Create `frontend/apps/dashboard/src/app/features/tenant/integrations/integrations.store.ts` — an NgRx SignalStore with state `{ items: IntegrationListItem[]; loading: boolean; error: string | null }` and a `load()` rxMethod. Copy the store structure (imports, `signalStore`, `withState`, `withMethods`, `rxMethod`, tenant-change `effect` in `withHooks`) from `frontend/apps/dashboard/src/app/features/tenant/audit-logs/audit-logs.store.ts`, dropping all filter/pagination state.

- [x] T023 [US1] Rewrite `frontend/apps/dashboard/src/app/features/tenant/integrations/integrations.component.ts` to render from `IntegrationsStore` (T022) instead of `RoutedPageStore` fixtures. Keep the existing page shell components (`app-page-container`, `app-page-header`, `app-loading-state`, `app-empty-state`, `app-dashboard-card`, `app-status-badge`) and the current visual layout. Each catalog entry renders name, description, category, an `app-status-badge` bound to `status`, a "Coming soon" marker when `is_available` is false, and links to the detail route. Remove the `IntegrationStatus` fixture import.

- [x] T024 [P] [US1] Add the detail route path `integrationDetail: 'integrations/:slug'` to the `tenant` section of `frontend/apps/dashboard/src/app/core/router/app-paths.ts` (next to the existing `integrations: 'integrations'` at line 31), and add a matching `PAGE_PERMISSIONS` entry mapping it to `'integrations.view'` in `frontend/apps/dashboard/src/app/core/authz/permissions.ts` (next to line 44).

- [x] T025 [US1] Create `frontend/apps/dashboard/src/app/features/tenant/integrations/integration-detail.store.ts` — a SignalStore with state `{ detail: IntegrationDetail | null; loading: boolean; error: string | null }` and a `load(slug)` rxMethod calling `IntegrationsApiService.detail`. Mirror T022's structure.

- [x] T026 [US1] Create `frontend/apps/dashboard/src/app/features/tenant/integrations/integration-detail.component.ts` — a read-only detail page showing name, description, category, status badge, and the config-schema field labels. Reuse the same shell components as T023. No forms yet (US2 adds them). Read the `slug` route param via `input()` binding.

- [x] T027 [US1] Register the detail route in `frontend/apps/dashboard/src/app/features/tenant/tenant.routes.ts` immediately after the existing integrations route (~line 144), lazy-loading `IntegrationDetailComponent`, with `requiredPermission: PAGE_PERMISSIONS[APP_PATHS.tenant.integrationDetail]` and the same `data`/`title` shape as the neighbouring route.

- [x] T028 [P] [US1] Create `frontend/apps/dashboard/src/app/features/tenant/integrations/integrations.store.spec.ts` and `integration-detail.store.spec.ts` covering load-success and load-error for each store. Copy the spec setup from `frontend/apps/dashboard/src/app/features/tenant/audit-logs/audit-logs.store.spec.ts`.

**Verify US1**: `cd backend && cargo test -p server --test integrations_catalog --test integrations_rbac --test integrations_isolation` then `cd frontend && pnpm ng test dashboard && pnpm ng build dashboard`

**Checkpoint**: The catalog is browsable and permission-gated end to end. This is the MVP.

---

## Phase 4: User Story 2 - Connect and disconnect an integration (Priority: P2)

**Goal**: Users with `integrations.manage` can connect (with secrets), update config, rotate secrets, and disconnect. Secrets are encrypted and never returned.

**Independent Test**: Connect `generic-webhook` with a signing secret → status becomes connected, response shows only a 4-char hint and a webhook URL; disconnect → status disconnected, secrets gone; reconnect → same connection row, history preserved.

### Backend for User Story 2

- [x] T029 [US2] Add config validation to `backend/crates/modules/integrations/src/model.rs`: `pub fn validate_against_schema(schema: &[ConfigFieldDto], config: &Map<String, Value>, secrets: &Map<String, String>, require_all: bool) -> Result<(), Vec<serde_json::Value>>`. Rules: reject unknown keys; when `require_all` is true every `required` field must be present and non-empty; `kind: "secret"` keys may only appear in `secrets`, `kind: "text"` keys only in `config`. Return the same per-field error JSON shape used by `validate_instance_fields` in `backend/crates/modules/widgets/src/admin_routes.rs`.

- [x] T030 [US2] Add the write helpers to `backend/crates/modules/integrations/src/queries.rs`, all taking `&mut Transaction<'_, Postgres>`: `upsert_connection` (insert or reactivate on the `(tenant_id, catalog_id)` unique constraint, setting `is_active = true`, a new `webhook_token_hash` **and** `webhook_token_ciphertext`/`webhook_token_nonce`, `connected_at`, `connected_by_membership_id`), `update_connection_config`, `upsert_secret` (on conflict `(connection_id, field_key)` replace ciphertext/nonce/hint), `delete_secrets_for_connection`, `deactivate_connection`, and `insert_event`.

- [x] T031 [US2] Add token generation to `backend/crates/modules/integrations/src/webhook.rs`: `pub fn generate_token() -> String` (32 random bytes from `rand`, URL-safe base64, no padding) and `pub fn hash_token(token: &str) -> Vec<u8>` (SHA-256). Copy `hash_token` verbatim from `backend/crates/modules/widgets/src/session.rs:15` and keep its two unit tests.

- [x] T032 [US2] Add the `connect_integration` handler to `backend/crates/modules/integrations/src/routes.rs` for `POST /tenant/integrations/{slug}/connect`. In one transaction: 404 unknown slug; 422 when `is_available` is false (this gate applies to **new** connections and reconnects only — T033/T034 must not check availability, so an existing connection to a retired entry stays updatable and disconnectable) or validation (T029) fails; 409 when an active connection exists; otherwise generate a token (T031), seal each secret with `crypto::seal` using `crypto::aad(tenant_id, slug, field_key)`, seal the token itself the same way with field key `"__webhook_token"` and store its hash alongside, upsert connection + secrets, insert a `connected` event, write the audit row via `audit::record_connected_in_tx`, commit, and return `201` with the detail body including the plaintext `webhook_url`. Extractor signature (copy from `get_snippet` in `backend/crates/modules/widgets/src/admin_routes.rs:565`): `State(pool): State<PgPool>, ctx: TenantContext, Extension(principal): Extension<Principal>, Extension(config): Extension<std::sync::Arc<config::AppConfig>>, Path(slug): Path<String>, ApiJson(payload): ApiJson<ConnectPayload>`.

- [x] T033 [US2] Add the `update_integration_config` handler to `backend/crates/modules/integrations/src/routes.rs` for `PUT /tenant/integrations/{slug}/config`. Same transaction pattern as T032, but: 409 if not actively connected; `secrets` is optional and only the provided keys are re-sealed; insert a `config_updated` event when config changed and a `secret_rotated` event when any secret changed, with the matching audit writers; return `200` with the detail body (`webhook_url` unchanged — do not rotate the token here).

- [x] T034 [US2] Add the `disconnect_integration` handler to `backend/crates/modules/integrations/src/routes.rs` for `POST /tenant/integrations/{slug}/disconnect`. In one transaction: 409 if not actively connected; otherwise `delete_secrets_for_connection`, `deactivate_connection`, insert a `disconnected` event, write the audit row, commit, and return `200` with the detail body showing `status: "disconnected"`, `secrets: []`, `webhook_url: null`.

- [x] T035 [US2] Update `get_integration` (T014) in `backend/crates/modules/integrations/src/routes.rs` so `webhook_url` is populated for active connections: decrypt `webhook_token_ciphertext` / `webhook_token_nonce` with `crypto::open` using `crypto::aad(tenant_id, slug, "__webhook_token")`, then build the URL as `format!("{}/hooks/v1/{}", config.public_dashboard_url.trim_end_matches('/'), token)`. Return `null` when the connection is inactive. Reuse the same `Extension(config): Extension<std::sync::Arc<config::AppConfig>>` extractor and `public_dashboard_url` field that `get_snippet` uses at `backend/crates/modules/widgets/src/admin_routes.rs:587`.

- [x] T036 [US2] Mount the three write handlers in `backend/crates/server/src/router.rs` next to the T015 block, each with `.layer(require_permission(Permission::IntegrationsManage))`, and register `ConnectPayload` / `UpdateConfigPayload` in `backend/crates/server/src/openapi.rs`.

### Tests for User Story 2

- [x] T037 [P] [US2] Create `backend/crates/server/tests/integrations_lifecycle.rs`: connect returns 201 and `status: "connected"`; a second connect returns 409; connecting `slack` (unavailable) returns 422; connect with a missing required field returns 422; update config returns 200 and changes the value; disconnect returns 200 with `status: "disconnected"` and empty secrets; reconnect returns 201 and the `integration_connections` row count for that tenant+catalog pair is still exactly 1. Then assert **SC-004 in full** — all four lifecycle actions land in *both* logs: after connect, an `integration.connected` row in `audit_logs` and a `connected` row in `integration_events`; after a config-only update, `integration.config_updated` + `config_updated`; after a secret rotation, `integration.secret_rotated` + `secret_rotated`; after disconnect, `integration.disconnected` + `disconnected`. Also assert the disconnect did not delete any pre-existing event rows (history is preserved, FR-006).

- [x] T038 [P] [US2] Create `backend/crates/server/tests/integrations_secret_confidentiality.rs`: connect with the secret `whsec_supersecret123`, then assert that string appears in **no** response body from connect, detail, list, or the events endpoint, and in no `audit_logs.details` row for that tenant; assert the detail response's secret hint equals `t123`; assert the `integration_secrets.ciphertext` column does not contain the plaintext bytes.

- [x] T039 [P] [US2] Add a Viewer-role case to `backend/crates/server/tests/integrations_rbac.rs`: Viewer gets 403 on connect, update, and disconnect, while Manager gets 2xx on all three.

### Frontend for User Story 2

- [x] T040 [US2] Add `connect(slug, body)`, `updateConfig(slug, body)`, and `disconnect(slug)` methods to `frontend/apps/dashboard/src/app/features/tenant/integrations/integrations-api.service.ts`, using `ApiService.post` / `ApiService.put` and the `integrationDetailFromWire` mapper.

- [x] T041 [US2] Add `connect`, `updateConfig`, and `disconnect` rxMethods to `frontend/apps/dashboard/src/app/features/tenant/integrations/integration-detail.store.ts`, each patching `detail` from the response and setting a `saving` boolean plus `error` on failure.

- [x] T042 [US2] Extend `frontend/apps/dashboard/src/app/features/tenant/integrations/integration-detail.component.ts` with the connect/update form. Render one input per `config_schema` field; `kind: "secret"` fields are write-only password inputs that show `••••` plus the stored hint when a secret already exists and are left blank to keep the current value on update. Add Connect / Save / Disconnect buttons wired to the T041 methods. When the connection is active, show its `webhook_url` with a copy button. **Do not inline `navigator.clipboard.writeText`** — Constitution Principle IX forbids duplicating UI logic, and that call is already copy-pasted into `features/tenant/widgets/widgets.component.ts:442` and `features/tenant/team/invite-dialog.component.ts`. Create `frontend/apps/dashboard/src/app/shared/utils/clipboard.ts` exporting `copyToClipboard(text: string): Promise<void>` (the `shared/utils/` directory does not exist yet — create it) and call that here. T061 migrates the two existing call sites.

- [x] T043 [US2] Hide all mutating controls in `integration-detail.component.ts` unless the user has `integrations.manage`, using the existing permissions service (see how `frontend/apps/dashboard/src/app/core/authz/permissions.service.spec.ts` exercises it). The backend still enforces this — the UI check is cosmetic only.

- [x] T044 [P] [US2] Extend `frontend/apps/dashboard/src/app/features/tenant/integrations/integration-detail.store.spec.ts` with connect-success, connect-error, and disconnect-success cases.

**Verify US2**: `cd backend && cargo test -p server --test integrations_lifecycle --test integrations_secret_confidentiality --test integrations_rbac` then `cd frontend && pnpm ng test dashboard`

**Checkpoint**: Full connection lifecycle works and secrets never leave the backend.

---

## Phase 5: User Story 3 - Receive webhooks and monitor health (Priority: P3)

**Goal**: Signed inbound deliveries are accepted, logged, and rate-limited; rejections are logged; health status and the event log are visible in the UI.

**Independent Test**: Connect an integration, POST a correctly signed body to its webhook URL → 202 and a `delivery_accepted` event; POST a bad signature → 404 and a `delivery_rejected` event; three consecutive failures flip the badge to error.

### Backend for User Story 3

- [x] T045 [US3] Add HMAC verification to `backend/crates/modules/integrations/src/webhook.rs`: `pub fn verify_signature(secret: &str, raw_body: &[u8], header: &str) -> bool`. Strip the `sha256=` prefix, hex-decode the remainder, and verify with `Hmac::<Sha256>::new_from_slice(secret.as_bytes())` + `mac.verify_slice(...)` (this is constant-time — do not hand-roll a comparison). Add unit tests for a valid signature, a wrong secret, a missing prefix, and malformed hex.

- [x] T046 [US3] Add lookup and delivery-write helpers to `backend/crates/modules/integrations/src/queries.rs`: `find_connection_by_token_hash(pool, hash)` (returns connection id, tenant id, catalog slug, `is_active`) and `insert_delivery(tx, tenant_id, connection_id, payload)`.

- [x] T047 [US3] Add the `receive_webhook` handler to `backend/crates/modules/integrations/src/webhook.rs` for `POST /hooks/v1/{token}`. Take the raw body as `axum::body::Bytes` (needed for HMAC over the exact bytes). Order: hash the token and look up the connection → unknown ⇒ plain `404`, log nothing; inactive ⇒ `404` + `delivery_rejected`/`inactive_connection` event (throttled, see below); rate limit exceeded ⇒ `429` + `rate_limited` event (throttled); open the stored `signing_secret` with `crypto::open` and verify (T045) ⇒ on failure `404` + `invalid_signature` event; body not valid JSON ⇒ `422` + `malformed_payload` event; otherwise insert the delivery, insert a `delivery_accepted` event, and return `202 {"status":"accepted"}`. Every 404 branch must return a byte-identical body.

  **Throttle the two unauthenticated rejection events.** `rate_limited` and `inactive_connection` are reachable by anyone holding a stale or guessed token, so writing one event row per request would let a flood of rejected requests generate unbounded database writes — exactly when the system is trying to shed load. Before inserting either event, gate it through the same rate-limit store with a once-per-minute budget:

  ```rust
  let should_log = store.check(&format!("integration_evt:{connection_id}:{reason}"), 1,
                               std::time::Duration::from_secs(60));
  ```

  Insert the event only when `should_log` is true; always return the status code regardless. `invalid_signature` and `malformed_payload` are **not** throttled — they are only reachable after the connection resolves and stay bounded by the 60/min limit that already ran.

- [x] T048 [US3] Add the `list_integration_events` handler to `backend/crates/modules/integrations/src/routes.rs` for `GET /tenant/integrations/{slug}/events`, using `queries::list_events` with the cursor helpers from T010. Clamp `limit` to 1..=100 with a default of 50 and build the `{data, pagination}` response exactly like `build_response` in `backend/crates/modules/audit/src/routes.rs`.

- [x] T049 [US3] Mount the routes in `backend/crates/server/src/router.rs`: add `list_integration_events` to the tenant router with `require_permission(Permission::IntegrationsView)`, and create a `hooks_routes()` function for `POST /hooks/v1/{token}` mounted **outside** the tenant middleware. Copy the standalone-router pattern from `widget_routes()` (~line 98) and its merge into the app router (~line 937), adding a `RequestBodyLimitLayer` of 256 KB. **The rate-limit store must be injected**: at ~line 936 the existing `let rate_store = Arc::new(kernel::InMemoryRateLimitStore::default());` is layered only onto `widget_router` — change it to `.layer(Extension(rate_store.clone()))` there and layer `Extension(rate_store)` onto the hooks router too, otherwise T050's extractor fails at runtime on every delivery. No request-id work is needed: `request_id_middleware` and `trace_middleware` are applied to the outermost router (~line 1067), so the hooks route inherits them.

- [x] T050 [US3] Add the per-connection rate limit **inside the T047 handler** in `backend/crates/modules/integrations/src/webhook.rs`: 60 requests per 60 seconds keyed by connection id. Add `Extension(store): Extension<std::sync::Arc<kernel::InMemoryRateLimitStore>>` to the handler's extractors and call `store.check(&format!("integration_conn:{connection_id}"), 60, std::time::Duration::from_secs(60))`, returning `ApiError::rate_limited("Too many requests")` when it returns `false`. Copy this exact pattern from `send_message` in `backend/crates/modules/widgets/src/public_routes.rs:480-497`. **Do NOT** add a helper to `backend/crates/server/src/rate_limit.rs` and call it from this crate — the `server` crate depends on `integrations` (T003), so calling back into `server` is a circular dependency that will not compile. Module crates always call the `kernel` store directly; `server/src/rate_limit.rs` is for server-owned middleware only.

- [x] T051 [US3] Register the webhook and events DTOs in `backend/crates/server/src/openapi.rs` (`IntegrationEventDto`, `IntegrationEventListResponse`, `PaginationInfo`) so `openapi_coverage.rs` stays green.

- [x] T052 [US3] Create `backend/crates/modules/integrations/src/retention.rs` with `pub async fn sweep_expired(pool: &PgPool) -> sqlx::Result<u64>`, deleting rows older than 90 days from both `integration_events` and `integration_webhook_deliveries` and returning the total deleted. Copy the query style from the notification retention sweep.

- [x] T053 [US3] Spawn the retention sweeper in `backend/crates/server/src/main.rs`. Copy the `notification_retention_sweeper` block (~line 146) verbatim, swapping in `integrations::retention::sweep_expired` and the log message `"integration retention sweep: deleted expired rows"`, and add the handle to the same shutdown/join set as its neighbours.

### Tests for User Story 3

- [x] T054 [P] [US3] Create `backend/crates/server/tests/integrations_webhook.rs`: a correctly signed delivery returns 202 and creates a `delivery_accepted` event and one `integration_webhook_deliveries` row; a bad signature returns 404 and creates a `delivery_rejected`/`invalid_signature` event; an unknown token returns 404 with no new event rows; a delivery to a disconnected connection returns 404 and creates an `inactive_connection` event; assert the unknown-token and bad-signature response bodies are identical. Also cover the FR-014 limits: a body larger than 256 KB returns `413`, and the 61st correctly signed delivery within one minute returns `429` (send 60 first) — then assert that the burst produced **at most one** `rate_limited` event row, proving the throttling in T047 works.

- [x] T055 [P] [US3] Create `backend/crates/server/tests/integrations_events.rs`: seed more than one page of events, assert newest-first ordering, that `limit` is honoured and clamped, that following `next_cursor` returns the next page without overlap, and that the final page has `has_more: false`. Add the cross-tenant case required by FR-013/SC-005 (the events endpoint is a tenant-scoped read surface that T019 could not cover, because it does not exist until this phase): seed events for tenant A, then assert an admin of tenant B receives none of them.

- [x] T056 [US3] Add a status-derivation integration case to `backend/crates/server/tests/integrations_catalog.rs` (edits the file T017 created, so not parallel-safe with it): after three consecutive rejected deliveries the list and detail endpoints report `status: "error"`; after a subsequent accepted delivery they report `"connected"` again.

- [x] T057 [P] [US3] Add a retention unit test in `backend/crates/modules/integrations/src/retention.rs` (or `backend/crates/server/tests/integrations_events.rs` if a live DB is needed): rows older than 90 days are deleted from both tables and newer rows are kept.

### Frontend for User Story 3

- [x] T058 [US3] Add `IntegrationEventWire`, `IntegrationEventListWire`, the camelCase `IntegrationEvent` model, and `integrationEventListFromWire` to `frontend/apps/dashboard/src/app/core/api/tenant-api.models.ts`, mirroring the T020 additions.

- [x] T059 [US3] Add an `events(slug, cursor)` method to `frontend/apps/dashboard/src/app/features/tenant/integrations/integrations-api.service.ts`, and `events`/`nextCursor`/`hasMore`/`loadMoreEvents` state and methods to `integration-detail.store.ts` (copy the load-more pagination handling from `audit-logs.store.ts`).

- [x] T060 [US3] Render the event log in `frontend/apps/dashboard/src/app/features/tenant/integrations/integration-detail.component.ts`: a newest-first list of event type, outcome, failure reason, and relative time, with a "Load more" button bound to `loadMoreEvents` and the existing empty/loading state components. **Do not write a fourth `relativeTime` function** — Constitution Principle IX forbids duplicating UI logic, and it already exists file-locally in `shared/components/notification-list/notification-list.component.ts:8`, `features/tenant/conversations/inbox-list.component.ts`, and `features/tenant/overview/overview.component.ts`. Move that exact implementation into `frontend/apps/dashboard/src/app/shared/utils/relative-time.ts` as an exported `relativeTime(dateStr: string): string` and import it here. T061 migrates the three existing call sites.

**Verify US3**: `cd backend && cargo test -p integrations && cargo test -p server --test integrations_webhook --test integrations_events --test integrations_catalog` then `cd frontend && pnpm ng test dashboard`

**Checkpoint**: All three user stories are independently functional.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [x] T061 Frontend cleanup, two parts (runs after T042 and T060, which create the shared utilities). (a) Delete any now-unused integration entries from `frontend/apps/dashboard/src/app/shared/fixtures/` and remove the `IntegrationStatus` type from `fixture.models.ts` if nothing else references it (grep first). (b) Finish the Principle IX de-duplication started in T042/T060: replace the file-local `relativeTime` in `shared/components/notification-list/notification-list.component.ts:8`, `features/tenant/conversations/inbox-list.component.ts`, and `features/tenant/overview/overview.component.ts` with an import from `shared/utils/relative-time.ts`, and replace the inline `navigator.clipboard.writeText` in `features/tenant/widgets/widgets.component.ts:442` and `features/tenant/team/invite-dialog.component.ts` with `copyToClipboard` from `shared/utils/clipboard.ts`. Behaviour must be identical — run `pnpm ng test dashboard` afterwards, since all five files have existing specs.

- [x] T062 [P] Update `backend/crates/server/tests/openapi_coverage.rs` expectations if it asserts an endpoint count, and run `cargo test -p server --test openapi_coverage --test openapi_valid --test openapi_contract`.

- [x] T063 [P] Document the new `APP_INTEGRATION_SECRETS_KEY` environment variable in `README.md`, immediately alongside the existing `APP_AI_KEY_ENCRYPTION_KEY` entry (same format: base64, exactly 32 bytes, required for integration secrets).

- [x] T064 [P] Update `CLAUDE.md` "Recent Changes" with a `028-integrations-foundation` bullet, and add an "Integrations" section to `frontend/CLAUDE.md` documenting the new store/service/component locations (mirror the existing "Audit Logs" section).

- [x] T065 Run the full validation in [quickstart.md](quickstart.md): all backend suites with a live DB, then `cd frontend && pnpm ng test dashboard && pnpm ng build dashboard && pnpm lint && pnpm format:check`.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: no dependencies — start immediately.
- **Phase 2 (Foundational)**: needs Phase 1. **Blocks all user stories.**
- **Phase 3 (US1)**: needs Phase 2.
- **Phase 4 (US2)**: needs Phase 2. Touches `routes.rs` and the detail component created in US1, so run it after US1 rather than truly in parallel.
- **Phase 5 (US3)**: needs Phase 2, and needs US2's connect flow to produce a webhook token for manual testing. T045–T047 (webhook module) can be written in parallel with US2 work.
- **Phase 6 (Polish)**: after all desired stories.

### Within Each Story

Backend queries → backend handlers → router/openapi wiring → backend tests → frontend wire types → frontend service → frontend store → frontend component.

### Parallel Opportunities

- Phase 1: T003 and T004 in parallel (different files).
- Phase 2: T008, T009, T011, T012 in parallel (four separate files); T007 and T010 are sequential because T010 uses T007's types.
- Phase 3: T017, T018, T019 in parallel (three separate test files); T024 and T028 in parallel with the component work.
- Phase 4: T037, T038, T039 in parallel.
- Phase 5: T054, T055, T057 in parallel. **T056 is no longer parallel-safe** — it edits `integrations_catalog.rs`, which T017 created.
- Phase 6: T062, T063, T064 in parallel. T061 must run after T042 and T060, since it migrates call sites onto the shared utilities those two tasks create.

**Important — same-file collisions, never run these in parallel**:

- `backend/crates/modules/integrations/src/routes.rs`: T013, T014, T032, T033, T034, T035, T048
- `backend/crates/modules/integrations/src/queries.rs`: T010, T030, T046
- `backend/crates/modules/integrations/src/webhook.rs`: T031, T045, T047, T050
- `backend/crates/server/src/router.rs`: T015, T036, T049
- `backend/crates/server/src/openapi.rs`: T016, T036, T051
- `backend/crates/server/tests/integrations_rbac.rs`: T018, T039
- `backend/crates/server/tests/integrations_catalog.rs`: T017, T056
- `frontend/.../integrations-api.service.ts`: T021, T040, T059
- `frontend/.../integration-detail.store.ts`: T025, T041, T059
- `frontend/.../integration-detail.component.ts`: T026, T042, T043, T060
- `frontend/.../core/api/tenant-api.models.ts`: T020, T058

---

## Parallel Example: Phase 2

```bash
# These four touch four different files with no shared dependencies:
Task: "T008 Create crypto.rs by copying ai-providers/src/crypto.rs"
Task: "T009 Create status.rs with derive_status + unit tests"
Task: "T011 Create audit.rs with four audit writers"
Task: "T012 Add integrations entry to audit CATEGORY_PREFIXES"
```

---

## Implementation Strategy

### MVP First (User Story 1 only)

1. Phase 1 (Setup) → 2. Phase 2 (Foundational) → 3. Phase 3 (US1) → 4. **STOP and validate**: catalog browsable, RBAC enforced, tenant-isolated. Demo-ready.

### Incremental Delivery

1. Setup + Foundational → foundation ready
2. US1 → catalog visible (MVP)
3. US2 → connections live, secrets safe
4. US3 → webhooks flowing, health and logs visible
5. Polish → docs, fixtures, full gate

### Notes

- Run backend tests with a live database — `cargo test --workspace` skips DB tests and can abort early, so always use the narrow `--test <name>` invocations shown in the phase verify steps.
- `cargo fmt` is red repo-wide at HEAD for unrelated reasons; do not treat pre-existing formatting failures as a regression from this feature.
- Commit after each task or logical group.

---

## Phase 7: Convergence

**Why this phase exists**: T004 added a new field `integration_secrets_key` to the `AppConfig` struct in `backend/crates/shared/config/src/lib.rs`. In Rust, adding a field to a struct breaks every place that builds that struct with a full `{ ... }` literal. The integrations test files were written with the new field, but **52 pre-existing** `server` test files build `config::AppConfig { ... }` inline and were **not** updated. As a result `cargo test -p server` (the whole server test suite) no longer compiles — only the narrow `--test integrations_*` runs used in the phase verify steps still pass, which is why every task above shows as done. This regression violates Constitution Principle VII (Test-First & Regression).

- [x] T066 **CRITICAL — fix compile regression** per Constitution VII / plan.md Constitution Check row VII (contradicts). Add the missing struct field `integration_secrets_key` to the inline `config::AppConfig { ... }` literal in each of the 52 pre-existing `server` test files listed below.

  **Exactly what to do in each file:** find the block that starts with `config::AppConfig {` (or `AppConfig {`) and ends with its matching closing `}`. Add this one line, on its own line, anywhere inside that block — the simplest safe spot is immediately **before** the block's closing `}` (Rust struct-literal field order does not matter):

  ```rust
  integration_secrets_key: None,
  ```

  Match the surrounding indentation. Change nothing else. Some files contain the literal more than once (e.g. two `AppConfig { ... }` blocks) — if so, add the line to **every** `AppConfig { ... }` literal in the file. Use `None` (these tests do not exercise integration secrets); do **not** copy the base64 value used by `ai_key_encryption_key`.

  **Do NOT touch** these — they already set the field correctly: `backend/crates/shared/config/src/lib.rs`, `backend/crates/server/src/router.rs`, and the seven `backend/crates/server/tests/integrations_*.rs` files.

  **The 52 files to edit (all under `backend/crates/server/tests/`):**
  `ai.rs`, `ai_agent.rs`, `ai_agent_prompt.rs`, `analytics_api.rs`, `audit_logs.rs`, `auth.rs`, `conversation_summary.rs`, `conversations.rs`, `cors.rs`, `customer_view_no_tool_leak.rs`, `customers.rs`, `engine_fallback.rs`, `engine_isolation.rs`, `engine_respond.rs`, `engine_supersede.rs`, `engine_tool_approval_approve.rs`, `engine_tool_approval_cancel.rs`, `engine_tool_approval_deny.rs`, `engine_tool_approval_expiry.rs`, `engine_tool_approval_race.rs`, `engine_tool_chain.rs`, `engine_tool_isolation.rs`, `engine_tool_refusal.rs`, `errors.rs`, `escalations.rs`, `feedback_api.rs`, `health.rs`, `knowledge_base.rs`, `live_deps.rs`, `message_confidence.rs`, `notifications.rs`, `platform_tenants.rs`, `rag_degradation.rs`, `rag_indexing.rs`, `rag_reindex.rs`, `rbac.rs`, `team_members.rs`, `tenancy.rs`, `tenant_defined_tool_execution.rs`, `tenant_defined_tool_isolation.rs`, `tenant_tool_credential_confidentiality.rs`, `tool_activity_endpoint.rs`, `tool_audit_completeness.rs`, `tool_decide_endpoint.rs`, `tool_settings_crud.rs`, `tool_tighten_only.rs`, `tracing.rs`, `widget_admin_crud.rs`, `widget_conversation_flow.rs`, `widget_public_foundation.rs`, `widget_session_lifecycle.rs`, `widget_tenant_isolation.rs`.

  **Verify:** `cd backend && cargo check -p server --tests 2>&1 | grep -c "error\[E0063\]"` must print `0`, and `cargo check -p server --tests` must finish with no `E0063` errors. (Ignore pre-existing `cargo fmt` redness — it is unrelated per the note above. If unused-import warnings appear, leave them; this task is scoped to the missing field only.)
