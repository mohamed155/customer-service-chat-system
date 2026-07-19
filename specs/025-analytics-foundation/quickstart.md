# Quickstart Validation: Analytics Foundation

Proves the feature works end-to-end. Details live in [contracts/analytics-api.md](contracts/analytics-api.md) and [data-model.md](data-model.md).

## Prerequisites

- PostgreSQL running with migrations applied through `0052_analytics_indexes.sql`
- Backend: `cd backend && cargo build`
- Frontend: `cd frontend && pnpm install`

## 1. Backend integration tests (primary proof)

```bash
cd backend
cargo test --test analytics_api
```

Expected: all tests pass, covering at minimum:

- **Correctness on seeded data**: known conversations/escalations/feedback/usage → exact expected summary values (SC-003)
- **Tenant isolation**: two seeded tenants; each summary reflects only its own data (SC-002)
- **RBAC**: Owner/Admin/Manager → 200; Agent/Viewer → 403; unauthenticated → 401
- **Date filtering**: range subsets return matching subtotals; invalid ranges → 422
- **Channel filtering**: channel-filtered metrics match per-channel seeds; token usage attribution via generation records
- **Zero-fill & empty states**: empty range → zeros/nulls; timeseries has one entry per day
- **Soft-delete exclusion**: soft-deleted conversation absent from every metric
- **Metric stability**: a past period's metrics are unchanged after new out-of-range activity arrives (FR-016)

Also run the OpenAPI contract guards (schema/coverage tests already in the suite):

```bash
cargo test --test openapi_contract --test openapi_coverage --test openapi_valid
```

## 2. Full backend suite

```bash
cd backend && cargo test
```

Expected: no regressions (notably `rbac.rs` after the Viewer matrix change).

## 3. Performance check (opt-in, verifies SC-004)

```bash
cd backend && cargo test --test analytics_api -- --ignored
```

Seeds 100,000 conversations across a 90-day window and asserts both endpoints answer in under 3 seconds. This is the budget that justified live aggregation over rollup tables — if it fails, revisit that decision rather than relaxing the threshold.

## 4. Frontend quality gates

```bash
cd frontend
pnpm ng test dashboard
pnpm ng build dashboard
pnpm lint
pnpm format:check
pnpm test:e2e analytics
```

Expected: analytics store/component/chart specs pass; build and lint clean; the Playwright end-to-end spec passes (cards, date preset, channel filter, charts, empty state).

## 5. Manual smoke (dev stack)

1. Start backend + dashboard dev servers, sign in as a tenant **Admin**.
2. Open **Analytics** in the tenant sidebar:
   - Metric cards show real values (not fixture data); empty tenant shows explicit empty states.
   - Switch date presets (7/30/90 days) and a custom range → cards + charts update within ~2 s (SC-006).
   - Apply a channel filter → all metrics recompute; channel breakdown still shows all channels.
3. Submit a widget feedback rating (024 flow), refresh analytics within a minute → satisfaction count increments (SC-005).
4. Sign in as a **Viewer** → Analytics is absent/forbidden (403 on direct API call).

## Definition of done for this feature

All of sections 1–4 pass in CI-equivalent local runs, and section 5 behaves as described.
