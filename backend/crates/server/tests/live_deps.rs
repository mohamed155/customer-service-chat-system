use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use observability::health::HealthReport;
use server::router;
use server::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

/// Integration test that requires live Postgres and Redis.
/// Skipped unless both TEST_DATABASE_URL and TEST_REDIS_URL are set.
#[tokio::test]
async fn live_readiness_with_real_deps() {
    let db_url = match std::env::var("TEST_DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("skipping live_deps test: TEST_DATABASE_URL not set");
            return;
        }
    };
    let redis_url = match std::env::var("TEST_REDIS_URL") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("skipping live_deps test: TEST_REDIS_URL not set");
            return;
        }
    };

    let pool = db::lazy_pool(&db_url, 2, Duration::from_secs(5));
    let cache = Arc::new(cache::Cache::new(&redis_url).unwrap());
    use cache::RedisHealthCheck;
    use db::PgHealthCheck;

    let health_checks: Vec<Arc<dyn observability::health::HealthCheck>> = vec![
        Arc::new(PgHealthCheck::new(pool.clone())),
        Arc::new(RedisHealthCheck::new((*cache).clone())),
    ];

    let cfg = config::AppConfig {
        database_url: db_url,
        redis_url,
        auth_jwt_secret: "test-auth-jwt-secret-at-least-32-bytes".into(),
        auth_session_ttl_seconds: 28_800,
        port: 0,
        bind_address: "0.0.0.0".into(),
        environment: config::Environment::Test,
        cors_allowed_origins: vec![],
        log_format: config::LogFormat::Pretty,
        smtp_url: None,
        smtp_from: None,
        public_dashboard_url: "http://localhost:4200".into(),
        db_max_connections: 2,
        db_acquire_timeout_ms: 5000,
        ready_probe_timeout_ms: 5000,
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
    let state = AppState {
        config: Arc::new(cfg),
        db: pool.clone(),
        cache,
        health_checks,
        escalations: escalations::presence::Runtime::new(pool, Duration::from_secs(45)),
        ai,
    };

    let app = router::app(state);

    // /health should always 200
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // /ready should 200 with real deps
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: HealthReport =
        serde_json::from_slice(&BodyExt::collect(resp.into_body()).await.unwrap().to_bytes())
            .unwrap();
    assert_eq!(body.checks.len(), 2);
}
