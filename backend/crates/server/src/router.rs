use axum::http::HeaderName;
use axum::middleware::from_fn_with_state;
use axum::response::{IntoResponse, Response};
use axum::{extract::Request, middleware, routing, Router};
use config::AppConfig;
use identity::{principal_middleware, IdentityConfig};
use kernel::ApiError;
use observability::health::HealthReport;
use observability::request_id::{request_id_middleware, REQUEST_ID_HEADER};
use observability::trace::trace_middleware;
use observability::{liveness, metrics};
use std::any::Any;
use std::time::Duration;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::state::AppState;

async fn ready_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> HealthReport {
    observability::health::readiness(
        state.health_checks,
        Duration::from_millis(state.config.ready_probe_timeout_ms),
    )
    .await
}

fn panic_handler(panic_info: Box<dyn Any + Send + 'static>) -> axum::response::Response {
    let payload = panic_info
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic_info.downcast_ref::<String>().map(|s| s.as_str()))
        .unwrap_or("unknown");
    tracing::error!(panic_payload = %payload, "handler panicked");
    ApiError::internal_error("Internal server error").into_response()
}

async fn test_panic_handler() -> Response {
    panic!("intentional panic for testing");
}

fn cors_layer(config: &AppConfig) -> CorsLayer {
    let origins: Vec<_> = config
        .cors_allowed_origins
        .iter()
        .filter_map(|o| o.parse::<axum::http::HeaderValue>().ok())
        .collect();

    let mut headers = vec![
        axum::http::header::CONTENT_TYPE,
        axum::http::header::AUTHORIZATION,
        REQUEST_ID_HEADER.clone(),
        HeaderName::from_static("idempotency-key"),
        HeaderName::from_static("x-tenant-id"),
    ];
    if matches!(
        config.environment,
        config::Environment::Development | config::Environment::Test
    ) {
        headers.push(HeaderName::from_static("x-dev-user-id"));
    }

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PATCH,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers(headers)
        .expose_headers([REQUEST_ID_HEADER.clone()])
}

pub fn app(state: AppState) -> Router {
    let config = state.config.clone();

    let identity_config = IdentityConfig {
        pool: state.db.clone(),
        environment: state.config.environment.clone(),
    };

    Router::new()
        .route("/health", routing::get(liveness))
        .route("/ready", routing::get(ready_handler))
        .route("/metrics", routing::get(metrics))
        .nest(
            "/api/v1",
            Router::new()
                .route("/tenant", routing::get(tenancy::routes::get_tenant))
                .route_layer(from_fn_with_state(
                    state.db.clone(),
                    tenancy::tenant_context_middleware,
                ))
                .route(
                    "/platform/tenants",
                    routing::get(tenancy::routes::list_tenants),
                )
                .route(
                    "/platform/tenants/{id}/switch",
                    routing::post(tenancy::routes::switch_tenant),
                )
                .fallback(|request: Request| async move {
                    let request_id = request
                        .headers()
                        .get(&REQUEST_ID_HEADER)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("unknown");
                    ApiError::not_found("Route not found").with_request_id(request_id)
                })
                .layer(from_fn_with_state(identity_config, principal_middleware))
                .with_state(state.db.clone()),
        )
        .fallback(|request: Request| async move {
            let request_id = request
                .headers()
                .get(&REQUEST_ID_HEADER)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("unknown");
            ApiError::not_found("Route not found").with_request_id(request_id)
        })
        .layer(CatchPanicLayer::custom(panic_handler))
        .layer(middleware::from_fn(trace_middleware))
        .layer(middleware::from_fn(request_id_middleware))
        .layer(cors_layer(&config))
        .with_state(state)
}

pub fn app_with_test_routes(state: AppState) -> Router {
    let config = state.config.clone();

    let identity_config = IdentityConfig {
        pool: state.db.clone(),
        environment: state.config.environment.clone(),
    };

    Router::new()
        .route("/health", routing::get(liveness))
        .route("/ready", routing::get(ready_handler))
        .route("/metrics", routing::get(metrics))
        .route(
            "/test-echo",
            routing::post(
                |body: kernel::ApiJson<serde_json::Value>| async move { axum::Json(body.0) },
            ),
        )
        .route("/test-panic", routing::get(test_panic_handler))
        .nest(
            "/api/v1",
            Router::new()
                .route("/tenant", routing::get(tenancy::routes::get_tenant))
                .route_layer(from_fn_with_state(
                    state.db.clone(),
                    tenancy::tenant_context_middleware,
                ))
                .route(
                    "/platform/tenants",
                    routing::get(tenancy::routes::list_tenants),
                )
                .route(
                    "/platform/tenants/{id}/switch",
                    routing::post(tenancy::routes::switch_tenant),
                )
                .fallback(|request: Request| async move {
                    let request_id = request
                        .headers()
                        .get(&REQUEST_ID_HEADER)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("unknown");
                    ApiError::not_found("Route not found").with_request_id(request_id)
                })
                .layer(from_fn_with_state(identity_config, principal_middleware))
                .with_state(state.db.clone()),
        )
        .fallback(|request: Request| async move {
            let request_id = request
                .headers()
                .get(&REQUEST_ID_HEADER)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("unknown");
            ApiError::not_found("Route not found").with_request_id(request_id)
        })
        .layer(CatchPanicLayer::custom(panic_handler))
        .layer(middleware::from_fn(trace_middleware))
        .layer(middleware::from_fn(request_id_middleware))
        .layer(cors_layer(&config))
        .with_state(state)
}
