//! Shared OpenAI responder for integration tests.
//!
//! The reply engine calls `/v1/chat/completions` with `"stream": true` and
//! parses server-sent events, while other paths (e.g. conversation summaries)
//! use the plain JSON completion. Tests describe the reply once, as a normal
//! completion body, and this responder answers the way the real API would for
//! whichever mode the request asked for.
//!
//! Include from a test crate with:
//! `#[path = "support/openai_mock.rs"] mod openai_mock;`

#![allow(dead_code)]

use serde_json::{json, Value};
use wiremock::{Request, Respond, ResponseTemplate};

pub struct OpenAiReply {
    body: Value,
}

impl OpenAiReply {
    /// `body` is a non-streaming chat completion:
    /// `{ "choices": [{ "message": {...}, "finish_reason": "stop" }], "model": ..., "usage": {...} }`
    pub fn new(body: Value) -> Self {
        Self { body }
    }
}

impl Respond for OpenAiReply {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let streaming = serde_json::from_slice::<Value>(&request.body)
            .ok()
            .and_then(|v| v.get("stream").and_then(Value::as_bool))
            .unwrap_or(false);
        if streaming {
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(completion_to_sse(&self.body))
        } else {
            ResponseTemplate::new(200).set_body_json(self.body.clone())
        }
    }
}

/// Convert a non-streaming completion body into the equivalent SSE stream.
pub fn completion_to_sse(body: &Value) -> String {
    let model = body["model"].as_str().unwrap_or("gpt-4");
    let choice = &body["choices"][0];
    let message = &choice["message"];
    let finish = choice["finish_reason"].as_str().unwrap_or("stop");

    let mut delta = json!({ "role": "assistant" });
    if let Some(content) = message["content"].as_str() {
        delta["content"] = json!(content);
    }
    if let Some(calls) = message["tool_calls"].as_array() {
        let deltas: Vec<Value> = calls
            .iter()
            .enumerate()
            .map(|(index, call)| {
                // Streamed tool-call arguments are a JSON string.
                let arguments = match &call["function"]["arguments"] {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                json!({
                    "index": index,
                    "id": call["id"],
                    "type": "function",
                    "function": { "name": call["function"]["name"], "arguments": arguments },
                })
            })
            .collect();
        delta["tool_calls"] = json!(deltas);
    }

    let chunks = [
        json!({ "model": model, "choices": [{ "index": 0, "delta": delta, "finish_reason": null }] }),
        json!({ "model": model, "choices": [{ "index": 0, "delta": {}, "finish_reason": finish }] }),
        json!({ "model": model, "choices": [], "usage": body["usage"] }),
    ];
    let mut out = String::new();
    for chunk in chunks {
        out.push_str(&format!("data: {chunk}\n\n"));
    }
    out.push_str("data: [DONE]\n\n");
    out
}

/// A ready-made streaming (SSE) response for the reply engine, for tests that
/// build their own `Respond` implementations from templates.
pub fn streaming_template(body: &Value) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(completion_to_sse(body))
}
