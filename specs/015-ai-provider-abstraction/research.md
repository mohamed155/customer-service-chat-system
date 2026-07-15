# Research: AI Provider Abstraction

**Feature**: 015-ai-provider-abstraction | **Date**: 2026-07-15

No `NEEDS CLARIFICATION` markers remained in the Technical Context; the questions below are the technology and design unknowns the plan had to settle.

## R1. Vendor HTTP client

**Decision**: `reqwest` with `rustls-tls`, `json`, and `stream` features, one shared `reqwest::Client` per process (connection pooling), injected into adapters by the registry.

**Rationale**: The workspace has no HTTP client yet. `reqwest` is the de-facto async client on Tokio, supports streamed response bodies (`bytes_stream()`) needed for vendor SSE, and `rustls` keeps the build free of OpenSSL system dependencies, matching the existing `runtime-tokio-rustls` choice in SQLx.

**Alternatives considered**: `hyper` directly — more control, but hand-rolling redirects/TLS/connection pooling buys nothing here. Official vendor SDKs (`async-openai`, `anthropic-sdk` crates) — rejected: three different SDK dependency trees with uneven quality/maintenance, and they would push vendor types toward the abstraction boundary; the per-vendor wire surface we need (one endpoint each) is small enough to own.

## R2. Vendor API surfaces and token-usage reporting

**Decision**: One endpoint per vendor, mapped to/from the uniform contract:

| Vendor | Endpoint | Auth | Streaming | Usage reporting |
|---|---|---|---|---|
| OpenAI | `POST {base}/v1/chat/completions` | `Authorization: Bearer` | `stream: true` (SSE), `stream_options: {include_usage: true}` for a final usage chunk | `usage.prompt_tokens` / `completion_tokens` |
| Anthropic | `POST {base}/v1/messages` | `x-api-key` + `anthropic-version` | `stream: true`, event-typed SSE (`message_start`, `content_block_delta`, `message_delta`, `message_stop`) | `usage.input_tokens` / `output_tokens` (input on `message_start`, output on `message_delta`) |
| Gemini | `POST {base}/v1beta/models/{model}:generateContent` (blocking) / `:streamGenerateContent?alt=sse` | `x-goog-api-key` header | SSE chunks of `GenerateContentResponse` | `usageMetadata.promptTokenCount` / `candidatesTokenCount` |

System instructions map to OpenAI's `system` role message, Anthropic's top-level `system` field, and Gemini's `systemInstruction`. Generation params map to `max_tokens`/`max_output_tokens` and `temperature`. Any vendor response missing usage figures yields `TokenUsage::Unreported` — the usage record stores NULL counts, never zero (spec edge case).

**Rationale**: These are each vendor's stable chat-completion surface, all three deliver streaming as SSE over POST, and all three report token usage in-band, so the uniform contract can carry usage without a second metering call.

**Alternatives considered**: OpenAI Responses API — newer, but Chat Completions remains the stable, universally documented surface and matches the other two vendors' request shape more closely. Gemini `?key=` query-param auth — rejected; the query string lands in URLs which risk log leakage, the header form does not.

## R3. Streaming decode

**Decision**: A minimal hand-rolled SSE decoder in `ai-providers/src/sse.rs`: buffer `bytes_stream()` chunks, split on blank-line event boundaries, expose `event:`/`data:` fields; each adapter parses its own `data:` JSON. Uniform output is a `futures::Stream<Item = Result<StreamEvent, ProviderError>>` (boxed), where `StreamEvent` is `Delta(String)` then a final `Done { usage, model, finish }`.

**Rationale**: All three vendors speak the same trivial subset of SSE (data lines + optional event names, OpenAI's `data: [DONE]` sentinel). A dependency (`eventsource-stream`, `reqwest-eventsource`) adds retry/reconnect semantics we must NOT use — reconnecting mid-completion would duplicate output; failover before first increment is the module's policy, not the transport's.

**Alternatives considered**: `reqwest-eventsource` — auto-reconnect behavior is wrong for one-shot completions, and it drags in an extra dependency for ~60 lines of parsing we can test exhaustively.

## R4. Provider contract shape

**Decision**: In `ai-providers/src/contract.rs`:

- `ChatRequest { messages: Vec<Message>, system: Option<String>, model: String, max_output_tokens: Option<u32>, temperature: Option<f32> }`
- `ChatCompletion { content: String, model: String, usage: TokenUsage, finish: FinishReason }`
- `TokenUsage { input: Option<u32>, output: Option<u32> }` (None = unreported)
- `ProviderError { category: ErrorCategory, retriable: bool, detail: String }` with `ErrorCategory ∈ {Authentication, RateLimited, Unavailable, Timeout, InvalidRequest}` (FR-012)
- `#[async_trait] trait ChatProvider: Send + Sync { async fn complete(&self, key: &SecretKey, req: &ChatRequest) -> Result<ChatCompletion, ProviderError>; async fn stream(&self, key: &SecretKey, req: &ChatRequest) -> Result<ChatStream, ProviderError>; }` where `ChatStream = BoxStream<'static, Result<StreamEvent, ProviderError>>`
- `ProviderKind { OpenAi, Anthropic, Gemini }` — the fixed catalog (Key Entities), each carrying `supports_streaming()` (all `true` today; the flag exists so FR-013's transparent fallback has a place to look)
- `registry.rs`: `Registry::provider(kind) -> &dyn ChatProvider`, constructed once with the shared client and per-kind base URLs (overridable via config for tests)

Credentials are passed per call (not held by adapters) because the same adapter serves every tenant's key (BYOK) — adapters stay stateless.

**Rationale**: Trait-object dispatch keyed by a closed enum matches "fixed vendor catalog" from the spec while keeping FR-003 true: a fourth provider = one adapter + one enum variant + one registry arm, zero caller changes (the SC-005 test drives a test-only impl through `AiService` via a registry seam). `async_trait` is already a workspace dependency.

**Alternatives considered**: Generic `AiService<P: ChatProvider>` — monomorphization can't represent per-request provider selection from config. String-keyed open registry — looser than the spec's fixed catalog and pushes validation to runtime; the enum gives exhaustive matching and CHECK-constraint parity in the DB.

## R5. API-key encryption at rest

**Decision**: AES-256-GCM via the `aes-gcm` crate (RustCrypto). Master key from new required env var `APP_AI_KEY_ENCRYPTION_KEY` (base64, exactly 32 bytes decoded), parsed/validated in `AppConfig` with redacted Debug like existing secrets. Per-credential random 12-byte nonce (from `getrandom`, already a workspace dep) stored beside the ciphertext; AAD binds scope: `tenant_id (or "platform") || provider`. Plaintext lives only in a `SecretKey` newtype: `Debug` prints `SecretKey(****)`, no `Serialize`/`Display`, zero out on drop. Masked hint = last 4 characters, computed at write time and stored as its own column — reads never touch ciphertext.

**Rationale**: FR-008 demands encryption at rest with keys never retrievable/never logged; AEAD with scope-binding AAD prevents both tampering and cross-scope ciphertext replay (moving a ciphertext row to another tenant fails to decrypt). Env-supplied master key satisfies "secrets never in source" (Constitution III) with the machinery the deployment already has; a KMS integration is an extension point documented in the module docs, not a v1 dependency.

**Alternatives considered**: `pgcrypto` in-database encryption — the master key would transit to the DB server and appear in statement logs; rejected. `chacha20poly1305` — equivalent security; AES-GCM chosen for hardware acceleration ubiquity. Envelope encryption per credential — overkill for one master key; rotation path (re-encrypt rows under a new key) is documented in research and does not change the schema.

## R6. Retry, backoff, and failover parameters

**Decision**: In `modules/ai/src/service.rs`, policy constants: per-provider attempts = 1 initial + 2 retries on `retriable` errors only (RateLimited, Unavailable, Timeout), backoff 200 ms then 1 s with ±25% jitter (`rand` is a workspace dep); then advance to the next fallback entry (each resolving its own credential per FR-017) and repeat the same per-provider policy; after the last fallback, surface the normalized error of the **last** attempt. Authentication/InvalidRequest fail immediately with no retry and no failover. For streams, the policy applies only until the first `Delta` is delivered to the caller; after first delivery, a failure terminates the stream with a normalized error event and partial usage is recorded (spec edge case: no mid-stream provider switch). Every attempt emits a `tracing` event (`provider`, `attempt`, `category`, `latency_ms`, request-id) — never message content or key material.

**Rationale**: FR-017 requires "small bounded retry with backoff"; 3 total attempts × (up to) 3 providers bounds worst-case added latency at roughly 2.4 s of sleep plus vendor timeouts, inside the plan's ~10 s failover cap. Last-error semantics give the caller the most recent (most actionable) category.

**Alternatives considered**: Retry crates (`backon`, `tower::retry`) — the interleaving of retry, credential re-resolution, and failover attribution is the core domain logic here; a generic combinator obscures exactly the part that needs unit tests. Circuit breaker — valuable later at scale, out of scope for v1 (no requirement); noted as an extension point.

## R7. Configuration storage shape

**Decision**: Single `ai_configurations` table for both scopes: `tenant_id UUID NULL` (NULL = platform default) with two partial unique indexes (`WHERE tenant_id IS NOT NULL AND deleted_at IS NULL` on `tenant_id`; `WHERE tenant_id IS NULL AND deleted_at IS NULL` constant-true expression) so exactly one live row exists per scope. Fallbacks as `fallbacks JSONB NOT NULL DEFAULT '[]'` — an ordered array of `{provider, model}` validated in `model.rs` (≤ 3 entries, providers from the catalog, no duplicate of primary). `capture_content BOOLEAN NOT NULL DEFAULT false` lives on the row; the platform-default row's flag is ignored by resolution (capture is a tenant decision per FR-018 — a tenant without an override creates one to opt in). Same single-table pattern for `ai_credentials` (`tenant_id NULL` = platform key), unique per live `(scope, provider)`.

**Rationale**: Resolution (FR-004) becomes one ordered two-row lookup per table (`ORDER BY tenant_id NULLS LAST LIMIT 1` with scope filter); admin handlers share one code path with a scope parameter. JSONB for fallbacks: the list is tiny, ordered, read whole-row, and never joined — a child table adds migrations and N+1 surface for zero relational benefit.

**Alternatives considered**: Separate platform tables — rejected in Complexity Tracking (duplication). Fallback child table — rejected above. Content-capture as a column on `tenants` — would put an AI-module-owned setting in a tenancy-owned table, violating module ownership (Constitution I).

## R8. Usage recording and content capture

**Decision**: `ai_usage_records` append-only: `tenant_id NOT NULL`, `provider`, `model` (the pair that **actually served** the call — FR-017 attribution), `input_tokens INT NULL`, `output_tokens INT NULL` (NULL = vendor did not report), `status TEXT CHECK (status IN ('success','failure'))`, `error_category TEXT NULL`, `latency_ms`, `request_id`, `created_at`, and nullable `request_content JSONB` / `response_content TEXT` populated only when the tenant's resolved `capture_content` is true. One record per call that reached a vendor — a call that exhausts retries/fallbacks writes one record attributed to the last provider attempted, with `failure` status; a request failing before any vendor call (NotConfigured) writes none (spec edge case). Recording happens after the response returns / the stream ends, in a spawned task with error logging, so a slow insert can't extend caller latency but a failed insert is observable. Retrieval: `GET /tenant/ai/usage` (metadata list, cursor-paginated, period filter) + `GET /tenant/ai/usage/summary` (one aggregate: calls, input, output) under `ai_agent.view`; `GET /tenant/ai/usage/{id}` returns captured content and requires `ai_agent.manage`.

**Rationale**: Satisfies FR-010/FR-011/FR-018 and SC-003 with one insert per call and one indexed aggregate; splitting content behind the manage-guarded detail endpoint makes "access-restricted" concrete without a second table. Content stays out of logs/traces by construction: it is never passed to `tracing`, and the record insert is the only sink.

**Alternatives considered**: Separate `ai_usage_content` table — cleaner separation but forces a join and a second insert on the hot path; content columns are NULL (free) for the default-off majority. Recording inline before returning — simpler, but violates the plan's latency goal for no correctness gain (spawned-task write-behind is acceptable because SC-003's "100%" is enforced by tests on the awaited task in test builds, and failures are logged/alertable).

## R9. Testing strategy

**Decision**: Three rings. (1) `ai-providers` unit tests with `wiremock`: each adapter's request mapping, response/usage parsing, SSE decoding (multi-chunk splits, `[DONE]`, event-typed frames), and status→category normalization (401→Authentication, 429→RateLimited, 5xx→Unavailable, connect/read timeout→Timeout, 400→InvalidRequest). (2) `modules/ai` unit tests: resolution precedence matrix (tenant config × platform config × BYOK × platform key × neither), failover ordering/attribution with scripted fake providers, AES round-trip + AAD scope-mismatch failure, hint masking; the SC-005 guard test registers a fourth test-only `ChatProvider` through the registry seam and drives `AiService` end-to-end. (3) `server/tests/ai.rs` integration (existing `DATABASE_URL` skip / `REQUIRE_DB_TESTS=1` force pattern): admin CRUD + audit rows, RBAC (403s) and cross-tenant `not_found` matrix, key masking on every read path, usage recording incl. capture off/on, and SC-008 (wiremock primary returns 503 → fallback serves → record attributes fallback). SC-001 live smoke test additionally gated on `LIVE_AI_OPENAI_KEY` (skips silently when absent). Base-URL override fields in `AppConfig` (test/dev only) let integration tests point real adapters at wiremock.

**Rationale**: Matches the repo's established gating pattern (`escalations.rs`, `live_deps.rs`), keeps CI hermetic (wiremock), and still proves SC-001 against a real vendor when a key is supplied.

**Alternatives considered**: Mocking at the trait level only (no wiremock) — would leave adapter wire-format code untested until the live test; the adapters are exactly where vendor differences hide.
