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
    pub port: u16,
    pub bind_address: String,
    pub environment: Environment,
    pub cors_allowed_origins: Vec<String>,
    pub log_format: LogFormat,
    pub db_max_connections: u32,
    pub db_acquire_timeout_ms: u64,
    pub ready_probe_timeout_ms: u64,
    pub shutdown_grace_seconds: u64,
}

impl fmt::Debug for AppConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppConfig")
            .field("database_url", &"[REDACTED]")
            .field("redis_url", &"[REDACTED]")
            .field("port", &self.port)
            .field("bind_address", &self.bind_address)
            .field("environment", &self.environment)
            .field("cors_allowed_origins", &self.cors_allowed_origins)
            .field("log_format", &self.log_format)
            .field("db_max_connections", &self.db_max_connections)
            .field("db_acquire_timeout_ms", &self.db_acquire_timeout_ms)
            .field("ready_probe_timeout_ms", &self.ready_probe_timeout_ms)
            .field("shutdown_grace_seconds", &self.shutdown_grace_seconds)
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

        Ok(Self {
            database_url: required("DATABASE_URL")?,
            redis_url: required("REDIS_URL")?,
            port,
            bind_address,
            environment,
            cors_allowed_origins,
            log_format,
            db_max_connections,
            db_acquire_timeout_ms,
            ready_probe_timeout_ms,
            shutdown_grace_seconds,
        })
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
                "DB_MAX_CONNECTIONS",
                "DB_ACQUIRE_TIMEOUT_MS",
                "READY_PROBE_TIMEOUT_MS",
                "SHUTDOWN_GRACE_SECONDS",
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
        ]);
        let config = AppConfig::from_env().unwrap();
        assert_eq!(config.port, 8080);
        assert_eq!(config.db_max_connections, 10);
        assert_eq!(config.db_acquire_timeout_ms, 3000);
        assert_eq!(config.ready_probe_timeout_ms, 2000);
        assert_eq!(config.shutdown_grace_seconds, 10);
        assert_eq!(config.log_format, LogFormat::Pretty);
    }

    #[test]
    #[serial_test::serial]
    fn production_requires_non_empty_cors() {
        let _g = EnvGuard::setup(&[
            ("DATABASE_URL", "postgres://localhost:5432/test"),
            ("REDIS_URL", "redis://localhost:6379"),
            ("APP_ENVIRONMENT", "production"),
            ("CORS_ALLOWED_ORIGINS", ""),
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
    }
}
