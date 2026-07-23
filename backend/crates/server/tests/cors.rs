use axum::body::Body;
use axum::http::{HeaderValue, Method, Request};
use server::router;
use server::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

fn state_with_origins(origins: Vec<&'static str>) -> AppState {
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
        cors_allowed_origins: origins.into_iter().map(|s| s.to_owned()).collect(),
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
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
    };
    let ai = ai::AiService::from_config(pool.clone(), &cfg).unwrap();
    AppState {
        config: Arc::new(cfg),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://unreachable:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool, Duration::from_secs(45)),
        ai,
    }
}

#[tokio::test]
async fn preflight_from_allowed_origin_returns_allow_headers() {
    let state = state_with_origins(vec!["http://localhost:4200"]);
    let app = router::app(state);
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/api/v1/test")
                .header(
                    "origin",
                    HeaderValue::from_str("http://localhost:4200").unwrap(),
                )
                .header(
                    "access-control-request-method",
                    HeaderValue::from_str("POST").unwrap(),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let allow_origin = response
        .headers()
        .get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok());
    assert_eq!(
        allow_origin,
        Some("http://localhost:4200"),
        "allowed origin should get allow-origin header"
    );
}

#[tokio::test]
async fn preflight_from_non_listed_origin_returns_no_allow() {
    let state = state_with_origins(vec!["http://localhost:4200"]);
    let app = router::app(state);
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/api/v1/test")
                .header(
                    "origin",
                    HeaderValue::from_str("http://evil.example").unwrap(),
                )
                .header(
                    "access-control-request-method",
                    HeaderValue::from_str("POST").unwrap(),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let allow_origin = response.headers().get("access-control-allow-origin");
    assert!(
        allow_origin.is_none(),
        "non-listed origin should NOT get allow-origin header"
    );
}

#[tokio::test]
async fn simple_get_from_allowed_origin_carries_grant() {
    let state = state_with_origins(vec!["http://localhost:4200"]);
    let app = router::app(state);
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/health")
                .header(
                    "origin",
                    HeaderValue::from_str("http://localhost:4200").unwrap(),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let allow_origin = response
        .headers()
        .get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok());
    assert_eq!(
        allow_origin,
        Some("http://localhost:4200"),
        "allowed origin GET should get allow-origin"
    );
}
