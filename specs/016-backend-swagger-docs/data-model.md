# Data Model: OpenAPI Schema & Annotation Inventory

**Feature**: 016-backend-swagger-docs | **Date**: 2026-07-15

This feature adds no database entities. The "data model" here is the set of Rust types that must carry OpenAPI schema derives (`#[derive(ToSchema)]` for bodies, `#[derive(IntoParams)]` for query params) and the handlers that must carry `#[utoipa::path]`. Every type below already exists in code; this inventory is the checklist of what to annotate and which doc-only wrapper types to add.

## Shared schemas (crate `shared/kernel`)

| Type | Kind | Derive to add | Notes |
|------|------|---------------|-------|
| `ErrorEnvelope` | response (all errors) | `ToSchema` | Root of every error body. |
| `ErrorBody` | nested | `ToSchema` | `code: string`, `message: string`, `details: array`, `request_id: string`. |
| `ErrorDetail` | nested | `ToSchema` | `field`, `code`, `message` — used in 422 detail arrays. |
| `Page<T>` | response envelope | `ToSchema` (generic) | `{ items: T[], nextCursor: string?, hasMore: bool }` (camelCase). |
| `PageParams` | query | `IntoParams` | `limit: u32 (default 25, clamp 1..100)`, `cursor: string?`. |

The error envelope is referenced by the `responses(...)` of every operation (FR-008). Register once in the root `OpenApi` `components`.

## Config (crate `shared/config`)

| Change | Detail |
|--------|--------|
| `AppConfig.docs_enabled: bool` | New field, sourced from `APP_DOCS_ENABLED` (default `false`). Drives production gating (FR-014, research Decision 7). Not a schema — a runtime flag. |

## Security scheme (crate `server`, new `openapi.rs`)

| Item | Detail |
|------|--------|
| `session_cookie` | `SecurityScheme::ApiKey` in cookie `app_session`, added via `Modify`. Applied to all non-public operations (FR-009). |
| Servers | `/api/v1` base server object via `Modify`. |
| Tags | One tag per functional area (see contracts/openapi-coverage.md grouping). |

## Module: identity (`modules/identity`)

| Type | Kind | Derive | Notes |
|------|------|--------|-------|
| `LoginRequest` | request body | `ToSchema` | `email: string`, `password: string` (write-only). |
| `Principal` | response (part of `/me` + login) | `ToSchema` | Confirm no secret fields serialized. |
| `AccountCreationInput` | internal | — | Not an HTTP body; skip unless surfaced. |

`LoginSuccess`/`LoginError` are internal control types (not serialized bodies) — no derive. Login's documented response is the `MeResponse` body plus a `Set-Cookie` header note (FR of cookie behavior edge case).

## Module: tenancy (`modules/tenancy`)

| Type | Kind | Derive | Notes |
|------|------|--------|-------|
| `TenantSummary` | response | `ToSchema` | list item for platform tenants. |
| `PlatformTenantDetail` | response | `ToSchema` | |
| `CreateTenantRequest` | request body | `ToSchema` | |
| `UpdateTenantRequest` | request body | `ToSchema` | PATCH — optional fields. |
| `ListTenantsParams` | query | `IntoParams` | pagination/filter for platform tenants list. |
| `MeResponse` | response | `ToSchema` | `/me` + login response. |
| `MembershipSummary` | nested | `ToSchema` | |
| `TeamMemberQuery` | query | `IntoParams` | |
| `TeamMemberResponse` | response | `ToSchema` | |
| `UpdateMemberPayload` | request body | `ToSchema` | |
| `CreateInvitationPayload` | request body | `ToSchema` | |
| `AcceptInvitationPayload` | request body | `ToSchema` | |
| `InvitationResponse`, `CreateInvitationResponse`, `InvitationListItem`, `InvitationDeliveryResponse`, `PreviewInvitationResponse`, `AcceptInvitationResponse` | responses | `ToSchema` | |
| `InvitationQuery` | query | `IntoParams` | |
| `MemberRow` | internal (DB row) | — | Skip; not a serialized body. |

## Module: customers (`modules/customers`)

| Type | Kind | Derive | Notes |
|------|------|--------|-------|
| `CustomerListItem` | response | `ToSchema` | |
| `CustomerDetail` | response | `ToSchema` | includes `identifiers`, `metadata` map. |
| `ChannelIdentifier` | nested | `ToSchema` | |
| `ChannelIdentifierInput` | request nested | `ToSchema` | |
| `CreateCustomerPayload` | request body | `ToSchema` | |
| `UpdateCustomerPayload` | request body | `ToSchema` | uses `TriState<T>` for tri-state fields. |
| `TriState<T>` | request field | custom `ToSchema` | Absent/Clear/Value — document as nullable+optional; needs a manual schema mapping (research Decision 4). |
| `CustomerListQuery` | query | `IntoParams` | `q`, `cursor`, `limit`. |
| `Customer` | internal | — | Skip if not a body. |
| **doc-only wrappers to add** | | | `CustomerDetailResponse { data: CustomerDetail }`, `CustomerListResponse { data: CustomerListItem[], pagination: PaginationEnvelope }` — mirror the inline `json!` envelopes. |

## Module: conversations (`modules/conversations`)

| Type | Kind | Derive | Notes |
|------|------|--------|-------|
| `ConversationStatus`, `MessageKind`, `ConversationStatusRef` | enums | `ToSchema` | document enum variants (FR-006 allowed values). |
| `Assignee`, `CustomerRef`, `LastMessagePreview`, `Participant` | nested | `ToSchema` | |
| `Conversation` | response | `ToSchema` | |
| `ConversationDetail` | response | `ToSchema` | polymorphic: optional escalation context (edge case) — document escalation fields as optional/nullable. |
| `Message` | response | `ToSchema` | |
| `AddMessageResponse` | response | `ToSchema` | |
| `CreateConversationPayload`, `CreateMessagePayload`, `AddMessagePayload`, `PatchConversationPayload` | request bodies | `ToSchema` | |
| `InboxQueryParams`, `TimelineQueryParams` | query | `IntoParams` | |

## Module: escalations (`modules/escalations`)

| Type | Kind | Derive | Notes |
|------|------|--------|-------|
| `RoutingReason`, `EscalationStatus`, `AvailabilityState` | enums | `ToSchema` | enum variants documented. |
| `RequiredSkillRef`, `RoutingInfo`, `CustomerRef`, `QueueEntryConversationRef` | nested | `ToSchema` | |
| `Escalation`, `QueueEntry`, `Skill`, `Availability` | responses | `ToSchema` | |
| `TeamMemberSkill`, `TeamMemberWithSkills` | responses | `ToSchema` | |
| `EscalatePayload`, `SetAvailabilityPayload`, `CreateSkillPayload`, `RenameSkillPayload`, `SetMemberSkillsPayload` | request bodies | `ToSchema` | |
| `QueueQueryParams` | query | `IntoParams` | |
| `EscalationAssignedEvent`, `EscalationQueuedEvent`, `EscalationRemovedEvent`, `AvailabilityChangedEvent` | SSE event payloads | `ToSchema` | registered as components; referenced by `GET /tenant/events` (FR-010). |

## Module: ai (`modules/ai`)

| Type | Kind | Derive | Notes |
|------|------|--------|-------|
| `ConfigPayload` | request body | `ToSchema` | provider/model/fallbacks/temperature. |
| `FallbackEntry` | nested | `ToSchema` | |
| `AiConfigurationView` | response | `ToSchema` | polymorphic platform vs tenant scope (edge case) — `scope` field distinguishes. |
| `CredentialView` | response | `ToSchema` | `key_hint` only — never the key. |
| `CredentialPayload` | request body | `ToSchema` + `write_only` | `api_key` must be marked write-only; MUST NOT appear in any response schema (FR-011). |
| `UsageListItem`, `UsageSummary`, `UsageDetailRow` | responses | `ToSchema` | |
| `Pagination`, `PaginatedResponse<T>` (ai::usage) | response envelope | `ToSchema` | another envelope variant — document as-is. |
| `AiConfigRow`, `UsageWrite` | internal | — | Skip (DB row / write model). |

## Server-local composite handlers (crate `server`)

| Handler | Path | Notes |
|---------|------|-------|
| `handlers::get_conversation_with_escalation` | `GET /tenant/conversations/{id}` | Response merges `ConversationDetail` + escalation context; add `#[utoipa::path]` and a wrapper schema. |
| `handlers::list_members_with_skills` | `GET /tenant/members` | Response is `TeamMemberWithSkills[]`; annotate. |

## Write-only / secret-bearing fields (FR-011 audit list)

- `LoginRequest.password` — write-only.
- `CredentialPayload.api_key` — write-only; verify absent from `CredentialView` and all responses.
- Confirm no `AiConfigRow`/DB-row type with secret columns is ever serialized into a response body.

## Enumerated types (FR-006 allowed-values coverage)

`ConversationStatus`, `MessageKind`, `RoutingReason`, `EscalationStatus`, `AvailabilityState`, provider names in `ConfigPayload`/`FallbackEntry` (validated against `ProviderKind`). Each enum's serde renames (e.g. `skill_match`, `queue_auto`) must be preserved in the schema so documented values match wire values.
