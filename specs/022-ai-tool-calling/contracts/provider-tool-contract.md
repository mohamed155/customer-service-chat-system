# Contract: Provider Tool-Calling Abstraction (internal)

Extension of `backend/crates/ai-providers/src/contract.rs`. Internal crate contract — consumed only by `AiService` and the engine; zero provider-specific types leak upward.

## Request side

```rust
pub struct ToolSpec {
    pub name: String,               // ^[a-z][a-z0-9_]{2,63}$
    pub description: String,
    pub input_schema: serde_json::Value, // JSON Schema object
}

pub struct ChatRequest {
    // ...existing fields...
    pub tools: Vec<ToolSpec>,       // empty = tool calling disabled (today's behavior)
}
```

Determinism: callers pass `tools` pre-sorted by (source, name); providers must serialize them in the given order.

## Message replay (multi-turn tool loop)

```rust
pub enum Role { System, User, Assistant, Tool }   // + Tool

pub struct ToolCall {
    pub id: String,                 // provider id; synthesized "{name}#{index}" for Gemini
    pub name: String,
    pub arguments: serde_json::Value,
}

pub struct Message {
    pub role: Role,
    pub content: String,
    pub tool_calls: Vec<ToolCall>,  // non-empty only on Assistant messages
    pub tool_call_id: Option<String>, // set only on Role::Tool result messages
}
```

Vendor mapping (owned by each provider impl):

| Concept | OpenAI | Anthropic | Gemini |
|---|---|---|---|
| Tool spec | `tools[].function` | `tools[]` | `tools[].functionDeclarations` |
| Model calls tool | `tool_calls[]` on assistant msg | `tool_use` content block | `functionCall` part |
| Result replay | `role:"tool"` + `tool_call_id` | `tool_result` content block in user msg | `functionResponse` part |

## Response / stream side

```rust
pub enum FinishReason { Stop, Length, ToolUse, Other }  // + ToolUse

pub struct ChatCompletion {
    // ...existing fields...
    pub tool_calls: Vec<ToolCall>,  // non-empty iff finish == ToolUse
}

pub enum StreamEvent {
    Delta(String),
    ToolCall(ToolCall),             // NEW: emitted complete (decoder accumulates arg deltas)
    Done { usage, model, finish },  // finish == ToolUse when calls were emitted
}
```

Guarantees:
- `StreamEvent::ToolCall` events are emitted only after the call's arguments are complete, valid JSON; consumers never see partial argument text.
- Text deltas and tool calls may interleave (model may "think aloud" then call); order is preserved.
- A stream finishing with `ToolUse` has emitted ≥ 1 `ToolCall`.
- Malformed vendor tool-call payloads surface as `ProviderError { category: InvalidRequest }` — never as a silent empty call.

## Compatibility

- `tools: vec![]` must produce byte-identical vendor requests to today's (no `tools` key serialized) — regression-tested per provider.
- Existing callers (021 engine paths, summary) compile unchanged apart from struct-literal updates.
