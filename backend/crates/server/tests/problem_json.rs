#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: a failed unwrap is the failed test
//! The two error formats through the real router (ADR-012 gap A2, MOH-197): problem+json for a
//! client that asks for it, the legacy envelope (marked deprecated and counted) for everyone else.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use server::router::app_with_test_routes;
use server::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

fn test_state() -> AppState {
    test_state_with_rate_limit(true)
}

const PROBLEM: &str = "application/problem+json, application/json;q=0.5";

/// The login route's rate limiter fails closed when Redis is unreachable, which these tests
/// need to get past to reach the database call.
fn test_state_with_rate_limit(rate_limit_enabled: bool) -> AppState {
    let pool = db::lazy_pool(
        "postgres://unreachable:5432/test",
        1,
        Duration::from_secs(1),
    );
    let cfg = config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: "redis://localhost:6379".into(),
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
        ai_key_encryption_key: Some("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=".into()),
        integration_secrets_key: None,
        secrets_root_key: None,
        secrets_root_key_previous: None,
        auth_mfa_required: false,
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
        trusted_proxy: false,
        outbound_http: config::OutboundHttpConfig::default(),
        rate_limit_enabled,
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

async fn call(uri: &str, accept: Option<&str>) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let mut request = Request::builder().uri(uri);
    if let Some(accept) = accept {
        request = request.header(header::ACCEPT, accept);
    }
    let response = app_with_test_routes(test_state())
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, headers, body)
}

#[tokio::test]
async fn an_unknown_route_answers_in_the_format_the_client_asked_for() {
    let (status, headers, body) = call("/api/v1/nowhere?x=1", Some(PROBLEM)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(headers[header::CONTENT_TYPE], "application/problem+json");
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], 404);
    assert_eq!(json["code"], "not_found");
    assert_eq!(json["type"], "urn:problem:not_found");
    assert_eq!(json["instance"], "/api/v1/nowhere");
    assert!(
        json["request_id"]
            .as_str()
            .is_some_and(|id| id.starts_with("req_")),
        "{json}"
    );
    assert!(headers.get("deprecation").is_none());

    let (status, headers, body) = call("/api/v1/nowhere", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(headers[header::CONTENT_TYPE], "application/json");
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "not_found");
    assert!(headers.contains_key("deprecation"));
    assert!(headers[header::LINK]
        .to_str()
        .unwrap()
        .contains("rel=\"deprecation\""));
}

#[tokio::test]
async fn a_missing_session_is_a_401_in_both_formats() {
    let (status, headers, body) = call("/api/v1/tenant/members", Some(PROBLEM)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(headers[header::CONTENT_TYPE], "application/problem+json");
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "unauthenticated");
    assert_eq!(json["title"], "Authentication required");
    assert_eq!(json["instance"], "/api/v1/tenant/members");

    let (status, headers, body) = call("/api/v1/tenant/members", Some("application/json")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap()["error"]["code"],
        "unauthenticated"
    );
    assert!(headers.contains_key("deprecation"));
}

#[tokio::test]
async fn a_panic_is_a_500_that_hides_the_cause_in_both_formats() {
    for accept in [None, Some(PROBLEM)] {
        let (status, headers, body) = call("/test-panic", accept).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        let text = String::from_utf8(body.clone()).unwrap();
        assert!(!text.contains("intentional panic"), "{accept:?}: {text}");
        let json: Value = serde_json::from_slice(&body).unwrap();
        if accept.is_some() {
            assert_eq!(headers[header::CONTENT_TYPE], "application/problem+json");
            assert_eq!(json["detail"], "Internal server error");
            assert!(json["request_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("req_")));
        } else {
            assert_eq!(json["error"]["message"], "Internal server error");
            assert!(headers.contains_key("deprecation"));
        }
    }
}

#[tokio::test]
async fn each_legacy_response_is_counted_by_route_and_nothing_else() {
    observability::metrics::init();
    // One legacy answer on a matched route, one problem+json answer on the same route.
    // A route no other test in this file calls: the recorder is process-wide, and `cargo test`
    // runs the tests of one file side by side in that process.
    let (_, _, _) = call("/api/v1/tenant/customers", None).await;
    let (_, _, _) = call("/api/v1/tenant/customers", Some(PROBLEM)).await;
    let (status, _, body) = call("/metrics", None).await;
    assert_eq!(status, StatusCode::OK);
    let text = String::from_utf8(body).unwrap();
    let line = text
        .lines()
        .find(|l| {
            l.starts_with("error_envelope_legacy_total{")
                && l.contains("route=\"/api/v1/tenant/customers\"")
        })
        .unwrap_or_else(|| panic!("no legacy counter for the route in:\n{text}"));
    assert!(
        line.ends_with(" 1"),
        "only the legacy answer counts: {line}"
    );
    // The only label is the route pattern: no tenant, user or client-chosen value.
    let labels = line
        .split_once('{')
        .and_then(|(_, rest)| rest.split_once('}'))
        .map(|(labels, _)| labels)
        .unwrap();
    let names: Vec<&str> = labels
        .split(',')
        .filter_map(|pair| pair.split_once('=').map(|(name, _)| name))
        .collect();
    assert_eq!(names, ["route"], "{line}");
}
