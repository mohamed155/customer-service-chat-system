# Implementation Plan: AI Provider Abstraction

**Branch**: `015-ai-provider-abstraction` | **Date**: 2026-07-15 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/015-ai-provider-abstraction/spec.md`

## Summary

Give the platform a provider-independent AI layer: a uniform chat-completion capability (blocking and streamed) that resolves per tenant which vendor/model/credential to use, calls OpenAI, Anthropic, or Gemini through interchangeable adapters, retries transient failures and fails over through an ordered fallback chain, records every vendor-reaching call as an append-only usage record (metadata-only by default, full content for tenants that opt in), and exposes an audited, role-guarded admin API for configurations and encrypted API keys. Consuming modules see only the abstraction — no vendor names, wire formats, or SDK types leak out (SC-005).

Technical approach: the two placeholder crates graduate. `backend/crates/ai-providers` becomes the pure vendor layer — a `ChatProvider` trait (chat request in, uniform completion or chunk stream out), a normalized `ProviderError` taxonomy, and three adapters (OpenAI Chat Completions, Anthropic Messages, Gemini `generateContent`) built on `reqwest` with per-adapter base-URL injection so tests can point them at a local mock server; it has no database, tenancy, or business knowledge. `backend/crates/modules/ai` becomes the AI application module and owner of three new tables (`ai_configurations`, `ai_credentials`, `ai_usage_records` — migrations 0038–0040): it resolves configuration (tenant override → platform default) and credential (tenant BYOK → platform key) independently per FR-004, runs the bounded-retry-then-ordered-failover policy (FR-017), encrypts keys with AES-256-GCM under an environment-supplied master key (FR-008), writes usage records with actual-provider attribution and optional content capture (FR-010/FR-018), and mounts the admin routes (tenant surface under `ai_agent.view/manage`, platform surface under `platform.admin` — zero new permission codes). Consuming modules call the in-process `ai::AiService` handle added to `AppState`; there is no HTTP hop for completions, which is what keeps FR-016 (the layer touches no business data) and Constitution IV enforceable at the crate-dependency level. This feature is API-only: no dashboard changes.

## Technical Context

**Language/Version**: Rust (Cargo workspace, `edition = "2021"` per `backend/Cargo.toml` `workspace.package`); no frontend changes this feature

**Primary Dependencies**: Axum (admin routes via the existing deny-by-default `.guarded()` builder), SQLx/PostgreSQL, existing `authz`/`tenancy`/`identity` crates and `tenancy::audit::record_in_tx` audit helper, `shared/config` `AppConfig` (new env fields). New workspace dependencies: `reqwest` (rustls-tls, `json`, `stream`) for vendor HTTP, `aes-gcm` for credential encryption at rest, `wiremock` (dev-dependency) for adapter and failover tests. Vendor SSE streams are decoded by a small hand-rolled `data:`-line parser inside `ai-providers` (no eventsource crate — see research R3)

**Storage**: PostgreSQL — migration `0038_ai_configurations.sql` (`ai_configurations`: nullable `tenant_id` where NULL = the single platform default, provider CHECK against the fixed catalog, model, generation params, ordered `fallbacks JSONB`, per-tenant `capture_content` flag, partial unique indexes enforcing one live config per tenant and one live platform default); `0039_ai_credentials.sql` (`ai_credentials`: nullable `tenant_id`, provider, AES-GCM `ciphertext`+`nonce`, `key_hint` last-4 mask, one live key per (scope, provider)); `0040_ai_usage_records.sql` (`ai_usage_records`: append-only, NOT NULL `tenant_id`, actual provider/model served, nullable token counts (NULL = vendor did not report), status + normalized error category, latency, request-id, nullable captured `request_content`/`response_content` JSONB, `(tenant_id, created_at)` index for period queries). Redis unused

**Testing**: `cargo test` — unit tests in `ai-providers` against `wiremock` (per-adapter request mapping, response/usage parsing, stream decoding, error normalization for 401/429/5xx/timeout); unit tests in `modules/ai` for resolution precedence, retry/failover ordering, encryption round-trip, and masking; SC-005 guard: a test-only fourth `ChatProvider` impl driven through `AiService` without touching any other crate; integration suite `backend/crates/server/tests/ai.rs` (DB-gated via the existing `DATABASE_URL`/`REQUIRE_DB_TESTS` pattern) covering config CRUD + audit rows, key set/rotate/delete + masking + never-in-logs, cross-tenant isolation matrix, usage recording/totals, content-capture off/on, and wiremock-backed end-to-end failover (SC-008); live vendor smoke test gated on `LIVE_AI_OPENAI_KEY` etc. (SC-001); `rbac.rs` route→permission additions; `shared/db/tests/schema.rs` assertions for 0038–0040

**Target Platform**: Linux server (backend only)

**Project Type**: Web application backend — existing Cargo workspace; API-only feature (dashboard UI deferred per spec Assumptions)

**Performance Goals**: Configuration/credential resolution adds one indexed query pair per AI call (single-digit ms) ahead of vendor latency, which dominates; streamed first-increment forwarded as soon as the vendor emits it (SC-006 — no buffering of the full reply); usage-record insert happens after the response is returned/stream ends so it never sits on the caller's latency path; retry backoff bounded (2 retries: ~200 ms, ~1 s + jitter) so worst-case failover across 2 fallbacks stays under ~10 s before the normalized error surfaces

**Constraints**: Vendor logic confined to `ai-providers` (SC-005/FR-001/FR-003 — adding a provider = one adapter file + one registry entry); `modules/ai` never reads or writes business tables (FR-016, Constitution IV); keys encrypted at rest, never in logs/traces/audit/errors, never retrievable in full (FR-008 — enforced by a `SecretKey` newtype with redacted `Debug`/no `Serialize`); prompt/reply content never in logs/traces regardless of capture setting (FR-018); all config/key writes audited via `tenancy::audit::record_in_tx` (Constitution III); deny-by-default `.guarded()` routing, cross-tenant access → `not_found`; schema changes via migrations only (Constitution VIII); no mid-stream failover — failover only before the first delivered increment (spec edge case)

**Scale/Scope**: 3 migrations, 3 new tables; 0 new permission codes; ~10 admin endpoints (7 tenant + 3 platform); 2 crates graduate from placeholder (`ai-providers`: ~6 source files; `modules/ai`: ~8 source files); 2 new `AppConfig` fields (`APP_AI_KEY_ENCRYPTION_KEY`, optional vendor base-URL overrides for tests); audit vocabulary +5 actions (`ai_config.updated`, `ai_config.deleted`, `ai_credential.set`, `ai_credential.deleted`, `ai_config.capture_content_changed`); 1 new integration test suite + additions to `rbac.rs` and `schema.rs`

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Assessment | Status |
|-----------|------------|--------|
| I. Enterprise Modular Monolith | Clean two-layer ownership: `ai-providers` (vendor adapters, no DB/tenancy) ← `modules/ai` (config, keys, usage, policy, routes). Consuming modules depend only on `modules/ai`'s public `AiService`; no module imports `ai-providers` directly. Either layer could be extracted (the provider layer even to a sidecar) without touching callers | ✅ Pass |
| II. Multi-Tenant Isolation | `ai_usage_records.tenant_id` NOT NULL; `ai_configurations`/`ai_credentials` use nullable `tenant_id` **only** to represent the platform-default scope — every tenant-facing query filters on the middleware-resolved tenant, usage queries are tenant-scoped, and cross-tenant reads answer `not_found`. Resolution can never pick another tenant's BYOK: credential lookup is `(tenant_id = ctx) OR (tenant_id IS NULL)` with tenant-first precedence | ⚠️ Justified deviation (nullable `tenant_id` for platform scope — see Complexity Tracking) |
| III. Zero-Trust Security & RBAC | Every route via `.guarded()`: tenant surface reuses `ai_agent.view`/`ai_agent.manage`, platform surface `platform.admin` — no matrix changes. Keys AES-256-GCM encrypted under an env master key (never in source), masked to last-4 on read, `SecretKey` type is non-serializable with redacted Debug; config changes, key set/rotate/delete, and capture-content toggles all write audit rows excluding secret material (FR-007/FR-008) | ✅ Pass |
| IV. AI Provider Independence & Tool-Mediated Access | This feature *implements* the provider-independence half of Principle IV: OpenAI/Anthropic/Gemini interchangeable behind one contract, a 4th provider = adapter + registry entry only (SC-005 test enforces it). The AI layer takes only caller-supplied messages and touches no business data (FR-016), preserving tool-mediated access for the future AI runtime | ✅ Pass |
| V. API-First & Contract Consistency | Admin endpoints in `contracts/rest-api.md` with the standard envelope, error vocabulary, and cursor pagination on usage listing; config PUT and credential PUT are idempotent (full-replace semantics); the internal consuming contract is versioned in `contracts/provider-contract.md` | ✅ Pass |
| VI. Observability by Default | Request-id propagates into vendor calls and onto usage records; every attempt/retry/failover emits structured trace events with provider, category, and latency (FR-015/FR-017); credentials and message content are excluded from logs and traces by construction (fields never handed to `tracing`) | ✅ Pass |
| VII. Test-First & Regression Discipline | Unit (adapters, normalization, resolution, failover, crypto), integration (CRUD/RBAC/isolation/usage/capture/failover via wiremock), schema assertions, rbac map, plus a live vendor smoke test for SC-001; SC-005 has a dedicated compile-and-run guard test | ✅ Pass |
| VIII. Database Integrity & Migration Discipline | Migrations 0038–0040 only; UUID PKs, timestamps per 005 conventions; partial unique indexes enforce one live config/credential per scope; `(tenant_id, created_at)` index backs the production usage query path; `ai_usage_records` is append-only and omits `updated_at`/`deleted_at`/`set_updated_at` like `audit_logs`/`messages` (see Complexity Tracking) | ⚠️ Justified deviation |
| IX. Design System Discipline | No UI in this feature (API-only per spec Assumptions) | ✅ Pass (N/A) |
| X. Performance & Efficiency | Streaming is first-class through the contract (SC-006); resolution is two indexed single-row lookups, usage writes are single inserts off the latency path; no N+1 (usage totals are one aggregate query); bounded backoff keeps failover latency capped | ✅ Pass |

**Initial gate**: PASS — two justified deviations recorded in Complexity Tracking.

**Post-design re-check (after Phase 1)**: PASS — design artifacts introduce no new deviations. Nuanced calls, all grounded in the spec: (1) content capture is a column on the tenant's `ai_configurations` row rather than a separate settings table — it is edited through the same audited config surface FR-018 requires, and a tenant with no override row toggles capture by creating an override (platform default never captures); (2) captured content is returned only from the usage **detail** endpoint guarded by `ai_agent.manage`, while the list endpoint stays metadata-only under `ai_agent.view` — "readable only by authorized roles" made concrete; (3) fallbacks are an ordered JSONB array on the configuration rather than a child table — the list is small (≤3 validated entries), read whole-row at resolution time, and never queried relationally.

## Project Structure

### Documentation (this feature)

```text
specs/015-ai-provider-abstraction/
├── plan.md                  # This file
├── research.md              # Phase 0 output
├── data-model.md            # Phase 1 output
├── quickstart.md            # Phase 1 output
├── contracts/
│   ├── rest-api.md          # Tenant + platform admin endpoints, payloads, errors, audit actions
│   ├── provider-contract.md # Internal contract: ChatRequest/Completion/StreamEvent, ChatProvider trait, error taxonomy, AiService
│   └── permissions.md       # Reused permission codes, route→permission map
└── tasks.md                 # Phase 2 output (/speckit-tasks — NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
backend/
├── migrations/
│   ├── 0038_ai_configurations.sql      # NEW — ai_configurations (platform default + tenant overrides, fallbacks, capture flag)
│   ├── 0039_ai_credentials.sql         # NEW — ai_credentials (encrypted keys, platform + BYOK scopes, masked hint)
│   └── 0040_ai_usage_records.sql       # NEW — ai_usage_records (append-only, attribution, nullable counts, content columns)
└── crates/
    ├── ai-providers/
    │   ├── Cargo.toml                  # MODIFIED — reqwest, serde, futures, tokio, async-trait, thiserror; wiremock (dev)
    │   └── src/
    │       ├── lib.rs                  # MODIFIED — module docs (Purpose/Responsibilities/Interfaces/Extension points), exports
    │       ├── contract.rs             # NEW — ChatRequest/Message/Role, ChatCompletion, StreamEvent, TokenUsage,
    │       │                           #        ProviderError taxonomy, ChatProvider trait (complete + stream)
    │       ├── registry.rs             # NEW — ProviderKind fixed catalog (streaming capability), registry: kind+base_url → adapter
    │       ├── sse.rs                  # NEW — minimal SSE data-line decoder shared by all three adapters
    │       ├── openai.rs               # NEW — OpenAI Chat Completions adapter (blocking + SSE stream, usage via stream_options)
    │       ├── anthropic.rs            # NEW — Anthropic Messages adapter (x-api-key/anthropic-version, event-typed SSE)
    │       └── gemini.rs               # NEW — Gemini generateContent / streamGenerateContent?alt=sse adapter
    ├── modules/
    │   └── ai/
    │       ├── Cargo.toml              # MODIFIED — ai-providers, axum, sqlx, serde, tokio, aes-gcm, authz, tenancy, kernel, config
    │       └── src/
    │           ├── lib.rs              # MODIFIED — module docs, exports (AiService, routes, model types)
    │           ├── model.rs            # NEW — AiConfiguration, FallbackEntry, CredentialRef, UsageRecord, payload/validation types
    │           ├── crypto.rs           # NEW — AES-256-GCM seal/open, SecretKey newtype (redacted Debug, no Serialize), hint derivation
    │           ├── resolution.rs       # NEW — per-request config + credential resolution (tenant → platform → NotConfigured)
    │           ├── service.rs          # NEW — AiService: complete/stream entry points, retry+failover policy, attribution
    │           ├── usage.rs            # NEW — usage-record writes (incl. capture), tenant usage list/summary queries
    │           ├── routes.rs           # NEW — tenant + platform admin handlers (config CRUD, credential set/delete, test, usage)
    │           └── audit.rs            # NEW — ai_config.* / ai_credential.* audit helpers via tenancy::audit::record_in_tx
    ├── shared/
    │   ├── config/src/lib.rs           # MODIFIED — APP_AI_KEY_ENCRYPTION_KEY (required outside test), optional base-URL overrides
    │   └── db/tests/schema.rs          # MODIFIED — 0038–0040 schema assertions (CHECKs, partial uniques, indexes)
    └── server/
        ├── src/
        │   ├── router.rs               # MODIFIED — tenant AI routes under mount_tenant, platform AI routes under mount_platform
        │   ├── state.rs                # MODIFIED — AiService (registry + master key) constructed into AppState
        │   └── main.rs                 # MODIFIED — wire AppConfig AI fields into AiService construction
        └── tests/
            ├── rbac.rs                 # MODIFIED — new routes in the route→permission map
            └── ai.rs                   # NEW — config/key CRUD+audit, masking, isolation matrix, usage+capture,
                                        #        wiremock failover end-to-end, live vendor smoke test (env-gated)
```

**Structure Decision**: The existing placeholder crates map exactly onto the two layers the spec demands. `ai-providers` stays a leaf crate with zero project dependencies (only vendor HTTP concerns), so SC-005 is enforced by the dependency graph, not convention: business modules cannot reach vendor types because only `modules/ai` links the crate. `modules/ai` owns all three tables and every policy decision (resolution, failover, capture, audit) and exposes two surfaces — the in-process `AiService` for consuming modules (the future AI runtime) and HTTP admin routes mounted through the standard guarded builders (`mount_tenant` for tenant config/keys/usage, `mount_platform` for platform defaults). No frontend changes; the dashboard AI-settings UI is a later spec.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| `ai_configurations`/`ai_credentials` allow `tenant_id IS NULL` (Principle II says every tenant-owned table carries `tenant_id`) | The spec's hybrid model (FR-004, Assumptions) requires platform-scoped rows — a platform default configuration and platform default keys per provider — that belong to no tenant by definition; partial unique indexes keep exactly one live row per scope | A separate `platform_ai_configurations` table pair would duplicate schema, queries, validation, and admin handlers for rows that differ only in scope; resolution would need UNION queries instead of one ordered lookup. Rows with `tenant_id` set behave exactly per Principle II (tenant-scoped queries, isolation tests in the matrix) |
| `ai_usage_records` omits `updated_at`/`deleted_at`/`set_updated_at` (deviation from 005 table conventions) | Usage records are append-only by requirement (Key Entities: "append-only record of one AI call"); they are never updated or soft-deleted, and correction semantics are intentionally absent so billing raw material stays trustworthy | Carrying mutability columns on an immutable ledger invites accidental writes and forces every reader to filter `deleted_at IS NULL` for no benefit; mirrors the `audit_logs`/`messages` precedent for append-only tables |
