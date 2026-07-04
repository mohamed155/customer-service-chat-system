# Tasks: AI Customer Service Platform

**Input**: plan.md, spec.md, data-model.md, contracts/, research.md, quickstart.md
**Organization**: By milestone (M0–M7 from plan.md), mapped to user stories (US1–US8 from spec.md). Each task is a ready-to-file GitHub Issue.

## Task format & legend

```text
- [ ] T### [P] [US#] Title — `primary file/dir`
  - **AC**: acceptance criteria (the issue's Definition of Done checklist)
  - **Notes**: technical notes | **Test**: testing requirements
  - **Size**: S(2–4h) M(4–8h) L(1–2d, split if possible) | **Labels** | **Prio** | **Deps**
```

- `[P]` = parallelizable (different files, no incomplete-dependency).
- `[US#]` = user story from spec.md (setup/foundational phases carry none).
- Title line = issue Title; AC bullets = issue checklist; every issue also inherits its milestone's Description/Objective from plan.md §Milestone Roadmap.
- **Design system rule (all frontend tasks)**: CSS uses **BEM naming** (`hx-block__element--modifier`); design tokens are extracted from **`./Helix Admin.html`** (CSS custom properties: `--accent`, `--accent-soft/strong/ink`, `--bg-app`, `--panel`, `--panel-2`, `--panel-3`, `--border`, `--border-strong`, `--sidebar`, `--text`, `--text-2`, `--text-3`, `--green/-soft`, `--amber/-soft`, `--red/-soft`, `--shadow`, `--shadow-lg`) into `libs/ui` — no hard-coded colors/spacing in feature code.
- **Backend rule**: every tenant-owned table gets `tenant_id` + composite indexes leading with it; repos require `TenantScope`; all schema via SQLx migrations.

---

## Phase 1: Setup — Milestone M0 (Foundations & Walking Skeleton)

- [ ] T001 Create Cargo workspace with crate skeletons — `backend/Cargo.toml`, `backend/crates/{server,shared/*,modules/*,ai-providers}`
  - **AC**: workspace builds; module crates compile empty; cross-module internal access impossible (crate boundaries)
  - **Notes**: research R-01 | **Test**: `cargo build` in CI
  - **Size**: M | **Labels**: backend, infrastructure | **Prio**: P0 | **Deps**: —
- [ ] T002 [P] Local infra docker-compose — `infra/docker-compose.yml`
  - **AC**: postgres16+pgvector, redis7, minio, mailhog, otel-collector+jaeger boot with one command; healthchecks pass
  - **Size**: S | **Labels**: infrastructure | **Prio**: P0 | **Deps**: —
- [ ] T003 SQLx migration framework + migration 0001 (extensions, helpers) — `backend/migrations/0001_init.sql`
  - **AC**: `sqlx migrate run` idempotent; pgvector+citext enabled; CI builds scratch DB from 0001
  - **Size**: S | **Labels**: database, backend | **Prio**: P0 | **Deps**: T001, T002
- [ ] T004 Observability baseline: request-ID middleware, tracing JSON logs + redaction, OTel export, /metrics — `backend/crates/shared/observability/`
  - **AC**: every response has `X-Request-Id`; spans visible in Jaeger; secrets/content redacted at source (NFR-LOG-002)
  - **Test**: unit tests for redaction layer; V-M0 scenario 3
  - **Size**: M | **Labels**: backend, observability | **Prio**: P0 | **Deps**: T001
- [ ] T005 API skeleton: /api/v1 router, error envelope, cursor pagination, health/readiness — `backend/crates/server/`, `backend/crates/shared/kernel/`
  - **AC**: envelope/pagination byte-match `contracts/rest-api.md`; unknown params → 400
  - **Test**: contract tests for envelope + pagination helpers
  - **Size**: M | **Labels**: backend, api | **Prio**: P0 | **Deps**: T001, T004
- [ ] T006 [P] Idempotency-key middleware + storage — `backend/crates/shared/kernel/idempotency.rs`
  - **AC**: replayed POST returns original result + `Idempotency-Replayed: true`
  - **Test**: replay integration test | **Size**: M | **Labels**: backend, api | **Prio**: P1 | **Deps**: T005
- [ ] T007 [P] Typed config + secret loading — `backend/crates/shared/config/`
  - **AC**: env-only secrets; `.env.example` committed; startup fails fast on missing config
  - **Size**: S | **Labels**: backend, security | **Prio**: P0 | **Deps**: T001
- [ ] T008 Domain event bus + transactional outbox — `backend/crates/shared/events/`
  - **AC**: in-txn dispatch for sync consumers; outbox table + drain worker with retries (research R-02); envelope per `contracts/domain-events.md`
  - **Test**: outbox redelivery + idempotent-consumer integration tests
  - **Size**: L | **Labels**: backend, infrastructure | **Prio**: P0 | **Deps**: T003, T005
- [ ] T009 [P] Angular workspace: apps/dashboard + apps/widget + lib scaffolds — `frontend/angular.json`, `frontend/apps/`, `frontend/libs/`
  - **AC**: both apps serve; libs buildable; strict TS; widget has separate build with bundle budget configured
  - **Size**: M | **Labels**: frontend, infrastructure | **Prio**: P0 | **Deps**: —
- [ ] T010 [P] Design tokens from `Helix Admin.html` → `libs/ui` — `frontend/libs/ui/src/styles/tokens.css`
  - **AC**: all Helix custom properties (`--accent*`, `--bg-app`, `--panel*`, `--border*`, `--sidebar`, `--text*`, `--green/amber/red[-soft]`, `--shadow*`) extracted as the single token source; light/dark honored; documented usage guide; BEM convention (`hx-` prefix) written down
  - **Notes**: Constitution IX — tokens before components | **Size**: M | **Labels**: frontend, design-system, documentation | **Prio**: P0 | **Deps**: T009
- [ ] T011 UI primitives (BEM, token-only): button, input, select, table, dialog, toast, badge, tabs — `frontend/libs/ui/src/components/`
  - **AC**: each component pure BEM (`hx-button`, `hx-button--danger`, `hx-table__row` …); zero hard-coded colors; keyboard + ARIA per WCAG 2.1 AA; storybook-style demo page
  - **Test**: component unit tests + axe checks
  - **Size**: L | **Labels**: frontend, design-system | **Prio**: P0 | **Deps**: T010
- [ ] T012 [P] CI backend pipeline — `.github/workflows/backend.yml`
  - **AC**: fmt, clippy `-D warnings`, tests, cargo audit/deny, SQLx offline check, migration dry-run vs scratch DB, image build; blocks merge
  - **Size**: M | **Labels**: ci-cd, backend | **Prio**: P0 | **Deps**: T001, T003
- [ ] T013 [P] CI frontend pipeline + widget bundle-budget gate — `.github/workflows/frontend.yml`
  - **AC**: lint/test/build both apps; widget gz budget (~50 KB) fails the build when exceeded; secret-leak scan
  - **Size**: S | **Labels**: ci-cd, frontend | **Prio**: P0 | **Deps**: T009
- [ ] T014 [P] Dockerfiles + compose parity docs — `infra/Dockerfile.backend`, `infra/Dockerfile.frontend`, `README.md`
  - **AC**: images build in CI; README quickstart matches specs/…/quickstart.md boot section
  - **Size**: S | **Labels**: infrastructure, documentation, good-first-issue | **Prio**: P1 | **Deps**: T002, T012, T013

**Checkpoint M0**: quickstart V-M0 passes — deployable walking skeleton.

---

## Phase 2: Foundational — Milestone M1 (Identity, Tenancy, RBAC, Audit) — BLOCKS all user stories

- [ ] T015 Identity migrations + models (UserIdentity, Session) — `backend/migrations/`, `backend/crates/modules/identity/`
  - **AC**: argon2id hashing; sessions opaque server-side w/ idle+absolute expiry (research R-09)
  - **Size**: M | **Labels**: backend, database, security | **Prio**: P0 | **Deps**: T003, T008
- [ ] T016 Signup/login/logout/session revocation endpoints — `backend/crates/modules/identity/api.rs`
  - **AC**: FR-AUTH-001/002; lockout 5×/15 min (FR-AUTH-006); events → audit
  - **Test**: API tests incl. lockout + revocation immediacy
  - **Size**: M | **Labels**: backend, api, security | **Prio**: P0 | **Deps**: T015
- [ ] T017 [P] Email verification + password reset flows — `backend/crates/modules/identity/`
  - **AC**: time-limited tokens; Mailhog verified locally (FR-AUTH-001/003)
  - **Size**: M | **Labels**: backend, security | **Prio**: P0 | **Deps**: T015
- [ ] T018 [P] TOTP 2FA (setup/verify; platform-mandatory, tenant-enforceable) — `backend/crates/modules/identity/twofa.rs`
  - **AC**: FR-AUTH-004; secrets encrypted; recovery codes
  - **Size**: M | **Labels**: backend, security | **Prio**: P1 | **Deps**: T016
- [ ] T019 [P] Scoped API credentials — `backend/crates/modules/identity/credentials.rs`
  - **AC**: FR-AUTH-007; hashed, shown once, last-used, revocable; scopes ⊆ creator perms
  - **Size**: M | **Labels**: backend, api, security | **Prio**: P1 | **Deps**: T016, T021
- [ ] T020 Tenancy module: Tenant model, lifecycle state machine, settings — `backend/crates/modules/tenancy/`
  - **AC**: FR-ORG-001/002/004/005; two-step Owner deletion + 30-day purge scheduling; settings include session-window default 30 min (R-13)
  - **Test**: lifecycle transition unit tests
  - **Size**: L | **Labels**: backend, database | **Prio**: P0 | **Deps**: T015
- [ ] T021 RBAC: permission catalog seed, role→permission mapping, Authorize service — `backend/crates/modules/rbac/`
  - **AC**: all 10 roles seeded as data; `authorize(actor, permission, scope)` called from application services; deny-by-default
  - **Test**: role×permission matrix unit tests
  - **Size**: L | **Labels**: backend, security | **Prio**: P0 | **Deps**: T020
- [ ] T022 TenantScope + Postgres RLS enforcement layer — `backend/crates/shared/db/`
  - **AC**: repos require `TenantScope` param (compile-time); RLS policies on all tenant tables; session tenant set at pool checkout (research R-03)
  - **Test**: RLS bypass attempt tests
  - **Size**: L | **Labels**: backend, database, security | **Prio**: P0 | **Deps**: T020
- [ ] T023 Tenant-isolation test suite as permanent CI gate — `backend/tests/isolation/`
  - **AC**: cross-tenant probes on every shipped endpoint → 403/404 never data; wired into CI as required check; extended each milestone
  - **Size**: M | **Labels**: testing, security, ci-cd | **Prio**: P0 | **Deps**: T022
- [ ] T024 Memberships + invitations + last-Owner guard — `backend/crates/modules/users/`
  - **AC**: FR-USER-001/003/004, FR-RBAC-004/005; role-change cache-bust ≤1 min
  - **Test**: last-Owner downgrade → 422; propagation timing test
  - **Size**: M | **Labels**: backend, api | **Prio**: P0 | **Deps**: T021
- [ ] T025 Audit module: in-txn sink, search APIs — `backend/crates/modules/audit/`
  - **AC**: FR-AUDIT-001..004; append-only; tenant + platform search; audit-coverage test walks domain-events catalog
  - **Size**: M | **Labels**: backend, security, observability | **Prio**: P0 | **Deps**: T008, T021
- [ ] T026 Tenant Switcher (assume/exit, scoped session, audit trail) — `backend/crates/modules/tenancy/switcher.rs`
  - **AC**: FR-RBAC-003; `Session.assumed_tenant_id`; every in-context action audit-tagged
  - **Test**: switcher role-grant matrix + audit assertions
  - **Size**: M | **Labels**: backend, security | **Prio**: P0 | **Deps**: T021, T025
- [ ] T027 Dashboard auth screens (login, signup, verify, reset, 2FA) — `frontend/apps/dashboard/src/app/auth/`
  - **AC**: BEM + Helix tokens; a11y AA; session handling in `libs/data-access` interceptor
  - **Size**: M | **Labels**: frontend | **Prio**: P0 | **Deps**: T011, T016
- [ ] T028 Dashboard shell: role-filtered nav, tenant-context indicator, switcher banner — `frontend/apps/dashboard/src/app/shell/`
  - **AC**: nav items hidden without permission (server still enforces); persistent amber switcher banner (`hx-context-banner--assumed`) with one-click exit
  - **Size**: M | **Labels**: frontend | **Prio**: P0 | **Deps**: T027
- [ ] T029 [P] Tenant admin UI: users, roles, invitations, audit log — `frontend/libs/feature-admin/`
  - **AC**: US2 scenario 3 (invitee lands with exact role permissions); audit search UI
  - **Size**: L | **Labels**: frontend | **Prio**: P0 | **Deps**: T028, T024, T025
- [ ] T030 [P] Platform UI: tenant directory, provisioning, platform users — `frontend/libs/feature-platform/`
  - **AC**: US6 scenario 1 (provision → invitation); role-scoped visibility (Sales/Finance limits)
  - **Size**: M | **Labels**: frontend | **Prio**: P1 | **Deps**: T028, T026

**Checkpoint M1**: quickstart V-M1 passes — secure multi-tenant admin system; isolation gate live.

---

## Phase 3: Milestone M2 — Conversations, Widget, Human-Only Chat (US3 substrate, US2 widget setup, US1 substrate)

- [ ] T031 [P] [US1] Customers module: profiles, merge, attributes, GDPR delete — `backend/crates/modules/customers/`
  - **AC**: FR-CUST-001..005; merge chain reads; purge ≤30 d job
  - **Test**: merge + GDPR integration tests | **Size**: M | **Labels**: backend, database | **Prio**: P0 | **Deps**: T022
- [ ] T032 [US1] Conversation + Message models & state machine — `backend/crates/modules/conversations/`
  - **AC**: statuses/transitions per data-model.md; `seq` unique per conversation; internal notes never public; all transitions emit events
  - **Test**: exhaustive transition unit tests | **Size**: L | **Labels**: backend, database | **Prio**: P0 | **Deps**: T031
- [ ] T033 [US1] Conversation REST APIs (list/search/detail/messages/ops) — per `contracts/rest-api.md`
  - **AC**: FR-CONV-004/005/006; filters typed; message POST idempotent
  - **Size**: M | **Labels**: backend, api | **Prio**: P0 | **Deps**: T032, T006
- [ ] T034 [US1] WebSocket gateway + auth frames + heartbeat — `backend/crates/modules/conversations/realtime/`
  - **AC**: protocol per `contracts/realtime.md`; widget + inbox surfaces; SSE fallback
  - **Size**: L | **Labels**: backend, api | **Prio**: P0 | **Deps**: T032
- [ ] T035 [US1] Redis pub/sub fan-out + seq-cursor resume/replay — `backend/crates/modules/conversations/realtime/`
  - **AC**: node-kill test: reconnect + replay, zero loss/dupe after client dedupe (V-M2 scenario 3)
  - **Test**: multi-node integration test | **Size**: L | **Labels**: backend, infrastructure | **Prio**: P0 | **Deps**: T034
- [ ] T036 [US2] Widget session tokens + widget REST surface — `backend/crates/modules/integrations/widget_auth.rs`
  - **AC**: short-lived signed tokens (R-09); identity assertion; per-token rate limits (1 msg/s, burst 5)
  - **Size**: M | **Labels**: backend, security, api | **Prio**: P0 | **Deps**: T032
- [ ] T037 [US2] Widget app: chat UI, streaming-ready, theming, offline states — `frontend/apps/widget/`
  - **AC**: BEM (`hx-widget__…`) with Helix tokens; ≤ budget; a11y AA embedded in third-party page; reconnect/resume via `libs/realtime`
  - **Test**: Playwright vs fixture page | **Size**: L | **Labels**: frontend | **Prio**: P0 | **Deps**: T011, T036, T034
- [ ] T038 [P] [US2] Widget config API + embed snippet + install detection — `backend/crates/modules/integrations/`, `frontend/libs/feature-admin/widget-config/`
  - **AC**: FR-INT-001; US2 flow step 5 | **Size**: M | **Labels**: backend, frontend, api | **Prio**: P1 | **Deps**: T036
- [ ] T039 [US3] Agent inbox: queues, conversation view, context panel, notes/tags — `frontend/libs/feature-inbox/`
  - **AC**: mine/unassigned/all; live updates; customer panel; BEM feature blocks
  - **Test**: Playwright widget↔inbox round-trip ≤500 ms p95 smoke | **Size**: L | **Labels**: frontend | **Prio**: P0 | **Deps**: T033, T035, T028
- [ ] T040 [P] [US3] Escalation queue v0 (manual claim) + agent availability — `backend/crates/modules/escalations/`, `backend/crates/modules/users/agent_status.rs`
  - **AC**: queue entry → inbox alert ≤5 s; availability states drive eligibility
  - **Size**: M | **Labels**: backend, api | **Prio**: P0 | **Deps**: T032
- [ ] T041 [P] [US1] CSAT (1–5 stars + comment) + auto-close job — `backend/crates/modules/conversations/csat.rs`
  - **AC**: FR-CONV-007/008; customer notified pre-close; tenant toggle
  - **Size**: S | **Labels**: backend | **Prio**: P1 | **Deps**: T032
- [ ] T042 [P] [US3] Notifications v0: center, real-time alerts, email sender — `backend/crates/modules/notifications/`, `frontend/libs/feature-admin/notifications/`
  - **AC**: FR-NOTIF-001/002 core; in-app + email channels
  - **Size**: M | **Labels**: backend, frontend | **Prio**: P1 | **Deps**: T008, T035
- [ ] T095 [P] [US2] Tenant settings UI — `frontend/libs/feature-admin/settings/`
  - **AC**: FR-SET-001 surfaces: org profile, branding (widget/email), business hours + holidays, locale/timezone, escalation & auto-close policies, CSAT toggle, session window (default 30 min), retention windows, security policy (2FA enforcement, session limits); all changes audited with before/after (FR-SET-003); BEM + Helix tokens
  - **Test**: settings round-trip API tests; audit assertions
  - **Size**: L | **Labels**: frontend, backend, api | **Prio**: P1 | **Deps**: T020, T028

**Checkpoint M2**: quickstart V-M2 passes — sellable human live-chat product.

---

## Phase 4: Milestone M3 — AI Foundation (US1 core, US5)

- [ ] T043 [US1] Provider capability trait + normalized models + error taxonomy — `backend/crates/ai-providers/src/lib.rs`
  - **AC**: exactly per `contracts/ai-provider-interface.md`; no vendor SDKs
  - **Size**: M | **Labels**: backend, api | **Prio**: P0 | **Deps**: T001
- [ ] T044 [P] [US1] OpenAI adapter — `backend/crates/ai-providers/src/openai.rs`
  - **AC**: chat/stream/tools/embed; recorded-fixture contract suite green
  - **Size**: M | **Labels**: backend | **Prio**: P0 | **Deps**: T043
- [ ] T045 [P] [US1] Anthropic adapter — `backend/crates/ai-providers/src/anthropic.rs`
  - **AC**: same fixture suite, identical normalized output
  - **Size**: M | **Labels**: backend | **Prio**: P0 | **Deps**: T043
- [ ] T046 [P] [US1] Gemini adapter — `backend/crates/ai-providers/src/gemini.rs`
  - **AC**: same fixture suite | **Size**: M | **Labels**: backend | **Prio**: P0 | **Deps**: T043
- [ ] T047 [US1] FailoverExecutor + routing policy resolution — `backend/crates/ai-providers/src/failover.rs`
  - **AC**: taxonomy-driven failover; mid-stream restart; `failover_from` recorded; SC-009 drill passes
  - **Test**: provider kill-switch integration test | **Size**: M | **Labels**: backend | **Prio**: P0 | **Deps**: T044, T045, T046
- [ ] T048 [P] [US6] Provider config + model catalog + routing admin (API+UI) — `backend/crates/modules/platform/providers.rs`, `frontend/libs/feature-platform/providers/`
  - **AC**: FR-PROV-003/006; envelope-encrypted keys, last-4 display, write-only
  - **Size**: M | **Labels**: backend, frontend, security | **Prio**: P0 | **Deps**: T043, T030
- [ ] T049 [US5] PromptVersion model + publish/rollback + optimistic draft lock — `backend/crates/modules/prompts/`
  - **AC**: FR-PROMPT-001..003/005; one published (partial unique index); rollback ≤1 min; concurrent-edit 409
  - **Test**: publish/rollback/conflict API tests | **Size**: M | **Labels**: backend, database, api | **Prio**: P0 | **Deps**: T022
- [ ] T050 [US5] Prompt editor UI (sections, history, diff, publish/rollback) — `frontend/libs/feature-ai-config/prompts/`
  - **AC**: structured sections; version history with change notes; audited actions surfaced
  - **Size**: L | **Labels**: frontend | **Prio**: P0 | **Deps**: T049, T028
- [ ] T051 [US5] Sandbox sessions (draft testing, live-isolated) — `backend/crates/modules/prompts/sandbox.rs`, `frontend/libs/feature-ai-config/sandbox/`
  - **AC**: FR-PROMPT-004; zero live-traffic impact (US5 scenario 1)
  - **Size**: M | **Labels**: backend, frontend | **Prio**: P1 | **Deps**: T049, T053
- [ ] T052 [US1] Deterministic context assembler — `backend/crates/modules/ai/assembler.rs`
  - **AC**: pure function of ordered inputs; snapshot + hash persisted (R-06); byte-equality CI test
  - **Size**: L | **Labels**: backend | **Prio**: P0 | **Deps**: T049
- [ ] T053 [US1] AI reply pipeline: streaming into WS, rolling summarization — `backend/crates/modules/ai/pipeline.rs`
  - **AC**: first token ≤3 s p95 staging; `message.delta` frames per realtime contract; queued-messages-considered edge case; replies honor AgentConfiguration.language_policy — respond in customer language when enabled, else tenant default (FR-AI-009)
  - **Size**: L | **Labels**: backend | **Prio**: P0 | **Deps**: T052, T047, T035
- [ ] T054 [US1] AiExecution timeline persistence + metering events — `backend/crates/modules/ai/execution.rs`
  - **AC**: FR-AI-007 complete record; `ai.execution_completed` event with execution-id idempotency key (R-11)
  - **Test**: timeline-coverage test (100% of turns) | **Size**: M | **Labels**: backend, database, observability | **Prio**: P0 | **Deps**: T053
- [ ] T055 [P] [US1] Timeline viewer UI in conversation detail — `frontend/libs/feature-inbox/timeline/`
  - **AC**: per-step timings, retrievals, model calls, decisions; Manager+ visibility
  - **Size**: M | **Labels**: frontend, observability | **Prio**: P1 | **Deps**: T054, T039
- [ ] T056 [US1] Confidence scoring + threshold behaviors + escalation triggers v1 — `backend/crates/modules/ai/confidence.rs`
  - **AC**: FR-AI-005/006/008; explicit-request always honored; triggers → M2 queue
  - **Test**: trigger matrix tests | **Size**: M | **Labels**: backend | **Prio**: P0 | **Deps**: T053, T040
- [ ] T057 [P] [US1] Agent-config UI (constraints, thresholds, rules) — `frontend/libs/feature-ai-config/agent/`
  - **AC**: blocked topics, disclaimers, business-hours behavior, threshold sliders; language policy config (enabled-language list per FR-AI-009)
  - **Size**: M | **Labels**: frontend | **Prio**: P1 | **Deps**: T056, T028

**Checkpoint M3**: quickstart V-M3 passes — AI answers (no RAG) with full debuggability.

---

## Phase 5: Milestone M4 — Knowledge & RAG (US1 completion, US4)

- [ ] T058 [US4] KnowledgeSource/Collection models + lifecycle APIs — `backend/crates/modules/knowledge/`
  - **AC**: FR-KB-001..003/005/007; statuses with actionable failure reasons; quotas
  - **Size**: M | **Labels**: backend, database, api | **Prio**: P0 | **Deps**: T022
- [ ] T059 [US4] S3 upload flow (presigned) + file storage — `backend/crates/modules/knowledge/storage.rs`
  - **AC**: size/type validation upfront; MinIO locally | **Size**: S | **Labels**: backend, infrastructure | **Prio**: P0 | **Deps**: T058
- [ ] T060 [US4] Ingestion job queue + workers (fetch→extract→segment→embed→swap) — `backend/crates/modules/knowledge/ingestion/`
  - **AC**: `FOR UPDATE SKIP LOCKED` (R-08); atomic generation swap; retries; progress events; PDF/Word/HTML/MD/text + URL/crawl bounded
  - **Test**: format-matrix + corrupt-file tests (US4 scenario 3) | **Size**: L | **Labels**: backend, database | **Prio**: P0 | **Deps**: T058, T059, T047
- [ ] T061 [US1] Hybrid retrieval: pgvector HNSW + FTS + rank fusion + thresholds — `backend/crates/modules/knowledge/retrieval.rs`
  - **AC**: ≤1 s p95 at 10 GB corpus; tenant+collection scoping in query layer; isolation test (cross-tenant embeddings)
  - **Size**: L | **Labels**: backend, database | **Prio**: P0 | **Deps**: T060
- [ ] T062 [US1] RAG in context assembler + citations end-to-end — `backend/crates/modules/ai/assembler.rs`, widget/inbox citation rendering
  - **AC**: US1 scenarios 1–3; citations staff-always/customer-per-setting; honest fallback below threshold; ≤5 min freshness (FR-KB-004)
  - **Size**: M | **Labels**: backend, frontend | **Prio**: P0 | **Deps**: T061, T053
- [ ] T063 [P] [US4] Retrieval test tool (API + UI) — `backend/crates/modules/knowledge/tester.rs`, `frontend/libs/feature-ai-config/retrieval-test/`
  - **AC**: FR-KB-006 — question → ranked passages with scores
  - **Size**: M | **Labels**: backend, frontend | **Prio**: P1 | **Deps**: T061
- [ ] T064 [P] [US4] Knowledge management UI (sources, collections, statuses, quotas) — `frontend/libs/feature-ai-config/knowledge/`
  - **AC**: US2 scenario 2 (status visibility); disable/delete/re-ingest flows
  - **Size**: L | **Labels**: frontend | **Prio**: P0 | **Deps**: T058, T028

**Checkpoint M4**: quickstart V-M4 + full US1 journey — grounded, cited AI product.

---

## Phase 6: Milestone M5 — Handoff Maturity, Tools, Webhooks (US3 completion)

- [ ] T065 [US3] Skill-tag routing + load fallback + tenant manual-claim toggle — `backend/crates/modules/escalations/routing.rs`
  - **AC**: FR-USER-006 exactly; requeue-with-priority on agent disconnect
  - **Test**: routing simulation suite | **Size**: M | **Labels**: backend | **Prio**: P0 | **Deps**: T040, T024
- [ ] T066 [US3] Offline behavior: expectation message / contact capture — `backend/crates/modules/escalations/offline.rs`
  - **AC**: US3 scenario 3; follow-up flag | **Size**: S | **Labels**: backend | **Prio**: P0 | **Deps**: T065
- [ ] T067 [US3] Handoff UX: AI summary + suggested knowledge panel, continuity messaging, return-to-AI — `frontend/libs/feature-inbox/handoff/`, `backend/crates/modules/escalations/`
  - **AC**: US3 scenarios 2/4; summary from rolling summarizer; agent→AI transition recorded
  - **Size**: L | **Labels**: frontend, backend | **Prio**: P0 | **Deps**: T065, T053, T062
- [ ] T068 [P] [US3] AI-suggested replies (agent-approved) — `backend/crates/modules/ai/suggestions.rs`, inbox UI
  - **AC**: suggestion never auto-sends; marked AI-generated
  - **Size**: M | **Labels**: backend, frontend, enhancement | **Prio**: P2 | **Deps**: T067
- [ ] T094 [P] [US1] Cross-conversation memory extraction (tenant opt-in) — `backend/crates/modules/ai/memory.rs`
  - **AC**: on `conversation.closed`, durable customer facts extracted to Customer profile per spec §11.4; tenant opt-in setting (default off); facts visible/editable in customer panel — never a hidden store; whitelisted into context assembly (T052) when enabled
  - **Notes**: consumes outbox event per contracts/domain-events.md | **Test**: opt-out ⇒ no extraction; edited fact wins over re-extraction
  - **Size**: M | **Labels**: backend, frontend, enhancement | **Prio**: P2 | **Deps**: T054, T031
- [ ] T069 [US1] Tool registry + JSON-Schema validation + invocation engine — `backend/crates/modules/tools/`
  - **AC**: FR-INT-005, FR-AI-002; timeouts, per-conversation rate limits, sandboxed egress; invocations in timeline; starter tools seeded
  - **Test**: invalid input/output, timeout, degrade-gracefully tests (V-M5 scenario 3)
  - **Size**: L | **Labels**: backend, security, api | **Prio**: P0 | **Deps**: T054, T047
- [ ] T070 [P] [US1] Tool management UI — `frontend/libs/feature-ai-config/tools/`
  - **AC**: register/approve/enable/disable; invocation visibility
  - **Size**: M | **Labels**: frontend | **Prio**: P1 | **Deps**: T069
- [ ] T071 [P] [US3] Outbound webhooks: HMAC signing, retries, delivery log (API+UI) — `backend/crates/modules/integrations/webhooks.rs`
  - **AC**: FR-INT-002 event catalog; backoff retries; signature verification documented
  - **Size**: M | **Labels**: backend, api, integrations | **Prio**: P1 | **Deps**: T008
- [ ] T072 [P] [US3] Notification preferences matrix + quiet hours + email templates — `backend/crates/modules/notifications/preferences.rs`
  - **AC**: FR-NOTIF-003/004; security categories locked-on
  - **Size**: M | **Labels**: backend, frontend | **Prio**: P2 | **Deps**: T042

**Checkpoint M5**: quickstart V-M5 + full US3 journey.

---

## Phase 7: Milestone M6 — Analytics & Billing (US7, US8)

- [ ] T073 [US7] Aggregation pipeline (rollups, ≤5 min freshness, labels) — `backend/crates/modules/analytics/aggregation.rs`
  - **AC**: FR-ANLT-001/006; consumes domain events via outbox; read-replica-ready queries
  - **Size**: L | **Labels**: backend, database | **Prio**: P0 | **Deps**: T008, T054
- [ ] T074 [US7] Tenant dashboards: KPIs, period-over-period, segmentation — `frontend/libs/feature-analytics/`
  - **AC**: US7 scenario 1; segments: channel/tag/agent/prompt-version/attributes; freshness label
  - **Size**: L | **Labels**: frontend | **Prio**: P0 | **Deps**: T073
- [ ] T075 [P] [US7] Topic clustering + knowledge-gap surfacing — `backend/crates/modules/analytics/topics.rs`
  - **AC**: FR-ANLT-003, FR-KB-008; embeddings via provider abstraction
  - **Size**: L | **Labels**: backend, enhancement | **Prio**: P1 | **Deps**: T073, T061
- [ ] T076 [P] [US7] Exports (CSV + async API) + platform cross-tenant aggregates — `backend/crates/modules/analytics/exports.rs`
  - **AC**: FR-ANLT-004/005; platform surfaces contain zero conversation content
  - **Size**: M | **Labels**: backend, api | **Prio**: P1 | **Deps**: T073
- [ ] T077 [US8] Plan catalog + subscriptions + trials — `backend/crates/modules/billing/plans.rs`
  - **AC**: FR-BILL-001/005/006; upgrade prorates now, downgrade next period with validation
  - **Size**: M | **Labels**: backend, database, api | **Prio**: P0 | **Deps**: T020
- [ ] T078 [US8] Idempotent metering consumer + usage APIs + thresholds — `backend/crates/modules/billing/metering.rs`
  - **AC**: SC-010 (unique idempotency key, replay-safe); ≤1 h lag; 80/100% notifications; limit behaviors incl. graceful widget fallback
  - **Test**: property-based double-billing tests | **Size**: L | **Labels**: backend, database | **Prio**: P0 | **Deps**: T077, T054, T042
- [ ] T079 [US8] Invoices + payment processor integration + dunning — `backend/crates/modules/billing/invoicing.rs`
  - **AC**: itemized invoices; processor webhooks signature-verified; dunning retry→notify→grace→suspend, all audit-logged
  - **Test**: dunning state-machine + invoice snapshot tests | **Size**: L | **Labels**: backend, integrations | **Prio**: P0 | **Deps**: T078
- [ ] T080 [P] [US8] Tenant suspension/reactivation mode — `backend/crates/modules/tenancy/suspension.rs`
  - **AC**: read-only dashboard, widget offline message, no data deletion in grace
  - **Size**: M | **Labels**: backend, frontend | **Prio**: P1 | **Deps**: T079
- [ ] T081 [P] [US8] Billing UIs: Owner (plan/usage/invoices) + platform Finance (plans/exceptions/credits) — `frontend/libs/feature-admin/billing/`, `frontend/libs/feature-platform/billing/`
  - **AC**: US8 scenarios 1–3; adjustments audit-logged
  - **Size**: L | **Labels**: frontend | **Prio**: P0 | **Deps**: T077, T079

**Checkpoint M6**: quickstart V-M6 + US7/US8 journeys.

---

## Phase 8: Milestone M7 — Operations, Hardening, GA (US6 completion)

- [ ] T082 [US6] Feature flags: model, resolution (tenant>plan>default), ≤5 min propagation, admin UI — `backend/crates/modules/flags/`, `frontend/libs/feature-platform/flags/`
  - **AC**: FR-FLAG-001..004; fail-safe defaults; Redis bust; history audited
  - **Size**: M | **Labels**: backend, frontend | **Prio**: P0 | **Deps**: T008, T030
- [ ] T083 [US6] Health dashboard + incidents + tenant banners — `backend/crates/modules/platform/health.rs`, `frontend/libs/feature-platform/health/`
  - **AC**: FR-HEALTH-001..004; provider status/cost; incident lifecycle → widget/dashboard banners
  - **Size**: L | **Labels**: backend, frontend, observability | **Prio**: P0 | **Deps**: T047, T042
- [ ] T084 [P] [US6] SLO burn-rate alerting + AI drift monitoring — `infra/` alert rules, `backend/crates/modules/platform/alerts.rs`
  - **AC**: NFR-MON-002/003; confidence-drift + escalation-anomaly alerts to on-call
  - **Size**: M | **Labels**: observability, infrastructure | **Prio**: P1 | **Deps**: T083
- [ ] T085 [P] [US6] Backups/PITR validation + DR runbooks + per-tenant restore — `docs/runbooks/`
  - **AC**: NFR-BKP-*, NFR-DR-*; rehearsal on staging timed vs RPO ≤5 min / RTO ≤4 h
  - **Size**: M | **Labels**: infrastructure, documentation | **Prio**: P0 | **Deps**: T002
- [ ] T086 [P] i18n completion: externalized strings, second locale, RTL in widget/inbox — `frontend/` i18n
  - **AC**: NFR-I18N-001..003 | **Size**: L | **Labels**: frontend | **Prio**: P1 | **Deps**: T037, T039
- [ ] T087 [P] WCAG 2.1 AA audit + fixes (dashboard + widget) — cross-cutting
  - **AC**: axe CI green; manual keyboard/screen-reader pass documented
  - **Size**: M | **Labels**: frontend, testing | **Prio**: P1 | **Deps**: T086
- [ ] T088 Load/soak to scale targets + N+1 sweep + index tuning — `backend/tests/load/`
  - **AC**: 10k concurrent conversations, 1M conv/month soak within SLOs; zero drops during rolling deploy
  - **Size**: L | **Labels**: testing, backend, database | **Prio**: P0 | **Deps**: T053, T061, T073
- [ ] T089 Security hardening: pen-test remediation, rate-limit tuning, prompt-injection red-team, SOC 2 evidence scaffolding — cross-cutting
  - **AC**: zero outstanding criticals; red-team suite in CI (NFR-SEC-003/005)
  - **Size**: L | **Labels**: security, testing | **Prio**: P0 | **Deps**: T069
- [ ] T093 [US2] SSO via OIDC federation for eligible plans — `backend/crates/modules/identity/sso.rs`
  - **AC**: FR-AUTH-005 — tenant Admin configures IdP (issuer, client, mapping); login via IdP lands user with existing membership role; plan-gated via feature flag; setup + logins audit-logged
  - **Notes**: OIDC only at GA (SAML post-GA); reuse session model from T015 | **Test**: mock-IdP integration tests incl. rejected/unmapped users
  - **Size**: L | **Labels**: backend, security, enhancement | **Prio**: P1 | **Deps**: T016, T082
- [ ] T096 [US6] Retention purge jobs + tenant data export — `backend/crates/modules/tenancy/retention.rs`
  - **AC**: scheduled purge honors tenant retention windows (FR-SET-001, A-05); Owner-triggered full tenant export (async, S3-delivered, audit-logged) per NFR-SEC-005/GDPR; audit-log export param added to T025 APIs (FR-AUDIT-004)
  - **Test**: purge boundary tests (window edge ±1 day); export completeness snapshot
  - **Size**: L | **Labels**: backend, security, database | **Prio**: P1 | **Deps**: T020, T025, T031
- [ ] T090 k8s manifests + rolling deploy with WS drain + CDN — `infra/k8s/`
  - **AC**: NFR-AVAIL-002 (no interrupted conversations); staging auto-deploy on main; tagged prod releases
  - **Size**: L | **Labels**: infrastructure, ci-cd, deployment | **Prio**: P0 | **Deps**: T035, T014
- [ ] T091 [P] Module documentation: purpose/responsibilities/interfaces/deps/data/extension points per module — `backend/crates/modules/*/README.md`
  - **AC**: Constitution Documentation section satisfied for all 18 modules
  - **Size**: M | **Labels**: documentation, good-first-issue | **Prio**: P1 | **Deps**: —
- [ ] T092 GA checklist: execute SC-001..SC-012 verification + record results — `docs/ga-verification.md`
  - **AC**: all 12 success criteria evidenced | **Size**: M | **Labels**: testing, documentation | **Prio**: P0 | **Deps**: T082–T090

**Checkpoint M7**: quickstart V-M7 — GA.

---

## Dependency graph

```text
Milestone order (strict): M0 → M1 → M2 → M3 → M4 → M5 → M6 → M7
                                      └── AI work forbidden before M0–M2 complete ──┘

M0: T001 ─┬─ T003 ─ T008        T009 ─ T010 ─ T011
          ├─ T004 ─ T005 ─ T006  T012, T013, T014 (CI/CD, parallel)
          └─ T007
M1: T015 ─ T016 ─ {T017,T018,T019}     T020 ─ T021 ─ {T024,T026}
    T020 ─ T022 ─ T023(GATE)           T008 ─ T025
    T011 ─ T027 ─ T028 ─ {T029,T030}
M2: T022 ─ T031 ─ T032 ─ {T033,T034,T036,T040,T041}
    T034 ─ T035 ─ {T037,T039,T042}     T036 ─ {T037,T038}
M3: T043 ─ {T044,T045,T046} ─ T047     T049 ─ {T050,T052}
    T052+T047+T035 ─ T053 ─ {T054,T056} ─ {T055,T057}   T051←T049+T053   T048←T043
M4: T058 ─ T059 ─ T060(+T047 embeddings) ─ T061 ─ {T062(+T053),T063}   T064←T058
M5: T040+T024 ─ T065 ─ {T066,T067(+T053,T062)} ─ T068
    T054+T047 ─ T069 ─ T070      T008 ─ T071     T042 ─ T072
M6: T008+T054 ─ T073 ─ {T074,T075(+T061),T076}
    T020 ─ T077 ─ T078(+T054,T042) ─ T079 ─ {T080,T081}
M7: T082←T008+T030   T083←T047+T042 ─ T084   T085,T086─T087,T091 (parallel)
    T088←T053+T061+T073   T089←T069   T090←T035+T014   T092←all M7
```

## Parallel execution examples

- **M0**: after T001+T002 → {T003, T004, T007, T009} in parallel; then {T005, T010, T012, T013}.
- **M1**: {T017, T018, T019} parallel post-T016; frontend track T027–T030 parallel to backend T020–T026.
- **M3**: three provider adapters T044/T045/T046 fully parallel; prompt track (T049–T051) parallel to provider track (T043–T048).
- **M6**: analytics track (T073–T076) and billing track (T077–T081) are independent teams.

## Implementation strategy

- **MVP** = M0→M4 critical path only (T001–T005, T008–T011, T015–T016, T020–T023, T031–T037, T039–T040, T043–T047, T049, T052–T054, T056, T058–T062): US1 end-to-end — a customer gets a grounded, cited, streamed AI answer with escalation safety net.
- Ship each checkpoint to staging; demo per milestone definition of "usable" (plan.md).
- Isolation gate (T023) and audit coverage (T025) run on every PR from M1 forward; never waived.
- Tasks sized S/M fit the 2–8 h target; L tasks carry internal subtask checklists in their ACs — split into sub-issues at filing time if a single engineer-day is exceeded.

## Phase 9: Convergence

- [ ] T097 Trim the widget app bootstrap until `ng build widget` fits its configured budget per T013/T037 (contradicts) — `frontend/apps/widget/`
  - **AC**: `pnpm exec ng build widget` succeeds with initial bundle at or under the 55/60 kB budgets set in `frontend/angular.json`; no budget-check regression for `dashboard`
  - **Notes**: current placeholder shell reports 94.07 kB (exceeds by 34-39 kB) — audit unused zone.js features/polyfills, verify `styles.css` isn't duplicated, and confirm the standalone bootstrap isn't pulling in router/http providers it doesn't use
  - **Size**: S | **Labels**: frontend, ci-cd | **Prio**: P0 | **Deps**: T009, T013
- [ ] T098 Add missing `@types/jasmine` (and any other absent test-runner types) so `ng test` compiles per T011/T013 (partial) — `frontend/package.json`
  - **AC**: `pnpm exec ng test dashboard --watch=false --browsers=ChromeHeadless` and `pnpm exec ng test widget --watch=false --browsers=ChromeHeadless` both compile and run without TS2593/TS2304 errors; existing `libs/ui/src/components/components.spec.ts` and `apps/dashboard/src/app.component.spec.ts` execute and pass
  - **Size**: S | **Labels**: frontend, testing, good-first-issue | **Prio**: P0 | **Deps**: T009, T011
- [ ] T099 Fix ESLint project-service coverage so `libs/**` (and all of `apps/**`) lint cleanly per T013 (partial) — `frontend/eslint.config.mjs`
  - **AC**: `pnpm exec eslint "apps/**/*.ts" "libs/**/*.ts"` runs with zero "not found by the project service" parsing errors across every existing `.ts` file in `apps/` and `libs/`
  - **Size**: S | **Labels**: frontend, ci-cd, good-first-issue | **Prio**: P1 | **Deps**: T013

## Phase 10: Convergence

- [ ] T100 Add missing `@angular/platform-browser-dynamic` dependency so `ng test` actually runs per T098 (partial) — `frontend/package.json`
  - **AC**: `pnpm exec ng test dashboard --watch=false --browsers=ChromeHeadless` and `pnpm exec ng test widget --watch=false --browsers=ChromeHeadless` both execute (not just compile) with zero "Module not found" errors, and all existing specs (`Dashboard shell`, each `libs/ui/src/components/components.spec.ts` entry) report passed
  - **Notes**: T098 correctly added `@types/jasmine`, which fixed the TS2593 compile errors, but karma's test-init script requires `@angular/platform-browser-dynamic/testing` at runtime and that package was never added as a dependency; add it at the same version range as the other `@angular/*` packages (^19.0.0)
  - **Size**: S | **Labels**: frontend, testing, good-first-issue | **Prio**: P0 | **Deps**: T098
- [ ] T101 Actually trim the widget bundle instead of only raising its budget per T097/T037 (partial) — `frontend/apps/widget/`, `frontend/angular.json`
  - **AC**: `pnpm exec ng build widget` succeeds with a materially smaller initial bundle than the current 94.07 kB (verify `main.ts`'s bootstrap doesn't pull in unused `provideRouter`/`provideHttpClient`, confirm `zone.js`/`zone.js/testing` aren't both loaded in the production build, dedupe `styles.css` against the dashboard); only keep the budget at or above the trimmed result — do not leave the ceiling at 100/105 kB if trimming gets the bundle meaningfully lower
  - **Notes**: T097 raised `maximumWarning`/`maximumError` from 55/60 kB to 100/105 kB without changing the bundle size at all (still 94.07 kB raw) — the gate now passes but does nothing to keep the widget lean ahead of real feature work in T037
  - **Size**: S | **Labels**: frontend, ci-cd | **Prio**: P1 | **Deps**: T097
- [ ] T102 Remove the invalid `//` comment from `frontend/angular.json` per T097 edit hygiene (contradicts) — `frontend/angular.json`
  - **AC**: `frontend/angular.json` parses as strict JSON (e.g., `node -e "require('./angular.json')"` succeeds without a SyntaxError); `ng build widget` continues to work afterward
  - **Notes**: T097's edit left a `// Angular's optimized bootstrap...` line comment in the widget build target's options — Angular CLI's own reader tolerates JSONC but the file must stay valid JSON for other tooling (IDEs, other CI steps, `jq`, etc.); move any rationale into a comment in the task/README instead
  - **Size**: S | **Labels**: frontend, good-first-issue | **Prio**: P1 | **Deps**: T097
