//! Integration tests: RAG graceful degradation (FR-017).
//!
//! Verifies that `AiService::embed_platform` degrades gracefully when the
//! embedding provider is unavailable or misconfigured, returning
//! `AiCallError::NotConfigured` instead of panicking or hanging.  These tests
//! cover every exit path in `embed_platform` that produces `NotConfigured`:
//!
//! - No platform AI configuration row
//! - Platform config with no `embedding_model`
//! - Platform config + model but no credential
//! - Provider without embedding support (Anthropic)
//! - No encryption master key configured
//!
//! Each test also asserts that no `ai_usage_records` are written when the
//! error occurs before any provider call.

use std::time::Duration;

use ai::crypto::{self, MasterKey};
use uuid::Uuid;

const TEST_ENV: config::Environment = config::Environment::Test;

fn test_config() -> config::AppConfig {
    config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: "redis://127.0.0.1:6379".into(),
        auth_jwt_secret: "test-auth-jwt-secret-at-least-32-bytes".into(),
        auth_session_ttl_seconds: 28_800,
        port: 0,
        bind_address: "0.0.0.0".into(),
        environment: TEST_ENV,
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
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
    }
}

fn make_ai_service(pool: sqlx::PgPool) -> ai::AiService {
    let cfg = test_config();
    ai::AiService::from_config(pool, &cfg).unwrap()
}

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            eprintln!("skipping rag_degradation tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping rag_degradation tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query(
        "TRUNCATE TABLE ai_usage_records, ai_credentials, ai_configurations, \
         knowledge_chunks, knowledge_index_state, knowledge_item_tags, \
         knowledge_documents, knowledge_items, knowledge_categories, \
         audit_logs, outbox_events, tenant_invitations, tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("failed to reset test tables");
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    let slug = format!("rag-deg-tenant-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("RAG Degradation Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_platform_ai_config(
    pool: &sqlx::PgPool,
    provider: &str,
    model: &str,
    embedding_model: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO ai_configurations (tenant_id, provider, model, fallbacks, embedding_model) \
         VALUES (NULL, $1, $2, '[]', $3)",
    )
    .bind(provider)
    .bind(model)
    .bind(embedding_model)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_platform_credential(
    pool: &sqlx::PgPool,
    provider: &str,
    api_key: &str,
    master: &MasterKey,
) {
    let aad = crypto::aad(None, provider);
    let (ciphertext, nonce) = crypto::seal(master, &aad, api_key).unwrap();
    let hint = crypto::hint(api_key);
    sqlx::query(
        "INSERT INTO ai_credentials (tenant_id, provider, ciphertext, nonce, key_hint) \
         VALUES (NULL, $1, $2, $3, $4)",
    )
    .bind(provider)
    .bind(ciphertext)
    .bind(nonce)
    .bind(hint)
    .execute(pool)
    .await
    .unwrap();
}

fn master_key() -> MasterKey {
    MasterKey::from_base64("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=").unwrap()
}

fn ai_context(tenant_id: Uuid) -> ai::AiCallContext {
    ai::AiCallContext {
        tenant_id,
        request_id: None,
    }
}

async fn assert_no_usage_records(pool: &sqlx::PgPool, tenant_id: Uuid) {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ai_usage_records WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "NotConfigured must not write usage rows");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn embed_no_platform_config_returns_not_configured() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let ai = make_ai_service(pool.clone());

    let err = ai
        .embed_platform(ai_context(tenant_id), vec!["test query".into()])
        .await
        .unwrap_err();

    assert!(matches!(err, ai::AiCallError::NotConfigured));
    assert_no_usage_records(&pool, tenant_id).await;
}

#[tokio::test]
async fn embed_no_embedding_model_returns_not_configured() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    seed_platform_ai_config(&pool, "openai", "gpt-4", None).await;
    let tenant_id = seed_tenant(&pool).await;
    let ai = make_ai_service(pool.clone());

    let err = ai
        .embed_platform(ai_context(tenant_id), vec!["test query".into()])
        .await
        .unwrap_err();

    assert!(matches!(err, ai::AiCallError::NotConfigured));
    assert_no_usage_records(&pool, tenant_id).await;
}

#[tokio::test]
async fn embed_no_credential_returns_not_configured() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    seed_platform_ai_config(&pool, "openai", "gpt-4", Some("text-embedding-3-small")).await;
    let tenant_id = seed_tenant(&pool).await;
    let ai = make_ai_service(pool.clone());

    let err = ai
        .embed_platform(ai_context(tenant_id), vec!["test query".into()])
        .await
        .unwrap_err();

    assert!(matches!(err, ai::AiCallError::NotConfigured));
    assert_no_usage_records(&pool, tenant_id).await;
}

#[tokio::test]
async fn embed_anthropic_provider_returns_not_configured() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let master = master_key();
    seed_platform_ai_config(
        &pool,
        "anthropic",
        "claude-sonnet-4-20250514",
        Some("claude-embedding"),
    )
    .await;
    seed_platform_credential(&pool, "anthropic", "sk-ant-test", &master).await;
    let tenant_id = seed_tenant(&pool).await;
    let ai = make_ai_service(pool.clone());

    let err = ai
        .embed_platform(ai_context(tenant_id), vec!["test query".into()])
        .await
        .unwrap_err();

    assert!(matches!(err, ai::AiCallError::NotConfigured));
    assert_no_usage_records(&pool, tenant_id).await;
}

#[tokio::test]
async fn embed_no_master_key_returns_not_configured() {
    let pool = match get_pool().await {
        Some(p) => p,
        None => return,
    };
    setup(&pool).await;

    let mut cfg = test_config();
    cfg.ai_key_encryption_key = None;
    let ai = ai::AiService::from_config(pool.clone(), &cfg).unwrap();

    seed_platform_ai_config(&pool, "openai", "gpt-4", Some("text-embedding-3-small")).await;
    let tenant_id = seed_tenant(&pool).await;

    let err = ai
        .embed_platform(ai_context(tenant_id), vec!["test query".into()])
        .await
        .unwrap_err();

    assert!(matches!(err, ai::AiCallError::NotConfigured));
    assert_no_usage_records(&pool, tenant_id).await;
}
