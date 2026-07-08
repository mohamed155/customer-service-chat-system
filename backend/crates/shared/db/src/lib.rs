//! DB — PostgreSQL connection pool and health check
//!
//! # Purpose
//! Centralise SQLx pool construction and expose a lazy pool that never blocks
//! startup on an unreachable database (FR-008a). Also provides `PgHealthCheck`
//! for the `/ready` endpoint.
//!
//! # Public Interfaces
//! - `lazy_pool(&str, u32, Duration) -> PgPool`
//! - `run_migrations(&pool)`
//! - `PgHealthCheck`
//!
//! # Dependencies
//! - `sqlx` (postgres, runtime-tokio-rustls)
//!
//! # Extension Points
//! - `PgHealthCheck` is one of many possible `HealthCheck` implementations.

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::str::FromStr;
use std::time::Duration;

pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../../migrations").run(pool).await
}

pub fn lazy_pool(database_url: &str, max_connections: u32, acquire_timeout: Duration) -> PgPool {
    let connect_opts = PgConnectOptions::from_str(database_url).expect("invalid database URL");
    PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(acquire_timeout)
        .connect_lazy_with(connect_opts)
}

pub struct PgHealthCheck {
    pool: PgPool,
}

impl PgHealthCheck {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl observability::health::HealthCheck for PgHealthCheck {
    fn name(&self) -> &'static str {
        "database"
    }

    async fn check(&self) -> Result<(), String> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|e| format!("database error: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lazy_pool_construction_succeeds_with_unreachable_url() {
        let pool = lazy_pool(
            "postgres://unreachable:5432/test",
            1,
            Duration::from_secs(1),
        );
        assert!(std::mem::size_of_val(&pool) > 0);
    }
}
