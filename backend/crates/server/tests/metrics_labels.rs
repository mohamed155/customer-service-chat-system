use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use server::router;
use server::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

fn make_state() -> AppState {
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
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
        trusted_proxy: false,
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
async fn metrics_body_contains_no_uuid_label_values() {
    observability::metrics::init();
    let app = router::app(make_state());

    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(
                    "/api/v1/widget/v1/conversations/3f9a2b1c-1111-2222-3333-444455556666/messages",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    let body = std::str::from_utf8(&bytes).unwrap();

    let uuid_re = regex::Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}").unwrap();
    let mut matches: Vec<&str> = uuid_re.find_iter(body).map(|m| m.as_str()).collect();
    matches.sort();
    matches.dedup();
    assert!(
        matches.is_empty(),
        "Metrics exposition contains UUID-shaped label value(s): {:?}\n\
         Per Principle II and metrics-exposition.md, no metric may carry a tenant_id, \
         user ID, session ID, or raw request path as a label value. Route templates \
         (e.g. /tenant/conversations/:id) must appear as-is, never with concrete UUIDs.",
        matches
    );
    assert!(
        body.contains("route="),
        "Metrics exposition must contain route= labels (not path=)\n\
         Per data-model.md, the label key is 'route', not 'path'."
    );
}
