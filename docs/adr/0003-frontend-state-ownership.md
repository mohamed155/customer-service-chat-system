# ADR-003: Frontend state ownership — NgRx Store, SignalStore, and signals

- **Status:** Accepted (MOH-15)
- **Date:** 2026-11-07
- **Deciders:** Platform engineering
- **Related:** [System architecture §4.2 and §17](../architecture/system-architecture.md#17-frontend-architecture-details),
  constitution Technology Standards ("one writable owner per piece of state"), `frontend/CLAUDE.md`,
  ADR-006 (tenant header flow), ADR-009 (realtime transport)

## Context

The dashboard is an Angular 22 SPA (zoneless, signals, `OnPush`). Angular now offers three
reasonable places to keep state, and they overlap:

- **NgRx Store** (+ Effects): one global, action-driven, devtools-inspectable store.
- **NgRx SignalStore**: lightweight, signal-based stores that can be scoped to a route or
  component, or provided in root.
- **Plain `signal()` / `computed()`** in a component or service.

Without a rule, the same data ends up in two places, gets out of sync, and survives when it
shouldn't. The worst case for this product is tenant-scoped data from tenant A still on screen
after switching to tenant B. That's a cross-tenant leak in the UI even when the backend is right.

The decision has to balance:

- **One source of truth** for each piece of state.
- **Tenant switch safety**: tenant-scoped state must reset on switch, every time.
- **Boilerplate**: global actions/reducers/effects for every list screen is too heavy.
- **Debuggability** for cross-cutting flows (sign-in, tenant switch, notifications).
- **Testability** of stores and components in isolation.

## Decision

### 1. Ownership rules

Every piece of state has **exactly one writable owner**, chosen by scope:

| Scope | Owner | Examples |
| -- | -- | -- |
| **Global, cross-feature, app-lifetime** | **NgRx Store** (+ Effects for side effects) | Authenticated principal, active tenant and switch status, global UI preferences (theme, sidebar), global notification badge count if shared across features. |
| **Feature or page** (lives while the feature is in use) | **NgRx SignalStore**, provided at the **route** (or feature component) level | Conversation list and filters, conversation detail, knowledge base items, team list, analytics range. |
| **Component-ephemeral** (UI-only, lost on destroy) | **`signal()` / `computed()`** in the component | Open/closed panels, hovered row, form-local draft, selected tab. |
| **Server state cache across features** | The owning feature's SignalStore, read by others through its facade | Never copied into a second store. |
| **URL state** (shareable filters, selected IDs) | **The router** | Read with `input()` / route params; stores react to it rather than duplicate it. |

Rules that apply to all of them:

1. **No duplication.** Other code reads through selectors, store signals or a facade. It never
   keeps its own writable copy.
2. **Derived state is computed**, never stored (`computed()`, `withComputed`, selectors).
3. **Components don't call `HttpClient`.** API services are called from stores (`rxMethod`,
   effects) or facades.
4. **Facades hide the mechanism.** Smart components use a feature facade or the store's public
   API, so moving state between mechanisms doesn't change components.
5. **`ui/` components are presentational:** inputs and outputs only, no store injection.
6. **Persistence is explicit.** Only state that must survive reloads (theme, last active tenant)
   goes to `localStorage`, through an effect, and never tokens or tenant data.

### 2. Tenant-scoped state resets on tenant switch

The switch happens in place: the shell and the open page stay mounted, so a store provided at the
route or the component survives it as much as a root store does. One mechanism therefore covers
all of them (MOH-61):

- `TenantScope` (`core/state/tenant-scope.ts`) announces every change of the active workspace id
  (`switchTenantSucceeded`, `setActiveTenant`, `clearActiveTenant`) from the reducer's own call
  stack, before anything renders. It is not an `effect()`, which could run one render late.
- **Every store that holds workspace data ends with `withTenantScope({ initialState, reload })`.**
  On a change it puts the store back to `initialState` in that same call, then calls `reload` if a
  workspace is now active. `cancel` stops a poll or timer the state does not hold. The scope covers
  every store under `features/tenant/` and every root-provided store. `pnpm lint:tenant-scope`
  (CI) fails a store that lacks it unless it is listed in `tools/check-tenant-scope.mjs` with a
  reason (a UI-only store, the platform operator's tenant directory).
- **State a component holds itself** (an open dialog, a picked row, a result message) uses
  `onTenantChange()` from the same file.
- **In-flight requests.** `tenantContextInterceptor` cancels every `/tenant/` request of the old
  workspace when the workspace changes (`takeUntil(TenantScope.changes$)`), so a late response
  cannot land in a store that now belongs to the new one. The caller's stream completes with no
  value; a one-shot `firstValueFrom` on such a call rejects with `EmptyError`.
- **Realtime.** `RealtimeService` reopens the stream for the new workspace, or closes it after
  `clearActiveTenant`. The stream carries the workspace it was opened for.
- Global Store slices: `appUi` (UI preferences) and `tenantContext` are the only slices, and neither
  holds workspace data, so no meta-reducer is needed. Add one before any workspace data goes into
  the global Store (gap F3).

### 3. Choosing between Store and SignalStore

Use **NgRx Store** only when at least one is true: several unrelated features read and write it;
it must survive navigation for the whole session; or its transitions need action-level auditing
in devtools (sign-in, tenant switch). Otherwise use a **SignalStore**.

## Alternatives considered

| Option | Why rejected |
| -- | -- |
| **NgRx Store for everything** | Actions, reducers, effects and selectors for every list screen is heavy boilerplate. Feature state outlives its page unless manually cleared, which makes tenant-switch leaks more likely. |
| **SignalStore only** (no global Store) | Workable, but cross-cutting flows lose action-level devtools history and the meta-reducer reset point. Root SignalStores re-create a global store without its tooling. |
| **Plain services with `signal()` / `BehaviorSubject`** | No conventions for side effects, loading/error state or tests. Every team member invents their own pattern, and duplication creeps in. |
| **TanStack Query / server-state library** | Good caching semantics, but adds a second state paradigm next to NgRx, and Angular support is less mature. Revisit if server-state caching becomes the dominant problem. |
| **Akita / Elf / NGXS** | Smaller communities and overlapping features with NgRx, which already ships both Store and SignalStore. |

## Consequences

**Positive**

- One answer to "where does this state live?", which makes reviews faster.
- Route-scoped stores give tenant-switch safety by construction, and cleanup on navigation.
- Less boilerplate for feature screens; full devtools history where it matters.
- Components stay presentational and easy to test.

**Negative and costs**

- Two NgRx mechanisms to learn instead of one.
- Root-provided stores need disciplined `reset()` hooks and tests; a missed one is a UI leak.
- Route-scoped stores reload data when the user navigates back, which may need caching later.

## Current state and migration

These gaps between the current code and this decision are follow-up work. No code changes in
this ADR.

| # | Gap | Where | Follow-up |
| -- | -- | -- | -- |
| F1 | The mechanisms already match: NgRx Store holds `appUi` and `tenantContext`; 25 SignalStores hold feature state; `@ngrx/store` / `signals` / `effects` 22.0.1 are installed. | `frontend/apps/dashboard/src/app/core/state/`, `features/**/**.store.ts` | None. |
| F2 | **`NotificationsStore` had no tenant-switch handling.** | `core/notifications/notifications.store.ts` | **Resolved (MOH-61):** the store uses `withTenantScope()`: it is emptied on a switch and reloads the unread count; the realtime stream reconnects for the new workspace. Covered by `tenant-scope.spec.ts`. |
| F3 | No meta-reducer clears tenant data from the global Store on tenant switch or sign-out. Today `tenantContext` is the only tenant-related slice, so the risk is low. | `core/state/` | Add the meta-reducer before any tenant-scoped data goes into the global Store. |
| F4 | No lint rule enforces "components don't call `HttpClient`" or "`ui/` doesn't inject stores". | `frontend/eslint.config.mjs` | Add boundary rules (`eslint-plugin-boundaries` or `no-restricted-imports`). |
