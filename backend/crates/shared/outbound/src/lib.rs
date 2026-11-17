//! Outbound HTTP clients (MOH-82, constitution Principle XI: a timeout on every outbound call).
//!
//! # Purpose
//! Every HTTP client the backend uses to call another system is built here, with a connect
//! timeout, a total request timeout, a redirect policy and a user agent. A slow or silent
//! upstream then fails after a known time instead of holding a worker. The circuit breaker
//! (`resilience`) opens on errors, and a timeout is one: without it a hang never counts.
//!
//! # Public Interfaces
//! - [`HttpSettings`]: the timeouts, redirect policy and user agent of one integration.
//! - [`build`]: the one function that makes a [`reqwest::Client`].
//! - [`BuildError`]: invalid settings, or the client could not be built.
//!
//! # Rules
//! - Code outside this crate never calls `reqwest::Client::new` or `reqwest::Client::builder`;
//!   `clippy.toml` forbids them (`disallowed-methods`), and this crate is the one exception.
//! - Redirects are off by default. The calls this backend makes go to a fixed API endpoint or a
//!   tenant-registered URL that was checked for SSRF at registration; following a redirect would
//!   skip that check. An integration that needs them says so with [`Redirects::Limited`].
//! - The request timeout covers the whole exchange, body included. A long-running stream needs a
//!   deliberately long value (the AI providers use 60 seconds), not no value.
//!
//! # Extension Points
//! Per-integration timeouts come from `config::OutboundHttpConfig` (`APP_HTTP_<NAME>_*`); the
//! server turns them into [`HttpSettings`] and builds one client per integration at startup.
//!
//! Other outbound paths are not HTTP clients built here and keep their vendor's own timeouts:
//! the S3 client (`storage`, configured from the same settings), SMTP (`lettre`), and the OTLP and
//! Sentry exporters.

use std::fmt;
use std::time::Duration;

/// The longest connect or request timeout accepted. A value above it is a mistake (a unit mix-up),
/// not a choice.
pub const MAX_TIMEOUT: Duration = Duration::from_secs(600);

/// What to do when the server answers with a redirect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Redirects {
    /// Return the redirect response to the caller as it is.
    None,
    /// Follow up to this many redirects, then fail.
    Limited(u8),
}

/// Timeouts, redirect policy and user agent of one integration's HTTP client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpSettings {
    /// How long to wait for the TCP and TLS connection.
    pub connect_timeout: Duration,
    /// How long the whole request may take, from sending it to the last byte of the body.
    pub request_timeout: Duration,
    pub redirects: Redirects,
    pub user_agent: String,
}

/// The user agent every client sends unless an integration sets its own.
pub fn default_user_agent() -> String {
    concat!("customer-service-platform/", env!("CARGO_PKG_VERSION")).to_string()
}

impl Default for HttpSettings {
    /// 5 seconds to connect, 30 seconds in all, no redirects.
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(30),
            redirects: Redirects::None,
            user_agent: default_user_agent(),
        }
    }
}

impl HttpSettings {
    pub fn new(connect_timeout: Duration, request_timeout: Duration) -> Self {
        Self {
            connect_timeout,
            request_timeout,
            ..Self::default()
        }
    }

    /// Settings from millisecond values, as configuration holds them.
    pub fn from_millis(connect_ms: u64, request_ms: u64) -> Self {
        Self::new(
            Duration::from_millis(connect_ms),
            Duration::from_millis(request_ms),
        )
    }

    pub fn with_redirects(mut self, redirects: Redirects) -> Self {
        self.redirects = redirects;
        self
    }

    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// Check the settings without building a client.
    pub fn validate(&self) -> Result<(), BuildError> {
        let bad = |what: &str| Err(BuildError::InvalidSettings(what.to_string()));
        if self.connect_timeout.is_zero() || self.request_timeout.is_zero() {
            return bad("timeouts must be greater than zero");
        }
        if self.connect_timeout > MAX_TIMEOUT || self.request_timeout > MAX_TIMEOUT {
            return bad("timeouts must not exceed 10 minutes");
        }
        if self.connect_timeout > self.request_timeout {
            return bad("the connect timeout must not exceed the request timeout");
        }
        if self.user_agent.trim().is_empty() {
            return bad("the user agent must not be empty");
        }
        Ok(())
    }
}

/// Why a client was not built.
#[derive(Debug)]
pub enum BuildError {
    InvalidSettings(String),
    Client(reqwest::Error),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSettings(why) => write!(f, "invalid outbound HTTP settings: {why}"),
            Self::Client(e) => write!(f, "could not build the HTTP client: {e}"),
        }
    }
}

impl std::error::Error for BuildError {}

/// Build the client for one integration. Clone the result freely: clones share one connection
/// pool.
pub fn build(settings: &HttpSettings) -> Result<reqwest::Client, BuildError> {
    settings.validate()?;
    let redirect = match settings.redirects {
        Redirects::None => reqwest::redirect::Policy::none(),
        Redirects::Limited(n) => reqwest::redirect::Policy::limited(usize::from(n)),
    };
    #[allow(clippy::disallowed_methods)] // the one place a client is built
    let builder = reqwest::Client::builder();
    builder
        .connect_timeout(settings.connect_timeout)
        .timeout(settings.request_timeout)
        .redirect(redirect)
        .user_agent(&settings.user_agent)
        .build()
        .map_err(BuildError::Client)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn defaults_are_safe_and_valid() {
        let s = HttpSettings::default();
        assert_eq!(s.connect_timeout, Duration::from_secs(5));
        assert_eq!(s.request_timeout, Duration::from_secs(30));
        assert_eq!(s.redirects, Redirects::None);
        assert!(s.user_agent.starts_with("customer-service-platform/"));
        assert!(s.validate().is_ok());
        assert!(build(&s).is_ok());
    }

    #[test]
    fn invalid_settings_are_refused() {
        let secs = Duration::from_secs;
        let cases = [
            (
                HttpSettings::new(Duration::ZERO, secs(5)),
                "greater than zero",
            ),
            (
                HttpSettings::new(secs(1), Duration::ZERO),
                "greater than zero",
            ),
            (HttpSettings::new(secs(1), secs(601)), "exceed 10 minutes"),
            (HttpSettings::new(secs(601), secs(601)), "exceed 10 minutes"),
            (
                HttpSettings::new(secs(10), secs(5)),
                "connect timeout must not exceed",
            ),
            (HttpSettings::default().with_user_agent("  "), "user agent"),
        ];
        for (settings, why) in cases {
            let err = build(&settings).unwrap_err();
            assert!(matches!(err, BuildError::InvalidSettings(_)));
            assert!(err.to_string().contains(why), "{err} should mention {why}");
        }
    }

    #[test]
    fn a_user_agent_that_is_not_a_header_value_does_not_build() {
        let settings = HttpSettings::default().with_user_agent("bad\nagent");
        assert!(matches!(build(&settings), Err(BuildError::Client(_))));
    }

    #[test]
    fn from_millis_builds_the_expected_durations() {
        let s = HttpSettings::from_millis(1500, 20_000);
        assert_eq!(s.connect_timeout, Duration::from_millis(1500));
        assert_eq!(s.request_timeout, Duration::from_secs(20));
    }

    #[tokio::test]
    async fn a_silent_upstream_fails_within_the_request_timeout() {
        // Accepts the connection, reads the request, never answers.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = socket.read(&mut buf).await;
                    tokio::time::sleep(Duration::from_secs(60)).await;
                });
            }
        });
        let client = build(&HttpSettings::from_millis(200, 300)).unwrap();

        let started = Instant::now();
        let err = client
            .get(format!("http://{addr}/"))
            .send()
            .await
            .unwrap_err();

        assert!(err.is_timeout(), "expected a timeout, got {err}");
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "took {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn a_slow_body_is_cut_by_the_request_timeout_too() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/slow"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
            .mount(&server)
            .await;
        let client = build(&HttpSettings::from_millis(200, 300)).unwrap();

        let started = Instant::now();
        let err = client
            .get(format!("{}/slow", server.uri()))
            .send()
            .await
            .unwrap_err();

        assert!(err.is_timeout());
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[tokio::test]
    async fn redirects_are_returned_by_default_and_followed_only_when_allowed() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/old"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", format!("{}/new", server.uri())),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/new"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let none = build(&HttpSettings::default()).unwrap();
        let response = none
            .get(format!("{}/old", server.uri()))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 302);

        let follow = build(&HttpSettings::default().with_redirects(Redirects::Limited(2))).unwrap();
        let response = follow
            .get(format!("{}/old", server.uri()))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 200);
    }

    #[tokio::test]
    async fn every_request_carries_the_user_agent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header("user-agent", "custom-agent/1"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        let client = build(&HttpSettings::default().with_user_agent("custom-agent/1")).unwrap();
        let response = client.get(server.uri()).send().await.unwrap();
        assert_eq!(response.status().as_u16(), 200);
    }
}
