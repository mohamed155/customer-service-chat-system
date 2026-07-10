# Quickstart: Validating Dashboard Shell

**Feature**: 009-dashboard-shell. End-to-end validation scenarios. Contracts: [ui-shell.md](./contracts/ui-shell.md); entities: [data-model.md](./data-model.md).

## Prerequisites

- Backend running with migrations applied and seeded users (see `specs/008-rbac-permissions/quickstart.md` §2 for per-role seed pattern); `ENVIRONMENT=development` allows the `X-Dev-User-Id` header flow, but sign-in via the login page exercises the full path.
- Frontend: `pnpm install` done; run `pnpm ng serve dashboard` from `frontend/`.

## 1. Automated verification (the gates)

```powershell
cd frontend
pnpm ng test dashboard     # user-menu, platform-nav, breadcrumb, layout-store drawer, sidebar (footer removed), page-container specs
pnpm ng build dashboard
pnpm lint
pnpm format:check
```

Expected: all pass. New specs live beside `layout/topbar/user-menu*`, `layout/topbar/platform-nav*`, `layout/breadcrumb/*`, `core/router/breadcrumb*`, `core/identity/role-display*`, `shared/components/page-container/*`; updated specs: `sidebar.component.spec.ts`, `app-shell.component.spec.ts`, `layout.store.spec.ts`, `topbar.component.spec.ts`.

## 2. Role-appropriate shell (US1 — P1)

1. Sign in as a tenant **Owner**: sidebar shows all eight workspace pages; header shows **no** tenant switcher and **no** platform control; avatar menu shows real name/email and "Owner · <Tenant>"; sign out returns to login.
2. Sign in as a tenant **Viewer**: sidebar shows only view pages (no Settings); avatar menu role line reads "Viewer · <Tenant>".
3. Sign in as platform **Super Admin**: header shows tenant switcher **and** platform control; platform control opens to "Platform overview" and navigates to `/platform`; sidebar has no tenant entries until a tenant is selected via the switcher, then shows the staff-permitted set.
4. Sign in as platform **Support Engineer** (production-shaped permissions): tenant switcher visible; platform control **absent** (no `platform.admin`); after switching into a tenant, sidebar shows the support working set.
5. Grep check: no `SIDEBAR_USER` remains in the codebase; sidebar renders no footer card.

## 3. Wayfinding (US2 — P2)

1. Visit each tenant page: breadcrumb reads `Workspace / <Page>`; the final crumb matches the page title.
2. Visit `/platform`: breadcrumb reads `Platform / Platform overview`.
3. Every page body sits inside the shared container (uniform max-width/padding) with the shared page header — visually compare any two pages.

## 4. Responsive behavior (US3 — P3)

1. Desktop (≥1024px): toggle sidebar via the menu button — collapses to icon rail and back; preference holds while navigating.
2. Narrow the window below 768px: sidebar disappears; menu button now opens it as an overlay drawer above a scrim.
3. With the drawer open: click a nav item → navigates and the drawer closes; reopen and press Escape → closes; click the scrim → closes.
4. At 360px width: no horizontal scrollbar on any page; switcher/theme/avatar still reachable in the header.

## 5. Skeleton & loading/empty states (US4 — P4)

1. DevTools → Network → throttle to "Slow 3G", hard-reload while signed in: the themed skeleton frame (sidebar rail + topbar + content block) paints immediately; no blank screen; no navigation/controls appear until the full shell renders complete — nothing flashes and disappears.
2. Set theme to dark, hard-reload: skeleton paints dark (no light flash).
3. Sign in as a user with **no roles/memberships**: lands on tenant-select with the standard empty state (explanation + no tenants) and can sign out via the avatar menu — no error loop (FR-015).
4. Spot-check a data page's loading state and an empty list's empty state render the shared components.

## 6. Theme regression (FR-011)

Cycle light → dark → system from the header on: sidebar, avatar menu (open), platform control (open), breadcrumbs, drawer (mobile width), skeleton (reload) — all legible in both themes; with `system` selected, flipping the OS theme updates the shell live.

## Expected outcomes summary

- SC-001: zero placeholder identity anywhere post-sign-in (fixture deleted).
- SC-002: nav/switcher visibility matches the §2 matrix for every role exercised.
- SC-003: breadcrumb correctness on every page (§3).
- SC-004: no horizontal overflow down to 360px; nav within two interactions (§4).
- SC-005: no entitlement flash across sign-in/refresh/tenant-switch (§5.1).
- SC-006: both themes pass visual review on all shell surfaces (§6).
- SC-007: all pages consume the shared header/container/loading/empty patterns (§3, §5.4).
