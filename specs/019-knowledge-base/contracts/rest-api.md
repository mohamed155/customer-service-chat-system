# REST API Contract: Knowledge Base

**Feature**: 019-knowledge-base

All endpoints follow the platform envelope (`ApiResponse<T>` success / `ErrorEnvelope` error with `X-Request-Id`), require an authenticated tenant context, and are registered in OpenAPI (utoipa) with coverage asserted by `openapi_contract.rs` / `openapi_coverage.rs`. Cross-tenant access to any identifier answers `not_found` (never `unauthorized` — no existence oracle). GET routes require `knowledge_base.view`; all writes require `knowledge_base.manage` (existing codes; matrix already grants manage to Owner/Admin/Manager — clarification #1).

Wiring uses the established `require_permission` / `merge_with_permissions` pattern in `server/src/router.rs`; object storage arrives via `Extension<Arc<dyn ObjectStorage>>` (research R2).

## Items

### GET `/tenant/knowledge/items` — list (view)

Query: `limit` (default 20, max 50), `before` (opaque cursor over `(updated_at, id)` desc), `type` (`article|faq|document`), `status` (`draft|published|archived`), `categoryId` (UUID), `tag` (string), `q` (title contains, case-insensitive). All filters optional and combinable (FR-010).

```jsonc
// 200 → data:
{
  "items": [
    {
      "id": "…", "itemType": "article", "title": "Refund policy",
      "status": "published", "category": { "id": "…", "name": "Billing" } | null,
      "tags": ["refunds", "billing"], "source": "authored",
      "createdBy": "Dana Ops", "updatedAt": "2026-07-17T10:00:00Z",
      "document": null | { "originalFilename": "manual.pdf", "contentType": "application/pdf", "sizeBytes": 123456 }
    }
  ],
  "hasMore": true,
  "nextCursor": "…" // present when hasMore
}
```

Tags for the page are fetched with one `ANY($ids)` query — no N+1.

### POST `/tenant/knowledge/items` — create article/FAQ (manage)

```jsonc
// request
{ "itemType": "article" | "faq", "title": "…", "body": "<p>…</p>" | null,
  "categoryId": "…" | null, "tags": ["…"] }
// 201 → data: full item detail (below)
```

Validation: title 1–200 (US1 scenario 4); body ≤ 100k, sanitized server-side (R10); `categoryId` must be a tenant-owned category else `validation_failed`; tags normalized, ≤ 20. Created as `draft` (FR-001). Audits `knowledge_item.created`.

### GET `/tenant/knowledge/items/{id}` — detail (view)

```jsonc
// 200 → data:
{
  "id": "…", "itemType": "article", "title": "…", "body": "<p>…</p>" | null,
  "status": "draft", "category": { "id": "…", "name": "…" } | null, "tags": ["…"],
  "source": "authored" | "uploaded", "createdBy": "Dana Ops",
  "createdAt": "…", "updatedAt": "…",
  "document": null | {
    "originalFilename": "manual.pdf", "contentType": "application/pdf",
    "sizeBytes": 123456, "uploadedAt": "…"
  }
}
```

### PATCH `/tenant/knowledge/items/{id}` — edit (manage)

All fields optional; only provided fields change (FR-002). `body` rejected with `validation_failed` for document items. Editing never changes `status` (clarification #2). Concurrency is last-save-wins by spec (edge case) — no version guard.

```jsonc
// request (any subset)
{ "title": "…", "body": "…", "itemType": "article" | "faq", "categoryId": "…" | null, "tags": ["…"] }
// 200 → data: full item detail
```

`itemType` may only switch between `article` and `faq` (never to/from `document`).

### POST `/tenant/knowledge/items/{id}/status` — lifecycle transition (manage)

```jsonc
// request
{ "status": "published" | "archived" | "draft" }
// 200 → data:
{ "id": "…", "status": "published", "changed": true, "updatedAt": "…" }
```

Rules (R7 / FR-003 / FR-004): only `draft→published`, `published→archived`, `archived→draft`; same-status → `changed: false` no-op (replay-safe); illegal edge → `validation_failed` listing allowed transitions; publish of an article/FAQ with empty body → `validation_failed`. Audits `knowledge_item.published` / `.archived` / `.restored` in-transaction (FR-013); no-ops do not audit.

## Documents

### POST `/tenant/knowledge/documents` — upload (manage)

`multipart/form-data`, route-scoped body limit 25 MB:

| Part | Required | Notes |
|------|----------|-------|
| `file` | yes | allowlist: pdf/docx/txt/md by extension **and** declared MIME; ≤ 20 MB (R5) |
| `title` | no | defaults to filename stem; 1–200 |
| `status` | no | `draft` (default) or `published` — uploader's choice (clarification #4) |
| `categoryId` | no | tenant-owned category |
| `tags` | no | comma-separated, normalized |

Flow per R3: validate → object put (`{tenant_id}/knowledge/{item_id}`) → transactional row insert + `knowledge_item.created` audit → on failure, best-effort object delete. Rejections (`validation_failed`) name the allowed types and size limit (US3 scenario 2). Incomplete uploads create nothing (FR-016).

```jsonc
// 201 → data: full item detail (itemType "document", source "uploaded", document metadata populated)
```

### GET `/tenant/knowledge/items/{id}/file` — download (view)

Streams the stored bytes with the recorded `Content-Type` and `Content-Disposition: attachment; filename="…"` (sanitized). Non-document item → `validation_failed`. Object missing out-of-band → `not_found` with code the frontend maps to the "file unavailable" state (edge case; item detail keeps serving metadata).

## Categories

### GET `/tenant/knowledge/categories` — list (view)

```jsonc
// 200 → data: { "categories": [ { "id": "…", "name": "Billing", "itemCount": 12 } ] }
```

Ordered by name; `itemCount` from one grouped query.

### POST `/tenant/knowledge/categories` — create (manage)

```jsonc
{ "name": "Billing" }        // request; 1–80 chars
// 201 → data: { "id": "…", "name": "Billing", "itemCount": 0 }
```

Duplicate name (case-insensitive, per tenant) → `conflict`.

### PATCH `/tenant/knowledge/categories/{id}` — rename (manage)

```jsonc
{ "name": "Payments" }
// 200 → data: { "id": "…", "name": "Payments", "itemCount": 12 }
```

### DELETE `/tenant/knowledge/categories/{id}` — delete (manage)

`204`. Items keep existing via `ON DELETE SET NULL` → uncategorized (FR-008 / US4 scenario 4). Replay: second delete → `not_found`.

## Error vocabulary

Standard platform codes only: `validation_failed` (with `details` naming fields/rules), `unauthenticated`, `unauthorized` (permission missing within own tenant), `not_found` (missing **or cross-tenant**), `conflict` (duplicate category name), `internal_error`. Upload size overflow surfaces as `validation_failed`, not a raw 413 body.

## Test obligations (Constitution VII)

- Integration suite `server/tests/knowledge_base.rs` (DB-gated by `REQUIRE_DB_TESTS`): CRUD matrix, transition matrix incl. no-op + illegal edges + empty-body publish block, upload happy/reject/orphan-safety paths (via `InMemoryStorage`), download incl. missing-object, category CRUD + `SET NULL` behavior, tag filtering, cursor pagination, RBAC (view vs manage per route), cross-tenant `not_found` per resource, audit rows for all four actions (attribution, no content in details).
- `openapi_contract.rs` / `openapi_coverage.rs`: all 11 paths + DTOs registered (multipart request body documented with `multipart/form-data` content type).
- `shared/db/tests/schema.rs`: 0046 assertions (tables, CHECKs, uniques, indexes, FK actions).
