# ADR-020: Tenant-scoped customer data export and erasure

- **Status:** Accepted
- **Date:** 2026-11-24
- **Deciders:** Identity & Access and Platform Engineering
- **Related:** MOH-103, [ADR-004](0004-multi-tenancy-rls-cells.md),
  [ADR-006](0006-authorization-model.md), [ADR-007](0007-postgres-queue-transactional-outbox.md),
  [ADR-017](0017-module-layering-and-ci-checked-boundaries.md), [ADR-019](0019-audit-log-shared-crate.md)

## Context

Constitution Principle X requires per-tenant export and deletion of personal data. Customer contact
details, conversations, derived records and files span multiple module owners. Analytics reads
operational rows directly. The [inventory](../security/personal-data-inventory.md) identifies rows
whose removal would change historical metrics and records that lack reliable subject attribution.

## Decision

### Requests and authorization

Add a `privacy` module called by the server. It owns `privacy_requests` and
`privacy_request_steps`. Each owning module exposes tenant-scoped export and erasure operations for
its own tables. Use Row-Level Security, the tenant isolation harness and composite tenant foreign
keys. Apply a unique partial index to active erasure requests. Repeated requests return the
existing erasure request, including a completed request for an already erased customer.

Under the tenant API, require dedicated `customers.export` and `customers.erase` permissions,
granted by default to Owner and Admin. Apply the existing MFA step-up middleware to both POST
routes. Require the submitted confirmation to equal the path customer ID and return 422 for
missing or mismatched confirmation. Both POST routes return a request ID and status with 202.
Use RFC 9457 errors. Return 404 for unknown customers or wrong tenant, and 409 when an export
is requested for a customer undergoing erasure or already erased.

- `POST /tenant/customers/{id}/export` queues an export.
- `POST /tenant/customers/{id}/erase` queues erasure and immediately marks the customer erasure-pending.
- `GET /tenant/customers/{id}/privacy-requests/{request_id}` returns status using authorization
  for that request kind.
- `GET /tenant/customers/{id}/export/{request_id}/download` checks tenant, customer,
  export permission, request completion and expiry before streaming the bundle.

### Execution and retention

Use a PostgreSQL claim, lease and retry worker with ordered, durable steps and bounded attempts.
Keep generic failure codes. Fence late workers with claim ownership and make every step idempotent.
A missing object counts as deleted. Keep object locators until deletion succeeds.

Create a ZIP with JSON records, original attachments and a manifest. Restrict it to the approved
subject scope. Use authenticated download and a 24-hour expiry. Delete prior bundles on erasure
and sweep expired objects, clear their locators and retain request status. Use a 2 GiB archive limit and bounded streaming
through the storage backend. Validate the ZIP dependency with Cargo Deny.

Keep the customer row as a tombstone with `erased_at`. Clear personal columns and remove channel
identifiers so a new contact can create a new customer. Remove widget sessions and attachments.
Preserve the operational rows and structural columns read by analytics. Scrub personal content
according to the inventory. Fence reads and new writes while erasure is pending, including
workers and delivery callbacks. Publish completion only after required steps finish.

Write `customer.export_requested`, `customer.export_completed`,
`customer.erase_requested` and `customer.erased` audit entries with actor, tenant, customer
and request identifiers. Keep the append-only constraint and retain historical security and accountability records. Older
audit rows may contain free text written by staff. Customer IDs are pseudonymous. New escalation
audit records use structured fields, reason length and skill IDs. Platform-access reasons remain
staff accountability records. Keep customer personal data out of those reasons.

## Known limitations

- Attribution follows the tenant customer row, its channel aliases, conversations, messages and
  widget sessions. Independently created customer records are separate subjects.
- Retain webhook content and AI usage without a reliable subject link, plus unattributed immutable
  prompt versions. The inventory documents historical audit and identity retention separately.
- Retention and restore procedures govern existing backups. Object versions, external provider
  copies and retained telemetry are outside the online erasure worker's coverage.
  Bytes from a download already admitted before erasure cannot be recalled.
- The worker stages each archive in a private temporary file. Normal cleanup removes it.
  After an abrupt worker termination, the host's temporary-file cleanup must remove any orphan.

## Alternatives considered

| Option | Why declined |
| --- | --- |
| Delete the entire customer graph | Conversation foreign keys restrict customer deletion, and operational aggregates would change. |
| Execute privacy requests in the HTTP handler | File operations and multi-module changes need durable progress and retry. |
| Give privacy direct SQL access to every table | It bypasses the module ownership checked by ADR-017. |
| Provide a long-lived presigned link | The authenticated endpoint enforces current authorization, expiry and erasure state at download time. |

## Consequences

**Positive**

- Modules keep their data boundaries and tenant checks. Historical metrics retain their inputs.
- Operators can observe requests and resume failed steps. Export links remain subject to tenant access.

**Negative / costs**

- Worker fencing, attachment cleanup and expiry require durable state.
- Pseudonymous IDs and aggregates need clear retention rules. Existing backups keep their
  original contents until expiry and require replay of erasures before restored data is served.
- Unattributed provider usage, opaque webhook payloads and immutable copies need specific policies.

**Follow-ups**

- Add upstream subject attribution when those systems can supply consistent structured IDs.
- Implement the catalog coverage check, marker and cross-tenant isolation tests, authorization
  negatives, retry tests and analytics invariance checks.
- Update the OpenAPI snapshot, generated client types and operations runbooks with the implementation.
