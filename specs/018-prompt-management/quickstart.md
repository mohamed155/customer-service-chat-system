# Quickstart: Prompt Management Validation

**Feature**: 018-prompt-management | **Date**: 2026-07-16

Proves the feature end-to-end: versioned saves, history + restore, variables + preview, validation, audit, and the responder consuming the active version. Contracts: [rest-api.md](./contracts/rest-api.md), [prompt-runtime.md](./contracts/prompt-runtime.md); schema: [data-model.md](./data-model.md).

## Prerequisites

- PostgreSQL + Redis running; migrations applied through `0045_agent_prompts.sql` (`cargo run -p server` applies on boot, or the migration workflow from 005).
- Backend: `cargo run -p server` from `backend/`.
- Frontend: `pnpm start` from `frontend/` (dashboard on the usual dev port).
- A tenant with an Owner or Admin member signed in (007 login), tenant context selected. For API-level checks, an `app_session` cookie + `X-Tenant-ID` header as in earlier features' quickstarts.

## Automated gates (all must pass)

```bash
# Backend — unit (placeholder scanner, validation codes/offsets, render determinism,
# no-op detection) + integration (DB-gated, REQUIRE_DB_TESTS pattern)
cd backend && cargo fmt --all --check && cargo clippy --workspace -- -D warnings && cargo test --workspace

# Key suites:
#   modules/ai unit tests            — prompt_validate, render_prompt, composer byte-equality
#   server/tests/ai_agent_prompt.rs — save→version, 409 conflict, no-op, history pagination,
#                                     restore (+ blocked restore), RBAC, isolation, audit rows,
#                                     responder uses active version + runtime substitution
#   server/tests/rbac.rs             — 5 new route→permission entries
#   server/tests/openapi_contract.rs — new paths/DTOs; agent DTO systemPrompt removal
#   shared/db/tests/schema.rs        — 0045 assertions (tables, CHECKs, uniques, column drop)

# Frontend
cd frontend && pnpm nx run-many -t lint test build --projects=dashboard
```

## Manual walkthrough (maps to spec user stories)

**US1 — version on save**
1. Navigate to AI Agent → the prompt section is now a summary card; click **Manage prompt** → the prompt page (`/t/<tenant>/ai-agent/prompt`) opens with editor, variables panel, preview panel.
2. First-time tenant: editor shows the starter default, history is empty. Type a prompt, save with a change note → toast confirms **v1**.
3. Edit again, save → **v2**. Save again without changes → "no changes" notice, still v2 (FR-013).
4. In a second browser session (same tenant, other admin), load the page, then save in the first session; saving in the second → conflict banner with review-and-retry, no silent overwrite (409 flow).

**US2 — history & restore**
5. Open the **Version history** drawer → versions newest-first with author, time, note; the active one is badged.
6. Select v1 → full content + diff against active. Click **Restore** and confirm → new version created, badged "Restored from v1"; history shows every prior version intact.

**US3 — variables & preview**
7. Place the cursor mid-prompt, click `customer_name` in the variables panel → `{{customer_name}}` inserted at the cursor.
8. Preview shows sample-substituted text (`Jamie Lee` etc.) and updates live as you type; saved versions store placeholders, not samples (verify via version detail).

**US4 — validation**
9. Type `{{business_hours}}` → inline `unknown_variable` error and highlighted chip in preview; **Save** is rejected (422) and editor content is preserved.
10. Try an unclosed `{{agent_name` → `malformed_placeholder`. Clear the editor → `required`. Fix everything → save succeeds.

**US5 — audit & attribution**
11. As two different admins, each save once; the history attributes each version to the right person.
12. Check `audit_logs` (psql or the audit surface): one `agent_prompt.version_created` per save and `agent_prompt.version_restored` for step 6, with version numbers and actor — and no prompt content in `details`.

**Runtime binding (FR-017)**
13. With the agent configured and a channel enabled (017), send a customer message (conversations flow) → the AI reply reflects the *latest* prompt version, with `{{agent_name}}`/`{{tenant_name}}`/`{{customer_name}}`/`{{channel}}` replaced by real values (inspect the reply and, if needed, the responder trace's `prompt_version` field).
14. Save a new version, send another customer message → the reply now binds the new version.

**Isolation & access**
15. Switch to a second tenant → its prompt page is independent (empty/its own history). As a Manager/Agent/Viewer member, the AI Agent nav/page (including the prompt route) is inaccessible (403/guard redirect).

## Expected outcomes

Every numbered step behaves as described; all automated gates green. Any deviation maps to a spec FR/SC — fix before `/speckit-implement` completion is claimed.
