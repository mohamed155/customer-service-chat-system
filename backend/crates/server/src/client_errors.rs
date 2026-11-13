//! Errors from users' browsers (MOH-168).
//!
//! The dashboard reports crashes it can't recover from (an unhandled error, an unhandled promise
//! rejection, a lazy chunk that won't load) so they show up next to the backend's own logs
//! instead of only when someone complains. One request carries a small batch.
//!
//! The reports are untrusted input from a signed-in browser. Every field has a length cap, `kind`
//! is a closed set, control characters are removed before anything is logged (so a report can't
//! forge a log line), and nothing from a report becomes a metric label. The client scrubs tokens,
//! cookies and form values before sending; the server doesn't rely on that for its own limits.
//!
//! A report is one `tracing` event with target `frontend`, which promtail ships to Loki like any
//! other backend log line, and a count in `frontend_errors_total{kind, release}` for the alert on
//! a spike. A signed-in user can only add `COUNTED_PER_USER` to that counter per `COUNTED_WINDOW`
//! (every report is still logged), so one account can never raise an alert by itself. There is no table: a report is not data the platform keeps, so there is no tenant
//! data to isolate. The tenant ID the browser names is **unverified** and is logged as such
//! (`claimed_tenant_id`): this route sits outside the tenant context, and a user who names
//! another tenant only labels their own report.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    http::{header::USER_AGENT, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Extension,
};
use config::AppConfig;
use identity::Principal;
use kernel::{ApiError, ApiJson, Budget, Decision, RateLimiter, Scope};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

/// The most reports one request may carry.
pub const MAX_REPORTS: usize = 20;
/// The most requests one signed-in user may send per window. The dashboard flushes about every
/// five seconds at most, so this leaves room for several tabs.
pub const REQUESTS_PER_USER: u32 = 30;
pub const REQUEST_WINDOW: Duration = Duration::from_secs(60);
/// The body limit of the route, in bytes. A browser's `keepalive` request body can't exceed 64 KiB.
pub const MAX_BODY_BYTES: usize = 64 * 1024;

const MAX_MESSAGE_CHARS: usize = 1_000;
const MAX_STACK_CHARS: usize = 8_000;
const MAX_ROUTE_CHARS: usize = 200;
const MAX_RELEASE_CHARS: usize = 64;
const MAX_USER_AGENT_CHARS: usize = 300;
/// A report stands for this many identical ones at most (the client counts repeats).
const MAX_COUNT: u32 = 1_000;
/// What one signed-in user may add to `frontend_errors_total` in `COUNTED_WINDOW`, one per report
/// whatever count it claims. Every report is still logged. The counter is what the alerts watch and
/// any signed-in user can send reports, so a user's share of it is small: a real spike is many
/// users, and a loop in one browser (or one person spamming) cannot reach an alert threshold alone.
pub const COUNTED_PER_USER: u32 = 5;
pub const COUNTED_WINDOW: Duration = Duration::from_secs(600);

/// What went wrong in the browser.
#[derive(Debug, Clone, Copy, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClientErrorKind {
    /// An error nobody caught.
    Error,
    /// A promise that was rejected and nobody handled.
    UnhandledRejection,
    /// A lazy-loaded script or stylesheet that could not be fetched (usually a deploy).
    ChunkLoad,
}

impl ClientErrorKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::UnhandledRejection => "unhandled_rejection",
            Self::ChunkLoad => "chunk_load",
        }
    }
}

/// One error seen in a browser.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ClientErrorReport {
    pub kind: ClientErrorKind,
    /// The error's message, already scrubbed by the dashboard. Up to 1000 characters.
    #[schema(max_length = 1000)]
    pub message: String,
    /// The stack, already scrubbed. Up to 8000 characters.
    #[schema(max_length = 8000)]
    pub stack: Option<String>,
    /// The route pattern the user was on, such as `tenant/customers/:id`: never the URL.
    #[schema(max_length = 200)]
    pub route: Option<String>,
    /// The dashboard release, taken from its main script's file name. Letters, digits, `.`, `_`, `-`.
    #[schema(max_length = 64, example = "4TYQZ6FJ")]
    pub release: Option<String>,
    /// The request ID of the API call involved, when there is one (`req_` and a lowercase UUID).
    #[schema(example = "req_0191f3a2-6b1e-7c3d-8e4f-5a6b7c8d9e0f")]
    pub request_id: Option<String>,
    /// The workspace the user had open. Not verified: it labels the report and nothing more.
    pub tenant_id: Option<Uuid>,
    /// How many identical errors this report stands for (default 1, at most 1000).
    #[schema(minimum = 1, maximum = 1000)]
    pub count: Option<u32>,
}

/// A batch of reports.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ClientErrorBatch {
    /// One to twenty reports.
    #[schema(min_items = 1, max_items = 20)]
    pub reports: Vec<ClientErrorReport>,
}

/// Remove control characters (a newline in a report must not start a fake log line) and cut to
/// `max` characters.
fn clean(text: &str, max: usize) -> String {
    text.chars().filter(|c| !c.is_control()).take(max).collect()
}

fn check(report: &ClientErrorReport) -> Result<(), &'static str> {
    if report.message.trim().is_empty() {
        return Err("message must not be empty");
    }
    if report.message.chars().count() > MAX_MESSAGE_CHARS {
        return Err("message is too long");
    }
    if report
        .stack
        .as_ref()
        .is_some_and(|s| s.chars().count() > MAX_STACK_CHARS)
    {
        return Err("stack is too long");
    }
    if report
        .route
        .as_ref()
        .is_some_and(|s| s.chars().count() > MAX_ROUTE_CHARS)
    {
        return Err("route is too long");
    }
    if let Some(release) = &report.release {
        let valid = !release.is_empty()
            && release.chars().count() <= MAX_RELEASE_CHARS
            && release
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        if !valid {
            return Err("release has characters that are not allowed");
        }
    }
    if report
        .request_id
        .as_deref()
        .is_some_and(|id| !observability::request_id::validate(id))
    {
        return Err("request_id is not a request ID");
    }
    if report.count.is_some_and(|c| c == 0 || c > MAX_COUNT) {
        return Err("count must be between 1 and 1000");
    }
    Ok(())
}

/// Receive a batch of browser error reports.
#[utoipa::path(
    post,
    path = "/client-errors",
    tag = "observability",
    operation_id = "report_client_errors",
    summary = "Report errors seen in the browser",
    description = "Takes one to twenty error reports from the dashboard and writes them to the \
                  backend log (target `frontend`) and to the `frontend_errors_total` counter. \
                  Needs a session; errors before sign-in are not reported. Rate limited per \
                  user. The body may be at most 64 KiB. The workspace ID in a report is not \
                  verified. Requires permission: none (any signed-in user).",
    request_body = ClientErrorBatch,
    responses(
        (status = 204, description = "Received."),
        (status = 400, description = "A report is not valid, or the body is not JSON or is larger than 64 KiB.", body = kernel::ErrorEnvelope),
        (status = 401, description = "Unauthenticated.", body = kernel::ErrorEnvelope),
        (status = 403, description = "A session-cookie request without an allowed origin and the `X-Requested-With` header.", body = kernel::ErrorEnvelope),
        (status = 429, description = "Too many requests.", body = kernel::ErrorEnvelope),
    ),
    security(("session_cookie" = [])),
)]
pub async fn report(
    Extension(config): Extension<Arc<AppConfig>>,
    Extension(limiter): Extension<Arc<dyn RateLimiter>>,
    principal: Principal,
    headers: HeaderMap,
    ApiJson(batch): ApiJson<ClientErrorBatch>,
) -> Response {
    if config.rate_limit_enabled {
        let decision = limiter
            .check(
                Scope::ClientErrors,
                &principal.user_id.to_string(),
                Budget {
                    max_requests: REQUESTS_PER_USER,
                    window: REQUEST_WINDOW,
                },
            )
            .await;
        if let Some(retry_after) = refusal(&decision) {
            return ApiError::rate_limited("Too many error reports", retry_after).into_response();
        }
    }
    if batch.reports.is_empty() || batch.reports.len() > MAX_REPORTS {
        return ApiError::validation_failed("reports must hold between 1 and 20 reports")
            .into_response();
    }
    for report in &batch.reports {
        if let Err(reason) = check(report) {
            return ApiError::validation_failed(reason).into_response();
        }
    }

    // The browser is read from the request's own header, not from the body.
    let user_agent = clean(
        headers
            .get(USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .unwrap_or(""),
        MAX_USER_AGENT_CHARS,
    );
    let release_label = config.release_sha.as_deref().unwrap_or("unknown");
    let user_id = principal.user_id.to_string();

    for report in batch.reports {
        let count = report.count.unwrap_or(1);
        if counts_toward_alerts(&config, &limiter, &user_id).await {
            observability::metrics::record_frontend_errors(report.kind.as_str(), release_label, 1);
        }
        tracing::warn!(
            target: "frontend",
            kind = report.kind.as_str(),
            // Not `message`: that is the name of the event's own text, and the two would collide.
            error_message = %clean(&report.message, MAX_MESSAGE_CHARS),
            stack = %clean(report.stack.as_deref().unwrap_or(""), MAX_STACK_CHARS),
            route = %clean(report.route.as_deref().unwrap_or(""), MAX_ROUTE_CHARS),
            client_release = %clean(report.release.as_deref().unwrap_or(""), MAX_RELEASE_CHARS),
            client_request_id = report.request_id.as_deref().unwrap_or(""),
            claimed_tenant_id = %report.tenant_id.map(|t| t.to_string()).unwrap_or_default(),
            user_id = %user_id,
            user_agent = %user_agent,
            count,
            "frontend error"
        );
    }
    StatusCode::NO_CONTENT.into_response()
}

/// Whether this user may still add to the alert counter in the current window. With rate limiting
/// off (local development) every report counts.
async fn counts_toward_alerts(
    config: &AppConfig,
    limiter: &Arc<dyn RateLimiter>,
    user_id: &str,
) -> bool {
    if !config.rate_limit_enabled {
        return true;
    }
    let decision = limiter
        .check(
            Scope::ClientErrorsCounted,
            user_id,
            Budget {
                max_requests: COUNTED_PER_USER,
                window: COUNTED_WINDOW,
            },
        )
        .await;
    refusal(&decision).is_none()
}

/// When a decision is a refusal, how long to wait. A `Degraded` decision (Redis unreachable, so
/// the process's own counters decided) is a refusal when its local decision is one: it must not
/// read as "allowed" just because it is wrapped.
fn refusal(decision: &Decision) -> Option<Duration> {
    match decision {
        Decision::Refused { retry_after } => Some(*retry_after),
        Decision::Degraded { local_decision } => refusal(local_decision),
        Decision::Allowed { .. } => None,
    }
}
