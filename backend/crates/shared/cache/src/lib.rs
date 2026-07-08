//! Cache — Redis client wrapper
//!
//! # Purpose
//! Isolates the `redis` crate behind a minimal public API so module crates
//! never import `redis` types directly. Provides lazy connection management
//! and a readiness probe (`ping`).
//!
//! # Public Interfaces
//! - `Cache::new(&str)` — construct without I/O
//! - `Cache::ping()` — async health probe
//! - `RedisHealthCheck` — implements `HealthCheck` for `/ready`
//!
//! # Dependencies
//! - `redis` (tokio-comp + connection-manager)
//!
//! # Extension Points
//! - Future modules can add methods (e.g. `get`/`set`) without changing
//!   callers' dependency on `redis`.

use redis::{aio::ConnectionManager, Client};
use tokio::sync::OnceCell;

#[derive(Clone)]
pub struct Cache {
    client: Client,
    manager: OnceCell<ConnectionManager>,
}

impl Cache {
    pub fn new(redis_url: &str) -> Result<Self, String> {
        let client =
            Client::open(redis_url.to_owned()).map_err(|e| format!("redis client error: {e}"))?;
        Ok(Self {
            client,
            manager: OnceCell::new(),
        })
    }

    async fn manager(&self) -> Result<&ConnectionManager, String> {
        self.manager
            .get_or_try_init(|| async {
                self.client
                    .get_connection_manager()
                    .await
                    .map_err(|e| format!("connection manager error: {e}"))
            })
            .await
    }

    pub async fn ping(&self) -> Result<(), String> {
        let mut mgr = self.manager().await?.clone();
        redis::cmd("PING")
            .query_async(&mut mgr)
            .await
            .map_err(|e| format!("cache error: {e}"))
    }
}

pub struct RedisHealthCheck {
    cache: Cache,
}

impl RedisHealthCheck {
    pub fn new(cache: Cache) -> Self {
        Self { cache }
    }
}

#[async_trait::async_trait]
impl observability::health::HealthCheck for RedisHealthCheck {
    fn name(&self) -> &'static str {
        "cache"
    }

    async fn check(&self) -> Result<(), String> {
        self.cache.ping().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_with_unreachable_url_succeeds() {
        let cache = Cache::new("redis://unreachable:6379");
        assert!(cache.is_ok());
    }
}
