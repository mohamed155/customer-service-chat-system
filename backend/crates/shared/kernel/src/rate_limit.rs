use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A bucket: the count so far, when its window started and how long that window is. Each bucket
/// carries its own window because scopes have different ones (a minute for most, ten for the
/// frontend error counter): a check for one scope must never expire another scope's bucket early.
type Bucket = (u32, Instant, Duration);

pub struct InMemoryRateLimitStore {
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl InMemoryRateLimitStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for InMemoryRateLimitStore {
    fn default() -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, strum::Display, strum::EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum Scope {
    Login,
    Mfa,
    MfaEnrol,
    /// Error reports sent by the dashboard (MOH-168), per signed-in user.
    ClientErrors,
    /// What one user may add to the frontend error counter (MOH-168).
    ClientErrorsCounted,
    WidgetSession,
    WidgetMessage,
    Webhook,
    TenantGlobal,
}

impl Scope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Mfa => "mfa",
            Self::MfaEnrol => "mfa_enrol",
            Self::ClientErrors => "client_errors",
            Self::ClientErrorsCounted => "client_errors_counted",
            Self::WidgetSession => "widget_session",
            Self::WidgetMessage => "widget_message",
            Self::Webhook => "webhook",
            Self::TenantGlobal => "tenant_global",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub max_requests: u32,
    pub window: Duration,
}

#[derive(Clone, Debug)]
pub enum Decision {
    Allowed {
        remaining: u32,
        reset_after: Duration,
    },
    Refused {
        retry_after: Duration,
    },
    Degraded {
        local_decision: Box<Decision>,
    },
}

#[async_trait::async_trait]
pub trait RateLimiter: Send + Sync {
    async fn check(&self, scope: Scope, identifier: &str, budget: Budget) -> Decision;
}

#[async_trait::async_trait]
pub trait TenantRateLimit: Send + Sync {
    async fn budget(&self, tenant_id: &str, scope: Scope) -> Option<Budget>;
    async fn record_refusal(&self, tenant_id: &str);
}

#[async_trait::async_trait]
impl RateLimiter for InMemoryRateLimitStore {
    async fn check(&self, scope: Scope, identifier: &str, budget: Budget) -> Decision {
        let key = format!("{}/{}", scope.as_str(), identifier);
        let mut buckets = self.buckets.lock().unwrap();
        let now = Instant::now();

        // Expire each bucket by its own window, not by the window of this call.
        buckets.retain(|_, (_, started, window)| now.duration_since(*started) < *window);

        let entry = buckets.entry(key).or_insert((0, now, budget.window));
        if now.duration_since(entry.1) >= budget.window {
            *entry = (0, now, budget.window);
        }
        entry.2 = budget.window;
        entry.0 += 1;

        if entry.0 <= budget.max_requests {
            let remaining = budget.max_requests - entry.0;
            Decision::Allowed {
                remaining,
                reset_after: budget.window.saturating_sub(now.duration_since(entry.1)),
            }
        } else {
            Decision::Refused {
                retry_after: budget.window.saturating_sub(now.duration_since(entry.1)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budget(max_requests: u32, window: Duration) -> Budget {
        Budget {
            max_requests,
            window,
        }
    }

    /// A check with a short window must not expire a bucket that has a longer one.
    #[tokio::test]
    async fn a_check_with_a_short_window_does_not_expire_a_longer_buckets() {
        let store = InMemoryRateLimitStore::new();
        let long = budget(1, Duration::from_secs(5));
        let short = budget(10, Duration::from_millis(100));

        assert!(matches!(
            store.check(Scope::ClientErrorsCounted, "user", long).await,
            Decision::Allowed { .. }
        ));
        tokio::time::sleep(Duration::from_millis(250)).await;
        // Another scope, with the short window, runs: the long bucket is only 250 ms old.
        store.check(Scope::ClientErrors, "user", short).await;

        assert!(
            matches!(
                store.check(Scope::ClientErrorsCounted, "user", long).await,
                Decision::Refused { .. }
            ),
            "the long bucket was expired by a check with a shorter window"
        );
    }

    #[tokio::test]
    async fn a_bucket_expires_after_its_own_window() {
        let store = InMemoryRateLimitStore::new();
        let b = budget(1, Duration::from_millis(100));

        assert!(matches!(
            store.check(Scope::Login, "a", b).await,
            Decision::Allowed { .. }
        ));
        assert!(matches!(
            store.check(Scope::Login, "a", b).await,
            Decision::Refused { .. }
        ));
        tokio::time::sleep(Duration::from_millis(150)).await;

        assert!(matches!(
            store.check(Scope::Login, "a", b).await,
            Decision::Allowed { .. }
        ));
    }
}
