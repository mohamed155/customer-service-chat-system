use std::collections::HashMap;
use std::sync::Arc;

use crate::anthropic::AnthropicAdapter;
use crate::contract::*;
use crate::gemini::GeminiAdapter;
use crate::guarded::{GuardedChat, GuardedEmbedding};
use crate::openai::OpenAiAdapter;
use resilience::{Guard, Hub, Policy};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    OpenAi,
    Anthropic,
    Gemini,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
        }
    }

    pub fn supports_streaming(self) -> bool {
        true
    }
}

impl std::str::FromStr for ProviderKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "openai" => Ok(Self::OpenAi),
            "anthropic" => Ok(Self::Anthropic),
            "gemini" => Ok(Self::Gemini),
            _ => Err(()),
        }
    }
}

impl ProviderKind {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }
}

pub struct RegistryConfig {
    pub openai_base_url: Option<String>,
    pub anthropic_base_url: Option<String>,
    pub gemini_base_url: Option<String>,
    /// Circuit breaker and bulkhead settings applied to every provider (MOH-40). Each provider
    /// gets its own guard, so one provider's outage or slowness never affects another.
    pub resilience_policy: Policy,
}

impl RegistryConfig {
    pub fn new() -> Self {
        Self {
            openai_base_url: None,
            anthropic_base_url: None,
            gemini_base_url: None,
            resilience_policy: Policy::default(),
        }
    }
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
struct Registered {
    kind: ProviderKind,
    provider: Arc<dyn ChatProvider>,
}

#[derive(Clone)]
pub struct Registry {
    #[allow(dead_code)]
    client: reqwest::Client,
    providers: Vec<Registered>,
    overrides: HashMap<String, Arc<dyn ChatProvider>>,
    embedding_providers: HashMap<String, Arc<dyn EmbeddingProvider>>,
    hub: Arc<Hub>,
    #[cfg_attr(not(feature = "test-providers"), allow(dead_code))]
    policy: Policy,
}

/// Name of the guard for a provider's chat calls (`ai:openai`).
fn chat_guard_name(provider: &str) -> String {
    format!("ai:{provider}")
}

/// Name of the guard for a provider's embedding calls (`ai:openai:embed`). Separate from the chat
/// guard, so a bulk re-index can't use up the slots that customer replies need.
fn embed_guard_name(provider: &str) -> String {
    format!("ai:{provider}:embed")
}

impl Registry {
    pub fn new(cfg: RegistryConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("reqwest Client::build");

        let openai_base_url = cfg
            .openai_base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com".to_string());
        let anthropic_base_url = cfg
            .anthropic_base_url
            .clone()
            .unwrap_or_else(|| "https://api.anthropic.com".to_string());
        let gemini_base_url = cfg
            .gemini_base_url
            .clone()
            .unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string());

        let hub = Hub::new();
        let policy = cfg.resilience_policy;
        let chat = |kind: ProviderKind, adapter: Arc<dyn ChatProvider>| -> Registered {
            let guard = hub.guard(&chat_guard_name(kind.as_str()), policy);
            Registered {
                kind,
                provider: Arc::new(GuardedChat::new(adapter, guard)),
            }
        };
        let embed =
            |name: &str, adapter: Arc<dyn EmbeddingProvider>| -> Arc<dyn EmbeddingProvider> {
                let guard = hub.guard(&embed_guard_name(name), policy);
                Arc::new(GuardedEmbedding::new(adapter, guard))
            };

        let providers = vec![
            chat(
                ProviderKind::OpenAi,
                Arc::new(OpenAiAdapter::new(client.clone(), openai_base_url.clone())),
            ),
            chat(
                ProviderKind::Anthropic,
                Arc::new(AnthropicAdapter::new(client.clone(), anthropic_base_url)),
            ),
            chat(
                ProviderKind::Gemini,
                Arc::new(GeminiAdapter::new(client.clone(), gemini_base_url.clone())),
            ),
        ];
        let embedding_providers = HashMap::from([
            (
                "openai".to_string(),
                embed(
                    "openai",
                    Arc::new(OpenAiAdapter::new(client.clone(), openai_base_url)),
                ),
            ),
            (
                "gemini".to_string(),
                embed(
                    "gemini",
                    Arc::new(GeminiAdapter::new(client.clone(), gemini_base_url)),
                ),
            ),
        ]);

        Self {
            client,
            providers,
            overrides: HashMap::new(),
            embedding_providers,
            hub: hub.clone(),
            policy,
        }
    }

    pub fn provider(&self, kind: ProviderKind) -> &dyn ChatProvider {
        for p in &self.providers {
            if p.kind == kind {
                let name = kind.as_str();
                if let Some(override_provider) = self.overrides.get(name) {
                    return override_provider.as_ref();
                }
                return p.provider.as_ref();
            }
        }
        panic!("unknown provider kind: {:?}", kind);
    }

    pub fn resolve(&self, name: &str) -> Option<&dyn ChatProvider> {
        if let Some(override_provider) = self.overrides.get(name) {
            return Some(override_provider.as_ref());
        }
        ProviderKind::from_str(name).map(|kind| self.provider(kind))
    }

    /// Replace a provider with a test double. The double gets the same circuit breaker and
    /// bulkhead as a real adapter, so tests exercise the guarded path.
    #[cfg(feature = "test-providers")]
    pub fn with_override(mut self, name: &str, provider: Arc<dyn ChatProvider>) -> Self {
        let guard = self.hub.guard(&chat_guard_name(name), self.policy);
        self.overrides.insert(
            name.to_string(),
            Arc::new(GuardedChat::new(provider, guard)),
        );
        self
    }

    /// The circuit breakers and bulkheads of this registry, for the platform health view.
    pub fn resilience(&self) -> &Arc<Hub> {
        &self.hub
    }

    /// True when a chat call to `name` would be refused right now because its circuit is open. The
    /// skipped call is counted as a refusal (`circuit_breaker_rejections_total`). Callers with
    /// another provider to try use it to move on instead of waiting for the refusal.
    pub fn skip_open_circuit(&self, name: &str) -> bool {
        self.hub
            .get(&chat_guard_name(name))
            .is_some_and(|guard: Arc<Guard>| guard.rejects_now())
    }

    /// Like [`skip_open_circuit`](Self::skip_open_circuit), for callers that only want to know:
    /// nothing is counted.
    pub fn circuit_open(&self, name: &str) -> bool {
        self.hub
            .get(&chat_guard_name(name))
            .is_some_and(|guard: Arc<Guard>| guard.would_reject())
    }

    pub fn provider_by_name(&self, name: &str) -> Option<&dyn ChatProvider> {
        self.overrides.get(name).map(|p| p.as_ref())
    }

    pub fn embedding_provider(&self, name: &str) -> Option<&dyn EmbeddingProvider> {
        self.embedding_providers.get(name).map(|p| p.as_ref())
    }
}
