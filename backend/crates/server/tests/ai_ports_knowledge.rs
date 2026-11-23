#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: a failed unwrap is the failed test
//! The knowledge port `ai` declares (`ai::ports::KnowledgeRetrieval`) and the server's adapter for
//! it (`server::ai_ports`), plus the embedder the knowledge indexer gets from the `ai` service
//! (MOH-195). `ai` no longer depends on `knowledge`, so these tests pin what the wiring must keep:
//! the same passages `knowledge::retrieval::search` finds, only the asking tenant's, only published.

use std::time::Duration;

use knowledge::indexer::{EmbedError, Embedder};
use sqlx::PgPool;
use uuid::Uuid;

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            eprintln!("skipping ai_ports_knowledge tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping ai_ports_knowledge tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup() {
    db::testing::migrate().await;
    sqlx::query(
        "TRUNCATE TABLE knowledge_index_state, knowledge_chunks, knowledge_item_tags, \
         knowledge_documents, knowledge_items, knowledge_categories, \
         ai_usage_records, ai_credentials, ai_configurations, \
         audit_logs, outbox_events, tenant_invitations, tenant_memberships, tenants, users \
         RESTART IDENTITY CASCADE",
    )
    .execute(&db::testing::owner_pool())
    .await
    .expect("failed to reset test tables");
}

async fn seed_tenant(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Ports Knowledge Tenant")
        .bind(format!("ports-kn-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

/// A 1536-dimension pgvector literal filled with one value.
fn make_vec(val: f32) -> String {
    let parts: Vec<String> = (0..1536).map(|_| val.to_string()).collect();
    format!("[{}]", parts.join(","))
}

/// Inserts a knowledge item with the given status and one chunk; returns the chunk id.
async fn seed_item_with_chunk(tenant_id: Uuid, status: &str, title: &str, content: &str) -> Uuid {
    let item_id: Uuid = sqlx::query_scalar(
        "INSERT INTO knowledge_items (tenant_id, item_type, title, body, status, source, \
         created_by_display) \
         VALUES ($1, 'article', $2, 'body', $3, 'authored', 'Test User') RETURNING id",
    )
    .bind(tenant_id)
    .bind(title)
    .bind(status)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();
    sqlx::query_scalar(
        "INSERT INTO knowledge_chunks (tenant_id, item_id, ordinal, content, embedding, content_hash) \
         VALUES ($1, $2, 0, $3, $4::vector, $5) RETURNING id",
    )
    .bind(tenant_id)
    .bind(item_id)
    .bind(content)
    .bind(make_vec(0.5))
    .bind(format!("hash-{}", Uuid::new_v4().simple()))
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap()
}

/// The ports the engine runs on, wired as the server wires them.
fn ports(pool: &PgPool) -> ai::ports::AiPorts {
    server::ai_ports::ai_ports(
        pool.clone(),
        escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
    )
}

fn query() -> Vec<f32> {
    vec![0.5; 1536]
}

/// The adapter returns exactly what `knowledge::retrieval::search` returns, field for field.
#[tokio::test]
async fn the_adapter_returns_what_the_knowledge_module_finds() {
    let Some(pool) = get_pool().await else { return };
    setup().await;
    let tenant = seed_tenant(&pool).await;
    let chunk = seed_item_with_chunk(tenant, "published", "Shipping", "Ships in 5-7 days.").await;

    let direct = knowledge::retrieval::search(&pool, tenant, &query(), 5, 0.70)
        .await
        .unwrap();
    let through_port = ports(&pool)
        .knowledge
        .search(&pool, tenant, &query(), 5, 0.70)
        .await
        .unwrap();

    assert_eq!(through_port.len(), 1);
    assert_eq!(direct.len(), through_port.len());
    let (d, p) = (&direct[0], &through_port[0]);
    assert_eq!(p.chunk_id, chunk);
    assert_eq!(p.chunk_id, d.chunk_id);
    assert_eq!(p.item_id, d.item_id);
    assert_eq!(p.tenant_id, tenant);
    assert_eq!(p.content, "Ships in 5-7 days.");
    assert_eq!(p.content, d.content);
    assert_eq!(p.content_hash, d.content_hash);
    assert_eq!(p.item_title, "Shipping");
    assert!((p.similarity - d.similarity).abs() < 1e-9);
    assert!(
        p.similarity > 0.99,
        "an identical vector is a near-perfect match"
    );
}

/// Another tenant's passage with the very same embedding is never returned, and a tenant with no
/// knowledge gets nothing even though other tenants hold identical vectors.
#[tokio::test]
async fn the_adapter_never_returns_another_tenants_passages() {
    let Some(pool) = get_pool().await else { return };
    setup().await;
    let (a, b, empty) = (
        seed_tenant(&pool).await,
        seed_tenant(&pool).await,
        seed_tenant(&pool).await,
    );
    let chunk_a = seed_item_with_chunk(a, "published", "A's policy", "A's text").await;
    let chunk_b = seed_item_with_chunk(b, "published", "B's policy", "B's text").await;
    let ports = ports(&pool);

    let for_a = ports
        .knowledge
        .search(&pool, a, &query(), 5, 0.70)
        .await
        .unwrap();
    assert_eq!(
        for_a.iter().map(|c| c.chunk_id).collect::<Vec<_>>(),
        [chunk_a]
    );
    assert!(for_a.iter().all(|c| c.tenant_id == a));

    let for_b = ports
        .knowledge
        .search(&pool, b, &query(), 5, 0.70)
        .await
        .unwrap();
    assert_eq!(
        for_b.iter().map(|c| c.chunk_id).collect::<Vec<_>>(),
        [chunk_b]
    );

    let for_empty = ports
        .knowledge
        .search(&pool, empty, &query(), 5, 0.70)
        .await
        .unwrap();
    assert!(
        for_empty.is_empty(),
        "no knowledge of its own means no passages"
    );
}

/// A draft is not knowledge the AI may cite; a threshold above the match returns nothing.
#[tokio::test]
async fn the_adapter_skips_drafts_and_matches_below_the_threshold() {
    let Some(pool) = get_pool().await else { return };
    setup().await;
    let tenant = seed_tenant(&pool).await;
    seed_item_with_chunk(tenant, "draft", "Unfinished", "Not yet approved.").await;
    let published = seed_item_with_chunk(tenant, "published", "Live", "Approved.").await;
    let ports = ports(&pool);

    let found = ports
        .knowledge
        .search(&pool, tenant, &query(), 5, 0.70)
        .await
        .unwrap();
    assert_eq!(
        found.iter().map(|c| c.chunk_id).collect::<Vec<_>>(),
        [published]
    );

    // An opposite vector is far from every stored passage.
    let far = vec![-0.5_f32; 1536];
    let none = ports
        .knowledge
        .search(&pool, tenant, &far, 5, 0.70)
        .await
        .unwrap();
    assert!(none.is_empty());
}

/// With no provider configured the indexer is told so (it then parks the item, instead of retrying
/// or failing it for good), exactly as when `ai` implemented the trait itself.
#[tokio::test]
async fn the_embedder_reports_an_unconfigured_provider_as_not_configured() {
    let Some(pool) = get_pool().await else { return };
    setup().await;
    let tenant = seed_tenant(&pool).await;
    let ai = ai::AiService::from_config(pool.clone(), &test_config()).unwrap();

    let result = server::ai_ports::AiEmbedder(ai)
        .embed(tenant, vec!["hello".into()], "req-1".into())
        .await;

    assert!(
        matches!(result, Err(EmbedError::NotConfigured)),
        "expected NotConfigured, got {result:?}"
    );
}

fn test_config() -> config::AppConfig {
    config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: "redis://127.0.0.1:6379".into(),
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
        secrets_root_key: None,
        secrets_root_key_previous: None,
        auth_mfa_required: false,
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
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
        trusted_proxy: false,
        outbound_http: config::OutboundHttpConfig::default(),
    }
}
