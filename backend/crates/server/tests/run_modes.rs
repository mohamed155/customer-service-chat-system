//! Run modes (MOH-37, ADR-010): which routes each mode mounts. A public request must not reach an
//! internal endpoint by path, and the two route sets must never overlap.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use server::router::{app_for_mode_with_test_routes, documented_paths};
use server::run_mode::RunMode;
use server::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

/// A valid 32-byte key, so the WhatsApp webhook (which needs it to verify signatures) is mounted.
const SECRETS_KEY: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=";

fn state() -> AppState {
    let pool = db::lazy_pool(
        "postgres://unreachable:5432/test",
        1,
        Duration::from_secs(1),
    );
    let cfg = config::AppConfig {
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
        ai_key_encryption_key: Some(SECRETS_KEY.into()),
        integration_secrets_key: Some(SECRETS_KEY.into()),
        secrets_root_key: None,
        secrets_root_key_previous: None,
        auth_mfa_required: false,
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
        trusted_proxy: false,
        rate_limit_enabled: false,
        rate_limit_login_max: 10,
        rate_limit_login_window_secs: 60,
        rate_limit_max_override: 10000,
        rate_limit_fail_mode: "auth_closed".into(),
        security_headers_enabled: true,
        csp_policy:
            "default-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'".into(),
        csp_report_only: true,
        hsts_max_age: 31536000,
        otlp_endpoint: None,
        trace_sample_ratio: 1.0,
        sentry_dsn: None,
        release_sha: None,
    };
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

async fn status(mode: RunMode, method: Method, uri: &str) -> StatusCode {
    let app = app_for_mode_with_test_routes(state(), mode);
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    response.status()
}

async fn is_unmounted(mode: RunMode, method: Method, uri: &str) -> bool {
    let app = app_for_mode_with_test_routes(state(), mode);
    let response = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    if response.status() != StatusCode::NOT_FOUND {
        return false;
    }
    let bytes = BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
    body["error"]["message"] == "Route not found"
}

const API_PATHS: [(&str, &str); 4] = [
    ("POST", "/api/v1/auth/login"),
    ("GET", "/api/v1/me"),
    ("GET", "/api/v1/platform/health"),
    ("GET", "/api/v1/tenant/members"),
];
const GATEWAY_PATHS: [(&str, &str); 3] = [
    ("GET", "/api/v1/widget/v1/config"),
    ("POST", "/api/v1/hooks/v1/some-token"),
    ("GET", "/integrations/whatsapp/webhook/some-token"),
];

fn method(name: &str) -> Method {
    Method::from_bytes(name.as_bytes()).unwrap()
}

/// The probes are part of the base OpenAPI document and are served by every mode on purpose.
fn without_probes(paths: std::collections::BTreeSet<String>) -> std::collections::BTreeSet<String> {
    paths
        .into_iter()
        .filter(|path| !matches!(path.as_str(), "/health" | "/ready" | "/metrics"))
        .collect()
}

#[test]
fn api_and_gateway_documented_paths_never_overlap_and_cover_everything() {
    let api = without_probes(documented_paths(RunMode::Api, false));
    let gateway = without_probes(documented_paths(RunMode::Gateway, false));
    let all = without_probes(documented_paths(RunMode::All, false));

    assert!(
        api.contains("/auth/login") && api.contains("/me"),
        "{api:?}"
    );
    assert!(
        gateway.contains("/widget/v1/config") && gateway.contains("/hooks/v1/{token}"),
        "{gateway:?}"
    );
    let overlap: Vec<_> = api.intersection(&gateway).collect();
    assert!(
        overlap.is_empty(),
        "mounted in both api and gateway: {overlap:?}"
    );
    assert!(
        gateway
            .iter()
            .all(|path| path.starts_with("/widget/") || path.starts_with("/hooks/")),
        "the gateway documents a non-public path: {gateway:?}"
    );
    assert!(
        api.iter()
            .all(|path| !path.starts_with("/widget/") && !path.starts_with("/hooks/")),
        "the api documents a public path: {api:?}"
    );
    let union: std::collections::BTreeSet<_> = api.union(&gateway).cloned().collect();
    assert_eq!(union, all, "every route belongs to exactly one unit");
    assert!(without_probes(documented_paths(RunMode::Worker, false)).is_empty());
}

#[tokio::test]
async fn api_mode_serves_the_dashboard_and_not_the_public_routes() {
    // The login route exists: the CSRF check answers 403 before any database call.
    assert_eq!(
        status(RunMode::Api, Method::POST, "/api/v1/auth/login").await,
        StatusCode::FORBIDDEN
    );
    for (verb, path) in GATEWAY_PATHS {
        assert!(
            is_unmounted(RunMode::Api, method(verb), path).await,
            "api mounts the public route {verb} {path}"
        );
    }
    assert_eq!(
        status(RunMode::Api, Method::GET, "/swagger-ui/").await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn gateway_mode_serves_only_the_public_routes() {
    for (verb, path) in API_PATHS {
        assert!(
            is_unmounted(RunMode::Gateway, method(verb), path).await,
            "gateway mounts the internal route {verb} {path}"
        );
    }
    for (verb, path) in GATEWAY_PATHS {
        assert_ne!(
            status(RunMode::Gateway, method(verb), path).await,
            StatusCode::NOT_FOUND,
            "gateway is missing {verb} {path}"
        );
    }
    // The API documentation is part of the internal surface.
    assert!(is_unmounted(RunMode::Gateway, Method::GET, "/api-docs/openapi.json").await);
}

#[tokio::test]
async fn worker_mode_serves_no_routes_except_probes() {
    for (verb, path) in API_PATHS.into_iter().chain(GATEWAY_PATHS) {
        assert!(
            is_unmounted(RunMode::Worker, method(verb), path).await,
            "worker mounts {verb} {path}"
        );
    }
}

#[tokio::test]
async fn every_mode_serves_the_probes() {
    for mode in [
        RunMode::Api,
        RunMode::Gateway,
        RunMode::Worker,
        RunMode::All,
    ] {
        assert_eq!(
            status(mode, Method::GET, "/health").await,
            StatusCode::OK,
            "{mode} /health"
        );
        assert_eq!(
            status(mode, Method::GET, "/ready").await,
            StatusCode::OK,
            "{mode} /ready"
        );
    }
}

#[tokio::test]
async fn all_mode_serves_everything() {
    assert_eq!(
        status(RunMode::All, Method::POST, "/api/v1/auth/login").await,
        StatusCode::FORBIDDEN
    );
    for (verb, path) in GATEWAY_PATHS {
        assert_ne!(
            status(RunMode::All, method(verb), path).await,
            StatusCode::NOT_FOUND,
            "all is missing {verb} {path}"
        );
    }
}
