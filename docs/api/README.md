# API contract

`openapi.json` is the OpenAPI 3.1 document of the backend API. It is **generated from the code**
(`utoipa`, see `backend/crates/server/src/openapi.rs` and `router.rs`) and committed so that it can be
reviewed, compared and used to generate clients. Never edit it by hand.

## Errors

An error answers in one of two formats, chosen by the request's `Accept` header
([ADR-012](../adr/0012-api-conventions.md), gap A2). The legacy format is being replaced; new clients
ask for the new one.

| Request says | Response |
| -- | -- |
| `Accept: application/problem+json, application/json;q=0.5` (or any list that gives `application/problem+json` a quality above zero and at least that of `application/json`) | `application/problem+json`, an [RFC 9457](https://www.rfc-editor.org/rfc/rfc9457) document |
| no `Accept`, `application/json`, `*/*`, anything else | the legacy envelope `{ "error": { code, message, details, request_id } }`, with `Deprecation` and a `Link` header (`rel="deprecation"`) to the ADR |

A problem document has `type` (`urn:problem:<code>`), `title`, `status`, `detail`, `instance` (the
request path, without the query), `code` (the stable machine code; branch on this, never on `detail`),
`request_id` (quote it to support) and `errors[]` of `{ field, code }`. Context that is not a field
error, such as the ID of the thing in conflict, is under `details`. A `5xx` carries only its public
message: an empty `errors`, no `details`, no internal text. `Retry-After` is sent in both formats. Both
formats vary on `Accept` (`Vary: accept`).

`openapi.json` lists both media types on every documented error response that uses the error envelope:
`application/problem+json` (`ProblemDetails`) and the legacy `application/json` (`ErrorEnvelope`),
which is marked deprecated (`x-deprecated`). A few responses have a body of their own instead of the
envelope (the AI configuration test's `422`, the tool decision's `409`); they list only their own
`application/json`.

The backend counts every legacy-format answer in `error_envelope_legacy_total{route}` so the envelope
can be removed when nobody receives it any more. It is removed only after the counter has shown no
legacy answer for two weeks across a full release cycle (ADR-012, gap A2): widget builds already
embedded on customers' pages are the slowest clients to update, and a shorter quiet period could
break one that comes back later.

## What checks it

| Check | Where | Fails when |
| -- | -- | -- |
| Document is current | backend test `openapi_snapshot` (job `verify`) | the committed file differs from what the router generates |
| No breaking change | job `openapi-breaking` in `.github/workflows/openapi.yml` | the pull request makes a breaking change for API clients compared with its base branch |
| Types are current | frontend job `verify`, step "Generated API types are current" | `apps/dashboard/src/app/core/api/generated/api.ts` differs from what `pnpm gen:api` generates |
| Wire types match | dashboard unit test `core/api/contract.spec.ts` | a hand-written wire type no longer matches the generated schema it describes |

## After you change the API

```bash
# 1. Regenerate the document (from backend/)
UPDATE_OPENAPI=1 cargo test -p server --test openapi_snapshot

# 2. Regenerate the TypeScript types (from frontend/)
pnpm gen:api
```

Commit `docs/api/openapi.json` and the generated file together with the code change.

## Breaking changes

The `openapi-breaking` job runs [`oasdiff`](https://github.com/oasdiff/oasdiff) (a pinned release,
verified by checksum, run on local files only) with `--fail-on ERR`. It reports, for example, a removed
endpoint, a new required request field, a response field that was removed or changed type, or a
narrowed enum in a request. Adding an endpoint or an optional field is not breaking.

A change that is breaking on purpose (for example after the last client was migrated) needs the pull
request label `breaking-api-change`. The job then lists the changes as a warning and passes. Describe
the migration in the pull request. If the label does not exist in the repository yet, create it first.

The first pull request that adds this file has nothing to compare with; the job says so and passes.

### Platform tenant access (MOH-97)

Clients calling `POST /platform/tenants/{id}/switch` must send a JSON body with a
`reason` of 10–500 characters after trimming. The optional `durationMinutes` accepts
5–240 minutes and defaults to 60. The response adds `accessSession` to the tenant summary.
Use the same sign-in for subsequent tenant requests. Retrieve the current session with
`GET /platform/access-session` and end it with `POST /platform/access-session/end`.

## The generated TypeScript types

`pnpm gen:api` (in `frontend/`) runs `openapi-typescript` on `openapi.json` and formats the result with
Prettier. The output is types only: `paths`, `components` and `operations`. There is no generated HTTP
client; the dashboard's `ApiService` still does the calls. Use the types like this:

```ts
import type { components } from './generated/api';
type Notification = components['schemas']['NotificationDto'];
```

See ADR-012 gap A6 for how far the dashboard has moved to them.
