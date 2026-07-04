# Contract: AI Provider Interface

The single seam between the platform and LLM vendors (Constitution IV,
FR-PROV-001..006). Lives in `backend/crates/ai-providers`. Adding a provider
= implementing this interface + registering it in the model catalog. Nothing
above this crate may reference a vendor name except platform provider-config
UI/admin surfaces.

## Capability trait (conceptual signature)

```rust
#[async_trait]
pub trait AiProvider: Send + Sync {
    fn id(&self) -> ProviderId;                      // openai | anthropic | gemini | ...
    fn capabilities(&self) -> &[Capability];         // Chat, ChatStream, ToolUse, Embed

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError>;
    async fn chat_stream(&self, req: ChatRequest)
        -> Result<BoxStream<'static, Result<ChatDelta, ProviderError>>, ProviderError>;
    async fn embed(&self, req: EmbedRequest) -> Result<EmbedResponse, ProviderError>;
}
```

## Normalized request model

`ChatRequest`:
- `model: ModelRef` (catalog entry, never a raw vendor string from callers)
- `messages: Vec<ChatMessage>` — roles {system, user, assistant, tool_result};
  content is the **already-assembled deterministic context** (R-06) — adapters
  MUST NOT inject, reorder, or rewrite content beyond vendor wire-format
  mapping
- `tools: Vec<ToolSpec>` — name, description, JSON-Schema input (adapters map
  to vendor tool/function format)
- `params: GenParams` — max_tokens, temperature, stop; unsupported params per
  vendor are rejected loudly (`ProviderError::UnsupportedParam`), never
  silently dropped
- `metadata: CallMeta` — request_id, tenant_id, execution_id (propagated to
  vendor headers where supported; always onto the tracing span)

`ChatResponse` / terminal `ChatDelta`:
- `content: Vec<ContentBlock>` — Text | ToolCall{ id, name, input }
- `stop_reason` ∈ {end, max_tokens, tool_call, safety, other(vendor_code)}
- `usage: TokenUsage` { input_tokens, output_tokens, cost_estimate } —
  computed per model-catalog pricing; feeds timelines + metering

`ChatDelta` (streaming): `TextDelta(String)` | `ToolCallDelta` |
`Terminal { stop_reason, usage }`. Adapters normalize vendor streaming
(SSE/chunk formats) into this shape; first `TextDelta` latency is the
first-token metric (SC-002).

`EmbedRequest/Response`: batch of texts → vectors + usage; dimensionality
declared in the model catalog (retrieval index depends on it).

## Error taxonomy (drives failover, FR-PROV-004)

```text
ProviderError::RateLimited { retry_after }   → retry same provider after delay, else failover
ProviderError::Unavailable { .. }            → failover immediately
ProviderError::Timeout                       → failover immediately
ProviderError::InvalidRequest { detail }     → NO failover (caller bug) — surface
ProviderError::ContentPolicy { detail }      → NO failover — surface to confidence/escalation logic
ProviderError::UnsupportedParam/Capability   → NO failover — configuration error
ProviderError::Auth                          → alert platform ops; failover; mark provider degraded
```

## FailoverExecutor

Wraps any capability call with the routing policy:
1. Resolve (tenant, capability) → ordered [primary, fallback…] from
   RoutingPolicy (per-plan/tenant overrides, BYO-key tenants pinned).
2. On failover-eligible error: next provider in chain; record
   `failover_from` in the execution timeline and emit
   `provider.failover_triggered`.
3. Mid-stream failure: restart the turn on the fallback (deterministic
   assembly makes the retry safe); widget sees an uninterrupted turn
   (SC-009).
4. Chain exhausted → `AllProvidersFailed` → graceful customer message +
   escalation path (NFR-AVAIL-003).

## Adapter obligations

- **No vendor SDKs** — direct HTTPS (R-05); connection pooling; per-provider
  concurrency limits.
- Wire-format mapping only; a recorded-fixture contract-test suite runs every
  adapter against the same scenario set (simple chat, streaming, tool call,
  tool-call streaming, rate-limit, timeout, malformed response) and asserts
  identical normalized output.
- Secrets pulled from AiProviderConfig (envelope-encrypted); never logged;
  tracing spans record provider/model/latency/tokens but no content
  (NFR-LOG-002 — content lives only in the AiExecution timeline with
  access control).
- Token accounting mandatory even on error paths (partial usage on abort).

## Catalog & routing (platform-owned data, see data-model.md)

- `AiProviderConfig.model_catalog`: model id, capabilities, context window,
  embedding dims, cost/token.
- `RoutingPolicy`: per capability default + fallback chain + overrides.
- Tenant-facing config expresses capability tiers, not vendor names
  (FR-PROV-003), except BYO-key enterprise tenants.
