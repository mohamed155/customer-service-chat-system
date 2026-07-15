# Quickstart: AI Provider Abstraction

**Feature**: 015-ai-provider-abstraction | **Date**: 2026-07-15

Validation guide — how to prove the feature works end-to-end. Contracts: [rest-api.md](./contracts/rest-api.md), [provider-contract.md](./contracts/provider-contract.md); schema: [data-model.md](./data-model.md).

## Prerequisites

- PostgreSQL and Redis running (same as every backend feature); `DATABASE_URL` exported.
- New required env var (all non-test environments):

```bash
# 32 random bytes, base64 — the AES-256-GCM master key for API-credential encryption
export APP_AI_KEY_ENCRYPTION_KEY="$(openssl rand -base64 32)"
```

- For the live-vendor check (SC-001): a real key for at least one vendor, e.g. `export LIVE_AI_OPENAI_KEY=sk-…`.

## Setup

```bash
cd backend
cargo sqlx migrate run          # applies 0038–0040
cargo run -p server             # dev server; dev identity header per 006 applies
```

All `curl` examples assume a session cookie (or the dev identity header) plus `X-Tenant-ID: <tenant-uuid>` on `/tenant/*` routes, per the platform HTTP contract.

## Scenario 1 — Configure and verify a provider (US1/US2/US3, SC-001)

```bash
# 1. Platform admin sets a platform default key + config
curl -X PUT localhost:8080/platform/ai/credentials/openai -d '{"api_key":"'$LIVE_AI_OPENAI_KEY'"}'
curl -X PUT localhost:8080/platform/ai/config \
     -d '{"provider":"openai","model":"gpt-5","max_output_tokens":256,"temperature":0.7}'

# 2. Connectivity test — a real vendor round-trip, no usage record
curl -X POST localhost:8080/platform/ai/config/test
# expect: {"ok":true,"provider":"openai","model":"gpt-5","latency_ms":…}

# 3. Tenant view resolves to the platform default
curl -H 'X-Tenant-ID: <T1>' localhost:8080/tenant/ai/config
# expect: scope=platform_default, credential.source=platform, key_hint masked (…last4)
```

**Expected**: test succeeds against the real vendor; every read shows only `key_hint` — grep the server logs for the key material and find nothing (SC-004).

## Scenario 2 — Config-only provider switch (US2, SC-002)

```bash
curl -X PUT -H 'X-Tenant-ID: <T1>' localhost:8080/tenant/ai/config \
     -d '{"provider":"anthropic","model":"claude-sonnet-5"}'
curl -X PUT -H 'X-Tenant-ID: <T1>' localhost:8080/tenant/ai/credentials/anthropic -d '{"api_key":"sk-ant-…"}'
curl -X POST -H 'X-Tenant-ID: <T1>' localhost:8080/tenant/ai/config/test
```

**Expected**: next test/call is served by Anthropic with the tenant's BYOK — zero code changes or restarts; `audit_logs` gains `ai_config.updated` and `ai_credential.set` rows with actor + timestamp (SC-007).

## Scenario 3 — Completion + usage recording (US1/US4, SC-003)

Completions are in-process (`AiService`) — exercised by the integration suite rather than curl:

```bash
cd backend
cargo test -p server --test ai                    # skips politely without DATABASE_URL
REQUIRE_DB_TESTS=1 cargo test -p server --test ai # forced (CI mode)
```

Then inspect recorded usage over HTTP:

```bash
curl -H 'X-Tenant-ID: <T1>' 'localhost:8080/tenant/ai/usage?limit=10'
curl -H 'X-Tenant-ID: <T1>' 'localhost:8080/tenant/ai/usage/summary?from=2026-07-15T00:00:00Z'
```

**Expected**: one record per vendor-reaching call (success and failure, streamed and blocking), correct token counts (or `null` = unreported, never zero), summary totals equal the sum of records.

## Scenario 4 — Failover (US1, SC-008) and streaming (US5, SC-006)

Covered by the integration suite (wiremock): primary returns 503 → bounded retry → fallback serves → the usage record and result name the **fallback** provider/model; the retry/failover trail is visible in structured logs. Streaming tests assert the first delta arrives before the vendor mock finishes emitting, and that an interrupted stream records partial usage with a normalized error.

## Scenario 5 — Content capture (US4, FR-018)

```bash
# opt in (audited)
curl -X PUT -H 'X-Tenant-ID: <T1>' localhost:8080/tenant/ai/config \
     -d '{"provider":"anthropic","model":"claude-sonnet-5","capture_content":true}'
# after the next AI call:
curl -H 'X-Tenant-ID: <T1>' localhost:8080/tenant/ai/usage/<record-id>   # needs ai_agent.manage
```

**Expected**: records created before the toggle stay metadata-only; after it, the detail endpoint returns `request_content`/`response_content`; the list endpoint never does; logs/traces contain no content either way; `ai_config.capture_content_changed` audit row exists.

## Full gate

```bash
cd backend
cargo fmt --check && cargo clippy --workspace -- -D warnings
cargo test --workspace                              # unit rings (adapters via wiremock, policy, crypto)
REQUIRE_DB_TESTS=1 cargo test -p server             # integration incl. rbac.rs map + schema.rs 0038–0040
LIVE_AI_OPENAI_KEY=sk-… cargo test -p server --test ai live_vendor -- --ignored   # SC-001 smoke
```

No frontend gates — this feature ships no dashboard changes.
