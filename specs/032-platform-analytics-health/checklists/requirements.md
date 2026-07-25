# Specification Quality Checklist: Platform Analytics & System Health

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-07-25
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

## Validation Notes

**Iteration 1 findings and fixes applied before finalising:**

1. *Health verdicts were initially ambiguous.* The existing readiness contract is binary, so "health indicators are visible" could have been satisfied by an untestable traffic light. Resolved by defining a three-state model with concrete numeric thresholds (FR-014, FR-015, FR-022) rather than leaving thresholds as "configurable".
2. *"Tenant users cannot access platform analytics" needed enforcement wording.* Added FR-003 (server-side enforcement, independent of route guards) and US1 scenario 5, plus SC-002, so the criterion is verifiable rather than satisfied by hiding navigation.
3. *Cross-tenant semantics were implicit.* Added FR-005 and US1 scenario 4 stating that results are not narrowed by the switched tenant context — a reader would otherwise assume the opposite.
4. *Storage health could have been vacuous.* FR-016/FR-017 require reporting the actually-selected backend and an actual round-trip probe, because the running system currently falls back silently.
5. *Permission scope was a real decision, not a detail.* Recorded in Assumptions: existing diagnostics permission for infrastructure pages (2 of 5 platform roles), a new analytics permission for business metrics (all 5 platform roles), because the acceptance criterion says "platform users", plural and unqualified.

**Iteration 2 findings and fixes applied:**

6. *FR-011 contradicted FR-002.* The Overview was required to embed a health verdict gated on the diagnostics permission, which only 2 of 5 platform roles hold — coupling the P1 story to P2 data, and leaving the only FR with no acceptance scenario behind it. Rewritten so the Overview embeds no health data and reaches health via a permission-gated navigation link (already covered by FR-004). P1 and P2 are now genuinely independent.
7. *SC-004 was not verifiable.* It asserted a 60-second detection latency, but readings are computed per request with no background monitor and no persisted history (see Key Entities, and "No alerting" in Assumptions) — so detection latency is not a property this system has. Rewritten as a request-time property: a reading taken during an outage reports unhealthy in that same response. Until this fix, "requirements are testable and unambiguous" was false.
8. *Query-path cost was unaddressed.* Existing indexes lead with `tenant_id` while every query here is cross-tenant and time-filtered, so SC-003 is unreachable without new indexes. Added to Assumptions as in-scope work — Constitution VIII makes indexes mandatory for production query paths, so this is also a Constitution Check item at plan time.
9. *The new permission was left unnamed* while the reused one was named, which read as an unresolved gap. Now named explicitly: `platform.analytics.view`.

**Iteration 3 — `/speckit-clarify` session 2026-07-25 (4 questions asked, quota 5):**

10. *Permission split confirmed* by the user, so it moves out of Assumptions-as-guess and into Clarifications-as-decision. FR-001 and FR-002 now name `platform.analytics.view` and `platform.diagnostics.view` explicitly, and FR-002 pins the diagnostics grant as unchanged (Super Admin + Developer) so the matrix edit is additive only.
11. *Performance strategy pinned.* Caching and rollups were both considered and explicitly rejected in favour of live queries plus new indexes. Added FR-031 (no cached or precomputed aggregates) and sharpened the Assumptions entry to say that if indexing alone misses the 2-second budget, that is an escalation at plan time — not a silent cache.
12. *FR-017 was under-specified.* "Round-trip probe" did not say read vs write, and a write probe every 30 seconds would have littered the bucket. Now a read-only metadata/listing probe, with FR-017a stating the accepted blind spot: a readable-but-not-writable bucket reads as healthy.
13. *Boundary with existing public probes was undefined.* The system already exposes unauthenticated `/health`, `/ready`, `/metrics` with a binary contract consumed by orchestrators. FR-019a now requires a separate permission-gated surface leaving those untouched, and FR-019b confines the new detail to it.
14. *Self-inflicted contradiction caught during validation.* FR-019b as first written forbade publicly readable dependency failure reasons, but `/ready` already exposes a per-check `error` field and FR-019a says leave it alone. FR-019b was rescoped to the detail this feature introduces, explicitly noting the pre-existing field is out of its scope.

**Remaining risk (not a checklist failure):** SC-003's 2-second budget over 1,000,000 usage records is now the sharpest constraint, because the live-query decision removed caching and rollups as escape hatches. Percentile computation (FR-020) over that volume is the likeliest place it bites. Treat it as a Constitution Check item (Principle VIII, indexes mandatory on production query paths) at plan time.

## Notes

- Items marked incomplete require spec updates before `/speckit-clarify` or `/speckit-plan`
