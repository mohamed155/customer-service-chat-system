use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use server::router::app_with_test_routes;
use server::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

async fn collect_bytes(response: axum::response::Response) -> Vec<u8> {
    BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes()
        .to_vec()
}

fn test_state() -> AppState {
    AppState {
        config: Arc::new(config::AppConfig {
            database_url: "postgres://localhost:5432/test".into(),
            redis_url: "redis://localhost:6379".into(),
            port: 0,
            bind_address: "0.0.0.0".into(),
            environment: config::Environment::Test,
            cors_allowed_origins: vec!["http://localhost:4200".into()],
            log_format: config::LogFormat::Pretty,
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
async fn unknown_route_returns_404_envelope() {
    let state = test_state();
    let app = app_with_test_routes(state);
    let response = app
        .oneshot(Request::builder().uri("/nope").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body: serde_json::Value = serde_json::from_slice(
        &BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    assert!(body.get("error").is_some(), "body should have error key");
    assert_eq!(body["error"]["code"], "not_found");
    assert!(
        body["error"]["request_id"]
            .as_str()
            .unwrap_or("")
            .starts_with("req_"),
        "request_id should start with req_"
    );
}

#[tokio::test]
async fn api_v1_unknown_route_returns_404_envelope() {
    let state = test_state();
    let app = app_with_test_routes(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/unknown")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body: serde_json::Value = serde_json::from_slice(
        &BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn malformed_json_returns_400_envelope() {
    let state = test_state();
    let app = app_with_test_routes(state);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-echo")
                .header("content-type", "application/json")
                .body(Body::from("not valid json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        response
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .starts_with("req_"),
        "x-request-id header should start with req_"
    );
    let body_bytes = BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["error"]["code"], "validation_failed");
}

#[tokio::test]
async fn panicking_handler_returns_500_envelope() {
    let state = test_state();
    let app = app_with_test_routes(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/test-panic")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body: serde_json::Value = serde_json::from_slice(&collect_bytes(response).await).unwrap();
    assert_eq!(body["error"]["code"], "internal_error");
    assert_eq!(body["error"]["message"], "Internal server error");
    let body_str = serde_json::to_string(&body).unwrap();
    assert!(
        !body_str.contains("intentional panic"),
        "body should not contain panic text: {body_str}"
    );
}

#[tokio::test]
async fn server_continues_after_panic() {
    let state = test_state();
    let app = app_with_test_routes(state);
    let panic_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/test-panic")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(panic_resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let health_resp = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health_resp.status(), StatusCode::OK);
}
