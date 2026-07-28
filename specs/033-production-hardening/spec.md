# Feature Specification: Production Hardening

**Feature Branch**: `033-production-hardening`

**Created**: 2026-07-26

**Status**: Draft

**Input**: User description: "Production Hardening — prepare the platform for real-world deployment. Scope: rate limiting, security headers, backup strategy, error tracking, deployment scripts, CI/CD hardening, load testing, observability dashboards, documentation. Acceptance criteria: app can be deployed to staging; CI runs reliably; logs and traces are available; security-sensitive endpoints are protected; backup and restore process is documented; developer setup documentation is complete."

## Context: Current Baseline

This feature closes gaps in an otherwise feature-complete platform (specs 001–032). The
starting state, verified against the codebase on 2026-07-26:

- **Abuse protection is partial.** Request throttling exists for the public widget surface
  only (session creation per source address, messages per session, a global per-tenant
  budget). It is held in per-process memory, so budgets multiply by the number of running
  server instances. Authenticated sign-in has no throttle at all, so credential-guessing
  against it is unbounded.
- **Browser-facing protections are unset.** Cross-origin access is restricted by an
  allowlist and request bodies are size-capped on the public widget surface, but no
  response-level browser hardening directives are emitted on any surface.
- **Telemetry is one-third built.** Structured logs, per-request correlation identifiers,
  and per-request trace spans exist. There is no export path to any external collector,
  the metrics endpoint is a hard-coded stub that returns no measurements, and there is no
  aggregated view of platform health.
- **Failures are invisible.** Unhandled errors reach the logs and nowhere else. Nobody is
  notified, and there is no grouping, frequency, or first-seen/last-seen record.
- **There is no deployed environment.** Container definitions for backend and frontend
  exist, plus a local-development service stack (database, cache, object storage, mail
  catcher). There is no staging environment, no promotion path to one, and no
  configuration separation between local and deployed settings.
- **The build gates are unreliable, not merely incomplete.** Two backend workflows
  (`ci.yml`, `backend.yml`) trigger on every push with overlapping responsibilities; the
  first runs no migrations and omits the flag that forces database-backed tests to
  execute, so a large share of the suite silently self-skips there while appearing green.
  The frontend workflow pins two different package-manager versions across its two jobs.
  No workflow pins the backend toolchain version, so gate results can shift without a
  code change.
- **Capacity is unmeasured.** No load or soak testing exists, so no concurrency or
  latency claim about the platform is currently evidence-backed.
- **Operator documentation is absent.** A developer quickstart and per-feature environment
  variable notes exist in the project README. There is no runbook, no restore procedure,
  and no consolidated configuration reference.

Backups do not exist in any form today.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Security Reviewer signs off on exposed endpoints (Priority: P1)

A security reviewer assesses whether the platform's internet-facing endpoints can be
abused. They attempt credential guessing against sign-in, flood the public widget and
webhook endpoints, and inspect what protective directives the application returns to
browsers. Every abusive pattern is refused after a bounded number of attempts with a
clear, non-revealing response, and legitimate traffic is unaffected. Throttles hold as a
single shared budget no matter which server instance answers.

**Why this priority**: This is the only item in scope that reduces active, exploitable
risk. Everything else improves the team's ability to operate the platform; this protects
the platform and its tenants from attack. It is also the largest confirmed gap — sign-in
is entirely unthrottled today.

**Independent Test**: Fully testable by driving abusive request patterns against a running
instance and asserting refusals, then inspecting responses for the required browser
directives. Delivers real risk reduction with nothing else in this feature built.

**Acceptance Scenarios**:

1. **Given** a valid account, **When** an attacker submits repeated wrong passwords for it
   from one source, **Then** further sign-in attempts for that account and source are
   refused once a documented threshold is crossed within a documented window, and the
   refusal reveals nothing about whether the account exists.
2. **Given** two or more server instances behind a load balancer, **When** an attacker
   spreads abusive requests across all of them, **Then** the total allowed through matches
   the single documented budget rather than the budget multiplied per instance.
3. **Given** a throttled request, **When** the caller inspects the response, **Then** it
   carries a distinct rejection status, states when the caller may retry, and does not
   disclose internal counters or other tenants' activity.
4. **Given** any application response, **When** a browser receives it, **Then** the
   response instructs the browser to refuse framing by third-party sites, to not
   re-interpret declared content types, to restrict which origins may supply executable
   or embedded content, to withhold full referrer paths from cross-origin navigations, and
   to require an encrypted transport for future visits.
5. **Given** a signed webhook endpoint or the public widget surface, **When** it is flooded
   from one source, **Then** requests are refused above the documented budget while a
   separate legitimate tenant's traffic continues to be served normally.
6. **Given** normal interactive use by a tenant user, **When** they work at realistic human
   speed for a sustained session, **Then** they are never throttled.

---

### User Story 2 - Platform Operator deploys a release to staging (Priority: P2)

A platform operator takes a specific commit and promotes it to a staging environment that
mirrors production's shape, using a repeatable documented procedure rather than
hand-typed steps. Schema changes apply as part of the promotion. When the release is bad,
the operator returns to the previous one without reconstructing state by hand.

**Why this priority**: A staging environment is the prerequisite for validating every
other item here — load tests, dashboards, and restore drills all need somewhere real to
run. It also converts the platform from "runs on a laptop" to "runs somewhere", which is
the headline acceptance criterion.

**Independent Test**: Fully testable by promoting a known commit to staging from a clean
state, exercising a sign-in and a conversation end to end against it, then rolling back
and confirming the prior release serves traffic again.

**Acceptance Scenarios**:

1. **Given** a commit that passes all gates, **When** the operator runs the documented
   promotion procedure, **Then** staging serves that exact commit's build and reports
   healthy without any manual step outside the procedure.
2. **Given** a release containing schema changes, **When** it is promoted, **Then** the
   changes are applied automatically before the new build starts accepting traffic, and a
   failure to apply them aborts the promotion without leaving a partially migrated
   environment serving requests.
3. **Given** a promoted release that proves faulty, **When** the operator runs the
   documented rollback, **Then** the previous release serves traffic again within a
   documented time budget.
4. **Given** staging, **When** its configuration is inspected, **Then** every secret and
   environment-specific value is supplied externally, none is present in version control,
   and the set of required values is documented in one place.
5. **Given** a newly provisioned staging environment, **When** an operator follows the
   procedure from nothing, **Then** it reaches a serving state without undocumented
   tribal knowledge.

---

### User Story 3 - On-call Engineer diagnoses a production incident (Priority: P3)

An on-call engineer is alerted that error rates have risen. They open a health view showing
request volume, error rate, latency distribution, and queue depth; identify the failing
operation; follow one failing request's correlation identifier through its trace to the
slow or failing step; and read the grouped error record with its frequency, first
occurrence, and stack context — without shell access to a server.

**Why this priority**: Directly serves the "logs and traces are available" criterion and
converts existing raw telemetry into something usable under time pressure. Ranked below
deployment because there is currently no deployed environment to observe.

**Independent Test**: Fully testable by injecting a deliberate failure and a deliberate
slow path into staging, then confirming both are findable end to end — dashboard to trace
to grouped error — using only the operator-facing views.

**Acceptance Scenarios**:

1. **Given** a request that fails, **When** the on-call engineer searches by its
   correlation identifier, **Then** they retrieve that request's logs and its trace
   showing each step and the step's duration.
2. **Given** an unhandled error in any module, **When** it occurs, **Then** it appears in
   the error record grouped with other occurrences of the same fault, with occurrence
   count, first and most recent occurrence, affected environment, and enough context to
   locate the code path.
3. **Given** the health view, **When** an operator opens it, **Then** it shows request
   rate, error rate, latency distribution, background-work backlog, and dependency health
   for the database, cache, object storage, and configured model providers.
4. **Given** telemetry containing sign-in attempts, message content, customer details, or
   provider credentials, **When** it is recorded or exported, **Then** secrets and
   customer personal data are masked and never leave the platform in readable form.
5. **Given** the error rate crossing a documented threshold, **When** the threshold is
   breached, **Then** a notification reaches the on-call engineer through a configured
   channel without anyone watching a screen.

---

### User Story 4 - Platform Operator restores tenant data after loss (Priority: P4)

A platform operator responds to data loss — an accidental destructive change or a storage
failure — by restoring from a recent backup into a separate environment, verifying tenant
data came back intact, and recording how much data was unrecoverable. They have rehearsed
this before needing it.

**Why this priority**: Protects against the most severe possible outcome, but only becomes
exercisable once a deployed environment with real backup targets exists. An untested
backup is not a backup, which is why the criterion is a rehearsed restore rather than a
scheduled dump. **This is the largest build in the feature**: the chosen single-VM target
(see Assumptions) self-hosts both the database and the object store, so there are no
managed snapshots and no point-in-time recovery to inherit — scheduling, encryption,
off-host transfer, retention expiry, failure alerting, and the restore path are all built
here rather than configured.

**Independent Test**: Fully testable by taking a backup of a populated staging database and
object store, restoring both into a fresh environment, and verifying record counts and
spot-checked content match per tenant. The backup and restore mechanics can be developed
and rehearsed against the local stack before staging exists, since both environments run
the same services.

**Acceptance Scenarios**:

1. **Given** a populated environment, **When** backups run on their schedule, **Then**
   both relational data and stored documents/attachments are captured, each backup lands on
   storage that does not share the fate of the host being backed up, and each backup's
   completion or failure is recorded where an operator will see it.
2. **Given** a backup set, **When** an operator performs the documented restore into a
   fresh environment, **Then** all tenants' data is present and consistent, with data loss
   bounded by the documented recovery point objective and restore duration bounded by the
   documented recovery time objective.
3. **Given** a completed restore drill, **When** it finishes, **Then** the measured data
   loss and elapsed restore time are recorded and compared against the documented
   objectives.
4. **Given** backup artifacts at rest, **When** their storage is inspected, **Then** they
   are encrypted, access to them is restricted, and they are retained for a documented
   period and then removed automatically.
5. **Given** a backup that fails to complete, **When** the schedule next runs, **Then** the
   failure is surfaced as an alert rather than discovered during an incident.

---

### User Story 5 - Release Manager trusts the build gate (Priority: P5)

A release manager relies on the automated gate to decide whether a change is safe to
promote. A green result means the full suite actually ran; a broken change is caught before
merge, not after. The same commit produces the same gate result on any day.

**Why this priority**: Serves the "CI runs reliably" criterion. It is high-leverage but
ranked here because the platform's confirmed exploitable and operational gaps (P1–P4)
carry more risk than a gate that is currently over-permissive rather than absent.

**Independent Test**: Fully testable by introducing a deliberately failing test and a
deliberately unformatted file on a branch and confirming the gate fails for each, then
confirming a clean commit passes.

**Acceptance Scenarios**:

1. **Given** a clean checkout of the default branch, **When** the gate runs, **Then** the
   complete suite executes and passes, with no category silently skipped.
2. **Given** a deliberately failing test, **When** the gate runs, **Then** the gate fails
   and names the failing test. *(This scenario is the point of the story: a gate that only
   reports green cannot be distinguished from a gate that checks nothing.)*
3. **Given** the gate's own report, **When** a release manager reads it, **Then** the count
   of tests that ran is visible, so a drop in executed tests is detectable rather than
   indistinguishable from success.
4. **Given** the same commit, **When** the gate runs on two different days with no code
   change, **Then** both runs reach the same verdict — every tool version that can change
   the verdict is pinned.
5. **Given** a change that introduces a dependency with a known published vulnerability or
   commits a credential-shaped string, **When** the gate runs, **Then** it fails and
   identifies the offending dependency or string.
6. **Given** the gate's overlapping workflows, **When** a change is pushed, **Then** each
   check runs once with a single authoritative verdict per concern, rather than the same
   concern being checked twice with different rigor.

---

### User Story 6 - New Developer reaches a running stack unaided (Priority: P6)

A developer joining the project follows written documentation from a fresh machine to a
running backend, dashboard, and widget with seeded data, then locates the runbook for
common operational tasks — all without asking a teammate.

**Why this priority**: Serves the "developer setup documentation is complete" criterion. It
is real, verifiable value and is ranked last because it documents work the other stories
produce; writing it earlier would mean documenting it twice.

**Independent Test**: Fully testable by having someone who has not set up the project
follow only the written documentation on a clean machine and reach a working stack,
recording every point where they had to ask or guess.

**Acceptance Scenarios**:

1. **Given** a clean machine and the written setup documentation, **When** a developer
   follows it start to finish, **Then** they reach a running backend, dashboard, and widget
   with usable seed data inside a documented time budget and with zero undocumented steps.
2. **Given** the configuration reference, **When** a developer consults it, **Then** every
   environment variable the platform reads is listed with purpose, whether it is required
   per environment, its default, and how to generate it when it is a secret.
3. **Given** an operational task — promote a release, roll back, restore a backup, rotate
   a credential, investigate an error — **When** an operator looks for it, **Then** a
   runbook entry gives the steps and the expected outcome.
4. **Given** a documented procedure, **When** the underlying behavior changes, **Then** the
   change is not considered complete until the documentation matches, so documentation
   cannot silently drift from reality.

---

### Edge Cases

- **Shared-address false positives.** Many legitimate users behind one corporate gateway or
  mobile carrier appear as a single source. Per-source throttles must not lock out an
  entire organization; identity-scoped budgets must take precedence where the caller is
  authenticated.
- **Throttle backing store unavailable.** If the shared counter store is unreachable, the
  platform must choose a documented behavior — fail closed for sensitive endpoints,
  degrade to per-instance counting elsewhere — rather than silently allowing unlimited
  traffic.
- **Legitimate bulk traffic.** A high-volume tenant's normal webhook or widget load must not
  be indistinguishable from an attack; budgets must be tenant-aware and adjustable without
  a code change.
- **Browser directives breaking embedded surfaces.** The widget is intentionally embedded in
  third-party sites via a frame; a blanket anti-framing directive would break the
  platform's own product. The widget surface needs a distinct policy from the dashboard.
- **Restore into a differently shaped environment.** A backup taken before a schema change
  and restored after one must either fail loudly or apply the intervening changes — never
  restore into a shape the application misreads.
- **Backup of a moving target.** Relational data and object storage are captured
  separately; a document referenced by a restored row must not be missing, so the
  procedure must address the consistency window between the two.
- **Load test as denial of service.** Load testing must be constrained to non-production
  targets with documented ceilings, and must not be capable of accidental execution against
  real tenants' environment.
- **Telemetry volume and cost.** Full-fidelity traces at production volume may be
  prohibitively expensive or slow; sampling must be adjustable, and sampling decisions must
  never drop the trace of a request that errored.
- **Secrets in telemetry.** Error payloads, request bodies, and trace attributes are the
  most common accidental exfiltration path for credentials and customer data; masking must
  apply at capture, not at display.
- **Rollback across a destructive schema change.** A release that removed or narrowed a
  column cannot be rolled back by redeploying the old build alone; the promotion procedure
  must state which changes are reversible and which require a restore.
- **Total loss of the single host.** The chosen staging target co-locates the application,
  database, and object store on one machine, so losing it loses all three at once. Backups
  written to that same machine would be lost with it, and the restore procedure must
  therefore assume a completely fresh host with nothing recoverable from the old one.
- **Backup competing with the workload it protects.** Dumping the database and
  synchronising the object store consume the same host's disk, memory, and network as the
  running application; the schedule and resource ceiling must keep a backup from degrading
  or destabilising the service, particularly while a load test is running.

## Requirements *(mandatory)*

### Functional Requirements

#### Abuse protection

- **FR-001**: The system MUST refuse further sign-in attempts for a given account and
  source once a documented number of failures occurs within a documented window, and the
  refusal MUST NOT disclose whether the account exists.
- **FR-002**: The system MUST enforce request budgets on every internet-facing endpoint
  group that does not require prior authentication, including the public widget surface,
  inbound channel webhooks, and authentication endpoints.
- **FR-003**: Request budgets MUST behave as one shared budget across all running server
  instances, so the enforced limit does not scale with instance count.
- **FR-004**: Budgets MUST be scoped by authenticated identity where the caller is
  authenticated, and by source address only where the caller is anonymous, so that shared
  network addresses do not cause collective lockout.
- **FR-005**: A refused request MUST return a distinct rejection status, MUST tell the
  caller when retrying is permitted, and MUST NOT disclose internal counters or other
  tenants' activity.
- **FR-006**: Budget thresholds MUST be adjustable per environment and per tenant without a
  code change.
- **FR-007**: When the shared counter store is unavailable, the system MUST follow a
  documented degradation behavior and MUST NOT silently permit unlimited traffic to
  authentication endpoints.
- **FR-008**: Refusals MUST be recorded as security-relevant events attributable to a
  source and, where known, an identity, and an operator MUST be able to query those records
  without server shell access. Refusals from **unauthenticated** callers MUST NOT be written
  to the append-only audit log: that table forbids UPDATE and DELETE, so an anonymous flood
  would write rows that can never be pruned, turning the audit trail itself into an
  amplification target. Unauthenticated refusals are therefore recorded as structured log
  events (queryable through the log store required by FR-014) plus an aggregate counter
  (FR-015). A durable audit-log entry is written only for a refusal attributable to an
  authenticated tenant identity, deduplicated to at most one entry per identifier per
  window.

#### Browser-facing hardening

- **FR-009**: Every application response MUST instruct browsers to refuse re-interpreting
  declared content types, to withhold full referrer paths on cross-origin navigation, and
  to require an encrypted transport on subsequent visits.
- **FR-010**: Dashboard responses MUST instruct browsers to refuse rendering the dashboard
  inside a frame owned by any other site.
- **FR-011**: Responses MUST restrict which origins may supply executable, style, frame,
  and connection content, and the restriction MUST be verified not to break the dashboard
  or widget in normal use.
- **FR-012**: The embeddable widget surface MUST carry a policy that permits its intended
  embedding in allowlisted customer sites while still restricting content sources — it MUST
  NOT inherit the dashboard's anti-framing policy.
- **FR-013**: Directive values MUST be configurable per environment, so a staging origin can
  differ from production without a code change.

#### Telemetry and error visibility

- **FR-014**: Logs and traces for a single request MUST be retrievable together by that
  request's correlation identifier from an operator-facing view, without server shell
  access.
- **FR-015**: The system MUST emit measurements of request rate, error rate, request latency
  distribution, background-work backlog, and dependency health, replacing the current stub
  that reports no measurements.
- **FR-016**: Traces MUST be exportable to an external collector, with the export target
  configurable per environment and the platform remaining fully functional when no
  collector is configured.
- **FR-017**: Unhandled errors MUST be captured into a grouped record carrying occurrence
  count, first and most recent occurrence, environment, release identifier, and the code
  path — deduplicated so one recurring fault is one entry.
- **FR-018**: An operator-facing health view MUST present the measurements in FR-015 over a
  selectable time range.
- **FR-019**: Secrets, credentials, message content, and customer personal data MUST be
  masked at the point telemetry is captured, before any export leaves the platform.
- **FR-020**: Trace sampling MUST be adjustable per environment, and a request that errored
  MUST always be retained regardless of the sampling rate.
- **FR-021**: The system MUST notify an on-call recipient through a configured channel when
  error rate, latency, or dependency health crosses a documented threshold.

#### Deployment

- **FR-022**: A documented, repeatable procedure MUST promote an identified commit to a
  staging environment with no manual steps outside the procedure.
- **FR-023**: Promotion MUST apply pending schema changes before the new build accepts
  traffic, and MUST abort without serving requests if they fail to apply.
- **FR-024**: A documented rollback procedure MUST return the previous release to service
  within a documented time budget, and MUST state which schema changes it cannot reverse.
- **FR-025**: All secrets and environment-specific configuration MUST be supplied externally
  at deploy time; none MUST be present in version control.
- **FR-026**: The staging environment MUST match production in service topology and
  configuration shape, differing only in scale, data, and credentials.
- **FR-027**: The deployed build MUST expose liveness and readiness signals that distinguish
  "process running" from "ready to serve", and promotion MUST wait for readiness before
  directing traffic.

#### Backup and restore

- **FR-028**: Relational data and stored objects MUST both be backed up on a documented
  schedule, with each run's success or failure recorded where an operator will see it.
  Backups MUST be written to storage that does not share the fate of the host being backed
  up — a copy residing only on the backed-up machine does not satisfy this requirement.
- **FR-029**: A documented restore procedure MUST reconstruct all tenants' data into a fresh
  environment, and MUST address the consistency window between the relational and object
  backups.
- **FR-030**: Documented recovery point and recovery time objectives MUST exist, and a
  restore drill MUST measure actual values against them.
- **FR-031**: Backup artifacts MUST be encrypted at rest, access-restricted, retained for a
  documented period, and removed automatically afterward.
- **FR-032**: A failed or missed backup MUST raise an alert rather than remaining silent.
- **FR-033**: Restoring a backup into an environment with a different schema shape MUST
  either apply the intervening changes or fail loudly — never silently produce a shape the
  application misreads.

#### Build gate

- **FR-034**: The gate MUST execute the complete test suite with no category self-skipping,
  and MUST fail if a category that should have run did not.
- **FR-035**: The gate MUST report the number of tests executed, so a drop is detectable.
- **FR-036**: Every tool version that can change the gate's verdict MUST be pinned, so the
  same commit yields the same verdict over time.
- **FR-037**: Each concern MUST be checked exactly once with a single authoritative verdict;
  the current duplicate, unequally rigorous backend workflows MUST be consolidated.
- **FR-038**: The gate MUST fail on a dependency with a known published vulnerability and on
  a committed credential-shaped string — neither MUST be advisory-only.
- **FR-039**: The gate MUST be verified to actually fail by introducing a deliberate
  failure, and that verification MUST be repeatable.
- **FR-040**: The gate MUST publish an artifact identifying the exact commit and build that
  the deployment procedure promotes.

#### Capacity evidence

- **FR-041**: A repeatable load test MUST exercise the platform's primary paths —
  authenticated dashboard reads, widget conversation traffic, and inbound channel webhooks
  — and report throughput, latency distribution, and error rate.
- **FR-042**: A documented concurrency and latency target MUST exist, and load test results
  MUST be recorded against it as the platform's capacity evidence.
- **FR-043**: Load tests MUST be constrained to non-production targets and MUST NOT be
  capable of accidental execution against real tenants' environment.
- **FR-044**: A sustained-duration run MUST confirm the platform does not degrade over time
  from resource exhaustion under steady load.

#### Documentation

- **FR-045**: Setup documentation MUST take a developer from a clean machine to a running
  backend, dashboard, and widget with seed data, with zero undocumented steps.
- **FR-046**: A single configuration reference MUST list every environment variable the
  platform reads, with purpose, per-environment necessity, default, and generation
  instructions for secrets.
- **FR-047**: A runbook MUST cover promoting a release, rolling back, restoring a backup,
  rotating a credential, and investigating a reported error.
- **FR-048**: Documentation MUST be validated by someone who has not set up the project
  following it unaided, and every point where they had to ask or guess MUST be corrected.
- **FR-049**: Each module's documented interfaces MUST be current as of this feature, per
  the constitution's documentation requirement.

### Traceability to Stated Acceptance Criteria

| Stated criterion | User story | Requirements | Success criteria |
|------------------|------------|--------------|------------------|
| App can be deployed to staging | US2 | FR-022 – FR-027 | SC-004, SC-005 |
| CI runs reliably | US5 | FR-034 – FR-040 | SC-008, SC-009 |
| Logs and traces are available | US3 | FR-014 – FR-021 | SC-006, SC-007 |
| Security-sensitive endpoints are protected | US1 | FR-001 – FR-013 | SC-001, SC-002, SC-003 |
| Backup and restore process is documented | US4 | FR-028 – FR-033 | SC-010, SC-011 |
| Developer setup documentation is complete | US6 | FR-045 – FR-049 | SC-012, SC-013 |
| Load testing *(scope item, no stated criterion)* | US2 | FR-041 – FR-044 | SC-014 |

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An automated abuse simulation against sign-in is refused within 10 attempts
  per account per minute, and 100% of subsequent attempts in that window are refused.
- **SC-002**: With the platform running on at least two instances, an abuse simulation
  spread across all of them is refused at the documented single budget, with total allowed
  requests within 10% of that budget rather than a multiple of it.
- **SC-003**: 100% of responses carry the required browser protection directives, verified
  by an automated check, with zero functional regressions in the dashboard or widget.
- **SC-004**: An operator promotes a chosen commit to staging in under 30 minutes of elapsed
  time following only the written procedure, with zero manual steps outside it.
- **SC-005**: A rollback returns the prior release to service in under 15 minutes.
- **SC-006**: For any failing request, an on-call engineer locates its logs and full trace
  in under 5 minutes starting only from a correlation identifier.
- **SC-007**: 100% of injected unhandled errors appear as grouped records within 2 minutes
  of occurring, and zero telemetry records contain unmasked secrets or customer personal
  data when audited against a deliberate injection of both.
- **SC-008**: The full suite passes on a clean checkout of the default branch, **and** a
  deliberately introduced failing test causes the gate to fail — both verified.
- **SC-009**: The same commit yields an identical gate verdict on three separate runs, and
  the executed-test count is visible in every run's report.
- **SC-010**: A restore drill reconstructs all tenant data with measured data loss within
  the documented recovery point objective and elapsed time within the documented recovery
  time objective.
- **SC-011**: A deliberately failed backup produces an operator alert within one scheduled
  interval, with zero silent failures.
- **SC-012**: A person who has never set up the project reaches a running stack in under 60
  minutes using only the written documentation, asking zero questions.
- **SC-013**: 100% of environment variables the platform reads appear in the configuration
  reference, verified against the code.
- **SC-014**: A load test demonstrates the documented concurrency target with error rate
  under 1% and latency within the documented target, and a sustained run of at least one
  hour shows no upward drift in latency or memory.

## Assumptions

Concrete technology choices below are assumptions recorded for the planning phase, not
requirements; the requirements above are stated as observable behavior.

- **Staging deployment target — decided.** A **single provisioned virtual machine running
  the existing container stack** (Option A). Staging reuses the shape of
  `infra/docker-compose.yml` under a deployment-specific profile: self-hosted Postgres with
  pgvector, self-hosted cache, self-hosted S3-compatible object storage, the backend, and
  both frontend builds, fronted by a reverse proxy terminating TLS. Promotion is a
  pull-and-restart of pinned image tags over an authenticated remote session. This keeps
  control of the pgvector version — the platform requires ≥ 0.8.0 for `hnsw.iterative_scan`,
  which managed database offerings cannot be assumed to provide.
- **Consequences of the single-VM target.** Every managed-service convenience must be built:
  there are no automated database snapshots, no point-in-time recovery, and no
  platform-native rollback. This moves real work into US4 (backup/restore) and US2
  (promotion/rollback) that a managed platform would have supplied. The host is also a
  single point of failure with no redundancy — accepted, because staging is a validation
  environment, and because high-availability topology is explicitly out of scope below.
- **Existing infrastructure is reused.** The already-present cache service is assumed
  available as the shared counter store for throttling (FR-003), and the existing
  relational database and object storage are the backup subjects (FR-028). No new datastore
  is introduced.
- **Multi-instance verification.** SC-002 requires at least two server instances to prove
  throttle budgets do not multiply per instance. On a single host this is satisfied by
  running two backend containers behind the reverse proxy — the requirement is two
  processes sharing one budget, not two machines.
- **Backup approach.** Because the database and object store are self-hosted, backups are
  self-built: scheduled logical database dumps, encrypted, written to **off-host** storage,
  plus a scheduled synchronisation of the object store's contents to the same off-host
  destination. Continuous write-ahead-log archiving is the assumed mechanism if the
  recovery point objective below proves too weak in practice. Assumed objectives absent
  other direction: 24-hour recovery point, 4-hour recovery time, 30-day retention.
- **Throttle thresholds.** Assumed defaults absent other direction: 10 failed sign-ins per
  account per minute; existing widget budgets retained; per-tenant authenticated budget
  retained at its current level. All adjustable per FR-006.
- **Browser directive set.** Assumed to mean anti-framing, content-type-sniffing prevention,
  a content source policy, a referrer policy, and transport security — with the content
  source policy rolled out in report-only mode first to find breakage before enforcing.
- **Observability stack.** Trace export via a vendor-neutral protocol to a collector;
  metrics scraped in the conventional text exposition format; dashboards defined as
  version-controlled definitions rather than hand-built in a UI. **Logs are shipped to an
  aggregator co-located on the same host and queried through the same dashboard tool as
  metrics** — FR-014 requires logs *and* traces to be retrievable together for one
  correlation identifier without shell access, which container stdout alone cannot satisfy.
  A single-binary aggregator writing to the host filesystem is assumed, matching the
  single-VM cost constraint; object-store or clustered backends are out of scope.
- **Alert delivery.** FR-021 requires notifying an on-call recipient. The alert *rules* and
  a routing component are in scope and version-controlled; the concrete destination
  (email/chat/pager) is supplied as external configuration at deploy time, since it depends
  on accounts this feature does not provision.
- **Load testing tool.** A scriptable load generator with version-controlled scenarios,
  runnable locally and against staging.
- **Scope boundaries.** Production environment provisioning, formal compliance
  certification (SOC 2, ISO 27001), third-party penetration testing, multi-region or
  high-availability topology, autoscaling, a public status page, and disaster-recovery
  failover to a second region are all **out of scope** for this feature. Staging is the
  target environment; the procedures are written so production can follow later.
- **No new user-facing product surface.** The health view (FR-018) and dashboards are
  operator-facing. This feature adds no tenant-visible feature; the platform's existing
  permission model governs any new operator view, reusing platform-scoped diagnostics
  permissions rather than introducing a parallel access model.
- **Documentation location.** Operator and developer documentation lives in the repository
  alongside the code so it is reviewed and versioned with it.
- **Verification of "reliable CI".** Reliability is assumed to mean deterministic and
  non-vacuous — the gate catches real breakage and yields the same verdict for the same
  commit. It is not assumed to mean a wall-clock runtime target, though gate duration
  should not regress materially.
