use axum::extract::FromRef;
use config::AppConfig;
use observability::health::HealthCheck;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db: PgPool,
    pub cache: Arc<cache::Cache>,
    pub health_checks: Vec<Arc<dyn HealthCheck>>,
    pub escalations: Arc<escalations::presence::Runtime>,
    pub ai: ai::AiService,
}

impl FromRef<AppState> for PgPool {
    fn from_ref(state: &AppState) -> Self {
        state.db.clone()
    }
}

impl FromRef<AppState> for ai::AiService {
    fn from_ref(state: &AppState) -> Self {
        state.ai.clone()
    }
}
