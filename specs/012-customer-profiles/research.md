# Phase 0 Research: Customer Profiles

All Technical Context unknowns resolved. Each decision below records what was chosen, why, and what was rejected.

## R1. Module ownership: where customer and conversation logic lives

- **Decision**: Customer records, identifiers, metadata, validation, and audit live in the `customers` module crate (currently a placeholder). The minimal conversation summary table and its read query live in the `conversations` module crate. The profile's conversation history endpoint handler is implemented in `conversations` and verifies customer existence through a public `customer_exists(tenant_id, customer_id)` function exported by `customers`.
- **Rationale**: Constitution I requires module boundaries drawn for future extraction. Conversations will become a large feature; its table must be born under its owner so the messaging feature extends rather than migrates it. The cross-module call is an application-service interface, not table access, which is the sanctioned communication path.
- **Alternatives considered**: (a) Everything in `customers` including the conversations table — rejected: the messaging feature would have to move or share the table across module boundaries later. (b) History endpoint in `customers` calling a conversations query — workable, but puts an HTTP surface for conversation data outside its owner; rejected to keep the read path and its future write path in one crate.

## R2. Metadata storage: JSONB column vs child table

- **Decision**: `metadata JSONB NOT NULL DEFAULT '{}'::jsonb` on `customers`, constrained to a JSON object (`CHECK (jsonb_typeof(metadata) = 'object')`), with the 50-key cap and per-key/value length limits enforced in the handler.
- **Rationale**: Clarified requirements make metadata schema-free, tenant-defined, read and written only alongside the profile, and capped at 50 keys. A JSONB object gives single-fetch reads, atomic replacement on update, and no join on the list query. Deviation from Constitution VIII normalization is recorded in plan.md Complexity Tracking.
- **Alternatives considered**: `customer_metadata(customer_id, key, value)` child table with `UNIQUE(customer_id, key)` — rejected: adds a join or second query to every read and per-key DML on every edit for data with no relational constraints or standalone query path in this feature. Revisit if a future feature needs indexed metadata search or typed attributes.

## R3. Channel vocabulary and identifier uniqueness

- **Decision**: `channel TEXT NOT NULL CHECK (channel IN ('email','phone','web_chat','whatsapp','telegram'))` on `customer_channel_identifiers`; uniqueness via `UNIQUE (tenant_id, channel, identifier)` on live rows. Identifier values normalized app-side before write: trimmed; email lowercased (comparison also handled by CITEXT where applicable); phone/WhatsApp digits normalized to `+`-prefixed E.164-style form.
- **Rationale**: Clarification fixed the channel set (email, phone, web chat, WhatsApp, Telegram). A DB CHECK mirrors existing vocabulary-constraint conventions (e.g., tenants.plan in 0016, membership status in 0018). Tenant-prefixed uniqueness implements FR-003/FR-014 and the cross-tenant coexistence edge case, and is the hook for future inbound-message matching.
- **Alternatives considered**: (a) Postgres enum type — rejected: adding channels later requires `ALTER TYPE`; TEXT+CHECK matches repo convention. (b) Free-form channel strings — rejected in clarification (inconsistent data, no per-channel validation). (c) Frontend fixture vocabulary (`web`, `mobile-sdk`) — rejected: the API contract defines the canonical set; the frontend maps display names/badges, and `mobile-sdk`/`web` fixtures are superseded by `web_chat`.

## R4. Contact info vs channel identifiers

- **Decision**: `customers` carries `display_name TEXT NOT NULL`, `email CITEXT NULL`, `phone TEXT NULL` as contact fields, independent of channel identifier rows. Creating a customer requires a display name plus at least one of: email, phone, or a channel identifier (FR-007), enforced in the handler.
- **Rationale**: The spec treats contact information (FR-002) and channel identifiers (FR-003) as distinct concepts — contact info is "how the team reaches out / who this is", identifiers are "how inbound activity is matched". Keeping both avoids forcing every email into an identifier row and matches the fixture page's display (name + email + phone columns).
- **Alternatives considered**: Deriving contact info from identifier rows (email identifier doubles as contact email) — rejected: conflates concerns, complicates uniqueness (contact email would become per-tenant unique as a side effect, which the spec does not require), and makes "customer with only a Telegram handle plus a known email" awkward.

## R5. Search implementation

- **Decision**: One SQL statement: `WHERE tenant_id = $1 AND deleted_at IS NULL AND (display_name ILIKE '%'||$q||'%' OR email ILIKE … OR phone ILIKE … OR EXISTS (SELECT 1 FROM customer_channel_identifiers i WHERE i.customer_id = c.id AND i.identifier ILIKE …))`, with `q` ILIKE-escaped (`%`, `_`, `\`). Indexes: pg_trgm GIN on `display_name` and `email` (`CREATE EXTENSION IF NOT EXISTS pg_trgm` in migration 0025), btree `(tenant_id, created_at DESC, id DESC)` for the keyset cursor, btree `(tenant_id, channel, identifier)` (the unique index) serving identifier lookups.
- **Rationale**: FR-006 requires partial matching across four fields in tenant scope; SC-002 requires <1s at 10k customers/tenant. Trigram GIN makes infix ILIKE index-served; at 10k rows per tenant even a residual filter scan is comfortably sub-second. Escaping handles the special-character edge case (match literally, never error).
- **Alternatives considered**: (a) Postgres full-text search — rejected: emails, phone fragments, and handles are not word-tokenizable; FTS mismatches the "directory search" requirement. (b) Separate search endpoint — rejected: 010/011 precedent is `?q=` on the list endpoint; one contract, one query path. (c) Relevance ranking / fuzzy matching — explicitly out of scope per spec assumptions.

## R6. Conversation summary shape and vocabulary

- **Decision**: Minimal `conversations` table: `id`, `tenant_id`, `customer_id` (FK), `channel` (same CHECK vocabulary as identifiers), `status TEXT NOT NULL CHECK (status IN ('open','escalated','closed'))`, `last_activity_at TIMESTAMPTZ NOT NULL`, standard timestamps + soft delete. Index `(tenant_id, customer_id, last_activity_at DESC)`. History endpoint returns the 20 most recent with a `has_more` indicator. No write API in this feature; tests seed rows directly.
- **Rationale**: Clarification Q1 scoped this feature to a summary record that future messaging extends. The status vocabulary matches the existing dashboard `ConversationStatus` fixture type (`open | escalated | closed`) so the already-built status badges render unchanged; future features may extend the CHECK by migration. The cap satisfies the "very large number of conversations" edge case without pagination machinery the future feature will redesign anyway.
- **Alternatives considered**: (a) Richer statuses (`pending`, `resolved`) — rejected: invents vocabulary ahead of the owning feature; UI already speaks the three-value set. (b) Full cursor pagination on history — rejected: profile shows "recent subset + more exists" (FR-010); a cap is sufficient and cheaper.

## R7. Uniqueness-conflict response (FR-014)

- **Decision**: `409 conflict` from the existing `kernel::ApiError` vocabulary, with a details entry carrying the conflicting `channel`, `identifier`, and the holding customer's `id` and `display_name`. Implemented by catching the unique-index violation and re-querying the holder (or pre-checking inside the write transaction).
- **Rationale**: The spec requires explaining the conflict and identifying the existing customer "where the viewer is permitted to see it" — always true here: the conflict is same-tenant by construction and every tenant role holds `customers.view`.
- **Alternatives considered**: Bare 409 without the holder — rejected: fails the acceptance scenario (agent can't resolve the conflict without knowing which record holds the identifier).

## R8. Validation rules and concurrency

- **Decision**: App-side validation with 422 + field-level details (kernel `unprocessable_entity` + `ErrorDetail`): display_name 1–200 chars; email RFC-basic format ≤320 chars (mirroring migration 0003's app-side email validation pattern); phone `+`-optional 7–15 digits after normalization; identifier value 1–320 chars, email-channel identifiers must be valid emails, phone/WhatsApp identifiers valid phone forms; metadata keys 1–100 chars, values strings ≤500 chars, ≤50 keys. DB CHECKs mirror length bounds where cheap (name length, object typeof). Concurrency: last-write-wins via single-statement partial UPDATE (spec edge case); no version column.
- **Rationale**: Matches the platform's existing split (formats app-side, vocabulary/length invariants DB-side) and the clarified 50-attribute cap. Field-level 422 satisfies FR-013/SC-006.
- **Alternatives considered**: Optimistic locking (`updated_at` precondition) — rejected: spec explicitly chose last-write-wins; adds a 409 path the UX has no design for.

## R9. Audit integration (FR-017)

- **Decision**: `customer.created` and `customer.updated` actions appended to `audit_logs` inside the write transaction, following the tenancy module's audit helper pattern (actor from authenticated identity, resource = customer id — required since 0013, `changed_fields` list in the payload for updates; metadata/identifier edits recorded as field names, not values).
- **Rationale**: Clarification Q3 mandated who/what/when + changed fields; the append-only audit_logs table and helper pattern already exist (005/0010/0013). Recording field names not values keeps PII out of the audit payload while satisfying "which fields changed".
- **Alternatives considered**: Full before/after value snapshots (011 pattern for role changes) — rejected for customer PII (emails, phones) in an append-only log; field-name granularity meets the clarified requirement.

## R10. Frontend integration strategy

- **Decision**: Upgrade `features/tenant/customers/` in place: new `customers-api.service.ts` (typed Observables over `ApiResponse<T>`/`PaginatedResponse<T>`), `customers.store.ts` (list: query/cursor/items/loading via `rxMethod`), rebuilt `customers.component.ts` list page (search-input with debounced query, data-table, create button gated by `customers.manage`), new `customer-profile.component.ts` + `customer-profile.store.ts` at `customers/:id`, and `customer-dialog.component.ts` (reactive form: contact fields, identifier rows with channel select, metadata key-value editor). `channel-badge` gains `email`/`phone` variants; `APP_PATHS.tenant.customerDetail` added; conversation history section uses existing status-badge/channel-badge with empty-state.
- **Rationale**: Spec-002/003 layering: feature-local state in SignalStores, HTTP via functional-interceptor pipeline with typed envelopes, shared components for all visuals, route paths from `APP_PATHS`. Replacing fixtures in place keeps the sidebar, permissions gating (`customers.view` already in `PAGE_PERMISSIONS`), and page-title wiring intact.
- **Alternatives considered**: New parallel feature folder — rejected: the customers page already exists with the right route and permission; in-place upgrade avoids dead fixture code paths. NgRx global store slice — rejected: customer state is feature-local per the state-placement rules.
