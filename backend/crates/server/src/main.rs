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
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    // No-op if `.env` is absent; never overrides vars already set in the environment.
    dotenvy::dotenv().ok();

    // Migrations are a separate step, never part of serving: they run as the schema owner
    // (`app_owner`) over their own connection string, while the server connects as
    // `app_runtime`, which cannot change the schema (ADR-004 section 3).
    if std::env::args().nth(1).as_deref() == Some("migrate") {
        std::process::exit(migrate().await);
    }

    let config = match AppConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Config error: {e}");
            std::process::exit(1);
        }
    };

    observability::init_observability(
        config.log_format.clone(),
        config.otlp_endpoint.clone(),
        config.trace_sample_ratio,
        config.sentry_dsn.clone(),
        config.release_sha.clone(),
    );

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

    observability::metrics::spawn_dependency_updater(health_checks.clone());

    {
        let pool = db.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
            loop {
                interval.tick().await;
                if let Ok((pending, _, _)) = platform::queries::queue_health(&pool).await {
                    observability::metrics::set_outbox_backlog(pending);
                }
            }
        });
    }

    let escalations_runtime =
        escalations::presence::Runtime::new(db.clone(), Duration::from_secs(45));
    if let Err(e) = escalations_runtime.startup_sweep().await {
        tracing::warn!(error = %e, "escalations startup sweep encountered errors (continuing)");
    }

    let ai_service = ai::AiService::from_config(db.clone(), &config).expect("AI service init");

    let budget_resolver = Arc::new(cache::limiter::BudgetResolver::new(
        config.rate_limit_max_override,
    ));

    let state = AppState {
        config: Arc::new(config),
        db,
        cache,
        health_checks,
        escalations: escalations_runtime,
        ai: ai_service,
        budget_resolver,
    };

    let (storage, _storage_backend) = storage::select_storage(state.config.s3.as_ref()).await;
    let email_sender = router::configured_email_sender(&state.config);
    let app =
        router::app_with_email_sender(state.clone(), email_sender.clone(), Some(storage.clone()));

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
    let widget_session_sweeper = {
        let pool = state.db.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(6 * 3600));
            loop {
                interval.tick().await;
                match widgets::queries::delete_expired_sessions(&pool).await {
                    Ok(count) => {
                        if count > 0 {
                            tracing::info!(count, "widget session sweep: deleted expired sessions");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(%e, "widget session sweep failed");
                    }
                }
            }
        })
    };
    let tool_expiry_sweeper = {
        let pool = state.db.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                match tools::approval::sweep_expired(&pool).await {
                    Ok(count) => {
                        if count > 0 {
                            tracing::info!(count, "tool expiry sweep: expired tool requests");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(%e, "tool expiry sweep failed");
                    }
                }
            }
        })
    };
    let notification_retention_sweeper = {
        let pool = state.db.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(6 * 3600));
            loop {
                interval.tick().await;
                match sqlx::query(
                    "DELETE FROM notifications WHERE created_at < now() - interval '90 days'",
                )
                .execute(&pool)
                .await
                {
                    Ok(result) => {
                        let count = result.rows_affected();
                        if count > 0 {
                            tracing::info!(
                                count,
                                "notification retention sweep: deleted expired notifications"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(%e, "notification retention sweep failed");
                    }
                }
            }
        })
    };
    let integration_retention_sweeper = {
        let pool = state.db.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(6 * 3600));
            loop {
                interval.tick().await;
                match integrations::retention::sweep_expired(&pool).await {
                    Ok(count) => {
                        if count > 0 {
                            tracing::info!(
                                count,
                                "integration retention sweep: deleted expired rows"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(%e, "integration retention sweep failed");
                    }
                }
            }
        })
    };
    let knowledge_indexer_worker = tokio::spawn(indexer::run_knowledge_indexer_worker(
        state.db.clone(),
        Arc::new(state.ai.clone()) as Arc<dyn knowledge::indexer::Embedder>,
        storage.clone(),
    ));
    let notifications_worker = tokio::spawn(notifications::worker::run_notification_outbox_worker(
        state.db.clone(),
        state.escalations.clone(),
    ));

    // WhatsApp channel workers (spec 029)
    let whatsapp_api: Arc<dyn whatsapp::api::WhatsAppApi> =
        Arc::new(whatsapp::api::GraphWhatsAppApi::new());
    let whatsapp_master_key = state
        .config
        .integration_secrets_key
        .as_ref()
        .and_then(|key| integrations::crypto::MasterKey::from_base64(key).ok());
    if let Some(mk) = whatsapp_master_key {
        let mk = Arc::new(mk);
        let _media_worker = tokio::spawn(whatsapp::media::run_media_fetch_worker(
            state.db.clone(),
            whatsapp_api.clone(),
            storage.clone(),
            mk.clone(),
        ));
        let _sender_worker = tokio::spawn(whatsapp::sender::run_whatsapp_sender_worker(
            state.db.clone(),
            whatsapp_api.clone(),
            mk,
        ));
    } else {
        tracing::warn!("integration_secrets_key not configured — whatsapp workers disabled");
    }

    let address = format!("{}:{}", state.config.bind_address, state.config.port);
    let listener = TcpListener::bind(&address)
        .await
        .expect("failed to bind HTTP listener");
    tracing::info!(%address, "server listening");

    tokio::select! {
        result = axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).with_graceful_shutdown(shutdown_signal()) => {
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
        result = widget_session_sweeper => {
            panic!("widget session sweeper stopped unexpectedly: {result:?}");
        }
        result = tool_expiry_sweeper => {
            panic!("tool expiry sweeper stopped unexpectedly: {result:?}");
        }
        result = knowledge_indexer_worker => {
            panic!("knowledge indexer worker stopped unexpectedly: {result:?}");
        }
        result = notifications_worker => {
            panic!("notifications outbox worker stopped unexpectedly: {result:?}");
        }
        result = notification_retention_sweeper => {
            panic!("notification retention sweeper stopped unexpectedly: {result:?}");
        }
        result = integration_retention_sweeper => {
            panic!("integration retention sweeper stopped unexpectedly: {result:?}");
        }
    }

    tracing::info!("observability: flushing remaining events");
    observability::errors::flush(Duration::from_secs(2));
    tracing::info!("graceful shutdown complete");
}

async fn shutdown_signal() {
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
}

/// `server migrate`: applies the embedded migrations as the schema owner and returns the process
/// exit code. Reads only `MIGRATION_DATABASE_URL`; there is deliberately no fallback to
/// `DATABASE_URL`, which points at `app_runtime` and cannot run DDL.
async fn migrate() -> i32 {
    let url = match std::env::var("MIGRATION_DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => {
            eprintln!(
                "migration failed: MIGRATION_DATABASE_URL is not set. Point it at the app_owner \
                 role (see docs/configuration.md); DATABASE_URL is the runtime role and cannot \
                 change the schema."
            );
            return 1;
        }
    };
    let pool = db::lazy_pool(&url, 1, Duration::from_secs(30));
    match db::run_migrations(&pool).await {
        Ok(()) => {
            println!("migrations applied");
            0
        }
        Err(e) => {
            eprintln!("migration failed: {e}");
            1
        }
    }
}
