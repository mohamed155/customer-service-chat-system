use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChatRequest {
    pub system: Option<String>,
    pub messages: Vec<Message>,
    pub model: String,
    pub max_output_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub request_id: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolSpec>,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TokenUsage {
    pub input: Option<u32>,
    pub output: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FinishReason {
    Stop,
    Length,
    ToolUse,
    Other,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChatCompletion {
    pub content: String,
    pub model: String,
    pub usage: TokenUsage,
    pub finish: FinishReason,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StreamEvent {
    Delta(String),
    ToolCall(ToolCall),
    Done {
        usage: TokenUsage,
        model: String,
        finish: FinishReason,
    },
}

pub type ChatStream = futures::stream::BoxStream<'static, Result<StreamEvent, ProviderError>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCategory {
    Authentication,
    RateLimited,
    Unavailable,
    Timeout,
    InvalidRequest,
}

impl ErrorCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Authentication => "authentication",
            Self::RateLimited => "rate_limited",
            Self::Unavailable => "unavailable",
            Self::Timeout => "timeout",
            Self::InvalidRequest => "invalid_request",
        }
    }

    pub fn retriable(&self) -> bool {
        matches!(self, Self::RateLimited | Self::Unavailable | Self::Timeout)
    }
}

#[derive(Clone, Debug)]
pub struct ProviderError {
    pub category: ErrorCategory,
    pub retriable: bool,
    pub detail: String,
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.category.as_str(), self.detail)
    }
}

impl std::error::Error for ProviderError {}

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretKey(String);

impl SecretKey {
    pub fn new(key: String) -> Self {
        Self(key)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKey(****)")
    }
}

/// Sanitize an error detail string by:
/// 1. Replacing sequences that look like API keys (20+ alphanumeric chars with
///    at least one uppercase letter and one digit) with `[REDACTED]`
/// 2. Limiting to 200 characters
/// 3. Stripping non-ASCII characters
pub fn sanitize_error_detail(detail: &str) -> String {
    let cleaned: String = detail.chars().filter(|c| c.is_ascii()).collect();
    let mut result = String::new();
    let mut buf = String::new();
    for ch in cleaned.chars() {
        if ch.is_ascii_alphanumeric() {
            buf.push(ch);
        } else {
            if buf.len() >= 20
                && buf.chars().any(|c| c.is_ascii_uppercase())
                && buf.chars().any(|c| c.is_ascii_digit())
            {
                result.push_str("[REDACTED]");
            } else {
                result.push_str(&buf);
            }
            result.push(ch);
            buf.clear();
        }
    }
    if !buf.is_empty() {
        if buf.len() >= 20
            && buf.chars().any(|c| c.is_ascii_uppercase())
            && buf.chars().any(|c| c.is_ascii_digit())
        {
            result.push_str("[REDACTED]");
        } else {
            result.push_str(&buf);
        }
    }
    result.chars().take(200).collect()
}

#[async_trait::async_trait]
pub trait ChatProvider: Send + Sync {
    async fn complete(
        &self,
        key: &SecretKey,
        req: &ChatRequest,
    ) -> Result<ChatCompletion, ProviderError>;

    async fn stream(&self, key: &SecretKey, req: &ChatRequest)
        -> Result<ChatStream, ProviderError>;
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingRequest {
    pub model: String,
    pub inputs: Vec<String>,
    pub request_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingResponse {
    pub embeddings: Vec<Vec<f32>>,
    pub model: String,
    pub usage: TokenUsage,
}

#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(
        &self,
        key: &SecretKey,
        req: &EmbeddingRequest,
    ) -> Result<EmbeddingResponse, ProviderError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_key_debug_redacted() {
        let key = SecretKey::new("sk-test-12345".into());
        assert_eq!(format!("{:?}", key), "SecretKey(****)");
    }

    #[test]
    fn error_category_retriable() {
        assert!(ErrorCategory::RateLimited.retriable());
        assert!(ErrorCategory::Unavailable.retriable());
        assert!(ErrorCategory::Timeout.retriable());
        assert!(!ErrorCategory::Authentication.retriable());
        assert!(!ErrorCategory::InvalidRequest.retriable());
    }
}
