use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use kernel::{Budget, Decision, InMemoryRateLimitStore, RateLimiter, Scope};
use redis::aio::ConnectionManager;
use sha2::{Digest, Sha256};
use tracing::warn;

use crate::Cache;

/// What a sign-in refused because the counter store is down tells the client to wait
/// (`contracts/rate-limit-responses.md`, store-unavailable behaviour).
const FAIL_CLOSED_RETRY_AFTER: Duration = Duration::from_secs(30);

/// How long a check waits to reach the counter store before it treats the store as down. The
/// connection manager's own retries take about nine seconds to give up, and a sign-in would wait
/// that long for its answer; this keeps an outage a fraction of a second slower than usual.
/// It matches the 500 ms the manager already allows for each command. It limits how long a check
/// waits, not how long the connection attempt may take: see `RedisRateLimiter::connection`.
const STORE_CONNECT_TIMEOUT: Duration = Duration::from_millis(500);

fn hash_identifier(identifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(identifier.as_bytes());
    hex::encode(hasher.finalize())
}

/// Select the identifier to key a rate-limit budget on.
///
/// When the caller is authenticated, key on the authenticated identity
/// (user ID for user-scoped budgets, tenant ID for tenant-scoped ones).
/// When anonymous, key on the source address.
///
/// This rule exists in one place (FR-004) so every call site applies the
/// same logic: many users behind one corporate NAT share a source address,
/// and keying an authenticated budget on it locks all of them out together.
pub fn select_identifier(authenticated_identity: Option<&str>, source_address: &str) -> String {
    authenticated_identity
        .filter(|id| !id.is_empty())
        .unwrap_or(source_address)
        .to_string()
}

/// Redis-backed rate limiter with sliding-window boundary correction.
///
/// Key format: `rl:{scope}:{identifier}:{window_epoch}`
///
/// Uses pipelined `INCR` + `EXPIRE` for the current window and reads the
/// previous window's count for a weighted correction that smooths the
/// boundary burst a naive fixed window allows.
///
/// On Redis errors, `Scope::Login` returns `Decision::Unavailable` (fail-closed, answered
/// `503`) unless the fail mode is `open`, and all other scopes degrade to an in-process
/// `InMemoryRateLimitStore` (fail-open for non-auth traffic). See `research.md` §2 and
/// `contracts/rate-limit-responses.md` (store-unavailable behaviour).
pub struct RedisRateLimiter {
    // Behind an `Arc` because the task that connects shares it (see `connection`); a clone of
    // `Cache` would carry a separate connection of its own.
    cache: Arc<Cache>,
    // Set while a task owned by this limiter is connecting.
    connecting: Arc<AtomicBool>,
    // How long a check waits for the connection; `STORE_CONNECT_TIMEOUT` outside the tests.
    connect_wait: Duration,
    fallback: InMemoryRateLimitStore,
    degraded_total: AtomicU64,
    refused_total: AtomicU64,
    fail_mode: String,
}

impl RedisRateLimiter {
    pub fn new(cache: Cache, fail_mode: String) -> Self {
        Self {
            cache: Arc::new(cache),
            connecting: Arc::new(AtomicBool::new(false)),
            connect_wait: STORE_CONNECT_TIMEOUT,
            fallback: InMemoryRateLimitStore::default(),
            degraded_total: AtomicU64::new(0),
            refused_total: AtomicU64::new(0),
            fail_mode,
        }
    }

    pub fn degraded_count(&self) -> u64 {
        self.degraded_total.load(Ordering::Relaxed)
    }

    pub fn refused_count(&self) -> u64 {
        self.refused_total.load(Ordering::Relaxed)
    }

    /// Reaches the counter store, waiting at most `connect_wait` ([`STORE_CONNECT_TIMEOUT`]).
    ///
    /// The connection attempt belongs to a task this limiter starts, not to the request. If it
    /// belonged to the request, the request giving up would drop the attempt with it, and a store
    /// that needs longer than the limit to answer would never finish connecting: sign-in would
    /// stay refused although the store is up. Here the attempt carries on, and a later check
    /// finds the connection ready. At most one such task runs at a time, and a request waits on
    /// the same attempt instead of starting another.
    async fn connection(&self) -> Result<ConnectionManager, String> {
        if !self.cache.is_connected() && !self.connecting.swap(true, Ordering::AcqRel) {
            let cache = Arc::clone(&self.cache);
            let connecting = Arc::clone(&self.connecting);
            tokio::spawn(async move {
                // A failure reaches whoever waits on the same attempt; this task only keeps the
                // attempt alive, so there is nothing to do with the result here.
                let _ = cache.manager().await;
                connecting.store(false, Ordering::Release);
            });
        }
        match tokio::time::timeout(self.connect_wait, self.cache.manager()).await {
            Ok(Ok(manager)) => Ok(manager.clone()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(format!("no connection within {:?}", self.connect_wait)),
        }
    }

    async fn degrade(&self, scope: Scope, identifier: &str, budget: Budget) -> Decision {
        self.degraded_total.fetch_add(1, Ordering::Relaxed);
        metrics::counter!("rate_limit_degraded_total", "scope" => scope.as_str()).increment(1);
        warn!(
            event = "rate_limit_degraded",
            scope = scope.as_str(),
            identifier_hash = %hash_identifier(identifier),
            "rate limit degraded",
        );

        match scope {
            Scope::Login if self.fail_mode != "open" => Decision::Unavailable {
                retry_after: FAIL_CLOSED_RETRY_AFTER,
            },
            _ => RateLimiter::check(&self.fallback, scope, identifier, budget).await,
        }
    }
}

#[async_trait::async_trait]
impl RateLimiter for RedisRateLimiter {
    async fn check(&self, scope: Scope, identifier: &str, budget: Budget) -> Decision {
        let window_secs = budget.window.as_secs();
        if window_secs == 0 {
            return Decision::Refused {
                retry_after: Duration::from_secs(1),
            };
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let epoch = now / window_secs;
        let prev_epoch = epoch.saturating_sub(1);
        let elapsed = now % window_secs;

        let current_key = format!("rl:{}:{}:{}", scope.as_str(), identifier, epoch);
        let prev_key = format!("rl:{}:{}:{}", scope.as_str(), identifier, prev_epoch);
        let ttl = (window_secs * 2) as i64;

        let mut conn = match self.connection().await {
            Ok(manager) => manager,
            Err(e) => {
                warn!("RedisRateLimiter: connection error: {e}");
                return self.degrade(scope, identifier, budget).await;
            }
        };

        // A pipeline replies with an array even when only one command's result
        // is kept, so decode a 1-tuple. Decoding straight into i64 fails on
        // redis-rs 1.x, which sent every check down the degraded path — and in
        // the default `auth_closed` mode that refused every sign-in with 429.
        let current_count: i64 = match redis::pipe()
            .atomic()
            .cmd("INCR")
            .arg(&current_key)
            .cmd("EXPIRE")
            .arg(&current_key)
            .arg(ttl)
            .ignore()
            .query_async::<(i64,)>(&mut conn)
            .await
        {
            Ok((c,)) => c,
            Err(e) => {
                warn!("RedisRateLimiter: INCR error: {e}");
                return self.degrade(scope, identifier, budget).await;
            }
        };

        // A failed read of the previous window is a store error, not an empty window: counting it
        // as zero would let a sign-in through that the weighted count should refuse. `INCR` may
        // already have committed, so it is not repeated.
        let prev_count: u32 = match previous_window_count(
            redis::cmd("GET")
                .arg(&prev_key)
                .query_async::<Option<u32>>(&mut conn)
                .await,
        ) {
            Ok(n) => n,
            Err(e) => {
                warn!("RedisRateLimiter: previous-window read error: {e}");
                return self.degrade(scope, identifier, budget).await;
            }
        };

        let weighted = sliding_window_count(current_count, prev_count, elapsed, window_secs);
        let current_u32 = current_count.max(0) as u32;
        let remaining_secs = window_secs.saturating_sub(elapsed);

        if weighted <= budget.max_requests as f64 {
            Decision::Allowed {
                remaining: budget.max_requests.saturating_sub(current_u32),
                reset_after: Duration::from_secs(remaining_secs),
            }
        } else {
            self.refused_total.fetch_add(1, Ordering::Relaxed);
            metrics::counter!("rate_limit_refusals_total", "scope" => scope.as_str()).increment(1);
            warn!(
                event = "rate_limit_refused",
                scope = scope.as_str(),
                identifier_hash = %hash_identifier(identifier),
                budget.max_requests = budget.max_requests,
                budget.window_secs = budget.window.as_secs(),
                "rate limit refused",
            );
            Decision::Refused {
                retry_after: Duration::from_secs(remaining_secs),
            }
        }
    }
}

/// Resolves a rate-limit budget using the 3-tier resolution order:
///
/// 1. **Override row** — per-tenant override from `tenant_rate_limit_overrides`
///    (supplied by the caller via the `fetcher` callback), capped at
///    `max_override`.
/// 2. **Environment config** — `APP_RATE_LIMIT_*` variables (pre-configured
///    defaults).
/// 3. **Compiled default** — hard-coded values (last resort).
///
/// Includes a **60-second in-process cache** keyed by `tenant_id:scope` so
/// override lookups do not hit the database on every request.
pub struct BudgetResolver {
    inner: Mutex<BudgetResolverInner>,
    max_override: u32,
}

struct CachedOverride {
    budget: Option<Budget>,
    expires_at: Instant,
}

struct BudgetResolverInner {
    cache: HashMap<String, CachedOverride>,
    defaults: HashMap<Scope, Budget>,
}

impl BudgetResolver {
    pub fn new(max_override: u32) -> Self {
        let mut defaults = HashMap::new();
        defaults.insert(
            Scope::Login,
            Budget {
                max_requests: 10,
                window: Duration::from_secs(60),
            },
        );
        defaults.insert(
            Scope::WidgetSession,
            Budget {
                max_requests: 10,
                window: Duration::from_secs(60),
            },
        );
        defaults.insert(
            Scope::WidgetMessage,
            Budget {
                max_requests: 10,
                window: Duration::from_secs(60),
            },
        );
        defaults.insert(
            Scope::Webhook,
            Budget {
                max_requests: 600,
                window: Duration::from_secs(60),
            },
        );
        defaults.insert(
            Scope::TenantGlobal,
            Budget {
                max_requests: 600,
                window: Duration::from_secs(60),
            },
        );

        Self {
            inner: Mutex::new(BudgetResolverInner {
                cache: HashMap::new(),
                defaults,
            }),
            max_override,
        }
    }

    pub fn update_default(&self, scope: Scope, budget: Budget) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.defaults.insert(scope, budget);
        }
    }

    fn default_budget(&self, scope: Scope) -> Budget {
        self.inner
            .lock()
            .ok()
            .and_then(|inner| inner.defaults.get(&scope).copied())
            .unwrap_or(Budget {
                max_requests: 10,
                window: Duration::from_secs(60),
            })
    }

    pub fn invalidate_tenant(&self, tenant_id: &str) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.cache.retain(|key, _| !key.starts_with(tenant_id));
        }
    }

    pub fn invalidate_all(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.cache.clear();
        }
    }

    pub async fn resolve<F, Fut>(&self, tenant_id: &str, scope: Scope, fetcher: F) -> Budget
    where
        F: FnOnce(String, Scope) -> Fut,
        Fut: std::future::Future<Output = Option<Budget>>,
    {
        let cache_key = format!("{}:{}", tenant_id, scope.as_str());

        {
            if let Ok(inner) = self.inner.lock() {
                if let Some(entry) = inner.cache.get(&cache_key) {
                    if entry.expires_at > Instant::now() {
                        return match entry.budget {
                            Some(b) => b,
                            None => *inner.defaults.get(&scope).unwrap_or(&Budget {
                                max_requests: 10,
                                window: Duration::from_secs(60),
                            }),
                        };
                    }
                }
            }
        }

        let override_budget = fetcher(tenant_id.to_string(), scope).await;

        let resolved = match override_budget {
            Some(b) => Budget {
                max_requests: b.max_requests.min(self.max_override),
                window: b.window,
            },
            None => self.default_budget(scope),
        };

        if let Ok(mut inner) = self.inner.lock() {
            inner.cache.insert(
                cache_key,
                CachedOverride {
                    budget: override_budget,
                    expires_at: Instant::now() + Duration::from_secs(60),
                },
            );
        }

        resolved
    }
}

/// Sliding-window estimate: the current window's count plus the share of the
/// previous window that still overlaps the trailing `window_secs`. Right
/// after a boundary the previous window counts in full, fading to nothing by
/// the end of the current window — so a burst cannot reset at a boundary.
/// The previous window's count from its `GET` reply: a missing key is an empty window (zero), and
/// a failed read stays an error for the caller to treat as a store failure.
fn previous_window_count<E>(reply: Result<Option<u32>, E>) -> Result<u32, E> {
    reply.map(|count| count.unwrap_or(0))
}

fn sliding_window_count(current: i64, previous: u32, elapsed: u64, window_secs: u64) -> f64 {
    let overlap = 1.0 - (elapsed as f64 / window_secs as f64);
    current as f64 + previous as f64 * overlap
}

#[cfg(test)]
mod tests {
    use super::sliding_window_count;

    #[test]
    fn previous_window_counts_in_full_right_after_a_boundary() {
        // 10 attempts just before the boundary, 1 just after: still 11.
        assert_eq!(sliding_window_count(1, 10, 0, 60), 11.0);
    }

    #[test]
    fn previous_window_fades_out_across_the_current_window() {
        assert_eq!(sliding_window_count(1, 10, 30, 60), 6.0);
        assert!((sliding_window_count(1, 10, 59, 60) - (1.0 + 10.0 / 60.0)).abs() < 1e-9);
    }

    use super::*;

    #[test]
    fn select_identifier_returns_identity_when_authenticated() {
        let result = select_identifier(Some("user-42"), "203.0.113.5");
        assert_eq!(result, "user-42");
    }

    #[test]
    fn select_identifier_returns_source_when_anonymous() {
        let result = select_identifier(None, "203.0.113.5");
        assert_eq!(result, "203.0.113.5");
    }

    #[test]
    fn select_identifier_returns_source_when_identity_empty() {
        let result = select_identifier(Some(""), "203.0.113.5");
        assert_eq!(result, "203.0.113.5");
    }

    #[test]
    fn different_authenticated_identities_same_source_get_independent_budgets() {
        let id_a = select_identifier(Some("user-a"), "10.0.0.1");
        let id_b = select_identifier(Some("user-b"), "10.0.0.1");
        assert_eq!(id_a, "user-a");
        assert_eq!(id_b, "user-b");
        assert_ne!(id_a, id_b);
    }

    /// A limiter whose Redis is an address nothing listens on, so every check fails to connect.
    /// It needs no running service, which is the point: these tests prove what happens when the
    /// store is down.
    fn limiter_with_redis_down(fail_mode: &str) -> RedisRateLimiter {
        let cache = Cache::new("redis://127.0.0.1:1").expect("a well-formed Redis URL");
        RedisRateLimiter::new(cache, fail_mode.into())
    }

    fn small_budget() -> Budget {
        Budget {
            max_requests: 2,
            window: Duration::from_secs(60),
        }
    }

    #[tokio::test]
    async fn login_with_the_store_down_is_unavailable_in_auth_closed_mode() {
        let limiter = limiter_with_redis_down("auth_closed");
        match limiter
            .check(Scope::Login, "a@example.com:10.0.0.1", small_budget())
            .await
        {
            Decision::Unavailable { retry_after } => {
                assert_eq!(retry_after, Duration::from_secs(30));
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
        assert_eq!(limiter.degraded_count(), 1);
        assert_eq!(
            limiter.refused_count(),
            0,
            "an outage is not a budget refusal"
        );
    }

    #[tokio::test]
    async fn any_fail_mode_other_than_open_fails_closed_for_login() {
        // The documented values are `auth_closed` and `open`; a typo must not open the door.
        let limiter = limiter_with_redis_down("pass_open");
        assert!(matches!(
            limiter
                .check(Scope::Login, "a@example.com:10.0.0.1", small_budget())
                .await,
            Decision::Unavailable { .. }
        ));
    }

    #[tokio::test]
    async fn login_with_the_store_down_counts_locally_in_open_mode() {
        let limiter = limiter_with_redis_down("open");
        let id = "a@example.com:10.0.0.1";
        for _ in 0..2 {
            assert!(matches!(
                limiter.check(Scope::Login, id, small_budget()).await,
                Decision::Allowed { .. }
            ));
        }
        // The local counter still limits: `open` does not mean unlimited.
        assert!(matches!(
            limiter.check(Scope::Login, id, small_budget()).await,
            Decision::Refused { .. }
        ));
    }

    #[tokio::test]
    async fn other_scopes_count_locally_with_the_store_down() {
        let limiter = limiter_with_redis_down("auth_closed");
        for _ in 0..2 {
            assert!(matches!(
                limiter
                    .check(Scope::Webhook, "conn-1", small_budget())
                    .await,
                Decision::Allowed { .. }
            ));
        }
        assert!(matches!(
            limiter
                .check(Scope::Webhook, "conn-1", small_budget())
                .await,
            Decision::Refused { .. }
        ));
    }

    #[test]
    fn a_missing_previous_window_is_zero_and_a_failed_read_stays_an_error() {
        assert_eq!(previous_window_count::<&str>(Ok(None)), Ok(0));
        assert_eq!(previous_window_count::<&str>(Ok(Some(7))), Ok(7));
        // Counting a failed read as zero would let a sign-in through that the weighted count
        // should refuse.
        assert_eq!(
            previous_window_count(Err("connection reset")),
            Err("connection reset")
        );
    }

    /// Listens on a local port and forwards every connection to the real Redis, but holds back
    /// the store's first reply by `delay`: a store that is slow to answer, not down. The redis
    /// client allows 500 ms per reply and a second to connect, so `delay` must stay below that
    /// for the connection to succeed at all. Returns the proxy's `redis://` URL.
    async fn redis_that_answers_slowly(real_url: &str, delay: Duration) -> String {
        let real_addr = real_url
            .trim_start_matches("redis://")
            .trim_end_matches('/')
            .to_owned();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind a local port");
        let url = format!("redis://{}", listener.local_addr().expect("local address"));
        tokio::spawn(async move {
            while let Ok((inbound, _)) = listener.accept().await {
                let real_addr = real_addr.clone();
                tokio::spawn(async move {
                    let Ok(outbound) = tokio::net::TcpStream::connect(real_addr).await else {
                        return;
                    };
                    let (mut from_client, mut to_client) = inbound.into_split();
                    let (mut from_store, mut to_store) = outbound.into_split();
                    tokio::spawn(async move {
                        let _ = tokio::io::copy(&mut from_client, &mut to_store).await;
                    });
                    tokio::time::sleep(delay).await;
                    let _ = tokio::io::copy(&mut from_store, &mut to_client).await;
                });
            }
        });
        url
    }

    /// A check gives up waiting after `connect_wait`, but the connection attempt must not be
    /// abandoned with it. If it were, a store that needs longer than the wait to answer would
    /// never finish connecting, and sign-in would stay refused while the store is up.
    #[tokio::test]
    async fn a_store_slower_than_the_wait_still_finishes_connecting_for_later_checks() {
        if std::env::var("REQUIRE_DB_TESTS").as_deref() != Ok("1") {
            return;
        }
        let real = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".into());
        // The store answers after 300 ms; the check waits 100 ms.
        let url = redis_that_answers_slowly(&real, Duration::from_millis(300)).await;
        let mut limiter = RedisRateLimiter::new(
            Cache::new(&url).expect("a well-formed URL"),
            "auth_closed".into(),
        );
        limiter.connect_wait = Duration::from_millis(100);
        let id = format!("slow-connect-{}", uuid::Uuid::new_v4());

        // The first check gives up before the store has answered.
        assert!(matches!(
            limiter.check(Scope::Login, &id, small_budget()).await,
            Decision::Unavailable { .. }
        ));

        // The attempt carried on. Given time to finish, the next check reaches the store.
        tokio::time::sleep(Duration::from_millis(1000)).await;
        assert!(
            matches!(
                limiter.check(Scope::Login, &id, small_budget()).await,
                Decision::Allowed { .. }
            ),
            "the connection attempt must survive the request that stopped waiting for it"
        );
    }

    async fn require_redis() -> Option<RedisRateLimiter> {
        if std::env::var("REQUIRE_DB_TESTS").as_deref() != Ok("1") {
            return None;
        }
        let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".into());
        let cache = Cache::new(&url).ok()?;
        if cache.ping().await.is_err() {
            panic!("REQUIRE_DB_TESTS=1 but Redis is unreachable at {url}");
        }
        Some(RedisRateLimiter::new(cache, "auth_closed".into()))
    }

    #[tokio::test]
    async fn allows_up_to_limit() {
        let Some(limiter) = require_redis().await else {
            return;
        };
        let budget = Budget {
            max_requests: 10,
            window: Duration::from_secs(60),
        };
        let id = &format!("allows_up_to_limit:{}", uuid::Uuid::new_v4());
        for i in 0..10 {
            match limiter.check(Scope::Login, id, budget).await {
                Decision::Allowed { .. } => {}
                other => panic!("request {i} should be allowed, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn refuses_past_limit() {
        let Some(limiter) = require_redis().await else {
            return;
        };
        let budget = Budget {
            max_requests: 10,
            window: Duration::from_secs(60),
        };
        let id = &format!("refuses_past_limit:{}", uuid::Uuid::new_v4());
        for i in 0..10 {
            match limiter.check(Scope::Login, id, budget).await {
                Decision::Allowed { .. } => {}
                other => panic!("request {i} should be allowed, got {other:?}"),
            }
        }
        let decision = limiter.check(Scope::Login, id, budget).await;
        assert!(
            matches!(decision, Decision::Refused { .. }),
            "11th request should be refused, got {decision:?}"
        );
    }

    #[tokio::test]
    async fn resets_after_window() {
        let Some(limiter) = require_redis().await else {
            return;
        };
        let budget = Budget {
            max_requests: 3,
            window: Duration::from_secs(1),
        };
        let id = &format!("resets_after_window:{}", uuid::Uuid::new_v4());
        for i in 0..3 {
            match limiter.check(Scope::Login, id, budget).await {
                Decision::Allowed { .. } => {}
                other => panic!("request {i} should be allowed, got {other:?}"),
            }
        }
        let decision = limiter.check(Scope::Login, id, budget).await;
        assert!(
            matches!(decision, Decision::Refused { .. }),
            "4th request should be refused, got {decision:?}"
        );
        // Sliding window: the previous window still weighs in until it has
        // fully aged out, so wait two whole windows for a clean slate.
        tokio::time::sleep(Duration::from_millis(2100)).await;
        match limiter.check(Scope::Login, id, budget).await {
            Decision::Allowed { .. } => {}
            other => panic!("request after window should be allowed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn different_keys_independent() {
        let Some(limiter) = require_redis().await else {
            return;
        };
        let budget = Budget {
            max_requests: 5,
            window: Duration::from_secs(60),
        };
        let run = uuid::Uuid::new_v4();
        let id_a = &format!("different_keys:{run}:tenant-a");
        let id_b = &format!("different_keys:{run}:tenant-b");
        for _ in 0..5 {
            limiter.check(Scope::Login, id_a, budget).await;
        }
        let decision_a = limiter.check(Scope::Login, id_a, budget).await;
        assert!(
            matches!(decision_a, Decision::Refused { .. }),
            "tenant-a should be refused, got {decision_a:?}"
        );
        match limiter.check(Scope::Login, id_b, budget).await {
            Decision::Allowed { .. } => {}
            other => panic!("tenant-b should still be allowed, got {other:?}"),
        }
    }
}
