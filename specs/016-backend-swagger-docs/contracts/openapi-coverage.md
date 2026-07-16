# Contract: OpenAPI Coverage Map

**Feature**: 016-backend-swagger-docs | **Date**: 2026-07-15

This is the authoritative inventory of every endpoint the OpenAPI document MUST cover (FR-003), with its request/response/error models, security, and RBAC permission (FR-009). The coverage test (FR-015) asserts the documented path+method set equals this inventory. All `/api/v1/*` paths are relative to the `/api/v1` server base. Test-only routes (behind `include_test_routes`) are intentionally excluded.

**Error responses**: every operation documents the shared `ErrorEnvelope` for the status codes it can emit. Common set: `400 validation_failed`, `401 unauthenticated`, `403 unauthorized`, `404 not_found`, `409 conflict`, `422 validation_failed (with details)`, `429 rate_limited`, `500 internal_error`. Per-endpoint the relevant subset is listed as "Errors".

**Security**: `cookie` = requires `session_cookie` (app_session). `public` = no security requirement.

## Public (tag: auth, invitations)

| Method | Path | Request | Success | Errors | Security | Permission |
|--------|------|---------|---------|--------|----------|------------|
| POST | `/auth/login` | `LoginRequest` | 200 `MeResponse` + `Set-Cookie: app_session` | 400, 401, 500 | public | — |
| POST | `/auth/logout` | — | 200 (clears cookie) | 401 | cookie | — |
| GET | `/invitations/{token}` | path `token: string` | 200 `PreviewInvitationResponse` | 404, 410 | public | — |
| POST | `/invitations/{token}/accept` | `AcceptInvitationPayload` | 200 `AcceptInvitationResponse` | 400, 404, 410, 422 | public | — |

## Authenticated (tag: identity)

| Method | Path | Request | Success | Errors | Security | Permission |
|--------|------|---------|---------|--------|----------|------------|
| GET | `/me` | — | 200 `MeResponse` | 401 | cookie | authenticated only |

## Platform — tenants (tag: platform-tenants)

| Method | Path | Request | Success | Errors | Security | Permission |
|--------|------|---------|---------|--------|----------|------------|
| GET | `/platform/tenants` | query `ListTenantsParams` | 200 `Page<TenantSummary>` | 401, 403 | cookie | `platform.tenants.list` |
| POST | `/platform/tenants` | `CreateTenantRequest` | 201 `PlatformTenantDetail` | 400, 403, 409, 422 | cookie | `platform.tenants.manage` |
| GET | `/platform/tenants/{id}` | path `id: uuid` | 200 `PlatformTenantDetail` | 401, 403, 404 | cookie | `platform.tenants.list` |
| PATCH | `/platform/tenants/{id}` | `UpdateTenantRequest` | 200 `PlatformTenantDetail` | 400, 403, 404, 422 | cookie | `platform.tenants.manage` |
| POST | `/platform/tenants/{id}/switch` | path `id: uuid` | 200 (switch result) | 403, 404 | cookie | `platform.tenants.switch` |

## Platform — AI config (tag: platform-ai)

| Method | Path | Request | Success | Errors | Security | Permission |
|--------|------|---------|---------|--------|----------|------------|
| GET | `/platform/ai/config` | — | 200 `AiConfigurationView` | 401, 403 | cookie | `platform.admin` |
| PUT | `/platform/ai/config` | `ConfigPayload` | 200 `AiConfigurationView` | 400, 403, 422 | cookie | `platform.admin` |
| PUT | `/platform/ai/credentials/{provider}` | path `provider`, `CredentialPayload` (write-only) | 200/204 | 400, 403, 422 | cookie | `platform.admin` |
| DELETE | `/platform/ai/credentials/{provider}` | path `provider` | 204 | 403, 404 | cookie | `platform.admin` |
| POST | `/platform/ai/config/test` | `ConfigPayload` | 200 (test result) | 400, 403, 422 | cookie | `platform.admin` |

## Tenant — profile (tag: tenant)

| Method | Path | Request | Success | Errors | Security | Permission |
|--------|------|---------|---------|--------|----------|------------|
| GET | `/tenant` | — | 200 (tenant profile) | 401, 403 | cookie | `overview.view` |

## Tenant — customers (tag: customers)

| Method | Path | Request | Success | Errors | Security | Permission |
|--------|------|---------|---------|--------|----------|------------|
| GET | `/tenant/customers` | query `CustomerListQuery` | 200 `CustomerListResponse` (`{data,pagination}`) | 400, 403 | cookie | `customers.view` |
| POST | `/tenant/customers` | `CreateCustomerPayload` | 201 `CustomerDetailResponse` (`{data}`) | 400, 403, 409, 422 | cookie | `customers.manage` |
| GET | `/tenant/customers/{id}` | path `id: uuid` | 200 `CustomerDetailResponse` | 403, 404 | cookie | `customers.view` |
| PATCH | `/tenant/customers/{id}` | `UpdateCustomerPayload` | 200 `CustomerDetailResponse` | 400, 403, 404, 409, 422 | cookie | `customers.manage` |
| GET | `/tenant/customers/{id}/conversations` | path `id: uuid` | 200 conversation history | 403, 404 | cookie | `customers.view` |

## Tenant — conversations & messages (tag: conversations)

| Method | Path | Request | Success | Errors | Security | Permission |
|--------|------|---------|---------|--------|----------|------------|
| GET | `/tenant/conversations` | query `InboxQueryParams` | 200 list of `Conversation` | 400, 403 | cookie | `conversations.view` |
| POST | `/tenant/conversations` | `CreateConversationPayload` | 201 `ConversationDetail` | 400, 403, 422 | cookie | `conversations.manage` |
| GET | `/tenant/conversations/{id}` | path `id: uuid` | 200 `ConversationDetail` (+ optional escalation) | 403, 404 | cookie | `conversations.view` |
| PATCH | `/tenant/conversations/{id}` | `PatchConversationPayload` | 200 `ConversationDetail` | 400, 403, 404, 422 | cookie | `conversations.manage` |
| GET | `/tenant/conversations/{id}/messages` | query `TimelineQueryParams` | 200 list of `Message` | 400, 403, 404 | cookie | `conversations.view` |
| POST | `/tenant/conversations/{id}/messages` | `AddMessagePayload` | 201 `AddMessageResponse` | 400, 403, 404, 422 | cookie | `conversations.manage` |

## Tenant — realtime & escalations (tag: escalations)

| Method | Path | Request | Success | Errors | Security | Permission |
|--------|------|---------|---------|--------|----------|------------|
| GET | `/tenant/events` | — | 200 `text/event-stream` (events: `EscalationAssignedEvent`, `EscalationQueuedEvent`, `EscalationRemovedEvent`, `AvailabilityChangedEvent`) | 401, 403 | cookie | `conversations.view` |
| POST | `/tenant/conversations/{id}/escalate` | `EscalatePayload` | 200/201 `Escalation` | 400, 403, 404, 409, 422 | cookie | `conversations.manage` |
| GET | `/tenant/escalations/queue` | query `QueueQueryParams` | 200 list of `QueueEntry` | 400, 403 | cookie | `conversations.view` |
| POST | `/tenant/escalations/{id}/claim` | path `id: uuid` | 200 `Escalation` | 403, 404, 409 | cookie | `conversations.manage` |

## Tenant — availability & skills (tag: escalations)

| Method | Path | Request | Success | Errors | Security | Permission |
|--------|------|---------|---------|--------|----------|------------|
| GET | `/tenant/availability/me` | — | 200 `Availability` | 401, 403 | cookie | `conversations.manage` |
| PUT | `/tenant/availability/me` | `SetAvailabilityPayload` | 200 `Availability` | 400, 403, 422 | cookie | `conversations.manage` |
| GET | `/tenant/skills` | — | 200 list of `Skill` | 401, 403 | cookie | `members.view` |
| POST | `/tenant/skills` | `CreateSkillPayload` | 201 `Skill` | 400, 403, 409, 422 | cookie | `members.view` |
| PATCH | `/tenant/skills/{id}` | `RenameSkillPayload` | 200 `Skill` | 400, 403, 404, 422 | cookie | `members.manage` |
| DELETE | `/tenant/skills/{id}` | path `id: uuid` | 204 | 403, 404 | cookie | `members.manage` |

## Tenant — members & invitations (tag: members)

| Method | Path | Request | Success | Errors | Security | Permission |
|--------|------|---------|---------|--------|----------|------------|
| GET | `/tenant/members` | query `TeamMemberQuery` | 200 list of `TeamMemberWithSkills` | 400, 403 | cookie | `members.view` |
| PATCH | `/tenant/members/{id}` | `UpdateMemberPayload` | 200 `TeamMemberResponse` | 400, 403, 404, 422 | cookie | `members.manage` |
| PUT | `/tenant/members/{membershipId}/skills` | `SetMemberSkillsPayload` | 200 (updated skills) | 400, 403, 404, 422 | cookie | `members.manage` |
| GET | `/tenant/members/invitations` | query `InvitationQuery` | 200 list of `InvitationListItem` | 400, 403 | cookie | `members.view` |
| POST | `/tenant/members/invitations` | `CreateInvitationPayload` | 201 `CreateInvitationResponse` | 400, 403, 409, 422 | cookie | `members.manage` |
| GET | `/tenant/members/invitations/{id}/delivery` | path `id: uuid` | 200 `InvitationDeliveryResponse` | 403, 404 | cookie | `members.view` |
| DELETE | `/tenant/members/invitations/{id}` | path `id: uuid` | 204 | 403, 404 | cookie | `members.manage` |

## Tenant — AI config & usage (tag: tenant-ai)

| Method | Path | Request | Success | Errors | Security | Permission |
|--------|------|---------|---------|--------|----------|------------|
| GET | `/tenant/ai/config` | — | 200 `AiConfigurationView` | 401, 403 | cookie | `ai.agent.view` |
| PUT | `/tenant/ai/config` | `ConfigPayload` | 200 `AiConfigurationView` | 400, 403, 422 | cookie | `ai.agent.manage` |
| DELETE | `/tenant/ai/config` | — | 204 | 403, 404 | cookie | `ai.agent.manage` |
| PUT | `/tenant/ai/credentials/{provider}` | path `provider`, `CredentialPayload` (write-only) | 200/204 | 400, 403, 422 | cookie | `ai.agent.manage` |
| DELETE | `/tenant/ai/credentials/{provider}` | path `provider` | 204 | 403, 404 | cookie | `ai.agent.manage` |
| POST | `/tenant/ai/config/test` | `ConfigPayload` | 200 (test result) | 400, 403, 422 | cookie | `ai.agent.manage` |
| GET | `/tenant/ai/usage` | query (pagination) | 200 `PaginatedResponse<UsageListItem>` | 400, 403 | cookie | `ai.agent.view` |
| GET | `/tenant/ai/usage/summary` | — | 200 `UsageSummary` | 403 | cookie | `ai.agent.view` |
| GET | `/tenant/ai/usage/{id}` | path `id: uuid` | 200 `UsageDetailRow` | 403, 404 | cookie | `ai.agent.manage` |

## Operational (tag: ops) — outside `/api/v1`

| Method | Path | Request | Success | Errors | Security | Permission |
|--------|------|---------|---------|--------|----------|------------|
| GET | `/health` | — | 200 liveness | — | public | — |
| GET | `/ready` | — | 200 / 503 readiness | 503 | public | — |
| GET | `/metrics` | — | 200 `text/plain` metrics | — | public | — |

## Notes on faithful documentation

- **Permission strings** above reflect the `Permission` enum guards in `server/src/router.rs`. Confirm exact serialized names against `authz::Permission` during implementation; the map uses dotted form for readability.
- **Response envelopes vary** and are documented as-is (research Decision 4): `{data}`, `{data,pagination}`, `Page<T>` (`{items,nextCursor,hasMore}`), and `PaginatedResponse<T>` (ai::usage). Do not normalize.
- **`/tenant/events`** is `text/event-stream`; its event payload schemas are components referenced from the description (FR-010).
- **Credential inputs** (`CredentialPayload.api_key`, `LoginRequest.password`) are `write_only` and must never appear in a response schema (FR-011).
- Exact success status codes (200 vs 201 vs 204) must be verified against each handler during annotation; the table records the intended contract.
