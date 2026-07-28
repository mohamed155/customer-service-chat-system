# Phase 0 Research: Production Hardening

**Feature**: 033-production-hardening | **Date**: 2026-07-26

All Technical Context unknowns are resolved below. Each decision records what was chosen,
why, and what was rejected. Decisions 1–3 are the ones that constrain the rest.

---

## 1. Shared rate-limit counter — algorithm and trait shape

**Decision**: A Redis sliding-window counter implemented in a new
`shared/cache/src/limiter.rs`, exposed through an **async trait `RateLimiter`** owned by
`kernel`, with two implementations: `RedisRateLimiter` (production/staging) and the
existing `InMemoryRateLimitStore` (tests, and the documented degraded mode).

The counter is a fixed window with a rolling sub-window correction: keys are
`rl:{scope}:{identifier}:{window_epoch}`, incremented with `INCR` and given `EXPIRE` on
first write, both issued in a single pipelined round-trip. The previous window's count is
weighted by elapsed fraction to smooth the boundary burst that a naive fixed window
allows.

**Rationale**: The existing `InMemoryRateLimitStore::check(&self, key, limit, window)` is
synchronous and is called at 5 sites (`server/src/rate_limit.rs`,
`widgets/src/public_routes.rs:480`, `feedback/src/public_routes.rs:40`,
`whatsapp/src/webhook.rs:119`, `integrations/src/webhook.rs:157`), each receiving it as
`Extension<Arc<InMemoryRateLimitStore>>`. Redis I/O is async, so the trait method must be
`async fn`; every call site changes regardless of algorithm. Given that the refactor is
unavoidable, the trait is designed once, properly, rather than minimally.

A pure fixed window was rejected because it permits 2× the budget across a window
boundary, which would fail SC-002's "within 10%" tolerance under an adversarial pattern.
A true sliding log (sorted set per key) was rejected as O(n) memory per identifier — on a
single VM sharing RAM with Postgres, storing one member per request is the wrong trade.

**Alternatives considered**: `tower-governor` (a maintained Axum rate-limit layer) —
rejected because its store abstraction is per-process by default and its keying model
does not express "authenticated identity when present, source address otherwise"
(FR-004) without substantial adaptation; wrapping it would be more code than the counter
it replaces. Redis Lua script for atomicity — rejected as unnecessary since `INCR` is
already atomic and the window correction is a read-side computation.

---

## 2. Degradation when Redis is unreachable (FR-007)

**Decision**: Split by endpoint sensitivity, configured by
`APP_RATE_LIMIT_FAIL_MODE` per scope:

- **Authentication endpoints fail closed.** If the counter store is unreachable, sign-in
  is refused with 503. An attacker who can knock Redis over must not thereby gain
  unlimited credential-guessing.
- **All other scopes degrade to per-instance in-memory counting.** Widget and webhook
  traffic continues to be served with a locally-enforced budget, which is strictly better
  than no limit and strictly worse than the shared one — matching today's behaviour.
- Both transitions emit a `WARN` log and increment
  `rate_limit_degraded_total{scope}`, so the condition is visible on the dashboard rather
  than silent.

**Rationale**: FR-007 requires a *documented* behaviour and forbids silently permitting
unlimited traffic to authentication endpoints. Uniform fail-closed was rejected because
a Redis outage would take the entire product offline — including the widget, which is
customer-facing — turning a cache failure into a total outage. Uniform fail-open was
rejected because it makes Redis availability a security boundary an attacker can attack.

On a single VM, Redis and the application share a host, so the realistic failure is a
Redis process crash or restart rather than a network partition; the degraded window is
expected to be seconds, which makes per-instance fallback proportionate for non-auth
traffic.

**Alternatives considered**: Queue-and-retry on the request path — rejected as
latency-unbounded on a hot path.

---

## 3. `/metrics` exposure and the 032 contract change

**Decision**: `/metrics` becomes real Prometheus text exposition (replacing the
`"# no metrics yet"` stub), stays unauthenticated at the application layer, and is
**restricted at the reverse proxy** to the compose-internal network and loopback. It is
not reachable from the internet.

**Rationale**: 032's clarification session recorded that `/health`, `/ready`, and
`/metrics` "keep their current contract and stay untouched". 033's FR-015 supersedes that
for `/metrics` specifically, and this document is the record of that superseding —
`/health` and `/ready` genuinely keep their contracts.

The exposure question is real: per-route error and latency series disclose traffic
patterns and operational weak points, so leaving a real `/metrics` open to the internet
would be a regression introduced by the very feature meant to harden the platform.
Network restriction is the correct answer *because* the target is a single VM: Prometheus
runs as a local process on the same host, so loopback and the compose network are the
only origins that ever need access. There is no cross-host scrape to accommodate.

**Alternatives considered**: Bearer-token auth on `/metrics` — rejected as a secret to
distribute and rotate for no gain over a network ACL in this topology, though it is the
right answer if the deployment shape later spans hosts, and the plan notes that.
Binding the metrics server to a second port on `127.0.0.1` — a clean alternative, rejected
only because it splits the Axum server into two listeners and complicates the container's
port model; the proxy ACL achieves the same result with less moving structure.

**Metric cardinality constraint**: series are labelled by route template, method, and
status class — **never by `tenant_id`, user ID, session, or raw path**. Per-tenant labels
would leak tenant activity to any scraper and grow series count without bound. Per-tenant
usage analytics already exist in the analytics module (spec 025) and belong there.

---

## 4. Security header delivery and the widget exception

**Decision**: A `tower-http::set_header::SetResponseHeaderLayer` stack applied at the
router root for the shared headers, plus a distinct layer on the widget public scope that
overrides the framing and content-source policy. Header **values** come from `AppConfig`
so they differ per environment (FR-013).

Shared on every response: `X-Content-Type-Options: nosniff`,
`Referrer-Policy: strict-origin-when-cross-origin`,
`Strict-Transport-Security: max-age=31536000; includeSubDomains`.

Dashboard/API scope: `X-Frame-Options: DENY` plus CSP `frame-ancestors 'none'`.

Widget public scope (`/widget/v1/*` and the widget app): **no** `X-Frame-Options`; CSP
`frame-ancestors` set from the widget instance's existing domain allowlist, since the
widget's entire purpose is being framed by allowlisted customer sites.

**Rationale**: A blanket `DENY` would break the product's own embeddable channel (spec
023), which is the single most likely way this feature ships a regression. The widget
module already stores a domain allowlist per widget instance, so the correct
`frame-ancestors` value is available without new data.

CSP is rolled out **`Content-Security-Policy-Report-Only` first**, with violations
collected for one staging soak before switching to enforcing mode. Angular's runtime
style injection is the known breakage risk, and report-only is how that is found without
an outage.

**Alternatives considered**: Emitting headers from Caddy rather than the application —
rejected because the headers would then be absent in local development and in the
`test:e2e:real` suite, so nothing would test them; SC-003 requires an automated check
that 100% of responses carry them. Caddy still sets HSTS at the TLS edge as
defence-in-depth.

---

## 5. Trace export, sampling, and error retention

**Decision**: `tracing-opentelemetry` layered onto the existing subscriber, exporting
OTLP/gRPC to a local OpenTelemetry Collector, which fans out to the trace backend.
Sampling is **parent-based, ratio sampled at `APP_TRACE_SAMPLE_RATIO`** (staging default
1.0, production default 0.1), with a **tail-based error rule in the Collector** that
retains 100% of traces containing an error span regardless of the head sampling decision.

**Rationale**: FR-020 requires that an errored request's trace is always retained. Head
sampling alone cannot satisfy this — the sampling decision is made at span start, before
it is known whether the request will fail. The Collector's tail sampling processor is the
standard mechanism, and running a Collector also decouples the application from the
choice of backend (Jaeger, Tempo, or a hosted vendor) which matters given the
constitution's provider-independence disposition.

The existing `observability::trace::trace_middleware` and `request_id` middleware already
create the span and correlation ID; this decision adds an export layer beneath them
rather than replacing them, so FR-014's "retrievable together by correlation identifier"
works by injecting the existing request ID as a span attribute.

**Alternatives considered**: Direct OTLP export from the application to a vendor —
rejected as it hard-codes the backend and loses tail sampling, which is a Collector
capability. Sampling everything at 1.0 in production — rejected on cost/throughput
grounds; deferred by making the ratio configurable.

---

## 6. Error tracking

**Decision**: Sentry via `sentry` + `sentry-tracing`, capturing `ERROR`-level tracing
events and panics, with a `before_send` scrubber. Self-hosted or SaaS is a deployment-time
choice: the DSN is configuration (`APP_SENTRY_DSN`), and **an unset DSN disables capture
entirely** so local development and CI send nothing.

**Rationale**: FR-017's requirements — grouping, occurrence count, first/last seen,
release identifier, code path — are exactly an error tracker's data model, and building
this on top of log aggregation would mean reimplementing fingerprinting and deduplication.
`sentry-tracing` integrates with the subscriber already initialised in
`observability::init_observability`, so capture is a layer rather than a call site change.

**Scrubbing is the load-bearing part** (FR-019). The `before_send` hook strips request
bodies, `Authorization` and `Cookie` headers, `X-Tenant-ID`, provider API keys, and
message content. The existing `observability::redact()` helper establishes the convention;
this extends it to the egress boundary because FR-019 requires masking *at capture, before
export*, not at display.

**Alternatives considered**: Deriving grouped errors from structured logs in Loki —
rejected because grouping/fingerprinting is the hard part and would be hand-built. A
self-hosted Sentry on the same VM — rejected for staging: it is a heavy multi-service
deployment that would compete with Postgres for the host's RAM, contradicting the
single-VM constraint. SaaS free tier is sufficient at staging volume.

---

## 7. Staging deployment mechanism

**Decision**: `infra/docker-compose.staging.yml` as an overlay on the existing compose
file, with all images referenced by **immutable tag** (the Git SHA). `ops/deploy.sh`
takes a SHA, verifies the images exist in the registry, runs migrations via a one-shot
container, waits for `/ready` to report ready, then switches traffic. `ops/rollback.sh`
takes the previous SHA and repeats the switch without the migration step.

Caddy provides TLS via automatic certificate issuance and holds the `/metrics` ACL.

**Rationale**: The compose stack already exists and is the local development environment,
so staging reuses a known-good topology and FR-026 ("staging matches production in shape")
is satisfied by construction. Immutable SHA tags are what make FR-040 (the gate publishes
an artifact identifying the exact build) and rollback meaningful — `latest` tags make
"deploy this commit" and "roll back" both unverifiable.

Migrations run **before** traffic switches (FR-023) as a separate one-shot container so
that a migration failure aborts the deploy with the old release still serving.

**Rollback across destructive migrations is explicitly not solved by the script.**
`ops/rollback.sh` detects whether any migration ran between the two SHAs and, if so,
refuses and directs the operator to `docs/backup-restore.md`. FR-024 requires the
procedure to state which changes it cannot reverse; making the script *refuse* is stronger
than documenting the caveat and hoping it is read.

**Alternatives considered**: Ansible/Terraform — rejected as disproportionate for one
host; the provisioning is a documented one-time bootstrap script. Kubernetes (k3s) —
rejected as far more operational surface than a single staging VM justifies.
Blue/green with two compose stacks — rejected because it doubles the RAM requirement on a
host already running Postgres, Redis, and MinIO; brief downtime during a staging switch is
acceptable.

---

## 8. Backup and restore

**Decision**: Nightly `pg_dump --format=custom` piped through `age` encryption to
**off-host** object storage (a separate provider bucket, not the local MinIO), plus
`mc mirror` of the MinIO bucket to the same off-host destination. Continuous WAL archiving
is **deferred** — the assumed 24-hour RPO does not require it, and it is recorded as the
upgrade path if the drill shows 24 hours is too weak.

Backup success/failure is exported as a Prometheus metric via the node-exporter textfile
collector (`backup_last_success_timestamp_seconds`), and the alert fires on staleness
rather than on a failure event — a backup that never ran produces no failure event, so
alerting on absence is the only construction that catches the silent case (FR-032).

`ops/verify-backup.sh` performs the FR-030 drill: restore the latest backup into a
throwaway container, run the migration checker, count rows per tenant, and compare against
the source.

**Rationale**: The single-VM target means no managed snapshots exist to inherit — this is
entirely self-built, which is why the spec flags US4 as the largest build. Off-host
storage is non-negotiable (FR-028): a backup on the machine being backed up is lost with
it. `age` is chosen over GPG for a substantially smaller operational surface (one
keypair, no keyring).

The **object/database consistency window** (FR-029) is handled by ordering: snapshot the
object store *first*, then dump the database. A row referencing an object then always
finds it, because the object existed before the row was captured. The reverse order can
produce a row pointing at an object that was not yet mirrored.

**Alternatives considered**: `pg_basebackup` + WAL for point-in-time recovery — deferred
as above, not rejected. Backing up to a second local disk — rejected, does not survive
host loss. Filesystem/VM snapshots — rejected as not application-consistent for Postgres.

---

## 9. CI consolidation and non-vacuous verification

**Decision**: Delete `.github/workflows/ci.yml` entirely; `backend.yml` becomes the single
authoritative backend gate. Add `backend/rust-toolchain.toml` pinning the toolchain and
components. Unify `frontend.yml` on pnpm 10.34.4 in both jobs. Add a `concurrency` group
so superseded pushes cancel.

Non-vacuity (FR-034, FR-035, FR-039) is enforced by three mechanisms:

1. `REQUIRE_DB_TESTS=1` is set for the whole job, so DB-backed tests fail rather than
   self-skip.
2. `cargo nextest` reports an executed-test count, and a step asserts the count is **at or
   above a committed floor**, so a silent drop in executed tests fails the build.
3. A scheduled `gate-verification` workflow applies a patch that breaks one test, confirms
   the gate fails, and reverts — a regression test for the gate itself.

`cargo audit` and the secret scan lose `continue-on-error` (FR-038).

**Rationale**: The current `ci.yml` runs no migrations and omits `REQUIRE_DB_TESTS`, so its
`cargo test --workspace` silently skips DB-backed tests while reporting green — the exact
failure mode SC-008 exists to close. Two workflows checking the same concern with
different rigour means the weaker one's green result is meaningless, so consolidation is
correctness, not tidiness.

The executed-count floor is the mechanism that makes FR-035 enforceable rather than
informational: a human reading a count in a log will not notice it dropping from 812 to
absent, but an assertion will.

**Alternatives considered**: Keeping both workflows and fixing `ci.yml` — rejected;
two gates for one concern is the defect. Coverage thresholds — rejected as a proxy metric
that is gameable and noisy; the executed-count floor targets the actual observed failure
mode directly.

---

## 10. Load testing

**Decision**: k6 with three scenarios in `loadtest/`, committed as code. Runs are manual
and against staging only. The base URL has **no default** — the script exits if
`K6_BASE_URL` is unset, and refuses any host not matching an allowlist pattern (FR-043).

**Rationale**: k6 scripts are JavaScript, so they live alongside the frontend toolchain the
team already uses, and its thresholds feature expresses SC-014's pass/fail criteria
directly in the script rather than in a human's reading of a report. Requiring an explicit
target with an allowlist is the mechanism for "MUST NOT be capable of accidental execution
against real tenants' environment" — a default of `localhost` would be one careless
environment variable away from pointing at production.

**Alternatives considered**: Locust — rejected, adds a Python toolchain the repo does not
otherwise have. Artillery — comparable, k6 chosen for better resource efficiency when the
load generator may share the staging host's network.
