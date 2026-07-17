# Contract: EmbeddingProvider (internal trait, `ai-providers` crate)

Internal Rust interface parallel to the existing `ChatProvider`. Not a REST endpoint. Defines how the platform generates embeddings provider-independently (Principle IV). Anthropic does not implement it (no embeddings API); capability is discovered explicitly, never assumed.

## Types (added to `ai-providers/src/contract.rs`)

```rust
pub struct EmbeddingRequest {
    pub model: String,
    /// One or more input passages embedded in a single call (batching).
    pub inputs: Vec<String>,
    pub request_id: Option<String>,
}

pub struct EmbeddingResponse {
    /// One vector per input, in input order. Every vector has the same length.
    pub embeddings: Vec<Vec<f32>>,
    pub model: String,
    pub usage: TokenUsage,      // reuse existing TokenUsage (input tokens)
}

#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(
        &self,
        key: &SecretKey,
        req: &EmbeddingRequest,
    ) -> Result<EmbeddingResponse, ProviderError>;
}
```

## Contract rules

1. **Order & cardinality**: `embeddings.len() == inputs.len()`, aligned by index. Implementations MUST error (`ErrorCategory::Other`/`InvalidRequest`) rather than silently drop or reorder.
2. **Uniform dimension**: All vectors in a response — and across calls for a given `model` — have identical length. Callers rely on this to match the fixed `vector(N)` column.
3. **Errors reuse `ProviderError`/`ErrorCategory`**: `rate_limited`, `unavailable`, `timeout` are retriable (drives the indexing retry policy, FR-016); `authentication`, `invalid_request` are terminal.
4. **Secrets**: key handled as `SecretKey`; never logged; error details pass through `sanitize_error_detail`.

## Registry capability discovery (added to `registry.rs`)

```rust
impl Registry {
    /// Returns None for providers without embedding support (e.g. Anthropic).
    pub fn embedding_provider(&self, name: &str) -> Option<&dyn EmbeddingProvider>;
}
```

## Implementations

| Provider | Endpoint | Notes |
|---|---|---|
| OpenAI | `POST /v1/embeddings` | `input` accepts an array; returns `data[].embedding`. Default model `text-embedding-3-small` (1536 dims). |
| Gemini | `models/{model}:batchEmbedContents` | batch request; returns `embeddings[].values`. |
| Anthropic | — | not implemented; `embedding_provider("anthropic") == None`. |

## AiService surface (`ai` module, `service.rs`)

```rust
impl AiService {
    /// Resolve the PLATFORM embedding config + credential, call the provider,
    /// append an ai_usage_records row, return vectors. Errors are classified
    /// so callers can apply the retry policy.
    pub async fn embed_platform(
        &self,
        ctx: AiCallContext,          // tenant_id used for usage attribution
        inputs: Vec<String>,
    ) -> Result<Vec<Vec<f32>>, AiCallError>;
}
```

`AiCallError::NotConfigured` is returned when no platform embedding config/credential exists (indexer marks items pending/failed accordingly; retrieval degrades to ungrounded).
