use axum::body::Body;
use axum::http::{Request, StatusCode};
use server::router;
use server::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

fn make_state(hsts_max_age: u64) -> AppState {
    make_state_with(hsts_max_age, |_| {})
}

fn make_state_with(hsts_max_age: u64, adjust: impl FnOnce(&mut config::AppConfig)) -> AppState {
    let pool = db::lazy_pool(
        "postgres://unreachable:5432/test",
        1,
        Duration::from_secs(1),
    );
    let mut cfg = config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: "redis://localhost:6379".into(),
        auth_jwt_secret: "test-auth-jwt-secret-at-least-32-bytes".into(),
        auth_session_ttl_seconds: 28_800,
        port: 0,
        bind_address: "0.0.0.0".into(),
        environment: config::Environment::Test,
        cors_allowed_origins: vec!["http://localhost:4200".into()],
        log_format: config::LogFormat::Pretty,
        smtp_url: None,
        smtp_from: None,
        public_dashboard_url: "http://localhost:4200".into(),
        db_max_connections: 1,
        db_acquire_timeout_ms: 1000,
        ready_probe_timeout_ms: 500,
        shutdown_grace_seconds: 1,
        docs_enabled: false,
        rate_limit_enabled: true,
        rate_limit_login_max: 10,
        rate_limit_login_window_secs: 60,
        rate_limit_max_override: 10000,
        rate_limit_fail_mode: "auth_closed".into(),
        security_headers_enabled: true,
        csp_policy:
            "default-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'".into(),
        csp_report_only: true,
        hsts_max_age,
        otlp_endpoint: None,
        trace_sample_ratio: 1.0,
        sentry_dsn: None,
        release_sha: None,
        ai_key_encryption_key: Some("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=".into()),
        integration_secrets_key: None,
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
        trusted_proxy: false,
    };
    adjust(&mut cfg);
    let ai = ai::AiService::from_config(pool.clone(), &cfg).unwrap();
    AppState {
        config: Arc::new(cfg),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://unreachable:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool, Duration::from_secs(45)),
        ai,
        budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
    }
}

#[tokio::test]
async fn scope_a_headers_present_on_health() {
    let state = make_state(31536000);
    let app = router::app(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("x-content-type-options")
            .and_then(|v| v.to_str().ok()),
        Some("nosniff")
    );
    assert_eq!(
        response
            .headers()
            .get("referrer-policy")
            .and_then(|v| v.to_str().ok()),
        Some("strict-origin-when-cross-origin")
    );
    let hsts = response
        .headers()
        .get("strict-transport-security")
        .and_then(|v| v.to_str().ok());
    assert!(
        hsts.is_some(),
        "HSTS should be present when hsts_max_age > 0"
    );
    assert!(
        hsts.unwrap().starts_with("max-age=31536000;"),
        "HSTS value should start with max-age=31536000;"
    );
}

#[tokio::test]
async fn x_frame_options_deny_on_authenticated_route() {
    let state = make_state(31536000);
    let app = router::app(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get("x-frame-options")
            .and_then(|v| v.to_str().ok()),
        Some("DENY")
    );
}

#[tokio::test]
async fn x_frame_options_absent_on_widget_route() {
    let state = make_state(31536000);
    let app = router::app(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/widget/v1/config?widget_id=test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        response.headers().get("x-frame-options").is_none(),
        "X-Frame-Options must be absent on widget routes"
    );
}

#[tokio::test]
async fn hsts_absent_when_max_age_zero() {
    let state = make_state(0);
    let app = router::app(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        response
            .headers()
            .get("strict-transport-security")
            .is_none(),
        "HSTS must be absent when hsts_max_age is 0"
    );
}

fn header(response: &axum::response::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

async fn get(state: AppState, uri: &str) -> axum::response::Response {
    router::app(state)
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

/// MOH-38: the policy is enforced, not report-only, on dashboard and API responses.
#[tokio::test]
async fn csp_is_enforced_with_the_default_policy_on_dashboard_routes() {
    let state = make_state_with(0, |cfg| {
        cfg.csp_policy = config::DEFAULT_CSP_POLICY.into();
        cfg.csp_report_only = false;
    });
    let response = get(state, "/api/v1/me").await;

    assert_eq!(
        header(&response, "content-security-policy").as_deref(),
        Some(config::DEFAULT_CSP_POLICY)
    );
    assert!(
        header(&response, "content-security-policy-report-only").is_none(),
        "an enforced policy must not also be sent as report-only"
    );
}

#[tokio::test]
async fn report_only_flag_sends_the_candidate_policy_as_report_only_instead() {
    let candidate = "default-src 'none'; frame-ancestors 'none'";
    let state = make_state_with(0, |cfg| {
        cfg.csp_policy = candidate.into();
        cfg.csp_report_only = true;
    });
    let response = get(state, "/api/v1/me").await;

    assert_eq!(
        header(&response, "content-security-policy-report-only").as_deref(),
        Some(candidate)
    );
    assert!(header(&response, "content-security-policy").is_none());
}

/// The widget keeps its own policy: it is framed on other sites, so the dashboard's
/// `frame-ancestors 'none'` and `X-Frame-Options: DENY` must never reach its responses.
#[tokio::test]
async fn widget_responses_carry_no_dashboard_csp_or_frame_options() {
    let state = make_state_with(0, |cfg| {
        cfg.csp_policy = config::DEFAULT_CSP_POLICY.into();
        cfg.csp_report_only = false;
    });
    let response = get(state, "/api/v1/widget/v1/config?widget_id=test").await;

    assert!(header(&response, "content-security-policy").is_none());
    assert!(header(&response, "content-security-policy-report-only").is_none());
    assert!(header(&response, "x-frame-options").is_none());
}

fn caddyfile() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../infra/Caddyfile");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The dashboard document is served by Caddy, so that is where the policy has to be right. The
/// edge and the application default are two copies of one string; this keeps them the same.
#[test]
fn caddy_serves_the_dashboard_document_with_the_enforced_default_policy() {
    let caddyfile = caddyfile();
    let quoted = format!("Content-Security-Policy \"{}\"", config::DEFAULT_CSP_POLICY);
    assert!(
        caddyfile.contains(&quoted),
        "the dashboard block of infra/Caddyfile must send exactly config::DEFAULT_CSP_POLICY"
    );
    assert!(
        !caddyfile
            .to_ascii_lowercase()
            .contains("content-security-policy-report-only"),
        "the edge must not send the dashboard policy as report-only"
    );
    assert!(caddyfile.contains("X-Frame-Options \"DENY\""));
}

/// The widget document keeps its own, documented policy: it may be framed anywhere (embedding is
/// authorised server-side by the widget's domain allowlist, spec 023) and carries no
/// `X-Frame-Options`.
#[test]
fn caddy_serves_the_widget_document_with_its_own_framing_policy() {
    let caddyfile = caddyfile();
    let widget_policy = "Content-Security-Policy \"frame-ancestors *\"";
    assert_eq!(
        caddyfile.matches(widget_policy).count(),
        2,
        "both the local-dev and the staging/production widget blocks set frame-ancestors *"
    );
    assert_eq!(
        caddyfile.matches("-X-Frame-Options").count(),
        2,
        "both widget blocks must remove X-Frame-Options"
    );
    assert!(!widget_policy.contains("'none'"));
}
