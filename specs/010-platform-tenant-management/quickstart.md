# Quickstart: Validating Platform Tenant Management

**Feature**: 010-platform-tenant-management. End-to-end validation scenarios. Contracts: [rest-api.md](./contracts/rest-api.md), [permissions.md](./contracts/permissions.md); entities: [data-model.md](./data-model.md).

## Prerequisites

- Postgres + Redis up (`podman compose -f infra/docker-compose.yml up -d` or Docker equivalent); migrations applied — `sqlx database reset -y` from `backend/` now includes `0016_tenant_business_metadata`.
- Seed one user per platform role and a tenant with members (pattern from `specs/008-rbac-permissions/quickstart.md` §2). Dev env: `X-Dev-User-Id` header flow works; login flow exercises the full path.
- Frontend: `pnpm ng serve dashboard` from `frontend/`.

## 1. Automated verification (the gates)

```powershell
cd backend
cargo fmt --check; cargo clippy --all-targets; cargo test   # incl. rbac.rs extension + platform_tenants.rs (live-gated)

cd ../frontend
pnpm ng test dashboard; pnpm ng build dashboard; pnpm lint; pnpm format:check
```

Expected: all green. New backend suite: `crates/server/tests/platform_tenants.rs`. New frontend specs beside `features/platform/tenants/*` (service, store, list, detail, form) plus updated `platform-nav`, `app.routes`, and `permissions` specs.

## 2. Onboarding (US1 — P1)

1. Sign in as **Super Admin** → platform-nav → Tenants → "New tenant": create with name + unused slug (leave plan default) → lands on/shows the new tenant, status badge **Active**, plan **Trial**.
2. Repeat as **Support Engineer** → also succeeds (clarified matrix).
3. Submit a duplicate slug → inline slug error (409 path); malformed slug (`Bad_Slug!`) and bad email → inline field errors (422 path); nothing created.
4. Switch into the new tenant via the switcher → works like any tenant.
5. `SELECT action, actor_user_id FROM audit_logs WHERE action='platform.tenant_created' ORDER BY created_at DESC LIMIT 1;` → row present with the creator's id.
6. As a tenant **Owner**, `POST /api/v1/platform/tenants` directly → 403 `unauthorized`.

## 3. Directory (US2 — P2)

1. As **Developer** (view-only): platform-nav shows Tenants; the list renders name/slug/status badge/plan; **no** "New tenant" button, **no** edit/status actions on detail.
2. Search by partial name and by slug → matching rows only; combine with status filter `Suspended` → intersection only.
3. Seed >1 page of tenants → "Load more" appends without duplicates or gaps (filter + cursor consistency).
4. No-match search → shared empty state (create action visible only to managers).
5. Detail page shows name, slug, status, plan, contact, created/updated dates; breadcrumb `Platform / Tenants / Tenant details`.
6. As a tenant user: `/platform/tenants` deep-link → redirected (no content flash); `GET /api/v1/platform/tenants/{id}` → 403.

## 4. Maintenance & control (US3 — P3)

1. As **Support Engineer**, edit a tenant's name, plan, and contact → saved, visible everywhere (list, detail, switcher label after refresh).
2. Change the slug → succeeds; `SELECT * FROM audit_logs WHERE action='tenant.slug_changed' ...` → trigger-written row with actor (transaction-actor contract).
3. With a tenant member signed in (another browser), **Deactivate** the tenant → member's very next request is refused ("Tenant is suspended"); directory still shows the tenant with a **Suspended** badge; **Reactivate** → member's next request succeeds.
4. Status change writes `platform.tenant_status_changed`; field edits write `platform.tenant_updated` with old/new values.
5. As **Sales**, attempt PATCH directly → 403; edit/status controls absent in UI.
6. Two edits racing (curl PATCH while form open) → last save wins; both audit rows present in order.

## 5. Regression spot-checks

- Tenant switcher and tenant-select still work (TenantSummary extension is additive).
- Platform overview placeholder: reachable by Super Admin only (`platform.admin`); Support Engineer reaching `/platform` lands on/limited to Tenants.
- 008 rbac suite still green (matrix extended, nothing loosened); 009 shell behaviors unchanged.

## Expected outcomes summary

- SC-001: onboarding < 1 min via §2.1 with zero DB involvement.
- SC-002: §3.2–3.3 — exact search/filter/pagination with no missing/duplicate rows.
- SC-003: §2.6, §3.6, §4.5 — 100% tenant-user refusal, zero management UI for view-only/tenant users.
- SC-004: §2.5, §4.2, §4.4 — every create/edit/status change audited (incl. trigger-written slug audits).
- SC-005: §4.3 — suspension/reactivation effective on the next request.
- SC-006: §3.1 — view-only roles complete all inspection tasks with zero management affordances.
