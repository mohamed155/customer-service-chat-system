# Data Model: Knowledge Base

**Feature**: 019-knowledge-base | **Migration**: `backend/migrations/0046_knowledge_base.sql`

Four new tables, all owned by `modules/knowledge`. Every table carries `tenant_id UUID NOT NULL REFERENCES tenants(id)` (Constitution II) — including child tables, so isolation is enforceable on every read without joins (018 precedent).

## knowledge_categories

Flat per-tenant category list (clarification #3).

| Column | Type | Constraints |
|--------|------|-------------|
| id | UUID | PK, default `gen_random_uuid()` |
| tenant_id | UUID | NOT NULL, FK → tenants(id) |
| name | TEXT | NOT NULL, CHECK `char_length(name) BETWEEN 1 AND 80` |
| created_at | TIMESTAMPTZ | NOT NULL default `now()` |
| updated_at | TIMESTAMPTZ | NOT NULL default `now()` |

**Indexes**: `UNIQUE (tenant_id, lower(name))` — case-insensitive per-tenant name uniqueness (duplicate create/rename → `conflict`).

**Deletion**: hard `DELETE`; `knowledge_items.category_id` is `ON DELETE SET NULL`, which implements FR-008's "items become uncategorized" atomically in the database. Deviation from the 005 soft-delete convention is justified in [research.md R6](./research.md).

## knowledge_items

One row per knowledge item of any type.

| Column | Type | Constraints |
|--------|------|-------------|
| id | UUID | PK, default `gen_random_uuid()` |
| tenant_id | UUID | NOT NULL, FK → tenants(id) |
| item_type | TEXT | NOT NULL, CHECK IN (`'article'`, `'faq'`, `'document'`) |
| title | TEXT | NOT NULL, CHECK `char_length(title) BETWEEN 1 AND 200` (FR-001 / US1 scenario 4) |
| body | TEXT | NULL, CHECK `char_length(body) <= 100000`; CHECK `item_type <> 'document' OR body IS NULL` (documents never carry body) |
| status | TEXT | NOT NULL default `'draft'`, CHECK IN (`'draft'`, `'published'`, `'archived'`) |
| category_id | UUID | NULL, FK → knowledge_categories(id) `ON DELETE SET NULL` |
| source | TEXT | NOT NULL, CHECK IN (`'authored'`, `'uploaded'`) — FR-012 origin half of source metadata |
| created_by_user_id | UUID | NULL, FK → users(id) — survives user deactivation (edge case: authorship preserved) |
| created_by_display | TEXT | NOT NULL — attribution snapshot at creation time (018 precedent: a live join cannot reproduce the historical fact) |
| created_at | TIMESTAMPTZ | NOT NULL default `now()` |
| updated_at | TIMESTAMPTZ | NOT NULL default `now()` |

**Indexes**:
- `(tenant_id, updated_at DESC, id DESC)` — drives the list's cursor pagination (R9).
- `(tenant_id, status)` — status filter and the "AI-available set" query (FR-015: `status = 'published'`).
- `(category_id)` — category filter + FK maintenance.

**Invariants enforced in code (store layer)**:
- Transitions limited to `draft→published`, `published→archived`, `archived→draft`; same-status = no-op (R7).
- Publish of `article`/`faq` requires non-empty (post-trim) body (FR-004).
- Body sanitized with `ammonia` before every write (R10).
- Edits update in place; status untouched (clarification #2); `updated_at` refreshed.
- No user-facing delete; `archived` is terminal (spec assumption).

## knowledge_documents

1:1 extension row for `item_type = 'document'` items (R6).

| Column | Type | Constraints |
|--------|------|-------------|
| item_id | UUID | PK, FK → knowledge_items(id) `ON DELETE CASCADE` |
| tenant_id | UUID | NOT NULL, FK → tenants(id) |
| storage_key | TEXT | NOT NULL, UNIQUE — object key `{tenant_id}/knowledge/{item_id}` (tenant prefix makes cross-tenant key collisions structurally impossible) |
| original_filename | TEXT | NOT NULL, CHECK `char_length(original_filename) BETWEEN 1 AND 255` |
| content_type | TEXT | NOT NULL |
| size_bytes | BIGINT | NOT NULL, CHECK `size_bytes > 0 AND size_bytes <= 20971520` (20 MB, R5) |
| created_at | TIMESTAMPTZ | NOT NULL default `now()` — upload time (FR-005 metadata) |

Uploader + upload-time attribution live on the parent item row (`created_by_*`, `created_at`) — a document item is created only by upload, so parent attribution *is* the upload metadata (FR-012).

## knowledge_item_tags

Tag-as-value join rows (R6 justification).

| Column | Type | Constraints |
|--------|------|-------------|
| item_id | UUID | NOT NULL, FK → knowledge_items(id) `ON DELETE CASCADE` |
| tenant_id | UUID | NOT NULL, FK → tenants(id) |
| tag | TEXT | NOT NULL, CHECK `char_length(tag) BETWEEN 1 AND 40` |

**Primary key**: `(item_id, tag)`.
**Indexes**: `(tenant_id, tag)` — tag filter (R9).

**Invariants enforced in code**: tags normalized (trimmed, lowercased, deduplicated) on write; max 20 tags per item (`validation_failed` beyond); tag writes replace the item's full tag set inside the item-update transaction.

## Entity → spec mapping

| Spec entity | Realization |
|-------------|-------------|
| Knowledge Item | `knowledge_items` row |
| Document File | `knowledge_documents` row + object in S3-compatible storage |
| Category | `knowledge_categories` row |
| Tag | `knowledge_item_tags` rows |
| Knowledge Source Metadata | `source`, `created_by_user_id`, `created_by_display`, `created_at` on the item (+ document row for file facts) |

## State machine

```text
            publish (body required for article/faq)
   draft ──────────────────────────────────────────▶ published
     ▲                                                   │
     │ restore                                   archive │
     └────────────────────────── archived ◀──────────────┘
```

All three transitions audit in-transaction (`knowledge_item.published/archived/restored`); creation audits as `knowledge_item.created` (R8).
