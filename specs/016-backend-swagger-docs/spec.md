# Feature Specification: Backend API Documentation (Swagger/OpenAPI)

**Feature Branch**: `016-backend-swagger-docs`

**Created**: 2026-07-15

**Status**: Draft

**Input**: User description: "add swagger to backend, make sure it covers all endpoints with all input/output models with data types"

## Clarifications

### Session 2026-07-15

- Q: Should the API documentation (interactive page + OpenAPI document) be exposed in production? → A: Opt-in flag, default off — docs served in dev/test always; in production only when an explicit config flag enables them.
- Q: How strictly should documentation completeness be enforced when an endpoint is missing from the docs? → A: Hard quality gate — an automated test compares registered routes against documented paths and fails when any non-test endpoint is undocumented.
- Q: How much authorization detail should each documented endpoint show? → A: Permission per endpoint — each endpoint documents its required RBAC permission (e.g., customers.manage) in addition to public vs. authenticated.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Browse Complete API Reference (Priority: P1)

A developer (frontend engineer, new backend hire, or integration partner) opens an interactive API documentation page served by the backend and can see every available endpoint, grouped by functional area (authentication, invitations, platform tenant management, platform AI configuration, tenant profile, customers, conversations, escalations, skills, availability, members, tenant AI configuration & usage, operational endpoints). For each endpoint they can see the HTTP method, path, path/query parameters, the exact shape of the request body, and the exact shape of every possible response — with a concrete data type for every field (e.g., string, UUID, integer, boolean, date-time, enum values, nullable/optional markers, arrays and nested objects).

**Why this priority**: This is the core value of the feature — a single trustworthy reference that eliminates guesswork and code-spelunking when consuming the API. Without it, nothing else in this feature matters.

**Independent Test**: Open the documentation page, pick any endpoint (e.g., create customer), and verify the displayed request/response fields and types match the actual API behavior when the same call is made against the running server.

**Acceptance Scenarios**:

1. **Given** the backend is running in a non-production environment, **When** a developer navigates to the documentation page, **Then** an interactive API reference loads listing all public, authenticated, platform, and tenant endpoints grouped by functional area.
2. **Given** the documentation page is open, **When** the developer expands any endpoint, **Then** they see its HTTP method, full path, path/query parameters with types, request body schema (when applicable) with a data type for every field, and response schemas for success responses.
3. **Given** the documentation page is open, **When** the developer inspects any request or response model, **Then** every field shows its data type, format where applicable (UUID, date-time, email), whether it is required or optional/nullable, and allowed values for enumerated fields.
4. **Given** an endpoint that requires authentication or a specific permission, **When** the developer views it in the documentation, **Then** the authentication requirement and the specific RBAC permission it demands are visibly indicated on that endpoint.
5. **Given** an endpoint that can return errors (validation failure, unauthenticated, forbidden, not found), **When** the developer views its documented responses, **Then** the shared error response model with its fields and data types is documented for the relevant status codes.

---

### User Story 2 - Consume a Machine-Readable API Specification (Priority: P2)

A developer or tooling pipeline downloads a machine-readable specification document (OpenAPI) from a stable URL on the backend and uses it to generate typed API clients, validate contracts in tests, or import the API into tools such as Postman or contract-testing suites.

**Why this priority**: The machine-readable document multiplies the value of the documentation — it enables client generation and automated contract checks — but it is only useful once the coverage from Story 1 exists.

**Independent Test**: Fetch the specification document from its URL, validate it against the OpenAPI standard with an off-the-shelf validator, and import it into an API client tool without errors.

**Acceptance Scenarios**:

1. **Given** the backend is running, **When** a client requests the specification document URL, **Then** a valid OpenAPI document is returned that passes standard OpenAPI validation.
2. **Given** the specification document, **When** it is imported into a standard API tool (e.g., Postman, an OpenAPI client generator), **Then** all endpoints, parameters, and models import without errors and with correct types.

---

### User Story 3 - Documentation Stays in Sync with the API (Priority: P3)

When a developer adds or changes an endpoint, request model, or response model, the documentation reflects the change without anyone hand-editing a separate documentation artifact. Endpoints that exist only for internal testing never appear in the published documentation.

**Why this priority**: Stale or drifting documentation is worse than none — but sync mechanics only matter after the documentation exists and is consumable.

**Independent Test**: Add a field to an existing response model, rebuild/restart the backend, and confirm the new field (with its type) appears in both the interactive page and the machine-readable document with no manual documentation edit.

**Acceptance Scenarios**:

1. **Given** a developer changes a request or response model in code, **When** the backend is rebuilt and restarted, **Then** the documentation reflects the change automatically.
2. **Given** the backend registers test-only routes in test/development configurations, **When** the documentation is viewed, **Then** test-only routes (e.g., synthetic permission-check routes, echo/panic routes) are absent.
3. **Given** a new endpoint is added without documentation annotations, **When** the project's quality gates run, **Then** an automated completeness test fails, identifying the undocumented route.

---

### Edge Cases

- The event-stream endpoint (`/tenant/events`, server-sent events) does not return JSON; its documentation must describe the stream content type and event payload models rather than a standard JSON response.
- Endpoints that set or clear cookies (login/logout) must document the session cookie behavior, since the "response body" alone does not capture the contract.
- Responses with polymorphic or variant shapes (e.g., conversation detail with optional escalation context, AI config that differs between platform and tenant scope) must document each variant's fields and types rather than a loose "object" type.
- Paginated list endpoints must document the pagination envelope (items, cursors/counts) and the query parameters that control paging, filtering, and sorting with their types and defaults.
- In production, the documentation surface must not weaken security: it must be disabled or explicitly gated, and must never expose secrets (credential values are write-only in the API and must be documented as such).
- If the specification document fails to generate (e.g., a model cannot be represented), the build or startup must fail loudly rather than silently publishing incomplete documentation.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The backend MUST publish a machine-readable OpenAPI specification document at a stable URL.
- **FR-002**: The backend MUST serve an interactive, browsable documentation page rendering that specification.
- **FR-003**: The documentation MUST cover every non-test HTTP endpoint exposed by the backend, including: public authentication and invitation endpoints; the authenticated `/me` endpoint; platform tenant management and platform AI configuration endpoints; tenant profile, customers, conversations, messages, events, escalations, skills, availability, members, invitations, and tenant AI configuration/usage endpoints; and the operational endpoints (health, readiness, metrics).
- **FR-004**: Test-only routes (synthetic permission-check routes, echo/panic routes) MUST be excluded from the published documentation.
- **FR-005**: Every documented endpoint MUST declare its HTTP method, path, and all path and query parameters with data types, required/optional status, and defaults where applicable.
- **FR-006**: Every endpoint that accepts a request body MUST document the complete input model: every field with its data type, format where applicable (UUID, date-time, email), required/optional status, nullability, and allowed values for enumerated fields.
- **FR-007**: Every endpoint MUST document its success response model(s) with the same field-level completeness as inputs, including nested objects, arrays, and pagination envelopes.
- **FR-008**: Every endpoint MUST document the error responses it can return (at minimum: validation failure, unauthenticated, forbidden, not found where applicable) using the shared error response model with its fields and data types.
- **FR-009**: The documentation MUST indicate, per endpoint, whether authentication is required and how it is supplied (session cookie), MUST distinguish public endpoints from authenticated ones, and MUST state the specific RBAC permission each guarded endpoint requires (e.g., `customers.manage`).
- **FR-010**: The event-stream endpoint MUST be documented with its stream content type and the schema of each event payload it can emit.
- **FR-011**: Write-only sensitive inputs (e.g., provider credential values, passwords) MUST be documented as write-only and MUST never appear in documented response models.
- **FR-012**: The specification MUST be generated from the same source of truth as the running API (code-first), so that model or endpoint changes are reflected without hand-editing a separate document.
- **FR-013**: The specification document MUST pass standard OpenAPI validation.
- **FR-014**: Documentation exposure MUST be environment-aware: always enabled in development and test environments; in production, disabled by default and served only when an explicit configuration flag opts in.
- **FR-015**: The project MUST enforce documentation completeness as a hard quality gate: an automated test compares the backend's registered routes against the documented paths and FAILS when any non-test endpoint is missing from the documentation.

### Key Entities

- **API Specification Document**: The machine-readable description of the whole API — endpoints, parameters, request/response models, security scheme, error model. Regenerated from code; never hand-maintained.
- **Endpoint Entry**: One method+path pair with its parameters, request model, response models per status code, and security requirement.
- **Model Schema**: A named request or response shape with field names, data types, formats, required/optional flags, nullability, and enum values; shared models (error envelope, pagination envelope) are defined once and referenced.
- **Security Scheme**: The description of how callers authenticate (session cookie) applied to all non-public endpoints.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of non-test endpoints exposed by the backend appear in the published documentation (verifiable by comparing registered routes to documented paths).
- **SC-002**: 100% of documented request and response models declare a concrete data type for every field — zero untyped/free-form fields except where the API genuinely accepts arbitrary data (and those are explicitly marked as such).
- **SC-003**: The specification document passes an off-the-shelf OpenAPI validator with zero errors.
- **SC-004**: A developer unfamiliar with the codebase can locate any endpoint's full request/response contract in the documentation in under one minute, without reading source code.
- **SC-005**: For a sample of at least 10 endpoints spanning all functional areas, live API responses match the documented schemas exactly (field names, types, optionality).
- **SC-006**: A model change made in code appears in the documentation after rebuild with zero manual documentation edits.

## Assumptions

- "Swagger" is interpreted as the OpenAPI standard plus an interactive documentation UI served by the backend; the exact tooling is a planning decision.
- Documentation is served by the backend itself (not a separately hosted portal) so it always matches the deployed version.
- Test-only routes are intentionally excluded; operational endpoints (health, readiness, metrics) are included since operators consume them.
- No additional authentication layer on the docs page itself is required in non-production environments; production exposure is governed by the opt-in flag per FR-014.
- The existing shared error envelope and response conventions (request-id header, error codes) are the contract to document; this feature does not change any endpoint behavior, only documents it.
- Per-endpoint RBAC permission names are part of the mandatory documentation scope (see FR-009); a full role-to-permission matrix per endpoint is out of scope.
