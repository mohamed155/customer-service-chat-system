# ADR-010: Separate `gateway` deployment unit for public widget and webhook traffic

- **Status:** Accepted (MOH-17)
- **Date:** 2026-11-07
- **Deciders:** Platform engineering
- **Related:** [System architecture §2.1](../architecture/system-architecture.md#21-deployment-units),
  ADR-001 (one codebase, several deployment units), ADR-005 (verify-only keys), ADR-009
  (customer streams), ADR-011 (widget)

## Context

Two very different kinds of traffic reach the backend:

- **Authenticated business traffic**: the dashboard and platform users, with sessions,
  permissions and CSRF protection.
- **Untrusted public traffic**: the embeddable widget on any customer website (anonymous
  visitors), and inbound webhooks from WhatsApp, Telegram and other channels.

The decision has to balance these forces:

- **Blast radius.** A flood of widget or webhook traffic (a viral customer site, a webhook storm,
  or an attack) must not take down the dashboard agents use to answer customers.
- **Different scaling signals.** Public traffic scales with inbound message rate and open customer
  streams; the dashboard with agent count and API latency.
- **Different security posture.** Public endpoints have no user session, need permissive CORS
  (any allowed widget origin), framing rules for the widget, webhook signature verification, and
  aggressive rate limits. The dashboard needs strict CORS, CSRF checks and `frame-ancestors 'none'`.
- **Operational cost.** A separate codebase or service would duplicate auth, tenancy and
  persistence code.

## Decision

### 1. `gateway` is its own deployment unit, built from the same codebase

The Rust workspace builds a `gateway` binary (or run mode) next to `api`, `worker` and
`scheduler` (ADR-001). It shares module crates; only its router and middleware stack differ.

### 2. What runs where

| Unit | Serves | Doesn't serve |
| -- | -- | -- |
| `gateway` | `/widget/v1/*` (config, sessions, conversations, messages, SSE), channel webhooks (`/webhooks/whatsapp`, `/webhooks/telegram`, …), widget static assets if not on a CDN | Dashboard or platform APIs, cookie sessions |
| `api` | `/api/v1/*` dashboard and platform APIs, `/auth/*`, agent SSE | Widget or webhook endpoints |

Each unit's router simply doesn't mount the other's routes, so a public request can't reach an
internal endpoint by path.

### 3. Gateway responsibilities and limits

- **Verify, normalise, persist, acknowledge.** Webhooks are signature-verified (constant-time,
  replay window), normalised by the channel adapter, written to the database with the outbox
  event in one transaction, and acknowledged quickly. AI replies and outbound delivery happen in
  `worker` (ADR-007), not in the gateway request.
- **Rate limits and quotas** per tenant, per widget, per visitor and per IP, enforced in Redis
  (with the DB fallback from §15). Request body size limits per endpoint.
- **Holds only verify-only credentials.** It can verify widget session tokens and JWTs with public
  keys (ADR-005), but holds no signing keys for dashboard sessions.
- **Own security headers:** widget-specific CORS (per-widget allowed origins) and framing policy,
  separate from the dashboard's strict headers.
- **Own scaling and alerts:** autoscale on inbound rate and open streams; separate error budget.

### 4. Network placement

`gateway` and `api` sit behind the same edge (Caddy / load balancer) with path-based routing, or
on separate hostnames (`widget.` / `hooks.` vs `app.`). Separate hostnames are preferred, so
cookies for the dashboard origin are never sent to public endpoints.

## Alternatives considered

| Option | Why rejected |
| -- | -- |
| **One `api` unit serving everything** (current) | A public traffic spike or attack degrades the dashboard. One CORS/CSRF/header policy has to fit two opposite needs, so it ends up with exceptions. Can't scale the two independently. |
| **A separate service with its own codebase** | Duplicates tenancy, auth, persistence and channel code; contracts between the two need versioning. ADR-001 rules this out without a measured need. |
| **Serverless functions for webhooks** | Cold starts on bursts, harder access to the database and outbox, and another deployment model. |
| **API gateway product** (Kong, AWS API Gateway) **in front of one backend** | Useful for edge rate limiting, but doesn't isolate the backend process or its resources. Could be added in front of either unit later. |

## Consequences

**Positive**

- Public traffic can't exhaust the dashboard's CPU, connections or database pool.
- Each unit has a security policy that fits its traffic, with fewer exceptions.
- Independent scaling and alerting.

**Negative and costs**

- One more process to deploy, configure and monitor; separate DB pool sizing per unit.
- Routes must be assigned to exactly one unit; a test should assert no route is mounted in both.
- Local development runs two processes (or one binary with both run modes enabled).

## Current state and migration

These gaps between the current code and this decision are follow-up work. No code changes in
this ADR.

| # | Gap | Where | Follow-up |
| -- | -- | -- | -- |
| G1 | **One process serves everything.** The single `server` binary mounts the widget public routes (`/widget/v1/*`) and the dashboard routes in one router, and also runs the background workers. | `backend/crates/server/src/router.rs`, `backend/crates/server/src/main.rs` | Split the router into `api` and `gateway` route sets with a shared composition root; add run modes; then move workers to `worker` (ADR-001 M4). |
| G2 | Security headers already have a widget-specific exception (feature 033), which this split would turn into two clean policies. | `backend/crates/server/src/security_headers.rs` | Move each policy to its unit when G1 lands. |
| G3 | Widget and webhook rate limits share the same process and Redis limiter as the dashboard. | `backend/crates/server/src/rate_limit.rs` | Keep the shared Redis limiter, but configure gateway limits separately. |
