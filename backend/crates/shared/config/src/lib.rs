//! AppConfig — validated environment-driven configuration
//!
//! # Purpose
//! Parse, validate, and expose all runtime configuration from environment
//! variables. Secret-bearing fields are redacted in `Debug` output.
//!
//! # Public Interfaces
//! - `AppConfig::from_env()` — parse and validate
//! - `Environment`, `LogFormat` enums
//!
//! # Dependencies
//! None beyond `std`.
//!
//! # Extension Points
//! - Add new fields by extending the struct, the parser, and the test matrix.

use std::{env, error::Error, fmt, str::FromStr};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Environment {
    Production,
    Staging,
    Development,
    Test,
}

impl FromStr for Environment {
    type Err = ConfigError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "production" => Ok(Self::Production),
            "staging" => Ok(Self::Staging),
            "development" => Ok(Self::Development),
            "test" => Ok(Self::Test),
            other => Err(ConfigError(format!(
                "invalid APP_ENVIRONMENT value '{other}': expected one of production, staging, development, test"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogFormat {
    Json,
    Pretty,
}

impl FromStr for LogFormat {
    type Err = ConfigError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "json" => Ok(Self::Json),
            "pretty" => Ok(Self::Pretty),
            other => Err(ConfigError(format!(
                "invalid LOG_FORMAT value '{other}': expected json or pretty"
            ))),
        }
    }
}

fn validate_origin(s: &str) -> Result<(), ConfigError> {
    if !s.starts_with("http://") && !s.starts_with("https://") {
        return Err(ConfigError(format!(
            "CORS origin '{s}' must start with http:// or https://"
        )));
    }
    let rest = s
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    if rest.is_empty() || !rest.contains('.') && !rest.contains("localhost") && !rest.contains(':')
    {
        return Err(ConfigError(format!(
            "CORS origin '{s}' does not contain a valid host"
        )));
    }
    Ok(())
}

#[derive(Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub database_url: String,
    pub redis_url: String,
    pub auth_jwt_secret: String,
    pub auth_session_ttl_seconds: u64,
    pub port: u16,
    pub bind_address: String,
    pub environment: Environment,
    pub cors_allowed_origins: Vec<String>,
    pub log_format: LogFormat,
    pub smtp_url: Option<String>,
    pub smtp_from: Option<String>,
    pub public_dashboard_url: String,
    pub ai_key_encryption_key: Option<String>,
    pub integration_secrets_key: Option<String>,
    pub ai_openai_base_url: Option<String>,
    pub ai_anthropic_base_url: Option<String>,
    pub ai_gemini_base_url: Option<String>,
    pub s3: Option<S3Config>,
    pub db_max_connections: u32,
    pub db_acquire_timeout_ms: u64,
    pub ready_probe_timeout_ms: u64,
    pub shutdown_grace_seconds: u64,
    /// When `true`, the interactive API documentation (`/swagger-ui`) and
    /// machine-readable spec (`/api-docs/openapi.json`) are served. In
    /// non-production environments the docs are always served regardless
    /// of this flag; in production the flag must be explicitly set to
    /// `true` to expose them (default `false` per FR-014).
    pub docs_enabled: bool,
}

impl fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let smtp_url = self.smtp_url.as_ref().map(|_| "[REDACTED]");
        let ai_key = self.ai_key_encryption_key.as_ref().map(|_| "[REDACTED]");
        let integration_secrets_key = self
            .integration_secrets_key
            .as_ref()
            .map(|_| "[REDACTED]");
        f.debug_struct("AppConfig")
            .field("database_url", &"[REDACTED]")
            .field("redis_url", &"[REDACTED]")
            .field("auth_jwt_secret", &"[REDACTED]")
            .field("auth_session_ttl_seconds", &self.auth_session_ttl_seconds)
            .field("port", &self.port)
            .field("bind_address", &self.bind_address)
            .field("environment", &self.environment)
            .field("cors_allowed_origins", &self.cors_allowed_origins)
            .field("log_format", &self.log_format)
            .field("db_max_connections", &self.db_max_connections)
            .field("db_acquire_timeout_ms", &self.db_acquire_timeout_ms)
            .field("ready_probe_timeout_ms", &self.ready_probe_timeout_ms)
            .field("docs_enabled", &self.docs_enabled)
            .field("smtp_url", &smtp_url)
            .field("smtp_from", &self.smtp_from)
            .field("public_dashboard_url", &self.public_dashboard_url)
            .field("shutdown_grace_seconds", &self.shutdown_grace_seconds)
            .field("ai_key_encryption_key", &ai_key)
            .field("integration_secrets_key", &integration_secrets_key)
            .field("ai_openai_base_url", &self.ai_openai_base_url)
            .field("ai_anthropic_base_url", &self.ai_anthropic_base_url)
            .field("ai_gemini_base_url", &self.ai_gemini_base_url)
            .field("s3", &self.s3)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct S3Config {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub force_path_style: bool,
}

impl std::fmt::Debug for S3Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Config")
            .field("endpoint", &self.endpoint)
            .field("region", &self.region)
            .field("bucket", &self.bucket)
            .field("access_key_id", &"[REDACTED]")
            .field("secret_access_key", &"[REDACTED]")
            .field("force_path_style", &self.force_path_style)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(String);
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for ConfigError {}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        fn required(name: &str) -> Result<String, ConfigError> {
            env::var(name).map_err(|_| {
                ConfigError(format!("required environment variable {name} is missing"))
            })
        }

        let environment: Environment = required("APP_ENVIRONMENT")?.parse()?;
        let port = env::var("PORT")
            .unwrap_or_else(|_| "8080".into())
            .parse()
            .map_err(|_| ConfigError("PORT must be a valid u16".into()))?;

        let bind_address = env::var("BIND_ADDRESS").unwrap_or_else(|_| "0.0.0.0".into());

        let cors_raw = required("CORS_ALLOWED_ORIGINS")?;
        let cors_allowed_origins: Vec<String> = cors_raw
            .split(',')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect();
        for origin in &cors_allowed_origins {
            validate_origin(origin)?;
        }
        if cors_allowed_origins.is_empty() && environment == Environment::Production {
            return Err(ConfigError(
                "CORS_ALLOWED_ORIGINS must contain at least one origin in production".into(),
            ));
        }

        let log_format = env::var("LOG_FORMAT")
            .ok()
            .map(|v| v.parse())
            .unwrap_or_else(|| {
                Ok(match environment {
                    Environment::Production | Environment::Staging => LogFormat::Json,
                    _ => LogFormat::Pretty,
                })
            })?;

        let db_max_connections = env::var("DB_MAX_CONNECTIONS")
            .unwrap_or_else(|_| "10".into())
            .parse()
            .map_err(|_| ConfigError("DB_MAX_CONNECTIONS must be a valid u32".into()))?;

        let db_acquire_timeout_ms = env::var("DB_ACQUIRE_TIMEOUT_MS")
            .unwrap_or_else(|_| "3000".into())
            .parse()
            .map_err(|_| ConfigError("DB_ACQUIRE_TIMEOUT_MS must be a valid u64".into()))?;

        let ready_probe_timeout_ms = env::var("READY_PROBE_TIMEOUT_MS")
            .unwrap_or_else(|_| "2000".into())
            .parse()
            .map_err(|_| ConfigError("READY_PROBE_TIMEOUT_MS must be a valid u64".into()))?;

        let shutdown_grace_seconds = env::var("SHUTDOWN_GRACE_SECONDS")
            .unwrap_or_else(|_| "10".into())
            .parse()
            .map_err(|_| ConfigError("SHUTDOWN_GRACE_SECONDS must be a valid u64".into()))?;

        let auth_jwt_secret = required("AUTH_JWT_SECRET")?;
        if auth_jwt_secret.len() < 32 {
            return Err(ConfigError(
                "AUTH_JWT_SECRET must be at least 32 bytes".into(),
            ));
        }

        let auth_session_ttl_seconds = env::var("AUTH_SESSION_TTL_SECONDS")
            .unwrap_or_else(|_| "28800".into())
            .parse()
            .map_err(|_| ConfigError("AUTH_SESSION_TTL_SECONDS must be a valid u64".into()))?;

        let smtp_url = env::var("SMTP_URL").ok();
        let smtp_from = env::var("SMTP_FROM").ok();

        let ai_key_encryption_key = match env::var("APP_AI_KEY_ENCRYPTION_KEY") {
            Ok(v) => {
                use base64::Engine;
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(&v)
                    .map_err(|_| {
                        ConfigError("APP_AI_KEY_ENCRYPTION_KEY must be valid base64".into())
                    })?;
                if decoded.len() != 32 {
                    return Err(ConfigError(
                        "APP_AI_KEY_ENCRYPTION_KEY must be a base64-encoded string of exactly 32 bytes".into(),
                    ));
                }
                Some(v)
            }
            Err(_) if environment == Environment::Test => None,
            Err(_) => {
                return Err(ConfigError(
                    "required environment variable APP_AI_KEY_ENCRYPTION_KEY is missing".into(),
                ));
            }
        };
        let integration_secrets_key = match env::var("APP_INTEGRATION_SECRETS_KEY") {
            Ok(v) => {
                use base64::Engine;
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(&v)
                    .map_err(|_| {
                        ConfigError("APP_INTEGRATION_SECRETS_KEY must be valid base64".into())
                    })?;
                if decoded.len() != 32 {
                    return Err(ConfigError(
                        "APP_INTEGRATION_SECRETS_KEY must be a base64-encoded string of exactly 32 bytes".into(),
                    ));
                }
                Some(v)
            }
            Err(_) => None,
        };
        let ai_openai_base_url = env::var("APP_AI_OPENAI_BASE_URL").ok();
        let ai_anthropic_base_url = env::var("APP_AI_ANTHROPIC_BASE_URL").ok();
        let ai_gemini_base_url = env::var("APP_AI_GEMINI_BASE_URL").ok();

        let s3_endpoint = env::var("APP_S3_ENDPOINT").ok();
        let s3_region = env::var("APP_S3_REGION").ok();
        let s3_bucket = env::var("APP_S3_BUCKET").ok();
        let s3_access_key_id = env::var("APP_S3_ACCESS_KEY_ID").ok();
        let s3_secret_access_key = env::var("APP_S3_SECRET_ACCESS_KEY").ok();

        let s3 = match (s3_endpoint, s3_region, s3_bucket, s3_access_key_id, s3_secret_access_key) {
            (Some(endpoint), Some(region), Some(bucket), Some(access_key_id), Some(secret_access_key)) => {
                Some(S3Config {
                    endpoint,
                    region,
                    bucket,
                    access_key_id,
                    secret_access_key,
                    force_path_style: env::var("APP_S3_FORCE_PATH_STYLE").map(|v| v == "true").unwrap_or(true),
                })
            }
            (None, None, None, None, None) => None,
            _ => return Err(ConfigError("APP_S3_* configuration is incomplete: set all of APP_S3_ENDPOINT, APP_S3_REGION, APP_S3_BUCKET, APP_S3_ACCESS_KEY_ID, APP_S3_SECRET_ACCESS_KEY, or none".into())),
        };

        let public_dashboard_url =
            env::var("PUBLIC_DASHBOARD_URL").unwrap_or_else(|_| match environment {
                Environment::Production | Environment::Staging => {
                    "https://dashboard.example.com".into()
                }
                Environment::Development | Environment::Test => "http://localhost:4200".into(),
            });
        validate_public_dashboard_url(&public_dashboard_url)?;

        let docs_enabled = env::var("APP_DOCS_ENABLED")
            .ok()
            .map(|v| match v.to_ascii_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => Ok(true),
                "0" | "false" | "no" | "off" => Ok(false),
                other => Err(ConfigError(format!(
                    "APP_DOCS_ENABLED must be a boolean, got '{other}'"
                ))),
            })
            .transpose()?
            .unwrap_or(false);

        Ok(Self {
            database_url: required("DATABASE_URL")?,
            redis_url: required("REDIS_URL")?,
            auth_jwt_secret,
            auth_session_ttl_seconds,
            smtp_url,
            smtp_from,
            public_dashboard_url,
            ai_key_encryption_key,
            integration_secrets_key,
            ai_openai_base_url,
            ai_anthropic_base_url,
            ai_gemini_base_url,
            s3,
            port,
            bind_address,
            environment,
            cors_allowed_origins,
            log_format,
            db_max_connections,
            db_acquire_timeout_ms,
            ready_probe_timeout_ms,
            shutdown_grace_seconds,
            docs_enabled,
        })
    }
}

fn validate_public_dashboard_url(url: &str) -> Result<(), ConfigError> {
    let parsed = Url::parse(url)
        .map_err(|e| ConfigError(format!("PUBLIC_DASHBOARD_URL must be a valid URL: {e}")))?;

    match parsed.scheme() {
        "http" | "https" => Ok(()),
        other => Err(ConfigError(format!(
            "PUBLIC_DASHBOARD_URL must use http or https, got {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EnvGuard;
    impl EnvGuard {
        fn setup(vars: &[(&str, &str)]) -> Self {
            for (k, v) in vars {
                env::set_var(k, v);
            }
            Self
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for key in &[
                "DATABASE_URL",
                "REDIS_URL",
                "APP_ENVIRONMENT",
                "PORT",
                "BIND_ADDRESS",
                "CORS_ALLOWED_ORIGINS",
                "LOG_FORMAT",
                "AUTH_JWT_SECRET",
                "AUTH_SESSION_TTL_SECONDS",
                "DB_MAX_CONNECTIONS",
                "DB_ACQUIRE_TIMEOUT_MS",
                "READY_PROBE_TIMEOUT_MS",
                "SHUTDOWN_GRACE_SECONDS",
                "APP_DOCS_ENABLED",
                "SMTP_URL",
                "SMTP_FROM",
                "PUBLIC_DASHBOARD_URL",
                "APP_AI_KEY_ENCRYPTION_KEY",
                "APP_INTEGRATION_SECRETS_KEY",
                "APP_AI_OPENAI_BASE_URL",
                "APP_AI_ANTHROPIC_BASE_URL",
                "APP_AI_GEMINI_BASE_URL",
                "APP_S3_ENDPOINT",
                "APP_S3_REGION",
                "APP_S3_BUCKET",
                "APP_S3_ACCESS_KEY_ID",
                "APP_S3_SECRET_ACCESS_KEY",
                "APP_S3_FORCE_PATH_STYLE",
            ] {
                env::remove_var(key);
            }
        }
    }

    #[test]
    #[serial_test::serial]
    fn missing_required_var_returns_error() {
        let _g = EnvGuard::setup(&[("DATABASE_URL", "postgres://localhost:5432/test")]);
        let err = AppConfig::from_env().unwrap_err();
        assert!(
            err.0.contains("REDIS_URL") || err.0.contains("APP_ENVIRONMENT"),
            "Expected error naming a missing variable, got: {err}"
        );
    }

    #[test]
    #[serial_test::serial]
    fn invalid_port_returns_error() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "development"),
            ("CORS_ALLOWED_ORIGINS", "http://localhost:4200"),
            ("PORT", "not_a_port"),
            (
                "APP_AI_KEY_ENCRYPTION_KEY",
                "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
            ),
        ]);
        let err = AppConfig::from_env().unwrap_err();
        assert!(err.0.contains("PORT"));
    }

    #[test]
    #[serial_test::serial]
    fn invalid_environment_returns_error() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "invalid"),
            ("CORS_ALLOWED_ORIGINS", "http://localhost:4200"),
        ]);
        let err = AppConfig::from_env().unwrap_err();
        assert!(err.0.contains("APP_ENVIRONMENT"));
    }

    #[test]
    #[serial_test::serial]
    fn invalid_cors_origin_returns_error() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "development"),
            ("CORS_ALLOWED_ORIGINS", "not-a-url"),
            (
                "APP_AI_KEY_ENCRYPTION_KEY",
                "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
            ),
        ]);
        let err = AppConfig::from_env().unwrap_err();
        assert!(err.0.contains("CORS"));
    }

    #[test]
    #[serial_test::serial]
    fn defaults_applied_for_optional_vars() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "development"),
            ("CORS_ALLOWED_ORIGINS", "http://localhost:4200"),
            ("AUTH_JWT_SECRET", "0123456789abcdef0123456789abcdef"),
            (
                "APP_AI_KEY_ENCRYPTION_KEY",
                "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
            ),
        ]);
        let config = AppConfig::from_env().unwrap();
        assert_eq!(config.port, 8080);
        assert_eq!(config.auth_session_ttl_seconds, 28_800);
        assert_eq!(config.db_max_connections, 10);
        assert_eq!(config.db_acquire_timeout_ms, 3000);
        assert_eq!(config.ready_probe_timeout_ms, 2000);
        assert_eq!(config.shutdown_grace_seconds, 10);
        assert_eq!(config.log_format, LogFormat::Pretty);
        assert_eq!(config.ai_openai_base_url, None);
        assert_eq!(config.ai_anthropic_base_url, None);
        assert_eq!(config.ai_gemini_base_url, None);
    }

    #[test]
    #[serial_test::serial]
    fn production_requires_non_empty_cors() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "production"),
            ("CORS_ALLOWED_ORIGINS", ""),
            (
                "APP_AI_KEY_ENCRYPTION_KEY",
                "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
            ),
        ]);
        let err = AppConfig::from_env().unwrap_err();
        assert!(err.0.contains("CORS_ALLOWED_ORIGINS") || err.0.contains("origin"));
    }

    #[test]
    #[serial_test::serial]
    fn debug_redacts_secrets() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://user:pass@localhost:5432/db"),
            ("REDIS_URL", "redis://user:pass@localhost:6379"),
            ("APP_ENVIRONMENT", "development"),
            ("CORS_ALLOWED_ORIGINS", "http://localhost:4200"),
            ("AUTH_JWT_SECRET", "0123456789abcdef0123456789abcdef"),
            ("SMTP_URL", "smtp://user:pass@smtp.example.com:587"),
            ("SMTP_FROM", "alerts@example.com"),
            (
                "APP_AI_KEY_ENCRYPTION_KEY",
                "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
            ),
        ]);
        let config = AppConfig::from_env().unwrap();
        let debug_str = format!("{config:?}");
        assert!(
            !debug_str.contains("user:pass"),
            "Debug output leaked secrets: {debug_str}"
        );
        assert!(
            debug_str.contains("[REDACTED]"),
            "Debug output should contain [REDACTED]: {debug_str}"
        );
        assert!(
            !debug_str.contains("0123456789abcdef0123456789abcdef"),
            "Debug output leaked JWT secret: {debug_str}"
        );
        assert!(
            !debug_str.contains("smtp://user:pass@smtp.example.com:587"),
            "Debug output leaked SMTP credentials: {debug_str}"
        );
        assert!(
            debug_str.contains("[REDACTED]"),
            "Debug output should contain a redacted SMTP URL: {debug_str}"
        );
    }

    #[test]
    #[serial_test::serial]
    fn invalid_public_dashboard_url_returns_error() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "development"),
            ("CORS_ALLOWED_ORIGINS", "http://localhost:4200"),
            ("AUTH_JWT_SECRET", "0123456789abcdef0123456789abcdef"),
            ("PUBLIC_DASHBOARD_URL", "ftp://example.com"),
            (
                "APP_AI_KEY_ENCRYPTION_KEY",
                "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
            ),
        ]);
        let err = AppConfig::from_env().unwrap_err();
        assert!(err.0.contains("PUBLIC_DASHBOARD_URL"));
        assert!(err.0.contains("http or https"));
    }

    #[test]
    #[serial_test::serial]
    fn missing_auth_jwt_secret_returns_error() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "development"),
            ("CORS_ALLOWED_ORIGINS", "http://localhost:4200"),
            (
                "APP_AI_KEY_ENCRYPTION_KEY",
                "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
            ),
        ]);
        let err = AppConfig::from_env().unwrap_err();
        assert!(err.0.contains("AUTH_JWT_SECRET"));
    }

    #[test]
    #[serial_test::serial]
    fn short_auth_jwt_secret_returns_error() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "development"),
            ("CORS_ALLOWED_ORIGINS", "http://localhost:4200"),
            ("AUTH_JWT_SECRET", "too-short"),
            (
                "APP_AI_KEY_ENCRYPTION_KEY",
                "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
            ),
        ]);
        let err = AppConfig::from_env().unwrap_err();
        assert!(err.0.contains("AUTH_JWT_SECRET"));
        assert!(err.0.contains("32"));
    }

    #[test]
    #[serial_test::serial]
    fn auth_session_ttl_seconds_can_be_overridden() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "development"),
            ("CORS_ALLOWED_ORIGINS", "http://localhost:4200"),
            ("AUTH_JWT_SECRET", "0123456789abcdef0123456789abcdef"),
            ("AUTH_SESSION_TTL_SECONDS", "3600"),
            (
                "APP_AI_KEY_ENCRYPTION_KEY",
                "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
            ),
        ]);
        let config = AppConfig::from_env().unwrap();
        assert_eq!(config.auth_session_ttl_seconds, 3600);
    }

    #[test]
    #[serial_test::serial]
    fn ai_key_encryption_key_rejects_non_32_byte_base64() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "development"),
            ("CORS_ALLOWED_ORIGINS", "http://localhost:4200"),
            ("AUTH_JWT_SECRET", "0123456789abcdef0123456789abcdef"),
            ("APP_AI_KEY_ENCRYPTION_KEY", "dG9vX3Nob3J0"), // base64 of "too_short" (9 bytes)
        ]);
        let err = AppConfig::from_env().unwrap_err();
        assert!(
            err.0.contains("32 bytes"),
            "Expected 32-byte validation error, got: {err}"
        );
    }

    #[test]
    #[serial_test::serial]
    fn invalid_auth_session_ttl_seconds_returns_error() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "development"),
            ("CORS_ALLOWED_ORIGINS", "http://localhost:4200"),
            ("AUTH_JWT_SECRET", "0123456789abcdef0123456789abcdef"),
            ("AUTH_SESSION_TTL_SECONDS", "not_a_number"),
            (
                "APP_AI_KEY_ENCRYPTION_KEY",
                "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
            ),
        ]);
        let err = AppConfig::from_env().unwrap_err();
        assert!(err.0.contains("AUTH_SESSION_TTL_SECONDS"));
    }

    #[test]
    #[serial_test::serial]
    fn docs_enabled_defaults_to_false() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "production"),
            ("CORS_ALLOWED_ORIGINS", "https://app.example.com"),
            ("AUTH_JWT_SECRET", "0123456789abcdef0123456789abcdef"),
            (
                "APP_AI_KEY_ENCRYPTION_KEY",
                "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
            ),
        ]);
        let config = AppConfig::from_env().unwrap();
        assert!(
            !config.docs_enabled,
            "docs_enabled should default to false when APP_DOCS_ENABLED is unset"
        );
    }

    #[test]
    #[serial_test::serial]
    fn docs_enabled_can_be_opted_in() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "production"),
            ("CORS_ALLOWED_ORIGINS", "https://app.example.com"),
            ("AUTH_JWT_SECRET", "0123456789abcdef0123456789abcdef"),
            ("APP_DOCS_ENABLED", "true"),
            (
                "APP_AI_KEY_ENCRYPTION_KEY",
                "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
            ),
        ]);
        let config = AppConfig::from_env().unwrap();
        assert!(config.docs_enabled);

        for value in ["false", "0", "no", "off", "FALSE", "No"] {
            env::set_var("APP_DOCS_ENABLED", value);
            let config = AppConfig::from_env().unwrap();
            assert!(
                !config.docs_enabled,
                "value '{value}' should parse to docs_enabled = false"
            );
        }
    }

    #[test]
    #[serial_test::serial]
    fn docs_enabled_rejects_non_boolean_values() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "production"),
            ("CORS_ALLOWED_ORIGINS", "https://app.example.com"),
            ("AUTH_JWT_SECRET", "0123456789abcdef0123456789abcdef"),
            ("APP_DOCS_ENABLED", "maybe"),
            (
                "APP_AI_KEY_ENCRYPTION_KEY",
                "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
            ),
        ]);
        let err = AppConfig::from_env().unwrap_err();
        assert!(err.0.contains("APP_DOCS_ENABLED"));
        assert!(err.0.contains("boolean"));
    }

    #[test]
    #[serial_test::serial]
    fn s3_all_unset_returns_none() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "development"),
            ("CORS_ALLOWED_ORIGINS", "http://localhost:4200"),
            ("AUTH_JWT_SECRET", "0123456789abcdef0123456789abcdef"),
            (
                "APP_AI_KEY_ENCRYPTION_KEY",
                "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
            ),
        ]);
        let config = AppConfig::from_env().unwrap();
        assert_eq!(config.s3, None);
    }

    #[test]
    #[serial_test::serial]
    fn s3_all_set_returns_populated_config() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "development"),
            ("CORS_ALLOWED_ORIGINS", "http://localhost:4200"),
            ("AUTH_JWT_SECRET", "0123456789abcdef0123456789abcdef"),
            (
                "APP_AI_KEY_ENCRYPTION_KEY",
                "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
            ),
            ("APP_S3_ENDPOINT", "https://s3.example.com"),
            ("APP_S3_REGION", "us-east-1"),
            ("APP_S3_BUCKET", "my-bucket"),
            ("APP_S3_ACCESS_KEY_ID", "AKID123"),
            ("APP_S3_SECRET_ACCESS_KEY", "secret123"),
        ]);
        let config = AppConfig::from_env().unwrap();
        let s3 = config.s3.expect("s3 should be Some");
        assert_eq!(s3.endpoint, "https://s3.example.com");
        assert_eq!(s3.region, "us-east-1");
        assert_eq!(s3.bucket, "my-bucket");
        assert_eq!(s3.access_key_id, "AKID123");
        assert_eq!(s3.secret_access_key, "secret123");
        assert!(s3.force_path_style);
    }

    #[test]
    #[serial_test::serial]
    fn s3_partial_set_returns_error() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "development"),
            ("CORS_ALLOWED_ORIGINS", "http://localhost:4200"),
            ("AUTH_JWT_SECRET", "0123456789abcdef0123456789abcdef"),
            (
                "APP_AI_KEY_ENCRYPTION_KEY",
                "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
            ),
            ("APP_S3_ENDPOINT", "https://s3.example.com"),
            ("APP_S3_REGION", "us-east-1"),
            // deliberately missing APP_S3_BUCKET, APP_S3_ACCESS_KEY_ID, APP_S3_SECRET_ACCESS_KEY
        ]);
        let err = AppConfig::from_env().unwrap_err();
        assert!(
            err.0.contains("APP_S3_*"),
            "Expected S3 partial error, got: {err}"
        );
    }

    #[test]
    #[serial_test::serial]
    fn s3_debug_redacts_secrets() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "development"),
            ("CORS_ALLOWED_ORIGINS", "http://localhost:4200"),
            ("AUTH_JWT_SECRET", "0123456789abcdef0123456789abcdef"),
            (
                "APP_AI_KEY_ENCRYPTION_KEY",
                "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=",
            ),
            ("APP_S3_ENDPOINT", "https://s3.example.com"),
            ("APP_S3_REGION", "us-east-1"),
            ("APP_S3_BUCKET", "my-bucket"),
            ("APP_S3_ACCESS_KEY_ID", "AKID123"),
            ("APP_S3_SECRET_ACCESS_KEY", "secret123"),
        ]);
        let config = AppConfig::from_env().unwrap();
        let debug_str = format!("{config:?}");
        assert!(
            !debug_str.contains("AKID123"),
            "Debug output leaked s3 access_key_id: {debug_str}"
        );
        assert!(
            !debug_str.contains("secret123"),
            "Debug output leaked s3 secret_access_key: {debug_str}"
        );
    }
}
