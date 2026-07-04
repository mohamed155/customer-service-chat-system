# General Spec Readiness Checklist: AI Customer Service Platform

**Purpose**: Broad requirements-quality pass across spec.md — completeness, clarity, consistency, measurability, and edge-case coverage — beyond the initial spec-quality gate in `requirements.md`
**Created**: 2026-07-03
**Feature**: [spec.md](../spec.md)

**Note**: This checklist tests the *requirements as written*, not the implementation. It does not verify code/behavior.

## Requirement Completeness

- [x] CHK001 Is the customer identity-verification mechanism (signature scheme, key rotation) specified in enough detail to implement, beyond "tenants MAY pass verified customer identity"? [Gap, Spec §FR-AUTH-008] — *Resolved: FR-AUTH-008 now specifies a signed identity-assertion token from the tenant's backend using the widget's shared secret.*
- [x] CHK002 Are requirements defined for what happens when a tenant has zero AI agent configurations (e.g., mid-onboarding, before Agent Configuration is created)? [Gap, Spec §8 Data Model] — *Resolved: FR-ORG-001 now requires auto-provisioning a default Agent Configuration at tenant creation.*
- [x] CHK003 Is the definition of "AI interaction" as the billing metering unit precise enough to eliminate ambiguity between a customer turn, a tool call, and a retry? [Completeness, Spec §A-10] — *Resolved: A-10 now precisely defines the unit as one AiExecution, retries excluded via idempotency key.*
- [x] CHK004 Are requirements defined for partial ingestion failure — e.g., a multi-page crawl where some pages succeed and others fail? [Gap, Spec §FR-KB-002] — *Resolved: FR-KB-002 now specifies per-page failure handling for crawls.*
- [x] CHK005 Are requirements defined for what a Viewer role sees when navigating to an action-only page (e.g., prompt editor) — read-only view vs. access-denied? [Gap, Spec §4.2 Tenant Users] — *Resolved: §4.2 now specifies read-only rendering, not access-denied, for readable pages.*

## Requirement Clarity

- [x] CHK006 Is "near-real-time" (FR-BILL-003, ≤1 hour lag) the only quantification of usage-visibility freshness, and is it consistent with the ≤5-minute freshness stated for analytics dashboards (FR-ANLT-006)? [Ambiguity, Spec §5.13 vs §5.12] — *Resolved: FR-BILL-003 now explicitly distinguishes the metering SLA from the analytics-freshness SLA.*
- [x] CHK007 Is "graceful customer-facing fallback" (FR-BILL-002, plan-limit hard-stop) defined with a specific behavior, or left to implementer interpretation? [Clarity, Spec §5.13] — *Resolved: FR-BILL-002 now specifies the exact hard-stop behavior.*
- [x] CHK008 Is "reasonable" retention for knowledge segments after source deletion quantified (immediate vs. eventual purge), or only "permanently excluded"? [Ambiguity, Spec §FR-KB-003] — *Resolved: FR-KB-003 now specifies removal within the same 5-minute window as FR-KB-004.*
- [x] CHK009 Is the confidence-to-behavior mapping (answer/caveat/clarify/escalate) given concrete threshold semantics in the spec itself, or only referenced as "tenant configures thresholds"? [Clarity, Spec §11.9] — *Resolved: §11.9 now states default numeric thresholds (0.35/0.55/0.75) per research.md R-14.*
- [x] CHK010 Is "actionable failure reasons" (FR-KB-002) accompanied by an enumerated or example set of reasons, or left fully open-ended? [Ambiguity, Spec §5.8] — *Resolved: FR-KB-002 now lists example failure-reason codes.*

## Requirement Consistency

- [x] CHK011 Do the escalation-trigger lists in FR-AI-006 and the Human Handoff user story (US3) name identical trigger categories, with no trigger present in one but not the other? [Consistency, Spec §5.7 vs User Story 3] — *Resolved: US3's intro now enumerates all five FR-AI-006 triggers by cross-reference instead of a shorter ad-hoc list.*
- [x] CHK012 Are the CSAT requirements (FR-CONV-008) and the analytics CSAT reporting requirements (FR-ANLT-001) consistent about whether CSAT is optional per-tenant or always collected? [Consistency, Spec §5.6 vs §5.12] — *Resolved: FR-ANLT-001 now clarifies CSAT is computed only over rated conversations, consistent with FR-CONV-008's opt-in.*
- [x] CHK013 Do the data-retention statements in Assumption A-05, FR-CUST-004 (30-day customer delete), and FR-ORG-004 (30-day tenant purge) use retention windows consistently, or could a reader infer conflicting defaults? [Consistency, Spec §Assumptions] — *Verified consistent as written: both FR-CUST-004 and FR-ORG-004 already specify 30-day windows; no edit required.*
- [x] CHK014 Is the Tenant Switcher's audit obligation (FR-RBAC-003) stated identically wherever Tenant Switcher is mentioned (§4.1, §10.3, US6), with no version omitting the audit requirement? [Consistency, Spec multiple §] — *Verified consistent as written: all three mentions (§4.1, §10.3, US6) state the audit obligation; no edit required.*

## Acceptance Criteria Quality

- [x] CHK015 Can SC-003 ("≥60% of conversations resolved by AI without human involvement within 90 days") be objectively measured given the spec's definition of "resolved," or does "resolved" need a precise state-machine tie-in? [Measurability, Spec §Success Criteria, §FR-CONV-002] — *Resolved: SC-003 now ties "resolved" to the `resolved`/`closed` conversation states never entering `active_human`.*
- [x] CHK016 Can SC-011 ("≥90% CSAT-response average of 4/5 or higher") be verified given that CSAT submission is optional per FR-CONV-008 — is the denominator (all conversations vs. only rated ones) specified? [Measurability, Spec §Success Criteria] — *Resolved: SC-011 now specifies the denominator as rated conversations only.*
- [x] CHK017 Is SC-009 ("switching provider... causes no conversation failures and no tenant-visible configuration changes") falsifiable, i.e., does the spec define what a "tenant-visible configuration change" would look like to check against? [Measurability, Spec §Success Criteria] — *Resolved: SC-009 now defines this concretely against the Agent Configuration/Prompt Version/Routing Policy screens.*

## Scenario Coverage

- [x] CHK018 Are requirements defined for the primary flow, alternate flow (e.g., multilingual), exception flow (provider outage), and recovery flow (rollback) for the AI-answer journey, or only some of these? [Coverage, Spec §User Story 1, §Edge Cases] — *Verified adequate: primary (US1 scenarios), exception (Edge Cases: AI provider outage), recovery (US5: rollback), and alternate (FR-AI-009 multilingual) are all present, just distributed rather than co-located; no structural change needed.*
- [x] CHK019 Are non-functional requirements (performance, security) cross-referenced to the specific user story/flow they gate, or stated only in the standalone NFR section with no scenario linkage? [Coverage, Spec §6 vs User Stories] — *No change needed: this is an intentional structure — NFRs are deliberately centralized as cross-cutting constraints rather than duplicated per story, to avoid drift between copies; most already carry inline FR/SC cross-references.*
- [x] CHK020 Are requirements defined for a customer's conversation history when their profile is later deleted mid-way through an active conversation (not just at rest)? [Gap, Edge Case] — *Resolved: new Edge Case added for mid-conversation deletion requests.*

## Edge Case Coverage

- [x] CHK021 Is the behavior specified when a webhook subscription's signing secret is rotated while deliveries are in flight? [Gap, Spec §FR-INT-002] — *Resolved: FR-INT-002 and Edge Cases now specify a dual-secret grace window.*
- [x] CHK022 Is the behavior specified when two tenants' knowledge sources happen to reference visually identical content (dedup expectations), or is cross-tenant dedup explicitly out of scope? [Gap, Spec §5.8] — *Resolved: FR-KB-001 now states cross-tenant dedup is explicitly out of scope.*
- [x] CHK023 Is the behavior specified for a prompt rollback that targets a version referencing a now-deleted or disabled tool? [Gap, Spec §FR-PROMPT-005 vs §FR-INT-005] — *Resolved: FR-PROMPT-005 and Edge Cases now specify graceful tool-unavailable handling instead of blocking rollback.*

## Non-Functional Requirements

- [x] CHK024 Are internationalization requirements (§6.6) specific about which UI surfaces (widget vs. dashboard vs. email notifications) must ship translated in v1 vs. which are English-only? [Clarity, Spec §6.6] — *Resolved: NFR-I18N-001 now specifies widget + dashboard core flows translated, email/platform-operator surfaces English-only in v1.*
- [x] CHK025 Are accessibility requirements (§6.5) specific about assistive-technology support scope (e.g., specific screen readers) or left as a general WCAG 2.1 AA claim without a defined test method? [Measurability, Spec §6.5] — *Resolved: NFR-ACC-001 now specifies automated axe-core CI scanning plus a manual NVDA/VoiceOver pass per release.*

## Dependencies & Assumptions

- [x] CHK026 Is Assumption A-03 (external payment processor "owns card handling") validated against the processor-webhook requirement in the API contracts, i.e., is the dependency's failure mode (processor outage) addressed anywhere in the spec? [Assumption, Spec §Assumptions vs §5.13] — *Resolved: FR-BILL-004 now specifies processor-outage handling (queue and retry, no service interruption).*
- [x] CHK027 Is the assumption that "tenants' end customers do not authenticate with the platform" (A-07) reconciled with the identity-verification language in FR-AUTH-008, or could a reader see tension between "no auth" and "verified identity"? [Assumption, Spec §A-07 vs §FR-AUTH-008] — *Resolved: A-07 now explicitly distinguishes the signed identity-assertion mechanism from platform authentication.*

## Ambiguities & Conflicts

- [x] CHK028 Does any spec section use "fast," "scalable," "robust," or "intuitive" without a quantified companion metric elsewhere in the same section? [Ambiguity, Spec-wide] — *Resolved: spec-wide scan found one instance ("seamless" in G-05), now cross-referenced to SC-004 and US3's acceptance scenarios for a measurable definition.*
- [x] CHK029 Is there a single canonical term for the entity holding AI behavior configuration (spec uses "Agent Configuration," "AI agent," "agent," interchangeably) — could this drift confuse a reader distinguishing it from a human Agent (the role)? [Ambiguity, Spec §5.7, §8, §4.2] — *Resolved: added an explicit terminology note in §4.4 establishing "Agent" (bare) = human role; AI is always "the AI" / "AI Agent" / "Agent Configuration". Existing usage throughout the spec already followed this convention.*

## Notes

- Check items off as completed: `[x]`
- This checklist supersedes none of `requirements.md` (spec-quality gate) — it is an additional, broader pass
- **Resolution pass completed 2026-07-03**: all 29 items resolved — 24 by editing spec.md (tightened definitions, added missing requirements/edge cases, reconciled cross-references), 5 by verification that no conflict existed (CHK013, CHK014, CHK018, CHK019) or scope confirmed sufficient as-is
- Two decisions required product input and were confirmed with the user before editing: CHK001 (signed-token identity verification) and CHK029 (terminology note vs. renaming the Agent RBAC role — kept the role name, added a disambiguation note)
