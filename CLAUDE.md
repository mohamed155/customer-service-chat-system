# AI Customer Service Platform — Agent Context

## Active Technologies

- **Specs**: Spec-Kit workflow in `specs/`; constitution in `.specify/memory/constitution.md`.

## Frontend rules

See `frontend/CLAUDE.md` for frontend-specific conventions (layering, state management, HTTP contracts, Helix visual system).

## Recent Changes

- 023-website-chat-widget: first customer-facing channel — embeddable loader + iframe widget (`frontend/apps/widget`), new backend `widgets` module (widget instances with branding/position/theme/domain allowlist, anonymous hashed-token sessions, public `/widget/v1` config/conversation/SSE endpoints, in-process rate limiting), dashboard widget settings with live preview, AI replies via existing outbox→responder pipeline, handoff/away/closed states from 014/021 signals. See `specs/023-website-chat-widget/plan.md`.
- 019-knowledge-base: knowledge module activation, S3-compatible document storage, draft/published/archived lifecycle, only published items are the AI-available set. See `specs/019-knowledge-base/plan.md`.
- 014-human-handoff-routing: AI→human escalation + routing (escalations module: skills catalog, agent availability with presence auto-revert, skill→load→queue routing under per-tenant advisory locks, claimable auto-draining queue, `GET /tenant/events` SSE + fetch-based realtime client, escalation banner/routing reason, topbar availability toggle) — see `specs/014-human-handoff-routing/plan.md`.
- 007-authentication: real sign-in (Argon2id password hashing, 8h JWT in httpOnly `app_session` cookie, revocation table, CSRF origin check, `POST /auth/login|logout`, login page, authGuard/guestGuard, credentials interceptor) — see `specs/007-authentication/plan.md`.
- 006-multi-tenancy-foundation: tenant isolation runtime (identity/tenancy modules, tenant-context middleware, `X-Tenant-ID` contract, switch audit, dashboard tenant context store, topbar switcher, dev identity header) — see `specs/006-multi-tenancy-foundation/plan.md`.
- 005-db-migration-foundation: migration workflow + four base tables (users, tenants, tenant_memberships, audit_logs) with conventions (UUID PK, timestamps, soft delete, partial unique indexes, append-only audit) — see `specs/005-db-migration-foundation/plan.md`.
- 003-helix-dashboard-visuals: Helix Admin visual system (tokens, shell, 8 tenant pages, 4 auth screens, fixtures, SignalStores) — see `specs/003-helix-dashboard-visuals/plan.md`.
- 002-angular-frontend-foundation: frontend foundation plan (Angular 22 modernization, Taiga UI, NgRx, layered structure) — see `specs/002-angular-frontend-foundation/plan.md`.
- 001-ai-customer-service-platform: platform-wide spec/plan/tasks; backend scaffolding and original frontend workspace.
