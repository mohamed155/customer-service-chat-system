use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use observability::health::{CheckStatus, HealthCheck, HealthReport, HealthStatus};
use server::router;
use server::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

struct OkCheck;
#[async_trait::async_trait]
impl HealthCheck for OkCheck {
    fn name(&self) -> &'static str {
        "ok_check"
    }
    async fn check(&self) -> Result<(), String> {
        Ok(())
    }
}

struct FailCheck;
#[async_trait::async_trait]
impl HealthCheck for FailCheck {
    fn name(&self) -> &'static str {
        "fail_check"
    }
    async fn check(&self) -> Result<(), String> {
        Err("fail".to_owned())
    }
}

struct TimeoutCheck;
#[async_trait::async_trait]
impl HealthCheck for TimeoutCheck {
    fn name(&self) -> &'static str {
        "timeout_check"
    }
    async fn check(&self) -> Result<(), String> {
        tokio::time::sleep(Duration::from_secs(10)).await;
        Ok(())
    }
}

fn make_state(checks: Vec<Arc<dyn HealthCheck>>) -> AppState {
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
        health_checks: checks,
        escalations: escalations::presence::Runtime::new(pool, Duration::from_secs(45)),
        ai,
    }
}

async fn collect_body(response: axum::response::Response) -> Vec<u8> {
    BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes()
        .to_vec()
}

#[tokio::test]
async fn health_returns_200_without_invoking_checks() {
    let state = make_state(vec![]);
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
    let body: serde_json::Value = serde_json::from_slice(&collect_body(response).await).unwrap();
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn ready_all_ok_returns_200_with_checks() {
    let state = make_state(vec![Arc::new(OkCheck), Arc::new(OkCheck)]);
    let app = router::app(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: HealthReport = serde_json::from_slice(&collect_body(response).await).unwrap();
    assert_eq!(body.status, HealthStatus::Ready);
}

#[tokio::test]
async fn ready_db_failing_returns_503() {
    let state = make_state(vec![Arc::new(OkCheck), Arc::new(FailCheck)]);
    let app = router::app(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: HealthReport = serde_json::from_slice(&collect_body(response).await).unwrap();
    assert_eq!(body.status, HealthStatus::NotReady);
    assert_eq!(body.checks[1].status, CheckStatus::Error);
    assert_eq!(body.checks[1].error.as_deref(), Some("fail"));
}

#[tokio::test]
async fn ready_timeout_returns_503_with_timed_out() {
    let state = make_state(vec![Arc::new(TimeoutCheck)]);
    let app = router::app(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: HealthReport = serde_json::from_slice(&collect_body(response).await).unwrap();
    assert_eq!(body.status, HealthStatus::NotReady);
    assert_eq!(body.checks[0].status, CheckStatus::Error);
    assert_eq!(body.checks[0].error.as_deref(), Some("timed out"));
}
