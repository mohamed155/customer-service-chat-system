# Feature Specification: Embeddings & RAG

**Feature Branch**: `020-embeddings-rag`

**Created**: 2026-07-17

**Status**: Draft

**Input**: User description: "Embeddings & RAG — Enable AI to retrieve tenant knowledge during conversations. Scope: text chunking, embedding generation, pgvector storage, similarity search, citation tracking, retrieval pipeline. Backend: generate embeddings for published knowledge, store vectors in PostgreSQL using pgvector, retrieve relevant chunks per conversation, attach citations to AI responses. Frontend: knowledge citation component, AI response citation view, re-index status indicator. Acceptance: published knowledge can be indexed; AI responses can include citations; retrieval is tenant-scoped; tests verify no cross-tenant retrieval."

## Clarifications

### Session 2026-07-17

- Q: How should embedding configuration work, given that vectors from different embedding models are not comparable? → A: Platform-wide embedding model — one embedding configuration for all tenants (via the provider abstraction); tenant AI config governs chat/completions only; changing the platform embedding model is an operational migration out of v1 scope.
- Q: When indexing fails, is recovery automatic or manual? → A: Automatic bounded retries with backoff; after retries are exhausted the item is marked failed (with reason) and recovery is manual via the re-index action.
- Q: What does a citation snapshot preserve at response time? → A: Passage snapshot — item ID + title + the cited passage text as it existed at response time; the citation always shows what the AI saw and also links to the current item when it still exists.
- Q: Who inspects retrieval records, and where? → A: Internal observability only in v1 — structured logs/traces correlated to the conversation and message, for platform operators and debugging; no user-facing retrieval-inspection surface.
- Q: What is the retrieval search query based on? → A: Context-aware query — the search query incorporates the latest customer message plus recent conversation context so follow-up questions retrieve correctly; the exact construction mechanism is a planning decision.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - AI answers grounded in tenant knowledge (Priority: P1)

A customer asks a question in a conversation handled by the AI agent. The system searches the tenant's published knowledge base for the passages most relevant to the question and supplies them to the AI, so the answer reflects the tenant's own articles, FAQs, and documents rather than generic model knowledge. Knowledge from other tenants is never consulted.

**Why this priority**: Grounded retrieval is the entire purpose of the feature — without it, indexing and citations have nothing to serve. A working publish → index → retrieve → answer loop is a viable MVP on its own.

**Independent Test**: Can be fully tested by publishing a knowledge article containing a distinctive fact, asking the AI a question about that fact in a conversation, and confirming the response reflects the article's content — no citation UI required.

**Acceptance Scenarios**:

1. **Given** a tenant with published knowledge items that have been indexed, **When** a customer asks a question related to that knowledge, **Then** the most relevant knowledge passages are retrieved and made available to the AI for composing its response.
2. **Given** tenant A and tenant B each have published knowledge, **When** a conversation in tenant B triggers retrieval, **Then** only tenant B's knowledge is searched and none of tenant A's content can appear in the retrieved set, regardless of similarity.
3. **Given** a tenant whose knowledge base contains only draft or archived items, **When** retrieval runs for a conversation, **Then** no knowledge passages are returned and the AI proceeds without knowledge grounding.
4. **Given** a customer question with no sufficiently relevant knowledge, **When** retrieval runs, **Then** the system returns no passages rather than loosely related ones, and the AI answers without misleading grounding.
5. **Given** retrieval runs for a conversation, **When** the interaction is later inspected, **Then** an inspectable record exists of what was searched, what was retrieved, and with what relevance, tied to the conversation.

---

### User Story 2 - Citations on AI responses (Priority: P2)

A tenant agent (or supervisor) reviewing a conversation sees which knowledge items the AI relied on for each response. Each AI response that used retrieved knowledge displays citations that identify the source items, and the agent can follow a citation to the underlying knowledge item to verify the answer.

**Why this priority**: Citations turn the AI from a black box into an auditable assistant — teams can verify answers, catch outdated content, and build trust. They depend on retrieval (Story 1) existing first.

**Independent Test**: Can be tested by triggering a knowledge-grounded AI response and confirming the response carries citations that name the source knowledge items and link to their detail views.

**Acceptance Scenarios**:

1. **Given** an AI response that used retrieved knowledge, **When** a tenant user views the conversation, **Then** the response displays citations identifying each source knowledge item used.
2. **Given** a citation on an AI response, **When** the user selects it, **Then** they are taken to (or shown) the cited knowledge item's detail so they can verify the source.
3. **Given** an AI response composed without any retrieved knowledge, **When** it is displayed, **Then** no citations are shown and the response is visually indistinguishable from today's uncited responses.
4. **Given** a cited knowledge item is later archived or deleted, **When** the historical conversation is viewed, **Then** the citation still identifies the source as it existed at response time, with a clear indication if the item is no longer available.

---

### User Story 3 - Indexing status and re-indexing (Priority: P3)

A tenant knowledge manager publishes or edits knowledge and can see, per item, whether it has been indexed for AI retrieval — pending, in progress, indexed, or failed. When something looks stale or has failed, they can trigger re-indexing and watch the status update.

**Why this priority**: Status visibility and manual recovery make the pipeline operable by tenants themselves, but the feature delivers value with automatic indexing alone, so this follows the retrieval and citation stories.

**Independent Test**: Can be tested by publishing an item, observing its status progress to "indexed", forcing a failure or staleness, triggering re-index, and confirming the status reflects each transition.

**Acceptance Scenarios**:

1. **Given** a knowledge item is published, **When** indexing begins and completes, **Then** the item's index status visibly progresses (e.g., pending → indexing → indexed) in the knowledge base UI.
2. **Given** an indexed published item is edited, **When** the changes are saved, **Then** the item is automatically re-indexed so retrieval reflects the current content, and its status reflects the refresh.
3. **Given** indexing fails for an item, **When** the knowledge manager views the item, **Then** the failure is visible with an understandable reason, and they can trigger a re-index.
4. **Given** a published item is archived (or reverted to draft), **When** the change takes effect, **Then** the item's content is promptly excluded from retrieval for new conversations.

---

### Edge Cases

- A knowledge item is edited or archived while a conversation retrieval is in flight — retrieval must serve a consistent set (the previously indexed content) and converge on the updated set for subsequent requests.
- An uploaded document contains no extractable text (e.g., a scanned image PDF) — the item is marked as not indexable with a clear status/reason rather than silently indexed as empty.
- A very large document produces many passages — indexing must complete without blocking other tenants' indexing, and per-item passage volume must be bounded.
- The embedding provider is unavailable during indexing — affected items are retried automatically (bounded, with backoff) and marked failed with reason once retries are exhausted, after which recovery is manual via re-index; already-indexed content remains retrievable throughout.
- The embedding provider is unavailable during a conversation — the AI degrades gracefully to answering without knowledge grounding instead of failing the conversation.
- A tenant has zero published knowledge — retrieval is skipped or returns empty without error, and responses simply carry no citations.
- The same content is re-indexed repeatedly (rapid successive edits) — indexing coalesces or supersedes prior runs so the final state matches the latest content.
- A knowledge item is deleted — its indexed passages are removed from retrieval, while historical citations in past conversations remain displayable.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST split the textual content of published knowledge items into passages (chunks) suitable for semantic search, preserving enough context for each passage to be independently meaningful.
- **FR-002**: System MUST generate a semantic vector representation (embedding) for each passage of published knowledge, using the platform's provider-abstracted AI layer under a single platform-wide embedding configuration (one embedding provider/model for all tenants), so that all stored vectors are mutually comparable.
- **FR-003**: System MUST store passage vectors durably, associated with their tenant, source knowledge item, and position within the source, so that every stored vector is attributable to exactly one tenant and one knowledge item.
- **FR-004**: System MUST index knowledge automatically when an item becomes published and re-index automatically when a published item's content changes; unpublishing (archive or revert to draft) and deletion MUST remove the item's passages from the retrievable set.
- **FR-005**: Only published knowledge items MUST ever be retrievable; draft and archived content MUST be excluded from search results at all times.
- **FR-006**: During an AI-handled conversation turn, the system MUST retrieve the passages most semantically relevant to a search query built from the customer's latest message plus recent conversation context (so follow-up questions retrieve correctly), limited to the conversation's tenant, and supply them to the AI response pipeline.
- **FR-007**: Retrieval MUST enforce tenant isolation at the data-access layer — a query for one tenant MUST be incapable of returning another tenant's passages — and automated tests MUST verify that cross-tenant retrieval is impossible.
- **FR-008**: System MUST apply a relevance threshold so that insufficiently relevant passages are not supplied to the AI, and MUST cap the number of passages supplied per response.
- **FR-009**: System MUST record, for each AI response that used retrieved knowledge, which knowledge items (and which passages) informed it, snapshotting at response time the source item's identity, its title, and the cited passage text, so that what grounded the response remains viewable even if the source is later changed, archived, or deleted.
- **FR-010**: AI responses MUST expose their citations through the conversation contract so that consuming clients can display which knowledge items were used.
- **FR-011**: The dashboard conversation view MUST display citations on AI responses via a reusable citation component, including the source item's title and a way to navigate to (or preview) the item; responses without retrieved knowledge display no citation affordance.
- **FR-012**: The knowledge base UI MUST show a per-item index status indicator covering at least: not indexed, pending, indexing, indexed, failed (with reason), and not indexable (no extractable text).
- **FR-013**: Users with knowledge management permission (Owner, Admin, Manager) MUST be able to trigger re-indexing of a knowledge item; Agent and Viewer roles see status read-only.
- **FR-014**: Each retrieval performed for a conversation MUST leave an inspectable record (what was searched, what was returned, relevance ordering) in the platform's observability tooling (structured logs/traces correlated to the conversation and message), consistent with the platform's observability requirements for RAG operations; no user-facing retrieval-inspection surface is required in v1.
- **FR-015**: Indexing failures MUST NOT corrupt or partially replace an item's previously indexed passages — an item's retrievable content is replaced atomically only on successful re-index.
- **FR-016**: System MUST automatically retry failed indexing attempts a bounded number of times with backoff; when retries are exhausted the item MUST be marked failed with an understandable reason, and further recovery is manual via the re-index action (FR-013).
- **FR-017**: Retrieval MUST NOT add noticeable delay to AI responses; if retrieval cannot complete within its time budget or the search infrastructure is unavailable, the AI response proceeds without knowledge grounding rather than failing.

### Key Entities

- **Knowledge Passage (Chunk)**: A contiguous slice of a knowledge item's text prepared for semantic search; belongs to exactly one knowledge item and one tenant; carries its position/ordering within the source and the text needed to display or cite it.
- **Passage Embedding**: The semantic vector representation of one passage; stored alongside the passage's tenant and source attribution; regenerated whenever the source content or embedding configuration changes.
- **Index State**: The per-knowledge-item lifecycle of searchability (not indexed, pending, indexing, indexed, failed with reason, not indexable), including when it was last successfully indexed and against which content version.
- **Citation**: A record linking one AI response to a knowledge item (and the specific passages) that informed it, snapshotted at response time with the source item's identity, title, and the cited passage text so the grounding remains viewable if the item later changes or disappears; links to the current item while it still exists.
- **Retrieval Record**: The inspectable trace of one retrieval operation — the conversation and message it served, what was searched, which passages were returned, and their relevance ordering.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After publishing a knowledge item, its content becomes retrievable in conversations within 2 minutes under normal operation.
- **SC-002**: 100% of AI responses that used retrieved knowledge display at least one citation, and every citation resolves to the correct source item (or a clear "no longer available" state).
- **SC-003**: Automated isolation tests demonstrate zero cross-tenant retrievals across the full test suite — a question asked in one tenant never surfaces another tenant's content, even with deliberately identical content in both tenants.
- **SC-004**: Retrieval adds no user-perceptible delay — a knowledge-grounded AI response begins being produced within 1 second of when an ungrounded response would have. (The agent responder currently composes a full reply via a non-streaming completion, so the budget applies to time-to-reply; if the responder later moves to streaming, the same budget applies to time-to-first-token.)
- **SC-005**: A knowledge manager can determine the index status of any knowledge item in one glance at the knowledge list or detail view, and can trigger and observe a re-index without leaving the page.
- **SC-006**: When a customer question is covered by published knowledge, the AI's answer reflects that knowledge (verified by test scenarios with known question/answer pairs) rather than answering generically.

## Assumptions

- **Indexable content**: Authored articles and FAQs are always indexable from their rich-text content. Uploaded documents are indexed when text can be extracted from them (e.g., plain text, markdown, PDF with a text layer); files without extractable text are marked "not indexable" rather than treated as errors. Expanding extraction coverage (OCR, additional formats) is out of scope for v1.
- **Citation audience**: Citations are shown in the tenant dashboard conversation view (agents, supervisors, managers). Whether end customers see citations in the chat widget is out of scope for v1.
- **Embedding provider**: Embedding generation goes through the existing AI provider abstraction (015), consistent with the constitution's provider-independence principle. A single platform-wide embedding configuration (provider + model) applies to all tenants; per-tenant AI configuration governs chat/completion generation only. Changing the platform embedding model is an operational migration (requiring re-embedding of stored vectors) and is out of scope for v1.
- **Knowledge lifecycle**: This feature builds on the knowledge base (019) draft/published/archived lifecycle — "published" is the sole gate for AI availability, and published edits are live immediately, which is why edits trigger automatic re-indexing.
- **Conversation pipeline**: AI response generation for conversations exists (013/015/017); this feature inserts a retrieval step into that pipeline and extends the response with citation data rather than introducing a new response flow.
- **Vector storage**: Vectors live in the platform's primary relational database using its vector-search capability, per the constitution's mandated stack; no separate search infrastructure is introduced.
- **Scale**: v1 targets knowledge bases up to the order of thousands of items per tenant; corpus-wide relevance tuning, hybrid keyword search, and re-ranking models are out of scope.
- **Re-index everything**: A tenant-wide "re-index all" bulk operation is not required for v1; per-item re-index plus automatic indexing on publish/edit covers the acceptance criteria.
