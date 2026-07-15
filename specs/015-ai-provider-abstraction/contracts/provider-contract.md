# Internal Contract: Provider Abstraction & AiService

**Feature**: 015-ai-provider-abstraction | **Date**: 2026-07-15

Two internal contracts, one per layer. Signatures are normative for shape and semantics; exact syntax may be refined during implementation without changing meaning.

## Layer 1 — `ai-providers` crate: the vendor contract (FR-001/FR-002/FR-003)

Pure vendor layer: no database, no tenancy, no business types. Only `modules/ai` may depend on this crate (SC-005 is enforced by the dependency graph).

```rust
// contract.rs
pub enum Role { System, User, Assistant }
pub struct Message { pub role: Role, pub content: String }

pub struct ChatRequest {
    pub system: Option<String>,          // system instructions
    pub messages: Vec<Message>,          // prior conversation
    pub model: String,                   // vendor model id (from configuration)
    pub max_output_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

pub struct TokenUsage { pub input: Option<u32>, pub output: Option<u32> }  // None = vendor unreported

pub enum FinishReason { Stop, Length, Other }

pub struct ChatCompletion {
    pub content: String,
    pub model: String,                   // model that actually produced the reply
    pub usage: TokenUsage,
    pub finish: FinishReason,
}

pub enum StreamEvent {
    Delta(String),                                        // incremental content
    Done { usage: TokenUsage, model: String, finish: FinishReason },
}
pub type ChatStream = futures::stream::BoxStream<'static, Result<StreamEvent, ProviderError>>;

pub enum ErrorCategory { Authentication, RateLimited, Unavailable, Timeout, InvalidRequest }  // FR-012
pub struct ProviderError {
    pub category: ErrorCategory,
    pub retriable: bool,      // RateLimited | Unavailable | Timeout → true; others false (FR-017)
    pub detail: String,       // sanitized; NEVER contains key material or message content
}

#[async_trait]
pub trait ChatProvider: Send + Sync {
    async fn complete(&self, key: &SecretKey, req: &ChatRequest) -> Result<ChatCompletion, ProviderError>;
    async fn stream(&self, key: &SecretKey, req: &ChatRequest) -> Result<ChatStream, ProviderError>;
}
```

```rust
// registry.rs
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind { OpenAi, Anthropic, Gemini }      // the fixed catalog (Key Entities)
impl ProviderKind {
    pub fn supports_streaming(self) -> bool;             // all true today; FR-013's capability flag
    pub fn as_str(self) -> &'static str;                 // "openai" | "anthropic" | "gemini" (DB CHECK parity)
}

pub struct Registry { /* shared reqwest::Client + per-kind base URLs */ }
impl Registry {
    pub fn new(cfg: RegistryConfig) -> Self;             // base URLs overridable (tests → wiremock)
    pub fn provider(&self, kind: ProviderKind) -> &dyn ChatProvider;
    #[cfg(any(test, feature = "test-providers"))]
    pub fn with_override(self, kind_name: &str, provider: Arc<dyn ChatProvider>) -> Self;  // SC-005 seam
}
```

**Adapter obligations** (each of `openai.rs`, `anthropic.rs`, `gemini.rs`):
- Map `ChatRequest` to the vendor wire format (research R2 table) and back; unmapped vendor extras are dropped, never leaked.
- Normalize every failure to `ProviderError`: HTTP 401/403 → `Authentication`; 429 → `RateLimited`; 5xx/connect errors → `Unavailable`; client timeout → `Timeout`; 4xx (incl. unknown-model rejections) → `InvalidRequest`.
- Streaming: emit `Delta`s as decoded (no buffering of the full reply — SC-006), terminate with exactly one `Done` carrying whatever usage the vendor reported (`TokenUsage { None, None }` when unreported).
- Stateless with respect to credentials: the key arrives per call and is used only to set the auth header.

## Layer 2 — `modules/ai` crate: the consuming contract (what business modules see)

```rust
// service.rs — the ONLY completion entry point in the platform (FR-001, FR-016)
pub struct AiService { /* PgPool + Registry + master SecretKey; cheap to clone, lives in AppState */ }

pub struct AiCallContext {
    pub tenant_id: Uuid,
    pub request_id: Option<String>,      // observability correlation (FR-015)
}

pub struct AiCallResult {
    pub content: String,
    pub provider: String,                // provider that ACTUALLY served the call (FR-017)
    pub model: String,
    pub usage: TokenUsage,
}

pub enum AiCallError {
    NotConfigured,                                        // FR-004 — no vendor was called, no usage row
    Provider { category: ErrorCategory, provider: String }, // after retries + all fallbacks (FR-012/FR-017)
    Internal(String),
}

impl AiService {
    /// Blocking completion. Resolves config+credential, applies retry/failover,
    /// records usage (always, when any vendor was reached), returns attributed result.
    pub async fn complete(&self, ctx: AiCallContext, input: AiInput) -> Result<AiCallResult, AiCallError>;

    /// Streamed completion. Same resolution/policy; failover only until the first
    /// delivered delta; usage recorded at stream end/failure (incl. partial content
    /// when the tenant opted into capture).
    pub async fn stream(&self, ctx: AiCallContext, input: AiInput) -> Result<AiResultStream, AiCallError>;
}

pub struct AiInput {                     // caller-supplied ONLY — the layer never fetches business data (FR-016)
    pub system: Option<String>,
    pub messages: Vec<Message>,          // re-exported from ai-providers::contract
}

pub enum AiStreamEvent {
    Delta(String),
    Done(AiCallResult),                  // same final metadata as blocking calls (US5)
    Error { category: ErrorCategory },   // normalized mid-stream failure; partial usage already recorded
}
pub type AiResultStream = futures::stream::BoxStream<'static, AiStreamEvent>;
```

**Semantics (normative)**:
1. **Resolution** per call: config = tenant override → platform default → `NotConfigured`; credential per attempted provider = BYOK → platform key → skip attempt (trace-visible) / `NotConfigured` if no attempt is possible. Model/params come from the configuration; the caller cannot override them (config-only switching, FR-006).
2. **Policy**: per provider ≤ 3 attempts (2 retries, backoff 200 ms / 1 s ± jitter) on `retriable` errors only, then next fallback in configured order; `Authentication`/`InvalidRequest` abort immediately with no retry/failover (FR-017).
3. **Attribution**: `AiCallResult.provider/model` and the usage record always name the provider/model that served (or, on total failure, the last attempted) — SC-008.
4. **Usage**: exactly one `ai_usage_records` row per call that reached any vendor, including failures and interrupted streams; none for `NotConfigured`. Content columns populated iff the tenant's `capture_content` was true at resolution time (FR-018).
5. **Streaming transparency**: if the configured provider/config cannot stream, `stream()` still succeeds — the full reply arrives as a single `Delta` followed by `Done` (FR-013). No failover after the first delivered `Delta`.
6. **Secrecy**: `SecretKey` (redacted `Debug`, no `Serialize`, zeroized drop) is the only plaintext-key carrier; message content and keys never reach `tracing` (FR-008/FR-018).

## Extension points (documented in `lib.rs` module docs per constitution)

- **New provider**: adapter file + `ProviderKind` variant + registry arm + CHECK-constraint migration. No caller changes (FR-003; SC-005 test guards it).
- **New capability** (tool calling, embeddings, multimodal): extend `ChatRequest`/`StreamEvent` additively; the contract deliberately keeps vendor-neutral room (spec Assumptions).
- **Key management**: `crypto.rs` seals/opens behind two functions; swapping env master key for KMS envelope encryption touches only that file.
- **Extraction**: `ai-providers` has no project deps and could become a sidecar; `AiService` is the module seam that would become an RPC client.
