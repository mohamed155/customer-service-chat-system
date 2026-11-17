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
        rate_limit_enabled: true,
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
    };
    let ai = ai::AiService::from_config(pool.clone(), &cfg).unwrap();
    AppState {
        config: Arc::new(cfg),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://unreachable:6379").unwrap()),
        health_checks: checks,
        escalations: escalations::presence::Runtime::new(pool, Duration::from_secs(45)),
        ai,
        budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
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
    assert_eq!(body.checks[1].error.as_deref(), Some("unavailable"));
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

async fn get(app: axum::Router, uri: &str) -> (StatusCode, serde_json::Value, String) {
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = collect_body(response).await;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_default(),
        text,
    )
}

/// A check whose error carries what a driver puts in one: a host, a port, a user.
struct LeakyCheck;
#[async_trait::async_trait]
impl HealthCheck for LeakyCheck {
    fn name(&self) -> &'static str {
        "database"
    }
    async fn check(&self) -> Result<(), String> {
        Err("database error: connect to 10.1.2.3:5432 as app_runtime: password rejected".into())
    }
}

#[tokio::test]
async fn ready_names_the_failing_dependency_and_keeps_the_internal_error_to_itself() {
    let state = make_state(vec![Arc::new(OkCheck), Arc::new(LeakyCheck)]);
    let (status, body, text) = get(router::app(state), "/ready").await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "not_ready");
    assert_eq!(body["checks"][1]["name"], "database");
    assert_eq!(body["checks"][1]["status"], "error");
    assert_eq!(body["checks"][1]["error"], "unavailable");
    for leaked in ["10.1.2.3", "5432", "app_runtime", "password"] {
        assert!(!text.contains(leaked), "/ready exposed '{leaked}': {text}");
    }
}

/// MOH-81: with Redis down, `/ready` is 503 and says the cache is the problem, while `/health`
/// (liveness, no dependencies) stays 200. Nothing listens on the port, as when Redis is stopped.
#[tokio::test]
async fn ready_is_503_when_redis_is_down_and_health_stays_200() {
    let closed_port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let redis = cache::Cache::new(&format!("redis://127.0.0.1:{closed_port}")).unwrap();
    let state = make_state(vec![
        Arc::new(OkCheck),
        Arc::new(cache::RedisHealthCheck::new(redis)),
    ]);
    let app = router::app(state);

    let (status, body, text) = get(app.clone(), "/ready").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "not_ready");
    assert_eq!(body["checks"][0]["status"], "ok");
    assert_eq!(body["checks"][1]["name"], "cache");
    assert_eq!(body["checks"][1]["status"], "error");
    assert!(
        !text.contains("127.0.0.1") && !text.contains(&closed_port.to_string()),
        "/ready exposed the Redis address: {text}"
    );

    let (status, body, _) = get(app, "/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn ready_fails_while_the_process_is_shutting_down_and_health_stays_200() {
    let draining = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let state = make_state(vec![
        Arc::new(OkCheck),
        Arc::new(observability::health::DrainingCheck::new(draining.clone())),
    ]);
    let app = router::app(state);

    let (status, _, _) = get(app.clone(), "/ready").await;
    assert_eq!(status, StatusCode::OK, "ready while running");

    draining.store(true, std::sync::atomic::Ordering::SeqCst);
    let (status, body, _) = get(app.clone(), "/ready").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        body["checks"][0]["status"], "ok",
        "the dependencies are still reported"
    );
    assert_eq!(body["checks"][1]["name"], "shutdown");
    assert_eq!(body["checks"][1]["error"], "shutting down");

    let (status, _, _) = get(app, "/health").await;
    assert_eq!(status, StatusCode::OK, "liveness does not follow readiness");
}

#[tokio::test]
async fn ready_includes_object_storage() {
    use storage::{InMemoryStorage, ObjectStorage, StorageBackend, StorageKind};
    let in_memory: Arc<dyn ObjectStorage> = Arc::new(InMemoryStorage::default());

    // Intended in-memory storage is ready.
    let ok = storage::StorageHealthCheck::new(
        in_memory.clone(),
        StorageBackend {
            kind: StorageKind::InMemory,
            s3_configured: false,
        },
    );
    let (status, body, _) = get(router::app(make_state(vec![Arc::new(ok)])), "/ready").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["checks"][0]["name"], "storage");

    // S3 configured but the client could not be created: not ready.
    let fallen_back = storage::StorageHealthCheck::new(
        in_memory,
        StorageBackend {
            kind: StorageKind::InMemory,
            s3_configured: true,
        },
    );
    let (status, body, _) = get(
        router::app(make_state(vec![Arc::new(fallen_back)])),
        "/ready",
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["checks"][0]["name"], "storage");
    assert_eq!(body["checks"][0]["error"], "unavailable");
}
