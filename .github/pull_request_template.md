## Summary

<!-- What does this change do, and why? One or two sentences. -->

## Linked issue

<!-- Required. Link the Linear issue, e.g. MOH-123. Use "N/A" only for changes with no tracked work (and say why). -->

Linear: 

## Review checklist

Mirrors the Definition of Done and review gate in `.specify/memory/constitution.md`.
Tick each item, or write `N/A` with a one-line reason.

### Tenancy and authorization (Principles I, II)

- [ ] **Tenancy scope checked**: every tenant-owned query/mutation is scoped by `tenant_id` via `TenantContext`; no cross-tenant path (including IDs guessed from another tenant); caches, storage keys, and queues are tenant-namespaced
- [ ] **Permissions declared**: each new/changed endpoint declares its required permission (permission, not role name); sensitive actions write an audit record

### Correctness and contracts (Principles VIII, X)

- [ ] **Input validated** at the boundary (HTTP, webhooks, files, channel messages, LLM output); untrusted input is never trusted
- [ ] **Errors mapped** to typed errors and returned as RFC 9457 Problem Details with no internals leaked; no `unwrap`/`expect`/`panic!` in request or job paths
- [ ] Migrations (if any) are backward compatible (expand → migrate → contract); retryable writes are idempotent

### Observability (Principle VII)

- [ ] **Observability added**: structured logs (tenant ID + actor, no secrets or raw PII), trace/request ID propagated, RED metrics (queue depth/age where a queue is added)

### Tests (Principle VI)

- [ ] **Tests added and passing**: unit, integration (real Postgres/Redis), contract, and component/e2e as applicable; new domain logic coverage ≥ 80% lines
- [ ] **Negative and isolation tests**: unauthorized, wrong tenant, and malformed-input cases covered wherever tenant data or permissions are touched
- [ ] Bug fixes include a regression test that fails before the fix
- [ ] CI is green (format, lint, type check, build, tests, security scans) and no new lint warnings

### Docs and decisions (Principles V, IX)

- [ ] **Docs/ADRs updated**: OpenAPI, module READMEs, user docs, and ADRs (`docs/adr/`) match the code; any constitution deviation is recorded in the plan's Complexity Tracking with an ADR
- [ ] Acceptance criteria from the linked issue/spec are verified (describe how below)

## Verification

<!-- How you verified the change: commands run, screenshots, QA notes. -->
