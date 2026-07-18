// SC-005 / FR-003: Adding a fourth provider must require zero changes in
// modules/ai internals or any consuming module. This test drives the policy
// seam (ai::run_attempts) directly with a fake provider registered via
// Registry::with_override — no Postgres needed, no real provider code needed.
//
// If this test breaks or requires changes when adding a new provider, then
// the abstraction is leaking and SC-005 has been violated.

use std::sync::Arc;

use ai::{run_attempts, Attempt};
use ai_providers::{
    ChatCompletion, ChatProvider, ChatRequest, ChatStream, FinishReason, ProviderError, Registry,
    RegistryConfig, SecretKey, TokenUsage,
};

struct EchoProvider;

#[async_trait::async_trait]
impl ChatProvider for EchoProvider {
    async fn complete(
        &self,
        _key: &SecretKey,
        _req: &ChatRequest,
    ) -> Result<ChatCompletion, ProviderError> {
        Ok(ChatCompletion {
            content: "echo-reply".into(),
            model: "echo-model".into(),
            usage: TokenUsage {
                input: Some(1),
                output: Some(1),
            },
            finish: FinishReason::Stop,
            tool_calls: vec![],
        })
    }

    async fn stream(
        &self,
        _key: &SecretKey,
        _req: &ChatRequest,
    ) -> Result<ChatStream, ProviderError> {
        use futures::StreamExt;
        let stream = futures::stream::iter(vec![
            Ok(ai_providers::StreamEvent::Delta("echo-reply".into())),
            Ok(ai_providers::StreamEvent::Done {
                usage: TokenUsage {
                    input: Some(1),
                    output: Some(1),
                },
                model: "echo-model".into(),
                finish: FinishReason::Stop,
            }),
        ])
        .boxed();
        Ok(stream)
    }
}

#[tokio::test]
async fn fourth_provider_works_through_run_attempts() {
    let registry =
        Registry::new(RegistryConfig::new()).with_override("echoai", Arc::new(EchoProvider));

    let attempts = vec![Attempt {
        provider: "echoai".into(),
        model: "echo-model".into(),
        key: SecretKey::new("dummy-key".into()),
        max_output_tokens: None,
        temperature: None,
    }];

    let result = run_attempts(&registry, &attempts, None, vec![], vec![])
        .await
        .unwrap();

    assert_eq!(result.content, "echo-reply");
    assert_eq!(result.provider, "echoai");
    assert_eq!(result.model, "echo-model");
    assert_eq!(result.usage.input, Some(1));
    assert_eq!(result.usage.output, Some(1));
}
// SC-005/FR-001 verification: no crate outside modules/ai imports ai-providers directly.
// Run: grep -rn "ai-providers\|ai_providers" backend/crates/modules backend/crates/server/src backend/crates/shared
// Expected: hits only in backend/crates/modules/ai/
