use std::{env, error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub database_url: String,
    pub redis_url: String,
    pub port: u16,
    pub environment: String,
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
        let port = env::var("PORT")
            .unwrap_or_else(|_| "8080".into())
            .parse()
            .map_err(|_| ConfigError("PORT must be a valid u16".into()))?;
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            redis_url: required("REDIS_URL")?,
            port,
            environment: required("APP_ENVIRONMENT")?,
        })
    }
}
