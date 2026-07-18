//! Server — HTTP composition root
//!
//! # Purpose
//! Thin binary that loads config, initialises observability, builds shared
//! state (database pool, Redis cache), assembles the Axum router, and binds
//! the HTTP listener with graceful shutdown.
//!
//! # Public Interfaces
//! (Binary crate — no public API. See `server::router` and `server::state`
//! for crate-internal re-exports used by integration tests.)
//!
//! # Dependencies
//! - `config`, `db`, `cache`, `observability`, `kernel`
//! - `axum`, `tokio`, `tower-http`
//!
//! # Extension Points
//! - Add `AppState` fields and seed them in `main`.
//! - Register additional health checks in `health_checks` vec.

use config::AppConfig;
use knowledge::indexer;
use server::router;
use server::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use storage::{InMemoryStorage, ObjectStorage, S3Storage};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let config = match AppConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Config error: {e}");
            std::process::exit(1);
        }
    };

    observability::init_observability(config.log_format.clone());

    let db = db::lazy_pool(
        &config.database_url,
        config.db_max_connections,
        Duration::from_millis(config.db_acquire_timeout_ms),
    );

    let cache = Arc::new(match cache::Cache::new(&config.redis_url) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to create Redis client (will retry on first use): {e}");
            cache::Cache::new("redis://127.0.0.1:6379").unwrap()
        }
    });

    use cache::RedisHealthCheck;
    use db::PgHealthCheck;

    let health_checks: Vec<Arc<dyn observability::health::HealthCheck>> = vec![
        Arc::new(PgHealthCheck::new(db.clone())),
        Arc::new(RedisHealthCheck::new((*cache).clone())),
    ];

    let escalations_runtime =
        escalations::presence::Runtime::new(db.clone(), Duration::from_secs(45));
    if let Err(e) = escalations_runtime.startup_sweep().await {
        tracing::warn!(error = %e, "escalations startup sweep encountered errors (continuing)");
    }

    let ai_service = ai::AiService::from_config(db.clone(), &config).expect("AI service init");

    let state = AppState {
        config: Arc::new(config),
        db,
        cache,
        health_checks,
        escalations: escalations_runtime,
        ai: ai_service,
    };

    let email_sender = router::configured_email_sender(&state.config);
    let app = router::app_with_email_sender(state.clone(), email_sender.clone());

    let storage: Arc<dyn ObjectStorage> = if let Some(s3_config) = &state.config.s3 {
        match S3Storage::new(s3_config).await {
            Ok(s) => Arc::new(s),
            Err(e) => {
                tracing::warn!(error = %e, "failed to create S3 storage, falling back to in-memory");
                Arc::new(InMemoryStorage::default())
            }
        }
    } else {
        Arc::new(InMemoryStorage::default())
    };

    let delivery_worker = tokio::spawn(tenancy::invitations::run_invitation_delivery_worker(
        state.db.clone(),
        email_sender,
    ));
    let escalation_worker = tokio::spawn(escalations::events::run_escalation_outbox_worker(
        state.db.clone(),
        state.escalations.clone(),
    ));
    let agent_responder_worker = tokio::spawn(ai::agent_responder::run_agent_responder_worker(
        state.db.clone(),
        state.ai.clone(),
        state.escalations.clone(),
    ));
    let knowledge_indexer_worker = tokio::spawn(indexer::run_knowledge_indexer_worker(
        state.db.clone(),
        Arc::new(state.ai.clone()) as Arc<dyn knowledge::indexer::Embedder>,
        storage.clone(),
    ));

    let address = format!("{}:{}", state.config.bind_address, state.config.port);
    let listener = TcpListener::bind(&address)
        .await
        .expect("failed to bind HTTP listener");
    tracing::info!(%address, "server listening");

    let grace = state.config.shutdown_grace_seconds;
    tokio::select! {
        result = axum::serve(listener, app).with_graceful_shutdown(shutdown_signal(grace)) => {
            result.expect("server failed");
        }
        result = delivery_worker => {
            panic!("invitation delivery worker stopped unexpectedly: {result:?}");
        }
        result = escalation_worker => {
            panic!("escalation outbox worker stopped unexpectedly: {result:?}");
        }
        result = agent_responder_worker => {
            panic!("agent responder worker stopped unexpectedly: {result:?}");
        }
        result = knowledge_indexer_worker => {
            panic!("knowledge indexer worker stopped unexpectedly: {result:?}");
        }
    }
}

async fn shutdown_signal(grace_seconds: u64) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let sigterm = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let sigterm = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = sigterm => {},
    }

    tracing::info!("shutdown signal received, starting graceful shutdown");
    tokio::time::timeout(
        Duration::from_secs(grace_seconds),
        std::future::pending::<()>(),
    )
    .await
    .ok();
    tracing::info!("graceful shutdown complete");
}
