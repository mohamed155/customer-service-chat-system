# Feature Specification: Knowledge Base

**Feature Branch**: `019-knowledge-base`

**Created**: 2026-07-17

**Status**: Draft

**Input**: User description: "Knowledge Base — Allow tenants to upload and manage knowledge used by AI. Scope: articles, FAQs, documents, categories, tags, draft/published status, knowledge source metadata. Backend: create/edit/publish/archive article, upload document metadata, store files in S3-compatible storage. Frontend: knowledge base list, article editor, article detail page, upload document flow, publish/archive actions. Acceptance: tenant users can manage knowledge; knowledge items are tenant-scoped; files are stored in object storage; draft and published states work."

## Clarifications

### Session 2026-07-17

- Q: Which tenant roles can manage knowledge (create, edit, publish, archive, upload, organize)? → A: Owner, Admin, and Manager can manage; Agent and Viewer are read-only.
- Q: What happens when a published article is edited? → A: Edit in place — the article stays published and saved changes are immediately live for AI use.
- Q: Are categories flat or hierarchical? → A: Flat — a single-level list of categories per tenant, no nesting.
- Q: What status does an uploaded document start in? → A: The uploader chooses at upload time — save as draft or publish immediately.
- Q: What content format do articles use? → A: Rich text — a WYSIWYG editor with formatting (headings, lists, links, emphasis).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Author and edit knowledge articles (Priority: P1)

A tenant knowledge manager creates a new article (or FAQ) in their tenant's knowledge base, writes its content, saves it as a draft, and returns later to continue editing. They can see all of their tenant's knowledge items in a list and open any item to view its details.

**Why this priority**: Authoring is the foundation of the entire feature — without the ability to create and edit knowledge items, nothing else (publishing, documents, organization) has any value. A working create/edit/list/detail loop is a viable MVP on its own.

**Independent Test**: Can be fully tested by signing in as a tenant user, creating an article, editing it, and confirming it appears in the knowledge base list and detail view — no publishing or document features required.

**Acceptance Scenarios**:

1. **Given** a signed-in tenant user with knowledge management permission, **When** they create an article with a title and body, **Then** the article is saved as a draft and appears in their tenant's knowledge base list.
2. **Given** an existing draft article, **When** the user edits its title, body, or metadata and saves, **Then** the changes persist and are visible on the article's detail page.
3. **Given** knowledge items exist in tenant A, **When** a user of tenant B views their knowledge base list, **Then** none of tenant A's items are visible or reachable, including by direct link.
4. **Given** a user attempts to create an article with no title, **When** they save, **Then** the system rejects the save with a clear validation message and no item is created.

---

### User Story 2 - Publish and archive knowledge (Priority: P2)

A tenant knowledge manager reviews a draft article and publishes it, making it eligible for use by the AI agent. Later, when the content becomes outdated, they archive it so it is no longer used, while retaining it for reference.

**Why this priority**: The draft/published lifecycle is what makes the knowledge base safe to use — teams can prepare content without exposing half-finished material to the AI. It is the second half of the core acceptance criteria.

**Independent Test**: Can be tested by taking an existing draft item, publishing it, verifying its status changes and it is marked as available to the AI, then archiving it and verifying it is excluded.

**Acceptance Scenarios**:

1. **Given** a draft article, **When** the user publishes it, **Then** its status becomes "published" and it is flagged as available for AI use.
2. **Given** a published article, **When** the user archives it, **Then** its status becomes "archived", it no longer counts as available for AI use, and it remains visible in the list under its archived status.
3. **Given** an archived article, **When** the user chooses to restore it, **Then** it returns to draft status for re-review before it can be published again.
4. **Given** any status change (publish, archive, restore), **When** it completes, **Then** an audit record captures who made the change and when.

---

### User Story 3 - Upload knowledge documents (Priority: P3)

A tenant knowledge manager uploads an existing document (e.g., a product manual or policy PDF) into the knowledge base. The file is stored in the platform's object storage, and the document appears as a knowledge item with its metadata (name, type, size, source) alongside articles and FAQs.

**Why this priority**: Documents let tenants reuse existing material instead of rewriting it, but the feature is valuable even with authored articles alone, so it follows the authoring and lifecycle stories.

**Independent Test**: Can be tested by uploading a supported file, confirming the file lands in object storage, and confirming a document knowledge item with correct metadata appears in the list with draft/published lifecycle available.

**Acceptance Scenarios**:

1. **Given** a signed-in tenant user with knowledge management permission, **When** they upload a supported document file and choose an initial status (draft or publish immediately), **Then** the file is stored in object storage and a document knowledge item is created with its name, file type, size, upload metadata, and the chosen status.
2. **Given** a user selects an unsupported file type or an oversized file, **When** they attempt the upload, **Then** the upload is rejected before storage with a clear message stating the allowed types and size limit.
3. **Given** an upload fails partway (e.g., connection loss), **When** the user retries, **Then** no orphaned half-item is left behind and the retry can succeed.
4. **Given** a stored document, **When** a user of another tenant attempts to access its file, **Then** access is denied.

---

### User Story 4 - Organize knowledge with categories and tags (Priority: P4)

A tenant knowledge manager assigns categories and tags to knowledge items so the growing knowledge base stays navigable. Users can filter the knowledge base list by category, tag, type, and status.

**Why this priority**: Organization becomes important as content volume grows, but a small knowledge base is usable without it, so it comes after the core authoring, lifecycle, and document stories.

**Independent Test**: Can be tested by creating categories and tags, assigning them to existing items, and verifying that list filtering by category, tag, type, and status returns exactly the matching items.

**Acceptance Scenarios**:

1. **Given** a tenant user with knowledge management permission, **When** they create a category and assign it to an article, **Then** the article displays that category and appears when the list is filtered by it.
2. **Given** an item with tags, **When** a user filters the list by one of those tags, **Then** only items carrying that tag are shown.
3. **Given** categories exist in tenant A, **When** a user in tenant B manages categories, **Then** they see only tenant B's categories.
4. **Given** a category that is assigned to items, **When** a user deletes the category, **Then** the affected items remain intact and simply become uncategorized.

---

### Edge Cases

- What happens when two users edit the same article at the same time? Last save wins, and the system must not corrupt content; the detail page reflects the most recent save.
- How does the system handle a knowledge item whose stored file has been deleted from object storage out-of-band? The item's detail page shows the metadata with a clear "file unavailable" indication rather than failing.
- What happens when a user publishes an article with an empty body? Publishing requires non-empty content; drafts may be empty but publish is blocked with a validation message.
- What happens to published items when the tenant user who created them is deactivated? Items remain owned by the tenant and stay available; authorship metadata is preserved.
- How does the list behave with a large knowledge base (hundreds of items)? The list is paginated and remains responsive.
- What happens when a document upload is interrupted or abandoned? No knowledge item is created for incomplete uploads, and a file already stored when the metadata write fails is removed by a compensating delete. (One residual case is documented in Assumptions.)

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST allow authorized tenant users to create knowledge articles with a title, rich-text body content (formatted text supporting headings, lists, links, and emphasis, authored in a WYSIWYG editor), and item type (article or FAQ), saved initially as drafts.
- **FR-002**: System MUST allow authorized tenant users to edit the title, body, type, category, and tags of existing knowledge items. Source metadata (origin and attribution) is recorded at creation and MUST NOT be editable.
- **FR-003**: System MUST support three lifecycle states for every knowledge item — draft, published, and archived — with transitions: draft → published, published → archived, and archived → draft (restore). Editing a published item updates it in place; the item remains published and saved changes take effect immediately.
- **FR-004**: System MUST require non-empty content before an article can be published; drafts may be incomplete.
- **FR-005**: System MUST allow authorized tenant users to upload document files, storing the file in S3-compatible object storage and recording the document's metadata (original filename, file type, size, uploader, upload time) as a knowledge item. At upload time the uploader chooses whether the document is saved as a draft or published immediately.
- **FR-006**: System MUST validate document uploads against an allowed set of file types and a maximum file size, rejecting non-conforming uploads before storage with a clear error message.
- **FR-007**: System MUST scope every knowledge item, category, and tag to a single tenant; users MUST NOT be able to read, list, modify, or download another tenant's knowledge data, including via direct identifiers or file links.
- **FR-008**: System MUST allow authorized tenant users to create, rename, and delete categories in a flat (non-nested) per-tenant list, and to assign at most one category per knowledge item; deleting a category MUST leave affected items uncategorized rather than deleting them.
- **FR-009**: System MUST allow authorized tenant users to assign and remove multiple free-form tags per knowledge item.
- **FR-010**: System MUST provide a knowledge base list showing each item's title, type, status, category, tags, and last-updated time, with filtering by type, status, category, and tag, and pagination for large collections.
- **FR-011**: System MUST provide a detail view for each knowledge item showing its full content (for articles/FAQs) or file metadata with download access (for documents), along with status, organization, and source metadata.
- **FR-012**: System MUST record knowledge source metadata on every item — where the knowledge came from (authored in-app or uploaded file) and authorship/upload attribution.
- **FR-013**: System MUST record an audit entry (who, what, when) for knowledge item creation, publishing, archiving, and restoration.
- **FR-014**: System MUST restrict knowledge management actions (create, edit, publish, archive, upload, organize) to tenant users with the Owner, Admin, or Manager role; Agent and Viewer roles have read-only access to the knowledge base.
- **FR-015**: System MUST mark published items as the set of knowledge available for AI use, so that downstream AI retrieval can distinguish usable knowledge from drafts and archived content.
- **FR-016**: System MUST NOT create a knowledge item for an upload that does not complete. Uploads rejected during validation MUST never reach storage; an upload whose metadata write fails after the file is stored MUST have the stored file removed by a compensating delete.

### Key Entities

- **Knowledge Item**: A unit of tenant knowledge. Has a type (article, FAQ, or document), title, lifecycle status (draft/published/archived), optional category, tags, source metadata (origin and attribution), and timestamps. Articles and FAQs carry body content; documents reference a stored file.
- **Document File**: The binary file behind a document-type knowledge item, held in object storage. Described by original filename, file type, size, and storage reference; always tied to exactly one knowledge item and one tenant.
- **Category**: A tenant-scoped named grouping for knowledge items in a flat, single-level list (no nesting); each item belongs to at most one category.
- **Tag**: A tenant-scoped free-form label; a knowledge item can carry many tags and a tag can apply to many items.
- **Knowledge Source Metadata**: Attribution attached to each item — how it entered the system (authored or uploaded), by whom, and when.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A tenant user can create, edit, and publish a knowledge article end-to-end in under 3 minutes.
- **SC-002**: A tenant user can upload a supported document and see it as a knowledge item in under 1 minute for files within the size limit.
- **SC-003**: 100% of knowledge items, categories, tags, and stored files are reachable only by users of the owning tenant, verified by cross-tenant access tests.
- **SC-004**: Status transitions (publish, archive, restore) take effect immediately — the item's new status is reflected in the list and detail views on the next load, and 100% of transitions produce an audit record.
- **SC-005**: The knowledge base list remains usable at scale: users can locate an item among 500+ items via filters in under 30 seconds.
- **SC-006**: Only published items are ever reported as available for AI use — 0 drafts or archived items appear in the AI-available set.

## Assumptions

- FAQs are a type of authored knowledge item sharing the article authoring flow (title + body), not a separate module.
- AI retrieval/ingestion (embedding, chunking, RAG search) is out of scope for this feature; this feature manages the knowledge and exposes which items are published/available so a later ingestion feature can consume them.
- Allowed document types default to common text-bearing formats (PDF, Word, plain text, Markdown) with a 20 MB per-file size limit; exact lists are configurable defaults, not user-visible policy choices requiring clarification.
- Document files are not edited in-app; they are replaced by uploading a new file if content changes (re-upload creates a new item in v1).
- Version history of article content is out of scope for v1; edits overwrite the current draft/published content (with audit records for lifecycle events).
- A separate "republish review" workflow is out of scope for v1 (published edits go live on save, per clarification).
- The existing authentication, tenant-context, and audit infrastructure from prior features is reused.
- Deleting knowledge items permanently is out of scope for v1; archiving is the terminal user-facing state (consistent with the platform's soft-delete conventions).
- A process crash in the narrow window between storing a file and committing its metadata can leave an unreferenced object in storage. Such objects are tenant-prefixed, unreachable by any user, and cost only storage; automated sweeping of them is out of scope for v1.
