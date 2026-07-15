use ai_providers::ProviderKind;
use kernel::ApiError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackEntry {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AiConfigRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub provider: String,
    pub model: String,
    pub max_output_tokens: Option<i32>,
    pub temperature: Option<f32>,
    pub fallbacks: serde_json::Value,
    pub capture_content: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigPayload {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub max_output_tokens: Option<i32>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub fallbacks: Option<Vec<FallbackEntry>>,
    #[serde(default)]
    pub capture_content: Option<bool>,
}

impl ConfigPayload {
    pub fn validate(&self) -> Result<(), ApiError> {
        if ProviderKind::from_str(&self.provider).is_none() {
            return Err(ApiError::validation_failed(format!(
                "unknown provider '{}'",
                self.provider
            )));
        }
        if self.model.trim().is_empty() {
            return Err(ApiError::validation_failed("model must not be empty"));
        }
        if let Some(ref max_tokens) = self.max_output_tokens {
            if *max_tokens <= 0 {
                return Err(ApiError::validation_failed(
                    "max_output_tokens must be positive",
                ));
            }
        }
        if let Some(ref temp) = self.temperature {
            if *temp < 0.0 || *temp > 2.0 {
                return Err(ApiError::validation_failed(
                    "temperature must be between 0 and 2",
                ));
            }
        }
        if let Some(ref fallbacks) = self.fallbacks {
            if fallbacks.len() > 3 {
                return Err(ApiError::validation_failed("at most 3 fallbacks allowed"));
            }
            let primary_pair = (self.provider.as_str(), self.model.as_str());
            for (i, fb) in fallbacks.iter().enumerate() {
                if ProviderKind::from_str(&fb.provider).is_none() {
                    return Err(ApiError::validation_failed(format!(
                        "fallback {}: unknown provider '{}'",
                        i, fb.provider
                    )));
                }
                if fb.model.trim().is_empty() {
                    return Err(ApiError::validation_failed(format!(
                        "fallback {}: model must not be empty",
                        i
                    )));
                }
                let fb_pair = (fb.provider.as_str(), fb.model.as_str());
                if fb_pair == primary_pair {
                    return Err(ApiError::validation_failed(format!(
                        "fallback {} is the same as the primary provider/model",
                        i
                    )));
                }
                for (j, other) in fallbacks.iter().enumerate() {
                    if i != j && fb.provider == other.provider && fb.model == other.model {
                        return Err(ApiError::validation_failed(format!(
                            "duplicate fallback provider/model pair at index {}",
                            i
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Deserialize)]
pub struct CredentialPayload {
    pub api_key: String,
}

impl std::fmt::Debug for CredentialPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialPayload")
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

impl CredentialPayload {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.api_key.is_empty() {
            return Err("API key must not be empty");
        }
        if self.api_key.len() > 512 {
            return Err("API key must not exceed 512 characters");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialView {
    pub source: String,
    pub provider: String,
    pub key_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfigurationView {
    pub scope: String,
    pub provider: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    pub fallbacks: Vec<FallbackEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_content: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<CredentialView>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> ConfigPayload {
        ConfigPayload {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            max_output_tokens: Some(4096),
            temperature: Some(0.7),
            fallbacks: Some(vec![FallbackEntry {
                provider: "anthropic".to_string(),
                model: "claude-3".to_string(),
            }]),
            capture_content: Some(true),
        }
    }

    #[test]
    fn test_valid_config() {
        assert!(valid_config().validate().is_ok());
    }

    #[test]
    fn test_unknown_provider() {
        let mut c = valid_config();
        c.provider = "fake".to_string();
        assert!(c.validate().is_err());
    }

    #[test]
    fn test_empty_model() {
        let mut c = valid_config();
        c.model = "   ".to_string();
        assert!(c.validate().is_err());
    }

    #[test]
    fn test_negative_max_tokens() {
        let mut c = valid_config();
        c.max_output_tokens = Some(-1);
        assert!(c.validate().is_err());
    }

    #[test]
    fn test_temperature_out_of_range() {
        let mut c = valid_config();
        c.temperature = Some(-0.1);
        assert!(c.validate().is_err());

        let mut c2 = valid_config();
        c2.temperature = Some(2.1);
        assert!(c2.validate().is_err());

        let mut c3 = valid_config();
        c3.temperature = Some(1.5);
        assert!(c3.validate().is_ok());
    }

    #[test]
    fn test_too_many_fallbacks() {
        let mut c = valid_config();
        c.fallbacks = Some(vec![
            FallbackEntry {
                provider: "openai".to_string(),
                model: "gpt-3.5".to_string(),
            },
            FallbackEntry {
                provider: "anthropic".to_string(),
                model: "claude-2".to_string(),
            },
            FallbackEntry {
                provider: "gemini".to_string(),
                model: "gemini-pro".to_string(),
            },
            FallbackEntry {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
            },
        ]);
        assert!(c.validate().is_err());
    }

    #[test]
    fn test_fallback_equals_primary() {
        let mut c = valid_config();
        c.fallbacks = Some(vec![FallbackEntry {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
        }]);
        assert!(c.validate().is_err());
    }

    #[test]
    fn test_duplicate_fallbacks() {
        let mut c = valid_config();
        c.fallbacks = Some(vec![
            FallbackEntry {
                provider: "anthropic".to_string(),
                model: "claude-3".to_string(),
            },
            FallbackEntry {
                provider: "anthropic".to_string(),
                model: "claude-3".to_string(),
            },
        ]);
        assert!(c.validate().is_err());
    }

    #[test]
    fn test_fallback_empty_model() {
        let mut c = valid_config();
        c.fallbacks = Some(vec![FallbackEntry {
            provider: "anthropic".to_string(),
            model: "   ".to_string(),
        }]);
        assert!(c.validate().is_err());
    }
}
