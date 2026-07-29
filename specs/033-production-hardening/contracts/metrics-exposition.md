# Contract: Metrics Exposition and Probe Endpoints

**Feature**: 033-production-hardening | **Requirements**: FR-014 – FR-021, FR-027

Documents the one endpoint contract this feature **changes**, and pins the two it must
leave alone.

---

## `GET /metrics` — CHANGED

**Prior contract** (spec 032 recorded these probes as untouched): returns
`text/plain; charset=utf-8` with the literal body `"# no metrics yet\n"`.

**New contract**: same status and content type, body is Prometheus text exposition format
carrying the series in `data-model.md`.

**Access**: unauthenticated at the application, restricted by Caddy to the
compose-internal network and loopback. Not reachable from the internet.

This is a deliberate supersession of the 032 clarification, recorded in `research.md` §3.
`/health` and `/ready` are **not** superseded.

### Label constraint (enforced by test)

No series may carry `tenant_id`, user ID, session ID, or a raw request path. Routes appear
as templates (`/tenant/conversations/:id`), never as concrete paths. A test asserts the
exposition contains no UUID-shaped label value — per-tenant labels would leak tenant
activity to any scraper (Principle II) and grow cardinality without bound (Principle X).

---

## `GET /health` — UNCHANGED

`200 OK`, body `{"status": "ok"}`. Liveness only. Must not acquire a database connection,
check a dependency, or require authorization — see `plan.md` Complexity Tracking for the
Principle III justification.

## `GET /ready` — UNCHANGED

Existing `HealthReport` shape from `observability::health`: `{ status, checks[] }` with
`200` when ready and `503` when not. Contract preserved exactly, because `ops/deploy.sh`
gates traffic on it (FR-027) and the container healthcheck depends on it.

`dependency_up` metrics are derived from these same check results rather than re-probing
(see the 032 boundary in `plan.md`).

---

## Trace and log correlation (FR-014)

The existing request-ID middleware generates the identifier; this feature attaches it as
a span attribute so one identifier joins the three systems:

| System | Field |
|--------|-------|
| Logs | `request_id` (structured field, already emitted) |
| Traces | `request_id` span attribute (new) |
| Errors | `request_id` tag on the Sentry event (new) |

An operator pastes one identifier into any of the three and reaches the other two. This is
the whole of FR-014 and SC-006.

## Scrubbing at capture (FR-019)

Applied in the Sentry `before_send` hook and in the OTLP span processor — **at egress,
before data leaves the process**, not at display:

- Removed: request bodies, `Authorization`, `Cookie`, `X-Tenant-ID`, any header matching
  `/secret|token|key|password/i`
- Removed: message content, customer names, email addresses, phone numbers
- Retained: request ID, route template, status, duration, tenant UUID (an opaque
  identifier, not personal data)

The existing `observability::redact()` helper establishes the convention; this extends it
to the export boundary.

## Sampling (FR-020)

Head sampling at `APP_TRACE_SAMPLE_RATIO`, parent-based. The Collector applies a
tail-based rule retaining **100% of traces containing an error span**, regardless of the
head decision. Head sampling alone cannot satisfy FR-020 — the sampling decision is made
at span start, before the request's outcome is known.
