# Contract: Security Response Headers

**Feature**: 033-production-hardening | **Requirements**: FR-009 – FR-013 | **SC**: SC-003

Cross-cutting response contract. Applies to every HTTP response the backend emits and to
the static frontend builds. Verified by `backend/crates/server/tests/security_headers.rs`
(new) and by an assertion in the Playwright real-backend suite.

---

## Scope A — every response (API, dashboard, widget, probes)

| Header | Value | Source |
|--------|-------|--------|
| `X-Content-Type-Options` | `nosniff` | constant |
| `Referrer-Policy` | `strict-origin-when-cross-origin` | constant |
| `Strict-Transport-Security` | `max-age={APP_HSTS_MAX_AGE}; includeSubDomains` | config; omitted entirely when max-age is `0` |

`Strict-Transport-Security` is emitted by both the application and Caddy. Duplicate
identical values are harmless; the application copy exists so local development and the
`test:e2e:real` suite exercise the same code path that production uses. **Local
development sets `APP_HSTS_MAX_AGE=0`** — a non-zero value pins `localhost` to HTTPS in the
developer's browser and is painful to unpin.

## Scope B — dashboard and API (everything except the widget scope)

| Header | Value |
|--------|-------|
| `X-Frame-Options` | `DENY` |
| `Content-Security-Policy` | includes `frame-ancestors 'none'` |

## Scope C — the widget document (`frontend/apps/widget` `index.html`)

**Corrected 2026-07-26.** An earlier draft of this contract specified per-request
`frame-ancestors` on the backend's `/widget/v1/*` JSON responses, derived from each widget
instance's domain allowlist. That was wrong on two counts, verified against the code:

1. The backend serves **no HTML** — there is no `ServeDir`, `ServeFile`, `Html(`, or
   `text/html` anywhere in `crates/server/src/` or `crates/modules/widgets/src/`. The
   framed document is `frontend/apps/widget/src/index.html`, a static Angular build served
   by the frontend container/Caddy.
2. `frame-ancestors` and `X-Frame-Options` only govern responses that are *documents being
   embedded*. They have **no effect on JSON** fetched by script inside an already-framed
   page. A per-request allowlist lookup on the JSON path would therefore be a no-op that
   also adds a database read to the hot path.

| Response | Header | Value |
|----------|--------|-------|
| Widget `index.html` (static, via Caddy) | `X-Frame-Options` | **absent — must not be set** |
| Widget `index.html` (static, via Caddy) | `Content-Security-Policy` | `frame-ancestors *` (see below) |
| `/widget/v1/*` JSON (backend) | `X-Frame-Options` | absent |
| `/widget/v1/*` JSON (backend) | `Content-Security-Policy` | Scope A headers only; no `frame-ancestors` |

**Why `frame-ancestors *` rather than a per-tenant allowlist**: the widget document is one
static artifact shared by every tenant, so it cannot carry a tenant-specific policy without
becoming dynamically served — a significant architecture change this feature does not make.
Embedding authorization is **already enforced** by the widget module's existing domain
allowlist (spec 023), which checks the requesting origin when a session is created. That is
a server-side check an attacker cannot bypass by framing the document, so it, not the
header, is the security boundary. `frame-ancestors *` on the document is therefore
accurate rather than permissive: the document genuinely may be framed anywhere, and framing
it without an allowlisted origin yields a widget that cannot open a session.

**Scope B must not leak onto the widget document.** Serving it with `X-Frame-Options: DENY`
breaks the platform's own embeddable channel — the most likely regression this feature could
ship, and the reason Scope B is defined as "everything except the widget" rather than as a
global default.

---

## CSP rollout

`Content-Security-Policy-Report-Only` is emitted instead of `Content-Security-Policy`
while `APP_CSP_REPORT_ONLY=true` (the default). Enforcing mode is enabled only after a
staging soak produces no violations from normal use.

Angular's runtime style injection is the known breakage risk. Base policy:

```
default-src 'self';
script-src 'self';
style-src 'self' 'unsafe-inline';
img-src 'self' data: blob:;
connect-src 'self' {api_origin} {otlp_origin};
frame-ancestors <per scope above>;
base-uri 'self';
form-action 'self';
object-src 'none'
```

`style-src 'unsafe-inline'` is a deliberate, documented concession to Angular's style
injection. It is **not** extended to `script-src`, where it would defeat the policy's
purpose.

---

## Non-goals

- No `Permissions-Policy` — no feature currently requires restricting browser APIs, and an
  unreviewed value risks breaking widget media capture later.
- No CSP `report-uri`/`report-to` endpoint. Report-only violations are read from browser
  consoles during the staging soak; a collection endpoint is unauthenticated write surface
  that this feature is not adding.

## Test impact

`backend/crates/server/tests/cors.rs` and `tests/health.rs` assert on response headers and
**will need updating** — these are the two existing suites this contract disturbs.
