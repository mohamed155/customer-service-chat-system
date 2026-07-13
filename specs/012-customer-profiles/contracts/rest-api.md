# REST API Contract: Customer Profiles

All endpoints are tenant-scoped, mounted under the existing `/tenant` family (tenant resolved by middleware from the authenticated session + `X-Tenant-ID` contract), registered deny-by-default via `.guarded()`. Envelope, pagination, and error formats follow `specs/001-ai-customer-service-platform/contracts/rest-api.md`: success bodies are `ApiResponse<T>` (`{ "data": … }`), lists are `PaginatedResponse<T>` (`{ "data": [...], "pagination": { "next_cursor": string|null, "has_more": bool } }`), errors use the standard envelope with `code`/`message`/`details[]`, and every response carries `X-Request-Id`.

Canonical channel vocabulary (identifiers and conversations): `email | phone | web_chat | whatsapp | telegram`.

## Resource representations

### Customer (list item)

```json
{
  "id": "uuid",
  "display_name": "Sara Ali",
  "email": "sara@example.com",
  "phone": "+201001234567",
  "channels": ["email", "whatsapp"],
  "created_at": "2026-07-13T10:00:00Z",
  "updated_at": "2026-07-13T10:00:00Z"
}
```

### Customer (detail)

List item fields plus:

```json
{
  "identifiers": [
    { "id": "uuid", "channel": "whatsapp", "identifier": "+201001234567" }
  ],
  "metadata": { "plan": "enterprise", "region": "EMEA" }
}
```

### Conversation summary

```json
{
  "id": "uuid",
  "channel": "web_chat",
  "status": "open",
  "last_activity_at": "2026-07-13T09:30:00Z",
  "created_at": "2026-07-12T14:00:00Z"
}
```

`status ∈ open | escalated | closed`.

## Endpoints

### `GET /tenant/customers` — list & search

**Permission**: `customers.view`

| Query param | Type | Notes |
|-------------|------|-------|
| `q` | string, optional | Partial match (case-insensitive, ILIKE-escaped) across display_name, email, phone, and channel identifier values |
| `cursor` | string, optional | Opaque keyset cursor from previous page |
| `limit` | int, optional | Default 25, max 100 |

**200** → `PaginatedResponse<Customer>` ordered `created_at DESC, id DESC`. Empty result → `data: []` (never an error).

### `POST /tenant/customers` — create

**Permission**: `customers.manage`

```json
{
  "display_name": "Sara Ali",            // required, 1–200 chars
  "email": "sara@example.com",           // optional, valid email ≤320
  "phone": "+201001234567",              // optional, 7–15 digits after normalization
  "identifiers": [                        // optional
    { "channel": "whatsapp", "identifier": "+201001234567" }
  ],
  "metadata": { "plan": "enterprise" }   // optional, object, ≤50 keys, key ≤100, string values ≤500
}
```

Rule: at least one of `email`, `phone`, or a non-empty `identifiers` entry is required.

**201** → `ApiResponse<CustomerDetail>`. Writes `customer.created` audit row in the same transaction.

### `GET /tenant/customers/{customer_id}` — view profile

**Permission**: `customers.view`

**200** → `ApiResponse<CustomerDetail>`.

### `PATCH /tenant/customers/{customer_id}` — update (partial)

**Permission**: `customers.manage`

Any subset of the create body's fields. Omitted fields unchanged; `identifiers` and `metadata`, when present, are replace-the-set semantics; explicit `null` clears nullable contact fields. Last write wins (no version precondition).

**200** → `ApiResponse<CustomerDetail>` with refreshed `updated_at`. Writes `customer.updated` audit row (`changed_fields` names) in the same transaction.

### `GET /tenant/customers/{customer_id}/conversations` — history section

**Permission**: `customers.view` (handler lives in the `conversations` module; customer existence checked via the `customers` module's public interface)

**200** →

```json
{
  "data": [ /* ConversationSummary, last_activity_at DESC, max 20 */ ],
  "pagination": { "next_cursor": null, "has_more": true }
}
```

`has_more: true` signals more conversations exist beyond the recent subset (FR-010); no cursor paging in this feature.

## Errors

| Status / code | Trigger |
|---------------|---------|
| `401 unauthenticated` | No valid session |
| `403 unauthorized` | Authenticated but lacking the route permission (e.g., Viewer on POST/PATCH) |
| `404 not_found` | Customer id absent **or belongs to another tenant** (indistinguishable, FR-011); also unknown/soft-deleted ids |
| `409 conflict` | Channel identifier already held by another customer in the tenant. Details entry: `{ "field": "identifiers", "channel": "...", "identifier": "...", "existing_customer_id": "uuid", "existing_customer_name": "..." }` (FR-014) |
| `422 validation_failed` | Format/length/limit violations, missing required contact rule, >50 metadata keys, invalid channel value — with field-level `details[]` (FR-013, SC-006) |

## Audit actions (append-only `audit_logs`)

| Action | Trigger |
|--------|---------|
| `customer.created` | POST success |
| `customer.updated` | PATCH success (payload lists `changed_fields` by name; no field values) |

## Idempotency & consistency notes

- `GET`s are safe/cacheable per standard semantics; `PATCH` is idempotent for a given body; `POST` is not idempotent — duplicate-identifier collisions surface as 409 rather than silent dedup.
- All writes (row + identifier set + audit) are single-transaction.
