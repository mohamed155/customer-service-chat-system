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

- A tenant switch dispatches `switchTenantSucceeded` and **resets every tenant-scoped store**.
- Prefer providing tenant-scoped SignalStores **at route level**, so they are destroyed with the
  feature and can't outlive it.
- Any tenant-scoped store that lives in root **watches the active tenant** (`selectActiveTenant`)
  and clears its state, then reloads, when it changes. `CustomersStore` is the reference
  implementation. A unit test asserts the store holds no previous-tenant data after a switch.
- Global Store slices holding tenant data are cleared by a **meta-reducer** on tenant switch and
  sign-out.

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
| F2 | **`NotificationsStore` has no tenant-switch handling.** It's `providedIn: 'root'` and holds tenant-scoped items and the unread count. Neither the store, `TenantContextService`, nor the realtime `NotificationsService` resets it on switch. The previous tenant's items look like they stay cached until the panel is reopened, and the badge count until the next refresh. This needs a reproduction to confirm. (The root-provided `CustomersStore` and `CustomerProfileStore` already follow the rule: they watch the active tenant and clear and reload on change.) | `core/notifications/notifications.store.ts` | Reproduce first (switch tenants with the panel open). If it's confirmed, add a `reset()` triggered by the active-tenant change (same pattern as `CustomersStore`), reload the unread count, and add a test asserting the store is empty after a switch. |
| F3 | No meta-reducer clears tenant data from the global Store on tenant switch or sign-out. Today `tenantContext` is the only tenant-related slice, so the risk is low. | `core/state/` | Add the meta-reducer before any tenant-scoped data goes into the global Store. |
| F4 | No lint rule enforces "components don't call `HttpClient`" or "`ui/` doesn't inject stores". | `frontend/eslint.config.mjs` | Add boundary rules (`eslint-plugin-boundaries` or `no-restricted-imports`). |
