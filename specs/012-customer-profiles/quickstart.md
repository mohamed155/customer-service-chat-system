# Quickstart Validation: Customer Profiles

Runnable checks proving the feature end-to-end. Contracts: [rest-api.md](./contracts/rest-api.md), [permissions.md](./contracts/permissions.md); schema: [data-model.md](./data-model.md).

## Prerequisites

- PostgreSQL + Redis running with the backend's standard env configuration (same setup as the existing live-gated suites, e.g. `server/tests/team_members.rs`); migrations 0001–0031 applied by the test harness/startup.
- `cargo` (backend), `pnpm` (frontend, run from `frontend/`).
- Two seeded tenants with memberships covering an Agent and a Viewer (the integration suite provisions its own).

## Backend validation

```bash
cd backend

# Schema: 0025/0026 tables, CHECKs, unique + trgm indexes
REQUIRE_DB_TESTS=1 cargo test -p db --test schema

# Feature suite: CRUD, search, pagination, per-operation tenant isolation,
# duplicate-identifier 409, validation 422, viewer 403, audit rows, history scoping
REQUIRE_DB_TESTS=1 cargo test -p server --test customers

# RBAC matrix: customer operations across all roles
REQUIRE_DB_TESTS=1 cargo test -p server --test rbac

# Full backend gate
REQUIRE_DB_TESTS=1 cargo test
```

**Expected outcomes** (each maps to a spec item):

| Check | Expectation |
|-------|-------------|
| Isolation (FR-011/FR-015, SC-003) | Every list/search/view/create/update/history call from tenant B against tenant A's data returns `404 not_found` or an empty list — zero cross-tenant rows in any response |
| Search (FR-006, SC-001) | `?q=` fragments of name/email/phone/identifier return the seeded customer; special-character queries return safely |
| Performance (SC-002) | 10,000-customer seeded volume check: name-fragment search responds in under 1 second |
| Conflict (FR-014) | Re-assigning a taken (channel, identifier) → `409 conflict` naming the holding customer; same identifier in the *other* tenant → succeeds |
| Validation (FR-013, SC-006) | Bad email/phone, 51st metadata key, unknown channel → `422` with the offending field in `details[]` |
| RBAC (FR-012) | Viewer: GETs 200, POST/PATCH 403; Agent+: all 200/201 |
| Audit (FR-017) | POST → `customer.created` row; PATCH → `customer.updated` row with `changed_fields` names; both share the write transaction |
| History (FR-010/FR-016) | Seeded conversations return newest-first, ≤20, `has_more` correct; customer with none → empty `data` |

## Frontend validation

```bash
cd frontend
pnpm ng test dashboard     # includes new service/store/list/profile/dialog specs
pnpm ng build dashboard
pnpm lint
pnpm format:check
```

All four quality gates must pass (CLAUDE.md).

## Manual end-to-end walkthrough (dev stack)

1. Start backend + `pnpm ng serve dashboard`, sign in as an **Agent** of tenant A.
   *(Composite FK constraints (migration 0027) enforce tenant_id consistency on customer child tables; cascade trigger (migration 0030) stamps identifier deleted_at when a parent customer is soft-deleted.)*
2. **Customers list** (sidebar → Customers): live rows render; type a name fragment in search → filtered results; nonsense query → empty state with clear-search affordance (US1).
3. **Create**: "New customer" → name + email + WhatsApp identifier + one metadata pair → appears in list immediately (US3, SC-004 under 1 minute).
4. **Profile**: open the customer → contact, identifiers (channel badges), metadata view, conversation history section with empty state; after seeding a conversation row, section shows channel/status/last-activity newest-first (US2, SC-005 single view).
5. **Conflict**: create a second customer reusing the same WhatsApp identifier → field-level conflict message naming the existing customer.
6. **Read-only role**: sign in as a **Viewer** → list and profile visible, no create/edit controls; direct PATCH via devtools → 403 (FR-012).
7. **Isolation spot-check**: as a member of tenant B, paste tenant A's customer profile URL → not-found experience (FR-011).

## Sign-off checklist

- [X] `cargo test -p db --test schema` green (82/82 passed)
- [X] `cargo test -p server --test customers` green (52/52 passed)
- [X] `cargo test -p server --test rbac` green (28/28 passed)
- [X] `pnpm ng test dashboard` (603/603 passed), `build` (green), `lint` (clean), `format:check` (clean) — **all four frontend gates pass
- [X] T160–T166 all implemented and verified: normalized no-op audit suppression, null-contact clears, request-id in 403s, identifier reuse, EXPLAIN decoding, full gate suite, E2E validation entries
- [ ] Manual walkthrough steps 2–7 observed (requires running backend + frontend dev server) — requires manual execution with dev stack
- [ ] Audit rows visible for the create/update performed in the walkthrough (requires running dev stack) — requires manual execution with dev stack

> **T106 verification**: Items marked `[ ]` require a human to run the dev stack and
> manually walk through steps 2–7 of the walkthrough above, then confirm audit
> visibility for the create/update operations performed during the walkthrough.
