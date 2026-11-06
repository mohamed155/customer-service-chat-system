# Implementation Plan: [FEATURE]

**Branch**: `[###-feature-name]` | **Date**: [DATE] | **Spec**: [link]

**Input**: Feature specification from `/specs/[###-feature-name]/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command. See `.specify/templates/plan-template.md` for the execution workflow.

## Summary

[Extract from feature spec: primary requirement + technical approach from research]

## Technical Context

<!--
  ACTION REQUIRED: Replace the content in this section with the technical details
  for the project. The structure here is presented in advisory capacity to guide
  the iteration process.
-->

**Language/Version**: [e.g., Python 3.11, Swift 5.9, Rust 1.75 or NEEDS CLARIFICATION]

**Primary Dependencies**: [e.g., FastAPI, UIKit, LLVM or NEEDS CLARIFICATION]

**Storage**: [if applicable, e.g., PostgreSQL, CoreData, files or N/A]

**Testing**: [e.g., pytest, XCTest, cargo test or NEEDS CLARIFICATION]

**Target Platform**: [e.g., Linux server, iOS 15+, WASM or NEEDS CLARIFICATION]

**Project Type**: [e.g., library/cli/web-service/mobile-app/compiler/desktop-app or NEEDS CLARIFICATION]

**Performance Goals**: [domain-specific, e.g., 1000 req/s, 10k lines/sec, 60 fps or NEEDS CLARIFICATION]

**Constraints**: [domain-specific, e.g., <200ms p95, <100MB memory, offline-capable or NEEDS CLARIFICATION]

**Scale/Scope**: [domain-specific, e.g., 10k users, 1M LOC, 50 screens or NEEDS CLARIFICATION]

**Performance Budget** (Constitution XI — required if hot paths are touched): [e.g., API p95 < 300 ms
excluding AI generation; first AI token p95 < 2.5 s, or N/A with reason]

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Mark each gate ✅ pass, ⚠ deviation (record in Complexity Tracking), or N/A with a one-line reason.
See `.specify/memory/constitution.md` for the full rule text.

- [ ] **I. Tenant isolation**: `tenant_id` on every tenant-owned table; `TenantContext` in repositories;
  RLS policies; composite `(tenant_id, id)` FKs; tenant-namespaced caches/keys/queues; cross-tenant tests
- [ ] **II. Secure by default**: permission declared per endpoint (permission, not role name);
  input validated at boundary; webhook signatures + replay protection; SSRF guard; audit records;
  no secrets client-side or in logs
- [ ] **III. AI constrained**: tool-mediated access only; tool declarations complete; LLM args
  schema-validated + authz-checked; prompts versioned; per-turn telemetry; safe fallback/escalation;
  provider abstraction only
- [ ] **IV. Module boundaries**: no cross-module table access; `api → application → domain`;
  domain free of Axum/SQLx/Redis/vendor SDKs; no circular crate deps
- [ ] **V. Spec-driven**: bounded scope; no stubs/placeholder logic shipped as complete
- [ ] **VI. Tests**: unit / integration (real Postgres + Redis) / contract / component / e2e as
  applicable; negative security tests; regression tests for fixes; eval suite for AI changes
- [ ] **VII. Observability**: structured logs with tenant + actor; trace propagation; RED metrics;
  queue depth/age; readiness reflects dependencies
- [ ] **VIII. Contracts & types**: `/api/v1`; OpenAPI generated from code; generated TS client;
  RFC 9457 errors; ID newtypes; no `unwrap`/`expect`/`panic!` in request/job paths; strict TS
- [ ] **IX. Simplicity**: no new infrastructure without ADR; module README updated
- [ ] **X. Data integrity**: expand → migrate → contract migrations; idempotent retryable writes;
  transactional outbox; optimistic concurrency or row locks; PII export/deletion
- [ ] **XI. Resilience & scale**: timeouts on all outbound calls; backoff + jitter; circuit breakers;
  background workers for bursty work; per-tenant rate limits; keyset pagination;
  performance budget stated (default API p95 < 300 ms, first AI token p95 < 2.5 s)
- [ ] **XII. UX**: Design System components + tokens; WCAG 2.2 AA; keyboard; light/dark/RTL;
  loading/empty/error/permission-denied states; i18n-externalized text

## Project Structure

### Documentation (this feature)

```text
specs/[###-feature]/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md        # Phase 1 output (/speckit-plan command)
├── quickstart.md        # Phase 1 output (/speckit-plan command)
├── contracts/           # Phase 1 output (/speckit-plan command)
└── tasks.md             # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)
<!--
  ACTION REQUIRED: Replace the placeholder tree below with the concrete layout
  for this feature. Delete unused options and expand the chosen structure with
  real paths (e.g., apps/admin, packages/something). The delivered plan must
  not include Option labels.
-->

```text
# [REMOVE IF UNUSED] Option 1: Single project (DEFAULT)
src/
├── models/
├── services/
├── cli/
└── lib/

tests/
├── contract/
├── integration/
└── unit/

# [REMOVE IF UNUSED] Option 2: Web application (when "frontend" + "backend" detected)
backend/
├── src/
│   ├── models/
│   ├── services/
│   └── api/
└── tests/

frontend/
├── src/
│   ├── components/
│   ├── pages/
│   └── services/
└── tests/

# [REMOVE IF UNUSED] Option 3: Mobile + API (when "iOS/Android" detected)
api/
└── [same as backend above]

ios/ or android/
└── [platform-specific structure: feature modules, UI flows, platform tests]
```

**Structure Decision**: [Document the selected structure and reference the real
directories captured above]

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified.**
> Every deviation MUST link an ADR in `docs/adr/` (Constitution preamble and Principle IX).

| Violation (Principle) | Why Needed | Simpler Alternative Rejected Because | ADR |
|-----------------------|------------|-------------------------------------|-----|
| [e.g., XI — synchronous call in handler] | [current need] | [why a worker is insufficient] | [docs/adr/NNNN-title.md] |
| [e.g., IX — new message broker] | [specific problem + measurement] | [why Postgres outbox insufficient] | [docs/adr/NNNN-title.md] |
