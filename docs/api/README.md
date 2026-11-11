# API contract

`openapi.json` is the OpenAPI 3.1 document of the backend API. It is **generated from the code**
(`utoipa`, see `backend/crates/server/src/openapi.rs` and `router.rs`) and committed so that it can be
reviewed, compared and used to generate clients. Never edit it by hand.

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

## The generated TypeScript types

`pnpm gen:api` (in `frontend/`) runs `openapi-typescript` on `openapi.json` and formats the result with
Prettier. The output is types only: `paths`, `components` and `operations`. There is no generated HTTP
client; the dashboard's `ApiService` still does the calls. Use the types like this:

```ts
import type { components } from './generated/api';
type Notification = components['schemas']['NotificationDto'];
```

See ADR-012 gap A6 for how far the dashboard has moved to them.
