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
use server::run_mode::RunMode;
use server::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::task::JoinSet;

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

    let mode = match RunMode::from_env() {
        Ok(mode) => mode,
        Err(e) => {
            eprintln!("Config error: {e}");
            std::process::exit(1);
        }
    };

    let mut config = match AppConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Config error: {e}");
            std::process::exit(1);
        }
    };
    if mode == RunMode::Gateway {
        // The public side serves no cookie sessions and decrypts no AI provider keys, so it
        // doesn't keep the secrets for them in memory (ADR-010 section 3). It still needs the
        // integration secrets key to verify channel-webhook signatures until envelope encryption
        // (ADR-014) replaces it.
        config.auth_jwt_secret = String::new();
        config.ai_key_encryption_key = None;
    }

    observability::init_observability(
        config.log_format.clone(),
        config.otlp_endpoint.clone(),
        config.trace_sample_ratio,
        config.sentry_dsn.clone(),
        config.release_sha.clone(),
    );

    tracing::info!(run_mode = %mode, "starting");
    if mode != RunMode::All {
        tracing::warn!(
            run_mode = %mode,
            "split run mode: realtime events and agent presence are in-process (ADR-009 RT1), so \
             events produced in another process are not delivered to clients connected here"
        );
    }

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

    if mode != RunMode::Gateway {
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
    if mode.runs_workers() {
        if let Err(e) = escalations_runtime.startup_sweep().await {
            tracing::warn!(error = %e, "escalations startup sweep encountered errors (continuing)");
        }
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
    let app = router::app_for_mode(state.clone(), mode, email_sender.clone(), storage.clone());

    let mut workers = if mode.runs_workers() {
        spawn_workers(&state, storage.clone(), email_sender.clone())
    } else {
        JoinSet::new()
    };

    let address = format!("{}:{}", state.config.bind_address, state.config.port);
    let listener = TcpListener::bind(&address)
        .await
        .expect("failed to bind HTTP listener");
    tracing::info!(%address, "server listening");

    tokio::select! {
        result = axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).with_graceful_shutdown(shutdown_signal()) => {
            result.expect("server failed");
        }
        // Pending forever when this mode runs no workers (the set is empty).
        Some(finished) = workers.join_next() => {
            panic!("background worker stopped unexpectedly: {finished:?}");
        }
    }

    tracing::info!("observability: flushing remaining events");
    observability::errors::flush(Duration::from_secs(2));
    tracing::info!("graceful shutdown complete");
}

/// Starts every background worker and sweeper and returns the set, so `main` can stop the process
/// when one of them ends. Only the `worker` and `all` modes call this.
fn spawn_workers(
    state: &AppState,
    storage: Arc<dyn storage::ObjectStorage>,
    email_sender: Arc<dyn email::EmailSender>,
) -> JoinSet<&'static str> {
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
                // Spans every tenant, so it goes through the audited function
                // `xtenant.notification_retention_sweep`.
                match sqlx::query_scalar::<_, i64>("SELECT xtenant.notification_retention_sweep()")
                    .fetch_one(&pool)
                    .await
                {
                    Ok(count) => {
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

    let mut workers = JoinSet::new();
    workers.spawn(async move {
        let _ = delivery_worker.await;
        "delivery worker"
    });
    workers.spawn(async move {
        let _ = escalation_worker.await;
        "escalation worker"
    });
    workers.spawn(async move {
        let _ = agent_responder_worker.await;
        "agent responder worker"
    });
    workers.spawn(async move {
        let _ = widget_session_sweeper.await;
        "widget session sweeper"
    });
    workers.spawn(async move {
        let _ = tool_expiry_sweeper.await;
        "tool expiry sweeper"
    });
    workers.spawn(async move {
        let _ = knowledge_indexer_worker.await;
        "knowledge indexer worker"
    });
    workers.spawn(async move {
        let _ = notifications_worker.await;
        "notifications worker"
    });
    workers.spawn(async move {
        let _ = notification_retention_sweeper.await;
        "notification retention sweeper"
    });
    workers.spawn(async move {
        let _ = integration_retention_sweeper.await;
        "integration retention sweeper"
    });
    workers
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
