use axum::extract::connect_info::ConnectInfo;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Extension;
use config::AppConfig;
use kernel::{ApiError, Budget, Decision, RateLimiter, Scope};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

/// Per-IP rate limit for session creation: 10 requests per 60 seconds.
pub const SESSION_CREATION_LIMIT: u32 = 10;
pub const SESSION_CREATION_WINDOW_SECS: u64 = 60;

/// Per-session rate limit for messages: 10 messages per 60 seconds.
pub const MESSAGES_PER_SESSION_LIMIT: u32 = 10;
pub const MESSAGES_PER_SESSION_WINDOW_SECS: u64 = 60;

/// Global per-tenant rate limit: 600 requests per 60 seconds.
pub const GLOBAL_TENANT_LIMIT: u32 = 600;
pub const GLOBAL_TENANT_WINDOW_SECS: u64 = 60;

/// Middleware that limits session creation to `SESSION_CREATION_LIMIT` per
/// client IP per window. Applied only to the session-creation route, not
/// to the entire widget scope.
///
/// The store must be injected as an `Extension<Arc<dyn RateLimiter>>`
/// on the parent router.
/// Middleware that rate-limits login attempts before the account lookup.
///
/// Keyed on `(email, client_ip)` with scope `Login` so that:
/// - An attacker trying many passwords for one email is throttled.
/// - An attacker trying one password for many emails from the same IP is also
///   throttled.
/// - The counter is incremented **before** the account lookup result is known,
///   preventing account enumeration via timing or response-code differences.
pub async fn login_rate_limit(
    Extension(store): Extension<Arc<dyn RateLimiter>>,
    Extension(config): Extension<Arc<AppConfig>>,
    request: Request,
    next: Next,
) -> Response {
    if !config.rate_limit_enabled {
        return next.run(request).await;
    }
    let (parts, body) = request.into_parts();
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "login_rate_limit: failed to read body, allowing through");
            return next
                .run(Request::from_parts(parts, axum::body::Body::empty()))
                .await;
        }
    };

    let client_ip = if config.trusted_proxy {
        match parts
            .headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
        {
            Some(header_val) => header_val
                .split(',')
                .next()
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            None => {
                return next
                    .run(Request::from_parts(
                        parts,
                        axum::body::Body::from(body_bytes),
                    ))
                    .await
            }
        }
    } else {
        parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    };

    let email: String = serde_json::from_slice::<serde_json::Value>(&body_bytes)
        .ok()
        .and_then(|v| {
            v.get("email")
                .and_then(|e| e.as_str().map(|s| s.trim().to_ascii_lowercase()))
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    let identifier = format!("{}:{}", email, client_ip);

    if let Decision::Refused { retry_after } = store
        .check(
            Scope::Login,
            &identifier,
            Budget {
                max_requests: config.rate_limit_login_max,
                window: Duration::from_secs(config.rate_limit_login_window_secs),
            },
        )
        .await
    {
        return ApiError::rate_limited("Too many login attempts", retry_after).into_response();
    }

    let request = Request::from_parts(parts, axum::body::Body::from(body_bytes));
    next.run(request).await
}

/// Middleware that rate-limits the second sign-in step per client address, before any code is
/// checked. The per-account lockout (five wrong codes, fifteen minutes) is separate and applies
/// whoever the client is; this limit slows a client that tries many accounts.
pub async fn mfa_rate_limit(
    Extension(store): Extension<Arc<dyn RateLimiter>>,
    Extension(config): Extension<Arc<AppConfig>>,
    request: Request,
    next: Next,
) -> Response {
    if !config.rate_limit_enabled {
        return next.run(request).await;
    }
    // Behind the trusted proxy the client is the address it forwards. With no such header the
    // peer address is used, so a request is never exempt from the limit.
    let forwarded = if config.trusted_proxy {
        request
            .headers()
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            // Only an address is a client; anything else in the header is ignored, so the
            // header cannot be used to name some other limiter key.
            .and_then(|first| first.trim().parse::<std::net::IpAddr>().ok())
            .map(|ip| ip.to_string())
    } else {
        None
    };
    let client_ip = forwarded.unwrap_or_else(|| {
        request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    });
    if let Decision::Refused { retry_after } = store
        .check(
            Scope::Mfa,
            &client_ip,
            Budget {
                max_requests: config.rate_limit_login_max,
                window: Duration::from_secs(config.rate_limit_login_window_secs),
            },
        )
        .await
    {
        return ApiError::rate_limited("Too many attempts", retry_after).into_response();
    }
    next.run(request).await
}

pub async fn per_ip_creation_limit(
    Extension(store): Extension<Arc<dyn RateLimiter>>,
    Extension(config): Extension<Arc<AppConfig>>,
    request: Request,
    next: Next,
) -> Response {
    if !config.rate_limit_enabled {
        return next.run(request).await;
    }
    let client_ip = if config.trusted_proxy {
        request
            .headers()
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(',').next().unwrap_or(s).trim())
            .unwrap_or("127.0.0.1")
            .to_string()
    } else {
        request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(addr)| addr.ip().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    };

    let identifier = cache::limiter::select_identifier(None, &client_ip);

    if let Decision::Refused { retry_after } = store
        .check(
            Scope::WidgetSession,
            &identifier,
            Budget {
                max_requests: SESSION_CREATION_LIMIT,
                window: Duration::from_secs(SESSION_CREATION_WINDOW_SECS),
            },
        )
        .await
    {
        return ApiError::rate_limited("Too many requests", retry_after).into_response();
    }

    next.run(request).await
}

async fn fetch_override(
    pool: &sqlx::PgPool,
    tenant_id: String,
    scope: kernel::Scope,
) -> Option<Budget> {
    let id = uuid::Uuid::parse_str(&tenant_id).ok()?;
    let mut uow = db::UnitOfWork::begin_for_tenant(pool, id)
        .await
        .inspect_err(|e| tracing::warn!(error = %e, "fetch_override: begin failed, using defaults"))
        .ok()?;
    let row: Option<(i32, i32)> = sqlx::query_as(
        "SELECT max_requests, window_seconds FROM tenant_rate_limit_overrides \
         WHERE tenant_id = $1 AND scope = $2 AND deleted_at IS NULL \
         LIMIT 1",
    )
    .bind(id)
    .bind(scope.as_str())
    .fetch_optional(uow.conn())
    .await
    .inspect_err(|e| tracing::warn!(error = %e, "fetch_override: query failed, using defaults"))
    .ok()
    .flatten();
    row.and_then(|(m, w)| {
        if m <= 0 || w <= 0 {
            tracing::warn!(
                max_requests = m,
                window_seconds = w,
                "fetch_override: invalid override row ignored"
            );
            return None;
        }
        Some(Budget {
            max_requests: m as u32,
            window: std::time::Duration::from_secs(w as u64),
        })
    })
}

pub struct ServerTenantRateLimit {
    pub pool: sqlx::PgPool,
    pub resolver: Arc<cache::limiter::BudgetResolver>,
    pub enabled: bool,
}

#[async_trait::async_trait]
impl kernel::TenantRateLimit for ServerTenantRateLimit {
    async fn budget(&self, tenant_id: &str, scope: Scope) -> Option<Budget> {
        if !self.enabled {
            return None;
        }
        Some(
            self.resolver
                .resolve(tenant_id, scope, |t, s| fetch_override(&self.pool, t, s))
                .await,
        )
    }
    async fn record_refusal(&self, tenant_id: &str) {
        if let Ok(id) = uuid::Uuid::parse_str(tenant_id) {
            crate::rate_limit_audit::record_refusal(&self.pool, id).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kernel::InMemoryRateLimitStore;

    #[tokio::test]
    async fn allows_up_to_limit() {
        let store = InMemoryRateLimitStore::default();
        let budget = Budget {
            max_requests: 10,
            window: Duration::from_secs(60),
        };
        for i in 0..10 {
            assert!(
                matches!(
                    RateLimiter::check(&store, Scope::Login, "key1", budget).await,
                    Decision::Allowed { .. }
                ),
                "request {i} should be allowed"
            );
        }
    }

    #[tokio::test]
    async fn rejects_past_limit() {
        let store = InMemoryRateLimitStore::default();
        let budget = Budget {
            max_requests: 10,
            window: Duration::from_secs(60),
        };
        for _ in 0..10 {
            RateLimiter::check(&store, Scope::Login, "key2", budget).await;
        }
        assert!(!matches!(
            RateLimiter::check(&store, Scope::Login, "key2", budget).await,
            Decision::Allowed { .. }
        ));
    }

    #[tokio::test]
    async fn resets_after_window() {
        let store = InMemoryRateLimitStore::default();
        let budget = Budget {
            max_requests: 10,
            window: Duration::from_millis(50),
        };
        for _ in 0..10 {
            RateLimiter::check(&store, Scope::Login, "key3", budget).await;
        }
        assert!(!matches!(
            RateLimiter::check(&store, Scope::Login, "key3", budget).await,
            Decision::Allowed { .. }
        ));
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(matches!(
            RateLimiter::check(&store, Scope::Login, "key3", budget).await,
            Decision::Allowed { .. }
        ));
    }

    #[tokio::test]
    async fn different_keys_independent() {
        let store = InMemoryRateLimitStore::default();
        let budget = Budget {
            max_requests: 10,
            window: Duration::from_secs(60),
        };
        for _ in 0..10 {
            RateLimiter::check(&store, Scope::Login, "tenant-a", budget).await;
        }
        assert!(!matches!(
            RateLimiter::check(&store, Scope::Login, "tenant-a", budget).await,
            Decision::Allowed { .. }
        ));
        assert!(matches!(
            RateLimiter::check(&store, Scope::Login, "tenant-b", budget).await,
            Decision::Allowed { .. }
        ));
    }
}
