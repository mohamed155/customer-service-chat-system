use std::time::Duration;

use cache::limiter::RedisRateLimiter;
use cache::Cache;
use kernel::{Budget, Decision, RateLimiter, Scope};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

fn redis_url() -> String {
    std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".into())
}

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn try_cache() -> Option<Cache> {
    let url = redis_url();
    let cache = match Cache::new(&url) {
        Ok(c) => c,
        Err(e) => {
            assert!(
                !require_db_tests(),
                "REQUIRE_DB_TESTS=1 but a Redis client could not be created for {url}: {e}"
            );
            return None;
        }
    };
    let reachable = tokio::time::timeout(CONNECT_TIMEOUT, cache.ping())
        .await
        .map(|r| r.is_ok())
        .unwrap_or(false);
    if !reachable {
        assert!(
            !require_db_tests(),
            "REQUIRE_DB_TESTS=1 but Redis is unreachable at {url}"
        );
        eprintln!("Redis unreachable at {url} - skipping shared_budget_two_instances");
        return None;
    }
    Some(cache)
}

#[tokio::test]
async fn shared_budget_two_instances() {
    let Some(cache) = try_cache().await else {
        eprintln!(
            "Redis unreachable at {} — skipping shared_budget_two_instances",
            redis_url()
        );
        return;
    };
    let limiter_a = RedisRateLimiter::new(cache.clone(), "auth_closed".into());
    let limiter_b = RedisRateLimiter::new(cache, "auth_closed".into());

    let budget = Budget {
        max_requests: 5,
        window: Duration::from_secs(60),
    };
    let identifier = "shared_budget_two_instances:key";
    let scope = Scope::Login;

    for i in 0..5 {
        let limiter = if i % 2 == 0 { &limiter_a } else { &limiter_b };
        match limiter.check(scope, identifier, budget).await {
            Decision::Allowed { .. } => {}
            other => panic!("request {i} should be allowed, got {other:?}"),
        }
    }

    let decision = limiter_b.check(scope, identifier, budget).await;
    assert!(
        matches!(decision, Decision::Refused { .. }),
        "6th request should be refused (shared budget exhausted), got {decision:?}"
    );
}
