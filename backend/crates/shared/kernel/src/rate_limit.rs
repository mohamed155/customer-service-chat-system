use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct InMemoryRateLimitStore {
    buckets: Mutex<HashMap<String, (u32, Instant)>>,
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

        buckets.retain(|_, (_, started)| now.duration_since(*started) < budget.window);

        let entry = buckets.entry(key).or_insert((0, now));
        if now.duration_since(entry.1) >= budget.window {
            *entry = (0, now);
        }
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
