# Implementation Plan: Production Hardening

**Branch**: `033-production-hardening` | **Date**: 2026-07-26 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/033-production-hardening/spec.md`

## Summary

Close the operational gaps between a feature-complete platform and one that can be
deployed and defended. Six workstreams: (1) replace per-process request throttling with a
Redis-backed shared limiter and extend it to sign-in and webhooks; (2) emit browser
security headers with a widget-specific exception; (3) turn the stub `/metrics` endpoint
into real Prometheus exposition, add OTLP trace export and Sentry error capture, and
define Grafana dashboards as code; (4) deploy to a single-VM staging host via a compose
profile driven by versioned scripts; (5) build self-hosted backup/restore with off-host
storage; (6) consolidate the two overlapping backend CI workflows into one pinned,
non-vacuous gate, add k6 load tests, and write the operator runbook plus a complete
configuration reference.

The dominant technical constraint is that **five modules currently take
`Extension<Arc<InMemoryRateLimitStore>>` and call a synchronous `check()`**. Redis makes
that call async, so FR-003 is a cross-module refactor behind a new trait, not a store swap.

The dominant scoping constraint is **spec 032**, which already ships a per-dependency
System Health page. This plan deliberately adds **no new dashboard page and no new
permission** — see the boundary in Technical Context below.

## Technical Context

**Language/Version**: Rust stable — currently unpinned; this feature adds
`backend/rust-toolchain.toml` (FR-036). Frontend TypeScript on Angular 22, pnpm 10.34.4
(currently split 10.12.4/10.34.4 across two CI jobs — consolidated here).

**Primary Dependencies**: Existing — Axum, Tokio, SQLx, `redis 0.28` (already a workspace
dependency with `tokio-comp` + `connection-manager`, currently used only by
`shared/cache` for `ping`), `tower-http` (already provides `CorsLayer`,
`DefaultBodyLimit`). New — `tower-http::set_header` (already in the dependency tree),
`opentelemetry` + `opentelemetry-otlp` + `tracing-opentelemetry`, `metrics` +
`metrics-exporter-prometheus`, `sentry` + `sentry-tracing`.

**Storage**: PostgreSQL 16 with pgvector ≥ 0.8.0 (self-hosted — the reason Option A was
chosen), Redis 7, MinIO for S3-compatible object storage. One new table
(`tenant_rate_limit_overrides`); no other schema change.

**Testing**: `cargo test --workspace` with `REQUIRE_DB_TESTS=1`, Karma for Angular unit
tests, Playwright for e2e (including the existing `test:e2e:real` real-backend suite),
k6 for load tests.

**Target Platform**: Linux x86-64. Local: Docker Compose. Staging: a single provisioned
virtual machine running the same compose stack under a deployment profile, behind Caddy
terminating TLS.

**Project Type**: Web application — Rust modular-monolith backend + two Angular
frontends (dashboard, widget), plus infrastructure and operations tooling.

**Performance Goals**: Sustain 200 concurrent virtual users against staging with error
rate < 1% and p95 request latency < 500 ms on dashboard reads (SC-014). Header and
rate-limit middleware must add < 5 ms p99 overhead.

**Constraints**: Single host — application, database, cache, and object store share one
machine's disk, memory, and network, so backup jobs must not destabilise the service and
backups must land off-host. No high availability. Rate-limit middleware sits on the hot
path for every request. Trace sampling must not drop errored requests (FR-020).

**Scale/Scope**: Staging-scale data. 49 functional requirements across 6 user stories.
Touches 5 backend modules for the limiter refactor, adds ~4 new backend source files,
1 migration, ~8 infrastructure/ops files, and 4 documentation files. **No new API
endpoints and no new frontend pages.**

### Boundary against spec 032 (decided, not incidental)

032 delivered `GET /platform/system-health` — point-in-time dependency reachability
(database, cache, queue, storage) with degraded/unhealthy verdicts and a 30-second
auto-refreshing page gated on `platform.diagnostics.view`. 033 covers a different axis:
**time-series measurements over a selectable range**, plus traces and grouped errors.

Resolution:

- **FR-018's "operator-facing health view" is satisfied by Grafana dashboards defined as
  code, not by a new Angular page.** 033 adds no route, no nav entry, and no permission.
  This is the design choice that keeps `authz/src/matrix.rs`,
  `server/tests/openapi_coverage.rs`, `server/tests/auth.rs`, and the dashboard sidebar
  specs green — see Test Impact below.
- **FR-015's "dependency health" measurement reads 032's existing probe results rather
  than re-probing.** 032 probes every 30 s; a metrics scrape re-probing every 15 s would
  double dependency load on the same single host for no added signal.
- **`/metrics` changes contract.** 032's clarification session recorded that `/health`,
  `/ready`, and `/metrics` "keep their current contract and stay untouched." 033's FR-015
  explicitly supersedes that for `/metrics` only. This is a deliberate, recorded contract
  change, not an oversight — see `research.md` §3 for the access-control decision that
  accompanies it. `/health` and `/ready` keep their contracts exactly.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design.*

| Principle | Assessment | Status |
|-----------|-----------|--------|
| **I. Enterprise Modular Monolith** | The shared limiter lives in `shared/cache` behind a kernel-owned trait; modules depend on the abstraction, not on Redis. No new cross-module data access. | PASS |
| **II. Multi-Tenant Isolation** | `tenant_rate_limit_overrides` carries `tenant_id` and is read through the existing tenant-scoped data-access path. Metrics are labelled by route and status, **never by `tenant_id`** — per-tenant labels would both leak tenant activity to anyone who can scrape and cause unbounded cardinality. | PASS |
| **III. Zero-Trust Security & RBAC** | **Deviation.** `/health`, `/ready`, and `/metrics` are unauthenticated today. See Complexity Tracking — `/metrics` is remediated, `/health` and `/ready` are justified. | DEVIATION — justified |
| **IV. AI Provider Independence** | Untouched. Provider latency/error metrics are emitted from the existing abstraction, adding no provider-specific code path. | PASS |
| **V. API-First & Contract Consistency** | 429 responses reuse the existing `ApiError`/`ErrorEnvelope` shape with a `Retry-After` header. `/metrics` contract change is documented in `contracts/`. | PASS |
| **VI. Observability by Default** | This feature *is* the principle's completion — request IDs and spans exist; export, metrics, and error capture are what's added. | PASS |
| **VII. Test-First & Regression Discipline** | Every FR maps to a test, and FR-039 adds a regression test for the CI system itself. **Deviation on ordering only**: `tasks.md` sequences several test tasks *after* the implementation they cover. See Complexity Tracking. | DEVIATION — justified (ordering only) |
| **VIII. Database Integrity & Migration Discipline** | One migration for `tenant_rate_limit_overrides`, indexed on `(tenant_id, scope)`. Restore procedure explicitly addresses schema-shape mismatch (FR-033). | PASS |
| **IX. Design System Discipline** | No new UI. Adding no page is what keeps this passing. | PASS |
| **X. Performance & Efficiency** | Limiter adds one Redis round-trip per limited request; mitigated by pipelining the INCR+EXPIRE and by a local pre-check. Metrics use lock-free atomic recorders. | PASS |

### Post-Design Re-Check (after Phase 1)

Re-evaluated against the completed `research.md`, `data-model.md`, and `contracts/`. The
verdicts above are unchanged, and Phase 1 tightened three of them:

- **Principle II** strengthened, not weakened: `contracts/metrics-exposition.md` now
  carries a *test-enforced* label constraint (no UUID-shaped label values in the
  exposition), so the "no tenant labels" rule is verified rather than merely intended.
- **Principle III**'s deviation narrowed: `/metrics` is remediated at the network edge and
  the two remaining unauthenticated probes carry no tenant data. No new unauthenticated
  surface was introduced by the design — notably, CSP violation reporting was rejected in
  `contracts/security-headers.md` specifically because it would add an unauthenticated
  write endpoint.
- **Principle IX** holds because FR-018 resolved to Grafana dashboards rather than an
  Angular page. Had it resolved the other way, this feature would have needed a new route,
  nav entry, and permission, and the three exact-inventory suites below would all have
  required updates.

Phase 1 surfaced one new obligation not visible at Phase 0: the existing
`X-Forwarded-For` read in `server/src/rate_limit.rs` is spoofable because nothing
sanitises the header. Caddy overwriting it at the edge is now a stated precondition of
FR-004's correctness (see `contracts/rate-limit-responses.md`), not an incidental
deployment detail.

No new violations. Gate passes.

### Test Impact (exact-inventory suites)

These suites assert complete inventories and redden on *any* addition. The design avoids
all of them, but the assumption must be verified during implementation rather than
trusted:

- `backend/crates/modules/authz/src/matrix.rs` — no new permission is added.
- `backend/crates/server/tests/openapi_coverage.rs` — no new route is added; the
  `/metrics` response body/content-type change may still need its OpenAPI annotation
  updated.
- `backend/crates/server/tests/auth.rs` — route/permission pairs unchanged.
- `frontend/apps/dashboard` sidebar specs — no nav entry added.
- `backend/crates/server/tests/cors.rs` and `tests/health.rs` — **will** need updating:
  new response headers appear on every response these tests assert against.

## Project Structure

### Documentation (this feature)

```text
specs/033-production-hardening/
├── plan.md              # This file
├── research.md          # Phase 0 — 9 technical decisions
├── data-model.md        # Phase 1 — one table + non-persisted structures
├── quickstart.md        # Phase 1 — validation scenarios per user story
├── contracts/           # Phase 1 — cross-cutting interface contracts
│   ├── security-headers.md
│   ├── rate-limit-responses.md
│   ├── metrics-exposition.md
│   └── deployment-scripts.md
├── checklists/
│   └── requirements.md  # From /speckit-specify
└── tasks.md             # Phase 2 output (/speckit-tasks — NOT created here)
```

### Source Code (repository root)

```text
backend/
├── rust-toolchain.toml                         # NEW — pins toolchain (FR-036)
├── crates/
│   ├── shared/
│   │   ├── kernel/src/rate_limit.rs            # MODIFIED — async RateLimiter trait
│   │   ├── cache/src/lib.rs                    # MODIFIED — Redis limiter impl
│   │   ├── cache/src/limiter.rs                # NEW — sliding-window counter
│   │   ├── config/src/lib.rs                   # MODIFIED — new env vars
│   │   └── observability/
│   │       ├── src/lib.rs                      # MODIFIED — real metrics()
│   │       ├── src/metrics.rs                  # NEW — recorder + middleware
│   │       ├── src/otlp.rs                     # NEW — trace export
│   │       └── src/errors.rs                   # NEW — Sentry capture
│   ├── server/src/
│   │   ├── rate_limit.rs                       # MODIFIED — async, login limiter
│   │   ├── security_headers.rs                 # NEW — header middleware
│   │   └── router.rs                           # MODIFIED — layer wiring
│   └── modules/
│       ├── identity/src/routes.rs              # MODIFIED — login throttle
│       ├── widgets/src/public_routes.rs        # MODIFIED — await limiter
│       ├── feedback/src/public_routes.rs       # MODIFIED — await limiter
│       ├── whatsapp/src/webhook.rs             # MODIFIED — await limiter
│       └── integrations/src/webhook.rs         # MODIFIED — await limiter
└── migrations/
    └── <ts>_tenant_rate_limit_overrides.sql    # NEW

infra/
├── docker-compose.yml                          # MODIFIED — profiles
├── docker-compose.staging.yml                  # NEW — staging overlay
├── Caddyfile                                   # NEW — TLS + header + /metrics ACL
├── Dockerfile.frontend                         # MODIFIED — ARG APP (dashboard|widget)
├── otel-collector-config.yaml                  # NEW
├── prometheus.yml                              # NEW
├── prometheus-alerts.yml                       # NEW — error/latency/dep/backup rules
├── alertmanager.yml                            # NEW — on-call receiver (FR-021)
├── promtail-config.yaml                        # NEW — log shipping (FR-014)
└── grafana/
    ├── dashboards/*.json                       # NEW — dashboards as code
    └── provisioning/datasources/*.yaml         # NEW — Prometheus + Loki datasources

ops/
├── deploy.sh                                   # NEW — promote a commit
├── rollback.sh                                 # NEW
├── backup.sh                                   # NEW — dump + object sync, off-host
├── restore.sh                                  # NEW
└── verify-backup.sh                            # NEW — restore drill

loadtest/
├── dashboard-read.js                           # NEW — k6
├── widget-conversation.js                      # NEW
└── webhook-ingest.js                           # NEW

docs/
├── deployment.md                               # NEW — promote/rollback runbook
├── backup-restore.md                           # NEW
├── configuration.md                            # NEW — every env var (FR-046)
└── runbook.md                                  # NEW — operational tasks

.github/workflows/
├── ci.yml                                      # DELETED — merged into backend.yml
├── backend.yml                                 # MODIFIED — gate + SHA-tagged image
│                                               #   push + release-manifest.json (FR-040)
├── gate-verification.yml                       # NEW — proves the gate fails (FR-039)
└── frontend.yml                                # MODIFIED — pnpm version unified
```

**Structure Decision**: Web application (backend + frontend), extended with three
top-level operational directories. `infra/` already exists and gains the staging overlay
and observability configuration. `ops/` and `loadtest/` are new and deliberately kept
outside `backend/` and `frontend/` — they are host-level operational tooling with no
compile-time relationship to either workspace, and burying them inside a language
workspace would misrepresent that. `docs/` already exists (currently holding
`superpowers/plans/`) and gains the operator-facing documentation set.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|--------------------------------------|
| **Principle III** — `/health` and `/ready` remain unauthenticated | The reverse proxy and container runtime must probe readiness *before* any credential material is in play, and an auth dependency in the liveness path means a failing auth backend makes every instance look dead, triggering a restart storm. Both endpoints return only `{"status"}` / per-dependency ok-or-error with no tenant data, counts, or identifiers. | Requiring a shared probe token was rejected: it adds a secret that must be distributed to the proxy and rotated, to protect a response that carries no sensitive information. Restricting to localhost was rejected for these two because the proxy and container healthchecks probe across the compose network, not the loopback. |
| **Principle III** — `/metrics` remains unauthenticated *at the application* | Prometheus scrape endpoints are conventionally unauthenticated; adding bearer auth to the exposition path complicates every scrape config. | Not fully accepted — remediated instead at the network edge: Caddy restricts `/metrics` to the compose-internal network and the loopback, so it is unreachable from the internet. This is the cheap correct answer specifically *because* the target is a single VM where Prometheus is a local process. Application-level auth remains available if the deployment shape later changes. |
| **Principle VII** — several test tasks are sequenced *after* the code they cover | Two of the three affected groups cannot be written first by construction. `tests/cors.rs` and `tests/health.rs` (T014, T015) are **existing, currently-passing** suites that go red only because new headers appear — there is no "write it failing first" state to reach. The four unit tests in `server/src/rate_limit.rs` (T023) are likewise existing tests being adapted from sync to `#[tokio::test]`. The genuinely new suites — T016, T030, T031, T047, T052 — are test-first-capable, and `tasks.md` instructs the implementer to write and run each *before* its implementation task, confirming it fails for the intended reason. | Renumbering the new suites ahead of their implementation was rejected because it conflicts with the stronger operational rule this task list runs under: `cargo build --workspace` must exit 0 after every task, and a test referencing a not-yet-existing function is a *compile* error, not a failing assertion. The deviation is recorded rather than hidden so a later `/speckit-analyze` does not re-raise it. |
| **Cross-module refactor** — 5 modules change to satisfy FR-003 | Redis I/O is async; the existing `InMemoryRateLimitStore::check()` is a sync `Mutex` lookup called directly in handlers. There is no way to make one shared budget without changing the call sites. | Keeping the sync signature and blocking on Redis inside it was rejected — it would block the Tokio worker thread on every limited request. A background-synced local cache was rejected as eventually-consistent, which cannot satisfy SC-002's "within 10% of the documented budget". |
