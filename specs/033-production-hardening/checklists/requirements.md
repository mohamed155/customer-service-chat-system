# Specification Quality Checklist: Production Hardening

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-26
**Feature**: [spec.md](../spec.md)

## Content Quality

- [X] No implementation details (languages, frameworks, APIs)
- [X] Focused on user value and business needs
- [X] Written for non-technical stakeholders
- [X] All mandatory sections completed

## Requirement Completeness

- [X] No [NEEDS CLARIFICATION] markers remain
- [X] Requirements are testable and unambiguous
- [X] Success criteria are measurable
- [X] Success criteria are technology-agnostic (no implementation details)
- [X] All acceptance scenarios are defined
- [X] Edge cases are identified
- [X] Scope is clearly bounded
- [X] Dependencies and assumptions identified

## Feature Readiness

- [X] All functional requirements have clear acceptance criteria
- [X] User scenarios cover primary flows
- [X] Feature meets measurable outcomes defined in Success Criteria
- [X] No implementation details leak into specification

## Notes

### Validation iterations

**Iteration 1** — three issues found and corrected:

1. *Implementation details in requirements.* The first draft named specific technologies
   (Redis, OTLP, Sentry, HSTS/CSP/X-Frame-Options header names, k6, `pg_dump`) inside
   FR items. Corrected by restating every FR as observable behavior and relocating the
   concrete technology choices to the **Assumptions** section, which exists for exactly
   this purpose. Example: "set `X-Frame-Options: DENY`" became FR-010, "Dashboard responses
   MUST instruct browsers to refuse rendering the dashboard inside a frame owned by any
   other site."
2. *Vacuous CI success criterion.* "CI passes" is satisfiable by a gate that checks
   nothing — and the repository's current gate demonstrably has this defect (`ci.yml` runs
   no migrations and omits `REQUIRE_DB_TESTS`, so database-backed tests self-skip while
   reporting green). Corrected: SC-008 now requires both that the suite passes on a clean
   checkout **and** that a deliberately introduced failing test causes the gate to fail;
   FR-035 requires the executed-test count be visible so a silent drop is detectable.
3. *Unnamed actors.* Infra-oriented user stories drift into passive voice. Corrected by
   naming a distinct actor per story: Security Reviewer, Platform Operator, On-call
   Engineer, Release Manager, New Developer.

**Iteration 2** — verification of factual baseline claims:

The **Context: Current Baseline** section asserts current-state defects, so each was
checked against the code rather than carried over from prior notes:

- Widget-only, in-memory throttling — confirmed at `backend/crates/server/src/rate_limit.rs`
  and `crates/shared/kernel/src/rate_limit.rs`.
- Sign-in unthrottled — confirmed; the `login` route in `crates/server/src/router.rs` has
  no rate-limit layer.
- Metrics endpoint is a stub returning `"# no metrics yet"` — confirmed at
  `crates/shared/observability/src/lib.rs`.
- Duplicate backend workflows with unequal rigor — confirmed by reading `.github/workflows/ci.yml`
  against `.github/workflows/backend.yml`.
- Frontend package-manager version split (10.12.4 vs 10.34.4 across two jobs) — confirmed
  in `.github/workflows/frontend.yml`.
- No backend toolchain pin — confirmed, no `rust-toolchain.toml` exists.
- **Claim retracted before it reached the spec**: a prior note held that `cargo fmt --check`
  was failing repo-wide at HEAD. Re-running it exits 0 with no output, so the spec does
  *not* assert a formatting failure. The unpinned-toolchain risk (FR-036) stands on its own.

**Iteration 3** — clarification resolved:

The sole `[NEEDS CLARIFICATION]` marker (staging deployment target, affecting FR-022 and
FR-026) was put to the user and answered: **a single provisioned virtual machine running
the existing container stack**. The marker and the `## Open Question` section were removed,
and the decision was propagated rather than merely recorded:

- **Assumptions** gained the decision, its rationale (self-hosting keeps control of the
  pgvector version, which the platform pins at ≥ 0.8.0 for `hnsw.iterative_scan`), and an
  explicit consequences bullet stating that managed snapshots, point-in-time recovery, and
  platform-native rollback must now be built rather than inherited.
- **US4** was re-characterised as the largest build in the feature, since backup scheduling,
  encryption, off-host transfer, retention expiry, alerting, and restore are all self-built
  under this target. Its independent test now notes the mechanics can be rehearsed locally.
- **FR-028** and US4 scenario 1 gained an off-host storage requirement — on a single-host
  target, a backup stored on the backed-up machine is not a backup.
- Two edge cases were added: total loss of the single host (restore must assume a fresh
  machine with nothing salvageable), and backup jobs contending with the workload they
  protect for the same host's disk, memory, and network.
- **Assumptions** also records how SC-002's ≥2-instance requirement is met on one host —
  two backend containers behind the reverse proxy, since the criterion concerns two
  processes sharing one budget, not two machines.

### Outstanding

None. All 16 checklist items pass; the specification is ready for `/speckit-plan`.

### Recorded for planning, not blocking

Story priorities were **left unchanged** (US2 deployment at P2, US4 backup/restore at P4)
despite US4's scope growing under this target. Sequencing follows the dependency — a
restore drill needs an environment to drill against — not the size of the work. Planning
should nonetheless expect US4 to carry disproportionate effort relative to its priority
rank, and may schedule the backup mechanics early against the local stack.

---

## Iteration 4 — post-`/speckit-analyze` remediation (2026-07-26)

`/speckit-analyze` raised 15 findings across spec/plan/tasks; two more surfaced during the
fix pass. Two required amending the **specification**, not just the task list:

- **FR-008 amended.** It required refusals to be "recorded as security-relevant events"
  without saying where. The obvious reading — write them to `audit_logs` — is unsafe:
  migration `0006_audit_logs.sql` installs a `forbid_mutation` trigger blocking UPDATE and
  DELETE, so an unauthenticated flood would write rows nobody can ever prune, turning the
  audit trail into an amplification target. FR-008 now splits the behaviour: structured log
  events plus a counter for anonymous refusals, and a deduplicated audit row only for
  refusals attributable to an authenticated tenant identity. Still one requirement, still
  observable, no longer self-defeating.
- **Assumptions gained a log-aggregation bullet and an alert-delivery bullet.** FR-014
  requires logs *and* traces retrievable together "without server shell access", and FR-021
  requires notifying an on-call recipient — neither was reachable from the artifacts as
  written, which had no log store and no notification component. Both are now in scope, with
  the single-VM cost constraint carried into the choice (single-binary aggregator on the
  host filesystem; alert destination supplied as external configuration at deploy time).

All 16 checklist items still pass. Requirements remain technology-agnostic and observable;
the technology named in the new Assumptions bullets is recorded as an assumption, matching
how the rest of the section is written.
