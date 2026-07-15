use std::collections::HashMap;
use std::sync::Arc;

use crate::anthropic::AnthropicAdapter;
use crate::contract::*;
use crate::gemini::GeminiAdapter;
use crate::openai::OpenAiAdapter;

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
}

impl RegistryConfig {
    pub fn new() -> Self {
        Self {
            openai_base_url: None,
            anthropic_base_url: None,
            gemini_base_url: None,
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

        Self {
            client: client.clone(),
            providers: vec![
                Registered {
                    kind: ProviderKind::OpenAi,
                    provider: Arc::new(OpenAiAdapter::new(client.clone(), openai_base_url)),
                },
                Registered {
                    kind: ProviderKind::Anthropic,
                    provider: Arc::new(AnthropicAdapter::new(client.clone(), anthropic_base_url)),
                },
                Registered {
                    kind: ProviderKind::Gemini,
                    provider: Arc::new(GeminiAdapter::new(client.clone(), gemini_base_url)),
                },
            ],
            overrides: HashMap::new(),
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

    #[cfg(feature = "test-providers")]
    pub fn with_override(mut self, name: &str, provider: Arc<dyn ChatProvider>) -> Self {
        self.overrides.insert(name.to_string(), provider);
        self
    }

    pub fn provider_by_name(&self, name: &str) -> Option<&dyn ChatProvider> {
        self.overrides.get(name).map(|p| p.as_ref())
    }
}
