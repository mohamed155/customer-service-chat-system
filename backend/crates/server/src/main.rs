use axum::{extract::Request, routing::get, Router};
use kernel::ApiError;
use observability::{health, metrics, ready, request_id_middleware};

fn app() -> Router {
    Router::new()
        .nest(
            "/api/v1",
            Router::new()
                .route("/health", get(health))
                .route("/ready", get(ready))
                .route("/metrics", get(metrics)),
        )
        .fallback(|request: Request| async move {
            let request_id = request
                .headers()
                .get("X-Request-Id")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("unknown");
            ApiError::not_found("Route not found").with_request_id(request_id)
        })
        .layer(axum::middleware::from_fn(request_id_middleware))
}

#[tokio::main]
async fn main() {
    observability::init_observability();
    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8080);
    let address = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .expect("failed to bind HTTP listener");
    tracing::info!(%address, "server listening");
    axum::serve(listener, app()).await.expect("server failed");
}
