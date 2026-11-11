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
    test_state_with_rate_limit(true)
}

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
        trusted_proxy: false,
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

// --- MOH-32: 5xx responses carry no internal text ---------------------------------------------

/// A `tracing` writer that keeps what was logged, so a test can read the server-side record.
#[derive(Clone, Default)]
struct LogCapture(Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for LogCapture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogCapture {
    type Writer = LogCapture;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn login_request() -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header("origin", "http://localhost:4200")
        .header("x-requested-with", "XMLHttpRequest")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"email":"someone@example.com","password":"correct horse battery"}"#,
        ))
        .unwrap()
}

/// The login handler queries the database. The test state points at a server that does not exist,
/// so the query fails with a real driver error whose text names the host and the connection.
#[tokio::test]
async fn database_failure_returns_generic_500_with_request_id_and_logs_the_cause() {
    let capture = LogCapture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(capture.clone())
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let app = app_with_test_routes(test_state_with_rate_limit(false));
    let response = app.oneshot(login_request()).await.unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let header_id = response
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .expect("x-request-id header")
        .to_owned();
    let text = String::from_utf8(collect_bytes(response).await).unwrap();
    let body: serde_json::Value = serde_json::from_str(&text).unwrap();

    assert_eq!(body["error"]["code"], "internal_error");
    assert_eq!(body["error"]["message"], "Internal server error");
    assert_eq!(body["error"]["request_id"], header_id.as_str());
    for internal in [
        "unreachable",
        "Database",
        "query failed",
        "postgres",
        "sqlx",
        "pool",
    ] {
        assert!(
            !text.contains(internal),
            "response body leaked {internal:?}: {text}"
        );
    }

    let logged = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert!(
        logged.contains(&header_id),
        "log has no line with request id {header_id}: {logged}"
    );
    assert!(
        logged.contains("Database query failed"),
        "log is missing the failed step: {logged}"
    );
    assert!(
        !logged.contains("correct horse battery"),
        "log contains the submitted password"
    );
}

#[tokio::test]
async fn panic_response_gets_the_request_id_and_the_cause_goes_to_the_log() {
    let capture = LogCapture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(capture.clone())
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let app = app_with_test_routes(test_state_with_rate_limit(false));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/test-panic")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let header_id = response
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap()
        .to_owned();
    let body: serde_json::Value = serde_json::from_slice(&collect_bytes(response).await).unwrap();
    assert_eq!(body["error"]["request_id"], header_id.as_str());
    assert_eq!(body["error"]["message"], "Internal server error");

    let logged = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
    assert!(
        logged.contains("intentional panic"),
        "panic cause not logged: {logged}"
    );
    assert!(
        logged.contains(&header_id),
        "log lacks the request id: {logged}"
    );
}

/// 4xx messages are for the client and must pass through untouched.
#[tokio::test]
async fn client_errors_keep_their_message() {
    let app = app_with_test_routes(test_state_with_rate_limit(false));
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("origin", "http://localhost:4200")
                .header("x-requested-with", "XMLHttpRequest")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"email":"","password":""}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = serde_json::from_slice(&collect_bytes(response).await).unwrap();
    assert_eq!(body["error"]["code"], "validation_failed");
    assert_eq!(body["error"]["message"], "Email and password are required");
}
