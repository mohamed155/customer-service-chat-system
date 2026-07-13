use axum::body::Body;
use axum::http::{HeaderValue, Request};
use server::router;
use server::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

fn test_state() -> AppState {
    AppState {
        config: Arc::new(config::AppConfig {
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
        }),
        db: db::lazy_pool(
            "postgres://unreachable:5432/test",
            1,
            Duration::from_secs(1),
        ),
        cache: Arc::new(cache::Cache::new("redis://unreachable:6379").unwrap()),
        health_checks: vec![],
    }
}

#[tokio::test]
async fn response_carries_x_request_id_header() {
    let state = test_state();
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
    let header = response
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok());
    assert!(header.is_some(), "response should have x-request-id header");
    let id = header.unwrap();
    assert_eq!(id.len(), 40, "request ID should be 40 chars: {id}");
    assert!(
        id.starts_with("req_"),
        "request ID should start with req_: {id}"
    );
}

#[tokio::test]
async fn valid_inbound_id_is_echoed() {
    let state = test_state();
    let app = router::app(state);
    let inbound = "req_0197f2b4-53a1-7cc3-9d2e-1a2b3c4d5e6f";
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .header("x-request-id", HeaderValue::from_str(inbound).unwrap())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let echoed = response
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok());
    assert_eq!(echoed, Some(inbound), "valid inbound ID should be echoed");
}

#[tokio::test]
async fn malformed_inbound_id_is_replaced() {
    let state = test_state();
    let app = router::app(state);
    let inbound = "not-a-valid-id";
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .header("x-request-id", HeaderValue::from_str(inbound).unwrap())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let echoed = response
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok());
    assert!(echoed.is_some(), "response should have x-request-id");
    let echoed = echoed.unwrap();
    assert_ne!(echoed, inbound, "malformed inbound should NOT be echoed");
    assert_eq!(echoed.len(), 40, "replaced ID should be 40 chars: {echoed}");
    assert!(
        echoed.starts_with("req_"),
        "replaced ID should start with req_: {echoed}"
    );
}
