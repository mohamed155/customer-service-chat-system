use sentry::protocol::{Event, Map};
use std::sync::OnceLock;

static SENTRY_GUARD: OnceLock<sentry::ClientInitGuard> = OnceLock::new();

/// Initialise Sentry error capture.
///
/// When `dsn` is `None`, no initialisation happens — unset DSN disables
/// capture entirely. The `before_send` scrubber strips request bodies,
/// sensitive headers, and customer PII before data leaves the process.
pub fn init(dsn: Option<&str>, release: Option<&str>) {
    let dsn = match dsn {
        Some(d) => d.to_owned(),
        None => return,
    };

    let guard = sentry::init((
        dsn,
        sentry::ClientOptions {
            release: release.map(|r| std::borrow::Cow::Owned(r.to_owned())),
            before_send: Some(std::sync::Arc::new(|event| Some(scrub_event(event))) as _),
            ..Default::default()
        },
    ));
    let _ = SENTRY_GUARD.set(guard);
}

/// Flush pending Sentry events; call during graceful shutdown.
pub fn flush(timeout: std::time::Duration) {
    if let Some(client) = sentry::Hub::current().client() {
        client.close(Some(timeout));
    }
}

fn scrub_event(mut event: Event<'static>) -> Event<'static> {
    if let Some(ref mut request) = event.request {
        request.data = None;
        request.query_string = None;
        request.cookies = None;

        let mut scrubbed_headers = Map::new();
        for (key, value) in request.headers.iter() {
            if is_sensitive_header(key) {
                scrubbed_headers.insert(key.clone(), "[REDACTED]".to_owned());
            } else {
                scrubbed_headers.insert(key.clone(), value.clone());
            }
        }
        request.headers = scrubbed_headers;
    }

    for exc in event.exception.values.iter_mut() {
        if let Some(ref mut mechanism) = exc.mechanism {
            mechanism.data.clear();
        }
    }

    if let Some(ref mut user) = event.user {
        user.email = None;
        user.username = None;
        user.id = None;
        user.ip_address = None;
        user.other.clear();
    }

    for b in event.breadcrumbs.values.iter_mut() {
        b.message = None;
        b.data.clear();
    }

    const RETAINED_EXTRA: [&str; 5] = ["request_id", "route", "status", "duration_ms", "tenant_id"];
    event
        .extra
        .retain(|k, _| RETAINED_EXTRA.contains(&k.as_str()));

    event
        .tags
        .retain(|k, _| RETAINED_EXTRA.contains(&k.as_str()));
    event.contexts.clear();
    event.server_name = None;

    event
}

fn is_sensitive_header(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "authorization"
        || lower == "cookie"
        || lower == "x-tenant-id"
        || lower == "x-api-key"
        || lower == "x-auth-token"
        || lower == "set-cookie"
        || lower == "proxy-authorization"
        || lower.contains("secret")
        || lower.contains("token")
        || lower.contains("key")
        || lower.contains("password")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentry::protocol::{Request, User};

    fn make_event(headers: Map<String, String>) -> Event<'static> {
        Event {
            request: Some(Request {
                headers,
                ..Default::default()
            }),
            user: Some(User {
                email: Some("user@example.com".into()),
                username: Some("jdoe".into()),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn scrubber_removes_known_secret_header() {
        let mut headers = Map::new();
        headers.insert("Authorization".into(), "Bearer secret123".into());
        let event = make_event(headers);
        let scrubbed = scrub_event(event);

        let request = scrubbed.request.unwrap();
        assert_eq!(
            request.headers.get("Authorization").map(|s| s.as_str()),
            Some("[REDACTED]")
        );
    }

    #[test]
    fn scrubber_removes_known_email() {
        let event = make_event(Map::new());
        let scrubbed = scrub_event(event);

        let user = scrubbed.user.unwrap();
        assert_eq!(user.email, None);
        assert_eq!(user.username, None);
    }

    #[test]
    fn scrubber_removes_x_tenant_id() {
        let mut headers = Map::new();
        headers.insert("X-Tenant-ID".into(), "tenant-123".into());
        let event = make_event(headers);
        let scrubbed = scrub_event(event);

        let request = scrubbed.request.unwrap();
        assert_eq!(
            request.headers.get("X-Tenant-ID").map(|s| s.as_str()),
            Some("[REDACTED]")
        );
    }

    #[test]
    fn scrubber_retains_allowlisted_extra_and_strips_sensitive() {
        let mut headers = Map::new();
        headers.insert("Authorization".into(), "Bearer secret123".into());
        let mut event = make_event(headers);
        event.extra.insert("request_id".into(), "req-abc".into());
        event.extra.insert("password".into(), "hunter2".into());

        let scrubbed = scrub_event(event);

        assert_eq!(
            scrubbed.extra.get("request_id").and_then(|v| v.as_str()),
            Some("req-abc"),
            "allowlisted request_id must be retained"
        );
        assert!(
            scrubbed.extra.get("password").is_none(),
            "non-allowlisted password must be stripped"
        );

        let user = scrubbed.user.unwrap();
        assert_eq!(user.email, None, "email must be scrubbed");

        let request = scrubbed.request.unwrap();
        assert_eq!(
            request.headers.get("Authorization").map(|s| s.as_str()),
            Some("[REDACTED]"),
            "Authorization header must be redacted"
        );
    }
}
