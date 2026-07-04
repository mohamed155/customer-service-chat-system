# Quickstart & Validation Guide

How to run the platform locally and prove each milestone works. Contracts:
[rest-api](./contracts/rest-api.md) · [realtime](./contracts/realtime.md) ·
[events](./contracts/domain-events.md) · [providers](./contracts/ai-provider-interface.md).
Entities: [data-model.md](./data-model.md).

## Prerequisites

- Rust stable (≥1.85) + `cargo`, `sqlx-cli`
- Node LTS + pnpm (Angular CLI via `pnpm exec`)
- Docker + docker compose
- AI provider API keys (M3+ live testing only; recorded fixtures cover CI)

## Boot

```bash
docker compose -f infra/docker-compose.yml up -d   # postgres+pgvector, redis, minio, mailhog, otel
cd backend && sqlx migrate run && cargo run -p server
cd frontend && pnpm install --frozen-lockfile && pnpm exec ng serve dashboard    # widget: pnpm exec ng serve widget
```

Expected: API on :8080 (`GET /api/v1/health` → 200 with `X-Request-Id`),
dashboard on :4200, Mailhog UI :8025, MinIO console :9001, traces in local
Jaeger.

Seed dev data (idempotent): `cargo run -p server -- seed --env dev`
(roles/permissions, plan catalog, starter tools, demo tenant + users per role).

## Validation scenarios (cumulative by milestone)

### V-M0 Walking skeleton
1. `curl -i localhost:8080/api/v1/health` → 200, request ID header.
2. Hit an unknown route → error envelope exactly per rest-api.md.
3. Open Jaeger → the health request's trace spans server layers.
4. `cargo test` + `pnpm exec ng test` green; CI pipeline green on a PR.

### V-M1 Identity, tenancy, RBAC, audit
1. Sign up → verification email in Mailhog → verify → login → session works.
2. Invite one user per tenant role from the demo tenant; accept each; verify
   each sees exactly their role's navigation and gets 403s beyond it
   (role×endpoint matrix test automates this).
3. Isolation: as tenant-A admin, request tenant-B resources by ID → 404/403,
   never data. `cargo test -p tests --test isolation` green.
4. Platform user with switcher enters demo tenant → banner visible → actions
   taken appear in `GET /platform/audit-events` with acting context.
5. Try to downgrade the last Owner → 422.

### V-M2 Live chat (human-only)
1. Open the widget fixture page (embeds built widget against a third-party
   origin), start a conversation as a customer.
2. Login as Agent in the inbox → claim from queue → bidirectional messages,
   typing indicators both ways; relay feels instant (≤500 ms perf smoke
   asserts p95).
3. Kill the API process mid-conversation, restart → widget reconnects and
   replays; no lost/duplicated messages (seq dedupe).
4. Resolve → CSAT stars appear (tenant setting on) → submit 4★ + comment.
5. GDPR: delete the test customer → profile + content purge job recorded.

### V-M3 AI replies (no RAG yet)
1. Configure a provider key in platform settings; publish prompt v1 with a
   distinctive persona.
2. Customer message → streamed AI reply (first token <3 s), persona evident.
3. `GET /conversations/{id}/timeline` → assembly snapshot, model call
   (provider/model/tokens/latency), confidence, decision — complete.
4. Determinism: `cargo test -p ai --test determinism` (same inputs ⇒ same
   context hash).
5. Publish v2 with changed persona → new conversation reflects it ≤1 min;
   rollback → reverted; both actions in audit log.
6. Sandbox a draft → live conversations unaffected.
7. Failover drill: revoke primary provider key mid-conversation → reply
   completes via fallback; timeline shows `failover_from`.
8. Say "I want a human" → escalation queued, agent alerted ≤5 s.

### V-M4 Knowledge & RAG
1. Upload a PDF + add a URL source → statuses queued→processing→ready.
2. Ask a question answerable only from the PDF → grounded, cited answer ≤5
   min after ready.
3. Retrieval tester: same question → see ranked passages.
4. Ask something outside the knowledge → honest fallback (no fabrication),
   offer to escalate.
5. Delete the PDF source → same question now takes the fallback path.
6. Upload a corrupt file → failed status with actionable reason; other
   sources unaffected.
7. Isolation: tenant-B retrieval never returns tenant-A segments
   (`--test retrieval_isolation`).

### V-M5 Handoff maturity & tools
1. Tag an agent with skill `billing`; escalate a `billing`-tagged
   conversation → routes to that agent; untagged escalation → least-loaded
   agent; all agents offline → offline capture flow.
2. Handoff panel shows AI summary + suggested knowledge; agent returns
   conversation to AI; customer thread reads continuously.
3. Register a custom tool against the local echo fixture; AI invokes it;
   timeline shows validated input/output. Break the fixture (timeout) → AI
   explains/escalates gracefully.
4. Webhook subscription to a local receiver → events delivered, HMAC
   verifies; stop the receiver → retries with backoff in the delivery log.

### V-M6 Analytics & billing
1. Run the fixture conversation generator → dashboards show volume,
   resolution rate, escalation rate, CSAT with correct period-over-period;
   freshness label ≤5 min.
2. Topic view clusters the fixture themes; knowledge-gaps lists the
   deliberately uncovered topic.
3. Metering: replay the same `ai.execution_completed` event → exactly one
   UsageRecord (`--test metering_idempotency`).
4. Cross plan-limit at 80%/100% → notifications fire; hard-stop plan →
   widget degrades gracefully.
5. Simulated payment failure → dunning: retries → suspension (dashboard
   read-only, widget offline message) → payment → reactivation; every step
   audit-logged. Invoice itemizes plan + overage.

### V-M7 Operations & GA
1. Flip a feature flag for one tenant → visible in a running session ≤5 min,
   no restart.
2. Health dashboard reflects a provider outage drill; declare an incident →
   tenant banner appears; resolve → banner clears.
3. DR rehearsal on staging: PITR restore meets RPO ≤5 min / RTO ≤4 h
   (runbook timed).
4. Load: 10k concurrent conversations soak within SLO; zero dropped
   conversations during a rolling deploy.
5. Full SC-001..SC-012 checklist executed and recorded.

## Success criteria traceability

| Scenario set | Proves |
|---|---|
| V-M1 | SC-005 (isolation), SC-012 (audit) — initial surface |
| V-M2 | NFR-PERF-003, NFR-AVAIL-002 (replay), FR-CONV-*, FR-CUST-004 |
| V-M3 | SC-002, SC-006, SC-007, SC-009 |
| V-M4 | US1 scenarios, FR-KB-004/006, NFR-PERF-004, SC-005 (retrieval) |
| V-M5 | SC-004, US3 scenarios, FR-USER-006, FR-INT-002/005 |
| V-M6 | SC-010, US7/US8 scenarios, FR-ANLT-006 |
| V-M7 | SC-001, SC-003, SC-008, SC-011 + NFR-DR/BKP targets |
