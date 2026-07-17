# Contract: Knowledge Indexing (REST, additive to knowledge module)

REST-first, versioned, `ApiResponse<T>` envelope, `X-Tenant-ID` scoping, `X-Request-Id` propagation — consistent with existing knowledge endpoints (Principle V).

## Additive field: index status on knowledge items

Every knowledge item list/detail response gains an `index_status` object sourced from `knowledge_index_state`.

```jsonc
// GET /tenant/knowledge/items  and  GET /tenant/knowledge/items/{id}
{
  "id": "…",
  "title": "…",
  "status": "published",              // existing draft/published/archived
  "index_status": {
    "status": "indexed",              // not_indexed|pending|indexing|indexed|failed|not_indexable
    "failure_reason": null,           // string when status in (failed, not_indexable)
    "last_indexed_at": "2026-07-17T10:00:00Z",  // null until first success
    "chunk_count": 12
  }
  // …existing fields…
}
```

- Draft/archived items report their real state (typically `not_indexed`); only `published` items progress toward `indexed`.
- Field is always present (never omitted); newly created items start at `not_indexed`.

## New endpoint: trigger re-index

```
POST /tenant/knowledge/items/{id}/reindex
```

**Authorization**: knowledge manage permission — Owner, Admin, Manager (existing `authz` policy). Agent/Viewer receive `403`.

**Semantics (idempotent, Principle V)**:
- If the item is `published`: transitions `index_status.status` → `pending`, enqueues a `knowledge.index_requested` outbox event, resets `attempts`. Re-invoking while already `pending`/`indexing` is a no-op that returns the current state (no duplicate work).
- If the item is not `published`: `409 Conflict` (`code: "not_publishable"`) — only published content is indexable.
- If the item has no extractable text, the worker will resolve it to `not_indexable`; the endpoint still accepts the request (returns `pending`).

**Responses**:
| Status | When | Body |
|---|---|---|
| `202 Accepted` | accepted (published item) | `{ "data": { "index_status": { "status": "pending", … } } }` |
| `403 Forbidden` | Agent/Viewer role | error envelope `code: "forbidden"` |
| `404 Not Found` | item not in tenant | error envelope |
| `409 Conflict` | item not published | error envelope `code: "not_publishable"` |

## Automatic indexing (no endpoint — event-driven)

Triggered transactionally by existing item mutations; no new API surface:

| Item change | Effect |
|---|---|
| draft/archived → **published** | enqueue index; `index_status` → `pending` |
| published item **content edited** | enqueue re-index; `index_status` → `pending` |
| published → **archived** or **draft** | remove chunks (excluded from retrieval); `index_status` → `not_indexed` |
| item **deleted** | chunks + index_state removed by cascade; existing citations retained (separate table, no FK) |

## Observability

Each indexing run emits an `rag.index` tracing span (tenant_id, item_id, chunk_count, attempt, outcome). No user-facing indexing-log endpoint in v1 (clarification: internal observability only).
