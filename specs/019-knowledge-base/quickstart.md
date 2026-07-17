# Quickstart: Knowledge Base

Validation guide for 019-knowledge-base. Contracts: [contracts/rest-api.md](./contracts/rest-api.md) · Schema: [data-model.md](./data-model.md).

## Prerequisites

```bash
# Postgres + Redis + MinIO (bucket storage) + mailhog
docker compose -f infra/docker-compose.yml up -d

# Backend env (in addition to existing APP_* vars) — all-or-nothing: set every
# APP_S3_* var or none, else the server refuses to boot (research R2).
export APP_S3_ENDPOINT=http://localhost:9000
export APP_S3_REGION=us-east-1
export APP_S3_BUCKET=app-dev
export APP_S3_ACCESS_KEY_ID=minioadmin
export APP_S3_SECRET_ACCESS_KEY=minioadmin
export APP_S3_FORCE_PATH_STYLE=true   # optional, defaults true (MinIO needs it)
# Create the bucket named above once (MinIO console at http://localhost:9001,
# credentials minioadmin/minioadmin, or `mc mb local/app-dev`) — the app does
# not create buckets. With no APP_S3_* set, the server falls back to in-memory
# storage and logs a warning: fine for non-upload work, useless for scenario 3.
```

Migrations apply on server start (0046 adds the four knowledge tables).

## Automated gates (all must pass)

```bash
# Backend — from backend/
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                          # unit + non-DB tests
REQUIRE_DB_TESTS=1 cargo test --workspace       # integration: knowledge_base.rs, schema.rs 0046, openapi_*

# Frontend — from frontend/
pnpm ng build dashboard && pnpm ng test dashboard && pnpm lint && pnpm format:check
```

## Manual validation scenarios

Sign in to the dashboard as a tenant **Admin** (or Owner/Manager) and open **Knowledge Base**.

1. **Author (US1)**: New article → title + rich-text body (headings/lists/links render in the editor) → save. It appears in the list as `draft`, opens in detail, and re-editing persists. Saving with an empty title is blocked inline.
2. **Lifecycle (US2)**: Publish the draft → status badge flips to `published`. Archive it → `archived`, still listed under the archived filter. Restore → back to `draft`. Publishing an article with an empty body is blocked with a validation message. Check `audit_logs` has `knowledge_item.published/archived/restored` rows with your user attribution and no body content in details.
3. **Upload (US3)**: Upload a PDF ≤ 20 MB, choose "Publish immediately" → item appears as `document`/`published` with filename/size metadata; the file exists in the MinIO bucket under `{tenant_id}/knowledge/{item_id}`; Download returns the original file. Then try a `.exe` (or >20 MB file) → rejected client- and server-side with the allowed types/limit named, and no item/object is created.
4. **Organize (US4)**: Create categories and tags, assign them, and confirm each list filter (type, status, category, tag) plus title search returns exactly matching items. Delete an assigned category → its items remain, now uncategorized.
5. **Isolation & RBAC (FR-007/FR-014)**: As a user of another tenant, the first tenant's items/categories are absent and direct item/file URLs answer not-found. As an **Agent** (or Viewer), the page is read-only: no New/Upload/Publish affordances, and direct write API calls return `unauthorized`.
6. **AI-available set (FR-015 / SC-006)**: `GET /tenant/knowledge/items?status=published` is exactly the published set — no drafts or archived items.

## Expected outcomes

- All cargo/pnpm gates green; DB-gated suite covers the matrices in [contracts/rest-api.md](./contracts/rest-api.md) § Test obligations.
- Scenarios 1–6 behave as described with no console/network errors and standard error envelopes on rejections.
