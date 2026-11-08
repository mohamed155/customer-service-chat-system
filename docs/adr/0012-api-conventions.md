# ADR-012: API conventions — `/api/v1`, OpenAPI 3.1, RFC 9457 errors, keyset pagination, idempotency keys

- **Status:** Accepted (MOH-18)
- **Date:** 2026-11-07
- **Deciders:** Platform engineering
- **Related:** [System architecture §14 and §17](../architecture/system-architecture.md#14-scalability),
  constitution Principle VIII, ADR-002 (typed errors, utoipa), ADR-006 (tenant header),
  ADR-007 (idempotent consumers), ADR-013 (identifiers)

## Context

The HTTP API is used by the Angular dashboard, the widget, channel webhooks, and later by
tenants' own servers through API keys. Every endpoint needs the same answers to the same
questions: where it lives, how it's described, how it fails, how lists page, and what happens when
a client retries.

The decision has to balance these forces:

- **Contract stability.** Frontend and third-party clients break when shapes change silently.
- **Generated clients.** The constitution requires the TypeScript client to be generated from the
  contract, so the contract must be complete and accurate.
- **Safe errors.** Errors must be machine-readable and must never leak internals (SQL, stack
  traces, hostnames).
- **Large lists.** Inboxes, messages and audit logs grow without bound; `OFFSET` gets slower and
  skips or repeats rows under concurrent writes.
- **Retries.** Networks fail after the server has acted. A retried "send message" or "create
  connection" must not happen twice.

## Decision

### 1. Versioned base path

- All business endpoints live under **`/api/v1`**. Public widget endpoints live under
  `/widget/v1` and webhooks under `/webhooks/...` on the `gateway` (ADR-010).
- Within a version, changes are **additive only**: new endpoints, new optional fields, new enum
  values that clients must tolerate. Removing or renaming a field, changing a type, or tightening
  validation is breaking.
- A breaking change ships as `/api/v2` for the affected resources, or after a **deprecation
  window** (at least 90 days for third-party clients) announced with `Deprecation` and `Sunset`
  headers.

### 2. OpenAPI 3.1 generated from code

- The OpenAPI 3.1 document is generated from handler annotations (utoipa, ADR-002), served at
  `/api-docs/openapi.json`, and committed as a CI artifact.
- **Every** route appears in the document with its request, response and error schemas, and its
  required permission (an `x-permission` extension).
- The **TypeScript client is generated** from the document; hand-written DTOs are not allowed.
- CI fails if the generated document changes without the client being regenerated, and runs a
  breaking-change diff (e.g. `oasdiff`) against `master`.
- Contract tests (e.g. Schemathesis) check that responses match the document.

### 3. Errors: RFC 9457 Problem Details

- Every error response is `application/problem+json`:

  ```json
  {
    "type": "https://docs.example.com/problems/validation-failed",
    "title": "Validation failed",
    "status": 422,
    "detail": "One or more fields are invalid.",
    "instance": "/api/v1/tenant/conversations/018f…",
    "code": "validation_failed",
    "request_id": "01J…",
    "errors": [{ "field": "email", "code": "invalid_format" }]
  }
  ```

- `code` is a stable, documented machine code; clients branch on `code`, never on `detail` text.
- **5xx responses never include internal detail.** The handler logs the real error with the
  request ID; the client gets a generic `detail` and the `request_id` to quote to support.
- Tenants the caller can't see return `404`, not `403` (ADR-006), to avoid enumeration.

### 4. Keyset (cursor) pagination

- List endpoints take `limit` (default 25, max 100) and an opaque `cursor`, and return
  `{ "data": [...], "next_cursor": "…" | null }`.
- The cursor encodes the sort key of the last row (e.g. `(created_at, id)`, base64url-encoded and
  integrity-protected), and the query is `WHERE (created_at, id) < ($1, $2) ORDER BY created_at
  DESC, id DESC LIMIT $n`, backed by a matching index with `tenant_id` first (ADR-004).
- `OFFSET` pagination is not used. Total counts are optional, separate, and may be estimates.

### 5. Idempotency keys

- Non-idempotent `POST` endpoints that create resources or trigger side effects (send message,
  create connection, start export, run tool, payments) **accept an `Idempotency-Key` header**
  (client-generated UUID), as in the IETF `Idempotency-Key` draft.
- The server stores `(tenant_id, actor, key)` with a hash of the request body and the final
  response, in **PostgreSQL** for 24 hours (Redis may cache hot keys, per §7.3).
- A repeat with the same key and body returns the stored response with `Idempotent-Replayed: true`.
  The same key with a different body returns `422`. A repeat while the first is still running
  returns `409`.
- Webhook consumers dedupe by provider message ID instead (ADR-007 inbox).

### 6. Other conventions

- JSON field names are `snake_case`; timestamps are RFC 3339 UTC; IDs are UUID strings (ADR-013);
  money is integer minor units plus currency code.
- Resource paths are plural nouns; actions that don't fit CRUD are sub-resources
  (`POST /conversations/{id}/claim`).
- Every response carries `X-Request-Id`; clients may send one, which is validated and propagated.
- Requests carry `X-Tenant-ID` for tenant-scoped routes (ADR-006).

## Alternatives considered

| Option | Why rejected |
| -- | -- |
| **GraphQL** | Flexible queries, but per-field authorization, query-cost limits and caching are harder, and the tenancy/permission model maps cleanly onto REST resources. Ruled out in architecture §2 without a measured need. |
| **gRPC for the browser** | Needs a proxy (gRPC-Web) and loses plain HTTP tooling, caching and simple SSE. Could be used internally later. |
| **Custom error envelope** (`{ "error": { code, message, details } }`) | Works, but it's non-standard; RFC 9457 is understood by tooling and clients. This is the current state (see below). |
| **Offset pagination** | Slow at depth and unstable under concurrent inserts (skips or duplicates). |
| **Versioning by header or media type** | Harder to see in logs, to route at the edge, and to test by hand. Path versioning is explicit. |
| **No idempotency keys, rely on client care** | Mobile networks and retries make duplicate sends inevitable. |

## Consequences

**Positive**

- One predictable shape for every endpoint, and a generated client that can't drift.
- Errors are safe to show and easy to support (`request_id`).
- Lists stay fast and stable at any depth.
- Retries are safe for every side-effecting call.

**Negative and costs**

- Keyset pagination can't jump to page N; UIs use "load more" or infinite scroll.
- Idempotency storage and its cleanup job.
- Every handler needs complete OpenAPI annotations, and CI enforcement to keep them complete.

## Current state and migration

These gaps between the current code and this decision are follow-up work. No code changes in
this ADR.

| # | Gap | Where | Follow-up |
| -- | -- | -- | -- |
| A1 | **Internal errors are sent to clients.** `ApiError::internal_error(format!(…{e}))` puts the underlying error text (e.g. `"Database transaction begin failed: {e}"`) into the response `message`. There are 68 such call sites. | `backend/crates/shared/kernel/src/lib.rs` (`internal_error`), call sites across `backend/crates/**` | **High priority (information disclosure).** Log the detail with the request ID and return a generic message for every 5xx. Add a test that a database error doesn't appear in the body. |
| A2 | **Errors aren't Problem Details.** Responses use a custom envelope `{ error: { code, message, details, request_id } }` with `application/json`. | `kernel/src/lib.rs` (`ErrorEnvelope`, `ApiError`) | Switch to `application/problem+json` with the fields above; keep `code` and `request_id`. Coordinate with the frontend error interceptor. |
| A3 | **Idempotency keys aren't enforced.** An `idempotency_middleware` and `IdempotencyKey` extractor exist, but the only store is an in-memory `HashMap` (lost on restart, not shared across instances, not tenant-scoped), and no router applies the middleware. | `backend/crates/shared/kernel/src/idempotency.rs`, `backend/crates/server/src/router.rs` | PostgreSQL-backed store keyed by `(tenant_id, actor, key)` with body hash; apply to the side-effecting endpoints listed above. |
| A4 | The base path, OpenAPI version and pagination already match: business routes are nested under `/api/v1`, the root document is OpenAPI 3.1 (utoipa), list endpoints use cursors, and no handler uses `OFFSET`. | `server/src/router.rs`, `server/src/openapi.rs` | None. |
| A5 | No CI check for OpenAPI breaking changes or client regeneration (the constitution also flags "generated TypeScript client not confirmed"). | `.github/workflows/` | Add the `oasdiff` check and client generation in CI. |
