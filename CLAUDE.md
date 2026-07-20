# AI Customer Service Platform — Agent Context

## Active Technologies

- **Specs**: Spec-Kit workflow in `specs/`; constitution in `.specify/memory/constitution.md`.

## Frontend rules

See `frontend/CLAUDE.md` for frontend-specific conventions (layering, state management, HTTP contracts, Helix visual system).

## Recent Changes

- 027-notifications: tenant-scoped notification inbox with bell/badge, notification list/panel, SSE live updates, escalation/assignment/AI-failure/tool-approval triggers, auto-resolve on claim/decide, deduplication, 90-day retention, crate rename (notifications→email). See specs/027-notifications/plan.md.
- 026-audit-logs: read-only audit surface over the existing append-only `audit_logs` table — activate the placeholder `audit` crate (`GET /tenant/audit-logs`, `GET /platform/audit-logs`, cursor pagination, category derived from action prefix), new `audit.view` (Owner/Admin) and `platform.audit.view` (all platform roles) permissions, `tool.executed` audit writer, tenant + platform dashboard pages with shared audit table/detail drawer. See `specs/026-audit-logs/plan.md`.
- 023-website-chat-widget: first customer-facing channel — embeddable loader + iframe widget (`frontend/apps/widget`), new backend `widgets` module (widget instances with branding/position/theme/domain allowlist, anonymous hashed-token sessions, public `/widget/v1` config/conversation/SSE endpoints, in-process rate limiting), dashboard widget settings with live preview, AI replies via existing outbox→responder pipeline, handoff/away/closed states from 014/021 signals. See `specs/023-website-chat-widget/plan.md`.
- 024-customer-feedback: Customer feedback feature (rating, comment, satisfaction badge, summary) — see `specs/024-customer-feedback/plan.md`
- 025-analytics-foundation: tenant analytics (summary + daily timeseries endpoints aggregating existing conversation/feedback/usage tables, no rollup tables; metric cards, inline-SVG trend charts, date-range and channel filters; analytics.view restricted to Owner/Admin/Manager). See specs/025-analytics-foundation/plan.md.
- 019-knowledge-base: knowledge module activation, S3-compatible document storage, draft/published/archived lifecycle, only published items are the AI-available set. See `specs/019-knowledge-base/plan.md`.
- 014-human-handoff-routing: AI→human escalation + routing (escalations module: skills catalog, agent availability with presence auto-revert, skill→load→queue routing under per-tenant advisory locks, claimable auto-draining queue, `GET /tenant/events` SSE + fetch-based realtime client, escalation banner/routing reason, topbar availability toggle) — see `specs/014-human-handoff-routing/plan.md`.
- 007-authentication: real sign-in (Argon2id password hashing, 8h JWT in httpOnly `app_session` cookie, revocation table, CSRF origin check, `POST /auth/login|logout`, login page, authGuard/guestGuard, credentials interceptor) — see `specs/007-authentication/plan.md`.
- 006-multi-tenancy-foundation: tenant isolation runtime (identity/tenancy modules, tenant-context middleware, `X-Tenant-ID` contract, switch audit, dashboard tenant context store, topbar switcher, dev identity header) — see `specs/006-multi-tenancy-foundation/plan.md`.
- 005-db-migration-foundation: migration workflow + four base tables (users, tenants, tenant_memberships, audit_logs) with conventions (UUID PK, timestamps, soft delete, partial unique indexes, append-only audit) — see `specs/005-db-migration-foundation/plan.md`.
- 003-helix-dashboard-visuals: Helix Admin visual system (tokens, shell, 8 tenant pages, 4 auth screens, fixtures, SignalStores) — see `specs/003-helix-dashboard-visuals/plan.md`.
- 002-angular-frontend-foundation: frontend foundation plan (Angular 22 modernization, Taiga UI, NgRx, layered structure) — see `specs/002-angular-frontend-foundation/plan.md`.
- 001-ai-customer-service-platform: platform-wide spec/plan/tasks; backend scaffolding and original frontend workspace.
