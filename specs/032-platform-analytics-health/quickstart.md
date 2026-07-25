# Quickstart: Platform Analytics & System Health (032) — validation guide

Proves the feature end-to-end. Contracts: [contracts/platform-analytics-api.md](contracts/platform-analytics-api.md); data model: [data-model.md](data-model.md); decisions: [research.md](research.md).

## Prerequisites

- PostgreSQL up with migrations applied through `0060_platform_analytics_indexes.sql` (`cd backend && sqlx migrate run`).
- Backend: `cd backend && cargo run -p server` (API at `http://localhost:<port>/api/v1`).
- Frontend: `cd frontend && pnpm ng serve dashboard`.
- Seeded users covering the permission split: one **Super Admin** or **Developer** (has `platform.diagnostics.view`), one **Sales**, **Support**, or **Finance** platform user (has `platform.analytics.view` but *not* diagnostics), and one tenant user with no platform role. Login sets the `app_session` cookie; platform calls send **no** `X-Tenant-ID`.
- Seed data spanning at least two tenants, with conversations in `open`/`pending`/`resolved` and `ai_usage_records` rows mixing `success`/`failure` across providers and error categories.

## Automated checks (all must pass)

```bash
# Backend — DB-gated suites must run with a live database, or they silently skip.
cd backend
REQUIRE_DB_TESTS=1 DATABASE_URL=postgres://…  cargo test -p authz -p platform -p server
```

`REQUIRE_DB_TESTS=1` is required: without it, `get_pool()` returns `None` and the integration tests print "skipping…" and pass vacuously. Run the narrow `-p` set above rather than `cargo test --workspace`, which aborts on the first failing crate and can mask later ones.

```bash
# Frontend
cd frontend && pnpm ng test dashboard && pnpm ng build dashboard && pnpm lint && pnpm format:check
```

**Exact-inventory suites that will fail loudly if a registration step is missed** (see research R6): `server/tests/openapi_coverage.rs` (3 new `EXPECTED` paths), `server/tests/rbac.rs` (route inventories), `server/tests/auth.rs` (permission-string list), `authz` matrix tests (permission catalog count).

> Note: `cargo fmt --check` is red repo-wide at HEAD for reasons unrelated to this feature. Do not treat that as a regression introduced here; check only that files this feature touches are formatted.

---

## Scenario 1 — Global usage, and it ignores tenant context (US1 / FR-005 / SC-001)

1. Sign in as a **Sales** platform user. Open Dashboard → Platform → Overview.
2. Expect tenant total with active/suspended split, platform-wide active conversation count, and AI usage for the default last-30-day range. Soft-deleted tenants must not be counted.
3. Use the tenant switcher to switch into a single tenant, then reload the Overview. **Figures must not change** — they remain platform-wide.
4. Narrow the date range; AI usage totals and `tenants.created_in_range` change, while `active_conversations` (a current figure) does not.
5. `curl` equivalent — no `X-Tenant-ID` header:
   ```
   GET /api/v1/platform/analytics/summary?from=2026-06-25&to=2026-07-25
   ```
   → `200` with the shape in the contract.
6. Invalid range → `422`: `?from=2026-07-25&to=2026-06-25`, and a span over 365 days.

## Scenario 2 — Tenant users are refused server-side (US1 / FR-003 / SC-002)

1. Sign in as a tenant user (any tenant role, no platform role).
2. The Platform area must be absent from navigation, and typing `/platform/overview-placeholder` must not render the dashboard.
3. **The part that matters**: call the API directly with that user's session cookie:
   ```
   GET /api/v1/platform/analytics/summary
   GET /api/v1/platform/health
   GET /api/v1/platform/ai-providers
   ```
   → all `403`. No aggregated data in any body. Hiding the nav is not sufficient evidence (FR-003); this step is the one that proves the criterion.

## Scenario 3 — The permission split is real (FR-001 / FR-002)

1. As the **Sales/Support/Finance** platform user: Overview loads; System Health and AI Provider Status are absent from nav and return `403` when called directly.
2. As **Super Admin** or **Developer**: all three pages load.
3. This asymmetry is intended — verify it rather than "fixing" it.

## Scenario 4 — Health indicators, including the degraded band (US2 / FR-012–FR-019)

1. As Super Admin, open Dashboard → Platform → System Health. With everything reachable and the queue empty: `overall: healthy`, four dependencies each with `probe_ms` and a `read_at` timestamp.
2. **Queue degraded** — insert ~150 `outbox_events` rows with `processed_at IS NULL` and `created_at` ~12 minutes old:
   → queue shows `degraded`, reason names both the backlog count and the oldest-wait age; `overall` becomes `degraded`.
3. **Queue unhealthy** — raise the backlog past 1,000 rows → queue `unhealthy`, `overall` `unhealthy`.
4. **Stuck retries** — a single unprocessed row with `attempts > 5` and a small backlog → at least `degraded` (FR-015), proving retry storms surface independently of backlog size.
5. **Storage fallback** — start the server with S3 configured but unreachable, so it falls back to in-memory:
   → storage reports `unhealthy` with `detail.backend: "in_memory"`, `s3_configured: true`. It must **not** report healthy — this is the silent-fallback gap the feature exists to expose.
6. **Dependency down** — stop PostgreSQL or Redis → that dependency `unhealthy` with a reason, **and the other dependencies still render** (FR-028).
7. Leave the page open 30 s → `read_at` advances without interaction (FR-019).
8. Confirm `/ready` still returns its original binary payload and status code, unchanged by this feature (FR-019a), and that it exposes no queue backlog or storage backend field (FR-019b).

## Scenario 5 — Provider errors are trackable (US3 / FR-020–FR-026)

1. Open Dashboard → Platform → AI Provider Status (Super Admin/Developer).
2. With 100 calls and 25 failures for one provider → `25%` failure rate, `unhealthy`. With 4 failures in 100 → `4%`, `degraded`.
3. A provider with 3 calls → `insufficient_data`, not `unhealthy`. A provider with 0 calls → `no_activity`, still listed, with `failure_rate: null` and null percentiles — **not** `0`.
4. Category counts sum to that provider's total failures.
5. Recent-failures list: newest first, capped at 50, each row showing provider, model, category, latency, UTC timestamp.
6. **Leak check (SC-008)**: inspect the raw JSON of all three endpoints — no `request_content`, no `response_content`, no API key or credential field anywhere.
7. Switch window `1h` / `24h` / `7d`; an unknown value → `422`.

## Scenario 6 — Accessibility and zero-state (FR-027 / SC-006 / Edge Cases)

1. With colour rendering disabled (grayscale filter or forced monochrome), every verdict on all three pages is still identifiable from its icon or text label.
2. Against an empty database: every page renders explicit zero/"no data yet" states — no blank cards, no errors, no rate computed from a zero denominator.

## Performance spot-check (SC-003)

With ~1,000 tenants and ~1,000,000 `ai_usage_records` rows seeded, each page's first figures must appear within 2 s. Confirm via `EXPLAIN (ANALYZE, BUFFERS)` that the new indexes from migration `0060` are actually chosen — in particular that the provider query uses `ai_usage_records_created_idx` and does **not** fall back to a sequential scan. The `percentile_cont` sort is the component most likely to dominate; if the budget is missed, escalate per research R4 rather than adding a cache (caching was explicitly rejected in clarification).
