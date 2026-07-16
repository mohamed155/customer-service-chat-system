# Quickstart: AI Agent Configuration

**Feature**: 017-ai-agent-config

Validation guide proving the feature end-to-end. Contracts: [rest-api.md](./contracts/rest-api.md), [agent-runtime.md](./contracts/agent-runtime.md); schema: [data-model.md](./data-model.md).

## Prerequisites

- PostgreSQL + Redis running; `DATABASE_URL` exported; migrations current (`0041`–`0043` apply cleanly on top of `0040`).
- Backend env per 015: `APP_AI_KEY_ENCRYPTION_KEY` set; for deterministic runs point a vendor base-URL override at a wiremock/local mock.
- Frontend: `pnpm install` done in `frontend/`.

## Automated validation

```bash
# Backend — all gates
cd backend
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                       # unit: validation, prompt composition byte-equality,
                                             # rule matching, options assembly
REQUIRE_DB_TESTS=1 cargo test -p server      # integration: ai_agent.rs (CRUD, RBAC, isolation,
                                             # concurrency 409, audit rows, responder pipeline via
                                             # process_agent_responder_once + wiremock), rbac.rs
                                             # (narrowed matrix), schema.rs (0041–0043), openapi_contract.rs

# Frontend — all gates
cd ../frontend
pnpm ng build dashboard && pnpm ng test dashboard && pnpm lint && pnpm format:check
```

## Manual scenario walkthrough

Sign in as a tenant **Owner/Admin**, tenant context set.

1. **Defaults, not errors (US1 S1)** — `GET /tenant/ai/agent` → `configured: false` with the editable default template. Dashboard: AI Agent page shows the pre-filled form.
2. **Unconfigured fallback (US6, FR-004a/b)** — Before any save, post a customer message in a `web_chat` conversation and run the responder: exactly one `system`-kind auto-acknowledgment appears (a second customer message adds no second ack), and the conversation shows `awaiting_ai_decision: true`. In the dashboard, the conversation banner offers the two choices:
   - Choose **platform AI** (requires the 015 platform AI layer resolvable — otherwise the option is disabled with the reason): the next customer message gets an `ai`-kind reply under the platform default persona; audit shows `conversation.ai_handling_set`.
   - On another conversation choose **human**: it lands in the escalation queue with reason "no AI agent configured".
3. **First save activates the tenant agent (US1, FR-004/FR-004c)** — Edit name/tone/prompt, save → `201`, `version: 1`. New customer messages now get `ai`-kind replies from the tenant agent — including in conversations that had a fallback choice (config supersedes).
4. **Persistence (US1 S2)** — Reload the page → saved values round-trip.
5. **Validation (US1 S4)** — Save with empty name / 9000-char prompt → `422` with per-field details; nothing persisted.
6. **Isolation (US1 S5 / SC-004)** — Tenant B's `GET` shows `configured: false`; tenant B cannot read or affect tenant A's agent (integration matrix covers the full read/write grid).
7. **Provider selection (US2)** — `GET /tenant/ai/agent/options` lists only credential-backed providers with curated models. Select provider/model, save; next AI reply's usage record (015 `GET /tenant/ai/usage`) attributes that provider/model. Remove the provider's credential → agent `GET` flags `provider_selection.stale: true`, replies fall back to the AI-layer default.
8. **Business rules (US3 S1)** — Add a rule ("never promise refunds"), save; prompt-composition unit test byte-verifies inclusion; live reply reflects it.
9. **Escalation rules (US3 S2/S3, SC-005)** — Add a `topic_keywords` rule ("refund" → skill "Billing"). Customer message containing "refund" → no AI reply; escalation appears in the queue with the rule name as routing reason. With **zero** rules configured, "I want to talk to a human" still escalates (baseline, FR-011).
10. **Broken skill ref (US3 S4)** — Delete the referenced skill → agent `GET` shows `broken_skill_refs` on that rule; settings page surfaces it.
11. **Channels (US4)** — Disable `web_chat`, save → new customer messages get no AI reply; re-enable → replies resume. Disable all channels → save succeeds, page shows the agent-inactive notice.
12. **Concurrency (FR-017)** — Two tabs: save in tab 1, then save stale `version` in tab 2 → `409`, page prompts reload; no silent overwrite.
13. **Audit (US5, SC-003)** — Every save/avatar change writes `agent_config.*` audit rows with actor + changed fields; a platform user editing via tenant switcher is attributed as the platform actor.
14. **RBAC (FR-013)** — Manager/Agent/Viewer: nav hides AI Agent; direct API calls → `403`. Owner/Admin: full access.
15. **Avatar (FR-006)** — Pick a preset → saved. Upload a 100 KB PNG → served at `GET /tenant/ai/agent/avatar`. Upload 300 KB / a PDF → rejected; previous avatar intact.

## Success-criteria spot checks

- **SC-002**: steps 3/7/8 — change config, verify the very next reply reflects it.
- **SC-006**: `schema.rs` asserts the three partial unique indexes; attempting a second live agent row for one tenant violates the v1 unique index; dropping only that index (scratch DB) allows a second named agent row with `is_default=false` — no other change needed.
