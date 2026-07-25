# Phase 0 Research: Platform Analytics & System Health

**Feature**: 032-platform-analytics-health | **Date**: 2026-07-25

All decisions below were verified against source at HEAD, not inferred from prior plans.

---

## R1. Storage reachability probe mechanism

**Context**: FR-017 (as clarified) requires a read-only round-trip probe — "a metadata or listing operation" — that never writes. The current abstraction cannot express one.

**Verified**: `crates/shared/storage/src/lib.rs:23-27` — `ObjectStorage` is exactly `put` / `get` / `delete`. There is no `head`, `list`, or metadata operation. Implementors are only three: `S3Storage` (`:67`), `InMemoryStorage` (`:134`), and a `FailingStorage` test double in `crates/modules/knowledge/src/upload.rs:505`.

**Decision**: Extend the trait with a purpose-named `probe(&self) -> Result<(), StorageError>` method.

- `S3Storage::probe` → `HeadBucket` on the configured bucket. This is a metadata call: no object read, no write, nothing left behind.
- `InMemoryStorage::probe` → `Ok(())` unconditionally.
- `FailingStorage` (test double) → returns an error, so the unhealthy path stays covered.

**Rationale**: FR-017 specifies a metadata/listing operation. `HeadBucket` is precisely that; it validates endpoint reachability, credentials, and bucket existence in one round trip, and it is the canonical S3 health check. The trait change is genuinely small (3 implementors, one of them a test).

**Alternative considered and rejected**: `get()` on a reserved key, treating `NotFound` as success — no trait change needed. Rejected on two grounds. First, `S3Storage::get` (`lib.rs:85-101`) distinguishes `NotFound` only by string-matching `e.to_string().contains("NoSuchKey")`; health correctness would then rest on an AWS SDK message format that no test pins. Second, S3 returns `403 AccessDenied` rather than `404` for a missing key when the caller lacks `s3:ListBucket`, so a correctly configured deployment could report unhealthy. A `get` is also an object read, not the metadata operation the requirement names.

**Consequence to state plainly**: `InMemoryStorage::probe` always succeeds, so the probe alone can never catch the silent fallback. **FR-016 (backend identity) is the requirement that actually catches it** — the probe only proves an already-selected backend is reachable. This is the same limit FR-017a already concedes for readable-but-unwritable buckets.

---

## R2. Storage backend identity (FR-016)

**Verified**: the S3-or-fallback selection exists **twice** and neither copy records which branch it took:

- `crates/server/src/main.rs:85-95`
- `crates/server/src/router.rs:998-1010` (this copy additionally calls `tokio::runtime::Handle::current().block_on(...)` inside an async context)

Both only emit `tracing::warn!` on fallback. `AppState` (`crates/server/src/state.rs`) has no storage field; storage reaches handlers solely through `.layer(Extension(storage.clone()))` at `router.rs:1047`.

**Decision**: Centralize selection into one function in the `storage` crate returning both the `Arc<dyn ObjectStorage>` and a `StorageBackend { kind: S3 | InMemory, s3_configured: bool }` descriptor. Both call sites use it; the descriptor is layered as its own `Extension`.

**Rationale**: FR-016 requires reporting the backend actually in use and flagging `kind == InMemory && s3_configured == true` as unhealthy. That is unimplementable while the choice exists only as a log line. Deduplicating the two call sites is therefore **in scope as a prerequisite**, not opportunistic cleanup — with two copies, the two entry points could disagree about what is running.

---

## R3. Handler dependency injection on platform routes

**Verified**: `platform_routes` returns `OpenApiRouter<sqlx::PgPool>` (`router.rs:262`), so `State` extraction yields the pool only. `health_checks: Vec<Arc<dyn HealthCheck>>` lives in `AppState` (`state.rs:12`) and has no `FromRef` impl. Cross-cutting dependencies are already delivered by `Extension` layers attached after the platform/tenant merges (`router.rs:1046-1051`: `email_sender`, `storage`, `config`, `escalations`, `ai`).

**Decision**: Add `.layer(Extension(state.health_checks.clone()))` and `.layer(Extension(storage_backend.clone()))` alongside the existing extensions. Diagnostics handlers take `State(pool): State<PgPool>` plus `Extension<...>` for the rest.

**Rationale**: Follows the established pattern rather than inventing one; no restructuring of the router's state type.

**Ordering trap to avoid**: those layers are attached to the merged router, so they apply everywhere. A new `Extension` attached *inside* `platform_routes` would not compose the same way. Attach at the same site as the existing ones.

---

## R4. Percentile computation (FR-020) under SC-003

**Context**: FR-020 requires median and p95 latency per provider. The Q2 clarification removed caching and rollups as options, so this must be a live query inside the 2-second budget over ~1,000,000 rows.

**Decision**: Compute exact percentiles with Postgres ordered-set aggregates — `percentile_cont(0.5)` and `percentile_cont(0.95) WITHIN GROUP (ORDER BY latency_ms)` — grouped by provider, over a **time-bounded window only**, backed by a new index leading with `created_at` (see R5).

**Rationale**: The provider window defaults to 24h and offers at most 7 days (FR-025). The percentile sort therefore operates on the rows in that window, not the full table — at realistic volumes that is a small fraction of 1M. Ordered-set aggregates sort rather than stream, so the governing factor is how many rows the window admits, which the index bounds directly.

**Guard rail**: SC-003's budget is stated against 1,000,000 total records. If a 7-day window over a much larger table pushes the sort past budget, the escalation path is explicit in the spec's Assumptions ("a finding to escalate at plan time rather than something to silently paper over with a cache"). Approximate percentiles (e.g. t-digest) and narrowing the maximum window are the two levers; neither is adopted now.

**Alternative considered and rejected**: precomputing latency buckets on write. Rejected — it changes an append-only table's write path for a read-side convenience, and Q2 explicitly chose live queries.

---

## R5. Index design

**Verified current state**:

- `ai_usage_records_tenant_created_idx ON ai_usage_records (tenant_id, created_at DESC)` (migration `0040`). Leads with `tenant_id`; every query in this feature is deliberately cross-tenant and filters on `created_at` alone, so this index does not serve them.
- `outbox_events` (migration `0002`) has only `outbox_invitation_delivery_pending_idx ON outbox_events (created_at, id) WHERE event_type = 'invitation.email_delivery' AND processed_at IS NULL` (migration `0021`). Scoped to a single `event_type`; queue health needs **all** unprocessed events regardless of type, so this partial index does not serve it either.
- `tenants` has `(deleted_at, status, id)` filtered on `deleted_at IS NULL` (migration `0017`) — adequate for FR-006/FR-010 counts.
- `conversations` has `(tenant_id, status, last_activity_at DESC, id DESC)` (migration `0033`) — leads with `tenant_id`; the cross-tenant active count (FR-007) needs a status-leading path.

**Decision** — new migration `0060_platform_analytics_indexes.sql` (next after max; note `0055` and `0058` are absent, so take next-after-max rather than assuming contiguity), additive only, no schema mutation:

| Index | Serves |
|---|---|
| `ai_usage_records (created_at DESC)` | FR-008 usage totals, FR-020/FR-021 per-provider aggregates and percentiles, FR-024 recent failures — all windowed by time across every tenant |
| `outbox_events (created_at) WHERE processed_at IS NULL` | FR-014 backlog count and oldest-wait age across all event types |
| `outbox_events (attempts) WHERE processed_at IS NULL` | FR-015 stuck-retry count (`attempts > 5`) |
| `conversations (status) WHERE deleted_at IS NULL` | FR-007 platform-wide active conversation count |

Exact column lists and partial predicates are specified in `data-model.md`.

**Constitution note**: Principle VIII makes indexes mandatory on production query paths, and Principle X treats N+1 and slow scans as defects. These indexes are the mechanism by which SC-003 is met, given caching and rollups were rejected.

---

## R6. Permission wiring

**Verified**: `platform.diagnostics.view` is **not** unused, contrary to the initial reading. It appears at:

- `crates/server/src/router.rs:385-387` — a test-only route `/test/platform/diagnostics/view`, gated behind `include_test_routes`, so it never reaches the documented OpenAPI or production.
- `crates/server/tests/rbac.rs:104,122,956,962` — already in `PLATFORM_OPERATIONS` and `PLATFORM_OPERATIONS_DENY_BY_DEFAULT`.
- `crates/server/tests/auth.rs:245`, `matrix.rs:66,72`, `permission.rs:59-60,125,162,232`.

There is **no production endpoint** behind it.

**Decision**: Reuse `PlatformDiagnosticsView` unchanged for health + provider endpoints (grant stays Super Admin + Developer, per FR-002). Add `PlatformAnalyticsView` (`platform.analytics.view`) granted to all five platform role arrays, per FR-001.

**Why this is good news**: the guard is already proven end-to-end for the diagnostics permission by existing RBAC tests, and `rbac.rs`'s inventory arrays give an established entry pattern to extend rather than invent.

**Exact-inventory files that must be updated together** (these fail loudly at the end of implementation if missed):

| File | Change |
|---|---|
| `backend/crates/modules/authz/src/permission.rs` | enum variant + `serde(rename)` + `as_str()` arm + `ALL` array |
| `backend/crates/modules/authz/src/matrix.rs` | add to `PLATFORM_ALL`, `PLATFORM_DEVELOPER`, `PLATFORM_TENANT_ACCESS`, `PLATFORM_SUPPORT`, `PLATFORM_FINANCE` |
| `backend/crates/server/tests/auth.rs` | permission-string list (~line 245) |
| `backend/crates/server/tests/rbac.rs` | `PLATFORM_OPERATIONS` + `PLATFORM_OPERATIONS_DENY_BY_DEFAULT` |
| `backend/crates/server/tests/openapi_coverage.rs` | `EXPECTED` inventory — three new paths |
| `frontend/.../core/authz/permissions.ts` | `Permission` union member + `PAGE_PERMISSIONS` entries |

---

## R7. Module placement

**Verified**: `crates/modules/platform/src/lib.rs` is a 1-line placeholder (`//! Placeholder module crate for the M0 workspace skeleton.`). The `analytics` crate (`model.rs`/`queries.rs`/`routes.rs`) is tenant-scoped: every query takes `ctx.tenant_id` from `TenantContext`.

**Decision**: Activate the `platform` crate as the home for this feature (`model.rs`, `queries.rs`, `health.rs`, `routes.rs`). Do **not** extend the tenant `analytics` crate.

**Rationale**: Principle I requires module boundaries drawn so a module could be extracted later. The tenant analytics crate's whole contract is tenant-scoped; adding cross-tenant aggregates to it would blur exactly the boundary Principle II protects, and would put a permission-gated platform surface inside a tenant module. The `platform` crate reads aggregate counts from tables owned by other modules — a deliberate, documented read-only coupling recorded in the plan's Constitution Check (Principle I), consistent with how the `analytics` crate already reads conversation and usage tables it does not own.

---

## R8. Relationship to existing public probes

**Verified**: `/health`, `/ready`, `/metrics` are mounted on the bare router outside `api_routes` (`router.rs:1117-1119`) — unauthenticated by construction. `readiness()` (`observability/src/health.rs:53`) already applies a per-check `tokio::time::timeout` and maps failures to `CheckStatus::Error` with a message.

**Decision**: Build a new permission-gated endpoint; leave `/health`, `/ready`, `/metrics` untouched (FR-019a). Reuse the existing `HealthCheck` trait objects for database and cache rather than writing new probes, and layer the three-state verdict on top in the `platform` crate.

**Rationale**: Orchestrators consume `/ready`'s binary contract and status codes; changing it risks restart loops. Reusing the `HealthCheck` trait objects avoids two implementations of the same probe (FR-018's bounded-timeout behavior comes free, since `readiness()` already enforces it).

**Mapping**: `CheckStatus::Ok` → healthy; `CheckStatus::Error` → unhealthy. Database and cache have no degraded band (recorded in the spec's Assumptions); only queue and storage produce `degraded`, from the FR-014/FR-015 thresholds and FR-016 backend identity respectively.

---

## R9. Frontend composition

**Verified** shared components already exist under `frontend/apps/dashboard/src/app/shared/components/`: `metric-card`, `trend-chart`, `sparkline`, `breakdown-bars`, `status-badge`, `data-table`, `select-filter`, `empty-state`, `loading-state`, `dashboard-card`, `section-header`, `toolbar`.

**Decision**: Compose all three pages from existing shared components. Add exactly one new shared component — a `health-indicator` presentational component rendering a three-state verdict with **icon + text label**, not colour alone (FR-027, SC-006) — because it is reused across the System Health page, the AI Provider Status page, and per-dependency rows.

**Rationale**: Principle IX mandates reusable components before pages and forbids duplicated UI logic. The verdict indicator is the one genuinely new visual primitive; everything else already exists.

**Route/nav**: `overview-placeholder` is replaced in place, retaining its path (spec Assumptions) so `breadcrumb.spec.ts:88-102` keeps passing. Two new paths join `APP_PATHS.platform` and `platform-nav.component.ts` destinations.

---

## Resolved unknowns

No `NEEDS CLARIFICATION` markers remain. The four `/speckit-clarify` decisions (permission split, live-query strategy, read-only storage probe, separate authenticated endpoint) are carried into R1–R8 above, and the two that turned out to have implementation consequences the clarification did not anticipate — R1 (trait lacks a metadata op) and R2 (fallback duplicated, identity unrecorded) — are resolved here rather than deferred to implementation.
