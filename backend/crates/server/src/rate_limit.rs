use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Extension;
use kernel::ApiError;
use std::sync::Arc;
use std::time::Duration;

use kernel::InMemoryRateLimitStore;

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
/// The store must be injected as an `Extension<Arc<InMemoryRateLimitStore>>`
/// on the parent router.
pub async fn per_ip_creation_limit(
    Extension(store): Extension<Arc<InMemoryRateLimitStore>>,
    request: Request,
    next: Next,
) -> Response {
    let client_ip = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim())
        .unwrap_or("127.0.0.1")
        .to_string();

    if !store.check(
        &format!("ip:{}", client_ip),
        SESSION_CREATION_LIMIT,
        Duration::from_secs(SESSION_CREATION_WINDOW_SECS),
    ) {
        return ApiError::rate_limited("Too many requests").into_response();
    }

    next.run(request).await
}

/// Check the per-session message rate limit inside a handler.
/// Returns `true` if the request is within budget, `false` if rate-limited.
pub fn check_session_message_limit(store: &InMemoryRateLimitStore, session_id: &str) -> bool {
    store.check(
        &format!("session:{}", session_id),
        MESSAGES_PER_SESSION_LIMIT,
        Duration::from_secs(MESSAGES_PER_SESSION_WINDOW_SECS),
    )
}

/// Check the global per-tenant rate limit inside a handler.
/// Returns `true` if the request is within budget, `false` if rate-limited.
pub fn check_global_tenant_limit(store: &InMemoryRateLimitStore, tenant_id: &str) -> bool {
    store.check(
        &format!("tenant:{}", tenant_id),
        GLOBAL_TENANT_LIMIT,
        Duration::from_secs(GLOBAL_TENANT_WINDOW_SECS),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_limit() {
        let store = InMemoryRateLimitStore::default();
        for _ in 0..10 {
            assert!(store.check("key1", 10, Duration::from_secs(60)));
        }
    }

    #[test]
    fn rejects_past_limit() {
        let store = InMemoryRateLimitStore::default();
        for _ in 0..10 {
            store.check("key2", 10, Duration::from_secs(60));
        }
        assert!(!store.check("key2", 10, Duration::from_secs(60)));
    }

    #[test]
    fn resets_after_window() {
        let store = InMemoryRateLimitStore::default();
        for _ in 0..10 {
            store.check("key3", 10, Duration::from_millis(50));
        }
        assert!(!store.check("key3", 10, Duration::from_millis(50)));
        std::thread::sleep(Duration::from_millis(60));
        assert!(store.check("key3", 10, Duration::from_millis(50)));
    }

    #[test]
    fn different_keys_independent() {
        let store = InMemoryRateLimitStore::default();
        for _ in 0..10 {
            store.check("tenant-a", 10, Duration::from_secs(60));
        }
        assert!(!store.check("tenant-a", 10, Duration::from_secs(60)));
        assert!(store.check("tenant-b", 10, Duration::from_secs(60)));
    }
}
