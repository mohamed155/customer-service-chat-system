//! Envelope encryption of tenant secrets (MOH-33, ADR-014), against real PostgreSQL.
//!
//! Covers the re-encryption job for every kind of secret, re-wrapping after a key rotation,
//! crypto-shredding when a tenant is deleted, that the earlier single-key rows still open, and
//! the credential routes sealing new secrets under envelope encryption.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use secrets::{SecretError, StoredSecret, Vault};
use server::router;
use server::secret_maintenance::{self, Op};
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_ENV: config::Environment = config::Environment::Test;
const AI_KEY: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=";
const INTEGRATION_KEY: &str = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIzNDU=";
const NEW_ROOT_KEY: &str = "enp6enp6enp6enp6enp6enp6enp6enp6enp6enp6enA=";

// Every test resets the same tables, so they take turns.
static DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
        ai_key_encryption_key: Some(AI_KEY.into()),
        integration_secrets_key: Some(INTEGRATION_KEY.into()),
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
    }
}

fn wiremock_state(pool: sqlx::PgPool, openai_uri: &str) -> AppState {
    let mut cfg = test_config();
    cfg.ai_openai_base_url = Some(openai_uri.to_string());
    AppState {
        config: Arc::new(cfg.clone()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(1)),
        ai: ai::AiService::from_config(pool, &cfg).unwrap(),
        budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
    }
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
            eprintln!("skipping engine_respond tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping engine_respond tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn reset(_pool: &sqlx::PgPool) {
    db::testing::migrate().await;
    sqlx::query(
        "TRUNCATE TABLE ai_credentials, ai_configurations, tenant_tools, tool_requests, \
         integration_secrets, integration_connections, tenant_memberships, tenants, users, \
         secret_keks RESTART IDENTITY CASCADE",
    )
    .execute(&db::testing::owner_pool())
    .await
    .expect("failed to reset test tables");
}

fn ai_vault(pool: &sqlx::PgPool) -> Vault {
    Vault::from_config(pool, Some(AI_KEY), Some(AI_KEY), None).unwrap()
}

fn integration_vault(pool: &sqlx::PgPool) -> Vault {
    Vault::from_config(pool, Some(INTEGRATION_KEY), Some(AI_KEY), None).unwrap()
}

struct Seeded {
    tenant_id: Uuid,
    membership_id: Uuid,
}

async fn seed_tenant(label: &str) -> Seeded {
    let slug = format!("envelope-{label}-{}", Uuid::new_v4().simple());
    let seed = db::testing::seed_pool();
    let tenant_id: Uuid =
        sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
            .bind(format!("Envelope {label}"))
            .bind(&slug)
            .fetch_one(&seed)
            .await
            .unwrap();
    let user_id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name) VALUES ($1, 'Envelope User') RETURNING id",
    )
    .bind(format!("{slug}@test.example"))
    .fetch_one(&seed)
    .await
    .unwrap();
    let membership_id: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, 'admin') \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(&seed)
    .await
    .unwrap();
    Seeded {
        tenant_id,
        membership_id,
    }
}

/// A secret sealed the earlier way (one static key), as every row was before MOH-33.
fn legacy(key_b64: &str, aad: &str, plaintext: &str) -> (Vec<u8>, Vec<u8>) {
    let key = ai::crypto::MasterKey::from_base64(key_b64).unwrap();
    ai::crypto::seal(&key, aad, plaintext).unwrap()
}

const WEBHOOK_FIELD: &str = integrations::routes::FIELD_WEBHOOK_TOKEN;

struct Plain {
    ai: &'static str,
    secret: &'static str,
    token: &'static str,
    tool: &'static str,
}

const PLAIN_A: Plain = Plain {
    ai: "sk-ai-a",
    secret: "whsec-a",
    token: "token-a",
    tool: "tool-cred-a",
};
const PLAIN_B: Plain = Plain {
    ai: "sk-ai-b",
    secret: "whsec-b",
    token: "token-b",
    tool: "tool-cred-b",
};

struct Rows {
    ai_credential: Uuid,
    integration_secret: Uuid,
    connection: Uuid,
    tool: Uuid,
}

/// One row of every kind of secret for a tenant, all sealed the earlier way.
async fn seed_legacy_secrets(tenant: &Seeded, plain: &Plain) -> Rows {
    let seed = db::testing::seed_pool();
    let tenant_id = tenant.tenant_id;

    let (ct, nonce) = legacy(
        AI_KEY,
        &ai::crypto::aad(Some(tenant_id), "openai"),
        plain.ai,
    );
    let ai_credential: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_credentials (tenant_id, provider, ciphertext, nonce, key_hint) \
         VALUES ($1, 'openai', $2, $3, 'hint') RETURNING id",
    )
    .bind(tenant_id)
    .bind(&ct)
    .bind(&nonce)
    .fetch_one(&seed)
    .await
    .unwrap();

    let catalog_id: Uuid =
        sqlx::query_scalar("SELECT id FROM integration_catalog WHERE slug = 'whatsapp'")
            .fetch_one(&seed)
            .await
            .unwrap();
    let (token_ct, token_nonce) = legacy(
        INTEGRATION_KEY,
        &integrations::crypto::aad(tenant_id, "whatsapp", WEBHOOK_FIELD),
        plain.token,
    );
    let connection: Uuid = sqlx::query_scalar(
        "INSERT INTO integration_connections \
         (tenant_id, catalog_id, is_active, config, webhook_token_hash, \
          webhook_token_ciphertext, webhook_token_nonce, connected_at) \
         VALUES ($1, $2, true, '{}'::jsonb, $3, $4, $5, now()) RETURNING id",
    )
    .bind(tenant_id)
    .bind(catalog_id)
    .bind(Uuid::new_v4().as_bytes().to_vec())
    .bind(&token_ct)
    .bind(&token_nonce)
    .fetch_one(&seed)
    .await
    .unwrap();
    let (secret_ct, secret_nonce) = legacy(
        INTEGRATION_KEY,
        &integrations::crypto::aad(tenant_id, "whatsapp", "app_secret"),
        plain.secret,
    );
    let integration_secret: Uuid = sqlx::query_scalar(
        "INSERT INTO integration_secrets \
         (tenant_id, connection_id, field_key, ciphertext, nonce, hint) \
         VALUES ($1, $2, 'app_secret', $3, $4, 'hint') RETURNING id",
    )
    .bind(tenant_id)
    .bind(connection)
    .bind(&secret_ct)
    .bind(&secret_nonce)
    .fetch_one(&seed)
    .await
    .unwrap();

    let (tool_ct, tool_nonce) = legacy(
        AI_KEY,
        &ai::crypto::aad(Some(tenant_id), "lookup_order"),
        plain.tool,
    );
    let b64 = &base64::engine::general_purpose::STANDARD;
    let tool: Uuid = sqlx::query_scalar(
        "INSERT INTO tenant_tools \
         (tenant_id, name, description, input_schema, endpoint_url, credential_ciphertext, \
          classification, created_by_membership_id) \
         VALUES ($1, 'lookup_order', 'd', '{\"type\":\"object\"}'::jsonb, 'https://example.test', \
                 $2, 'auto', $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(format!(
        "{}||{}",
        base64::Engine::encode(b64, &tool_ct),
        base64::Engine::encode(b64, &tool_nonce)
    ))
    .bind(tenant.membership_id)
    .fetch_one(&seed)
    .await
    .unwrap();

    Rows {
        ai_credential,
        integration_secret,
        connection,
        tool,
    }
}

async fn seed_legacy_platform_credential() {
    let (ct, nonce) = legacy(AI_KEY, &ai::crypto::aad(None, "openai"), "sk-platform");
    sqlx::query("SELECT * FROM xtenant.platform_ai_credential_set($1, 'openai', $2, $3, 'hint')")
        .bind(Uuid::nil())
        .bind(&ct)
        .bind(&nonce)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap();
}

type Columns = (
    Vec<u8>,
    Vec<u8>,
    Option<Vec<u8>>,
    Option<String>,
    Option<i32>,
    Option<String>,
);

async fn columns(sql: &str, id: Uuid) -> StoredSecret {
    let (ct, nonce, dek, kek, ver, alg): Columns =
        sqlx::query_as(sqlx::AssertSqlSafe(sql.to_owned()))
            .bind(id)
            .fetch_one(&db::testing::seed_pool())
            .await
            .unwrap();
    StoredSecret::from_columns(ct, nonce, dek, kek, ver, alg).unwrap()
}

async fn ai_secret(id: Uuid) -> StoredSecret {
    columns(
        "SELECT ciphertext, nonce, wrapped_dek, kek_id, kek_version, algorithm \
         FROM ai_credentials WHERE id = $1",
        id,
    )
    .await
}

async fn integration_secret(id: Uuid) -> StoredSecret {
    columns(
        "SELECT ciphertext, nonce, wrapped_dek, kek_id, kek_version, algorithm \
         FROM integration_secrets WHERE id = $1",
        id,
    )
    .await
}

async fn webhook_token(id: Uuid) -> StoredSecret {
    columns(
        "SELECT webhook_token_ciphertext, webhook_token_nonce, webhook_token_wrapped_dek, \
                webhook_token_kek_id, webhook_token_kek_version, webhook_token_algorithm \
         FROM integration_connections WHERE id = $1",
        id,
    )
    .await
}

async fn tool_secret(id: Uuid) -> StoredSecret {
    let (text, dek, kek, ver, alg): (
        String,
        Option<Vec<u8>>,
        Option<String>,
        Option<i32>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT credential_ciphertext, credential_wrapped_dek, credential_kek_id, \
                credential_kek_version, credential_algorithm FROM tenant_tools WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();
    tools::credential::decode(&text, dek, kek, ver, alg).unwrap()
}

async fn open(vault: &Vault, aad: &str, stored: &StoredSecret) -> String {
    String::from_utf8(vault.open(aad, stored).await.unwrap().to_vec()).unwrap()
}

/// Every secret of a tenant opens to what was put in, whichever way it is sealed.
async fn assert_opens(pool: &sqlx::PgPool, tenant: &Seeded, rows: &Rows, plain: &Plain) {
    let t = tenant.tenant_id;
    let ai = ai_vault(pool);
    let integrations = integration_vault(pool);
    assert_eq!(
        open(
            &ai,
            &ai::crypto::aad(Some(t), "openai"),
            &ai_secret(rows.ai_credential).await
        )
        .await,
        plain.ai
    );
    assert_eq!(
        open(
            &integrations,
            &integrations::crypto::aad(t, "whatsapp", "app_secret"),
            &integration_secret(rows.integration_secret).await
        )
        .await,
        plain.secret
    );
    assert_eq!(
        open(
            &integrations,
            &integrations::crypto::aad(t, "whatsapp", WEBHOOK_FIELD),
            &webhook_token(rows.connection).await
        )
        .await,
        plain.token
    );
    assert_eq!(
        open(
            &ai,
            &ai::crypto::aad(Some(t), "lookup_order"),
            &tool_secret(rows.tool).await
        )
        .await,
        plain.tool
    );
}

async fn all_secrets(rows: &Rows) -> [StoredSecret; 4] {
    [
        ai_secret(rows.ai_credential).await,
        integration_secret(rows.integration_secret).await,
        webhook_token(rows.connection).await,
        tool_secret(rows.tool).await,
    ]
}

macro_rules! db_or_return {
    () => {{
        match get_pool().await {
            Some(pool) => pool,
            None => return,
        }
    }};
}

#[tokio::test]
async fn the_job_seals_every_kind_of_secret_with_envelope_encryption_and_they_still_open() {
    let pool = db_or_return!();
    let _serial = DB_LOCK.lock().await;
    reset(&pool).await;
    let cfg = test_config();
    let a = seed_tenant("a").await;
    let b = seed_tenant("b").await;
    let rows_a = seed_legacy_secrets(&a, &PLAIN_A).await;
    let rows_b = seed_legacy_secrets(&b, &PLAIN_B).await;
    seed_legacy_platform_credential().await;

    // Before: every secret is the earlier kind and opens.
    for secret in all_secrets(&rows_a).await {
        assert!(secret.envelope.is_none());
    }
    assert_opens(&pool, &a, &rows_a, &PLAIN_A).await;

    let report = secret_maintenance::run(&pool, &cfg, Op::Reencrypt)
        .await
        .unwrap();
    assert_eq!(report.failed, 0);
    assert_eq!(report.raced, 0);
    // Two tenants with four secrets each, and the platform credential.
    assert_eq!((report.scanned, report.changed), (9, 9));

    // After: each carries a data key under its own tenant's key, and still opens.
    for (tenant, rows) in [(&a, &rows_a), (&b, &rows_b)] {
        for secret in all_secrets(rows).await {
            let envelope = secret.envelope.as_ref().expect("sealed with a data key");
            assert_eq!(envelope.kek_id, format!("tenant:{}", tenant.tenant_id));
            assert_eq!(envelope.kek_version, 1);
            assert_eq!(envelope.algorithm, "aes-256-gcm");
        }
    }
    assert_opens(&pool, &a, &rows_a, &PLAIN_A).await;
    assert_opens(&pool, &b, &rows_b, &PLAIN_B).await;
    let platform: (Option<String>,) = sqlx::query_as(
        "SELECT kek_id FROM ai_credentials WHERE tenant_id IS NULL AND provider = 'openai'",
    )
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();
    assert_eq!(platform.0.as_deref(), Some("platform"));

    // Every secret has its own data key.
    let keys: Vec<Vec<u8>> = all_secrets(&rows_a)
        .await
        .into_iter()
        .chain(all_secrets(&rows_b).await)
        .map(|s| s.envelope.unwrap().wrapped_dek)
        .collect();
    let distinct: std::collections::HashSet<_> = keys.iter().collect();
    assert_eq!(distinct.len(), keys.len());

    // Run again: nothing left to do.
    let again = secret_maintenance::run(&pool, &cfg, Op::Reencrypt)
        .await
        .unwrap();
    assert_eq!((again.scanned, again.changed, again.failed), (9, 0, 0));
}

#[tokio::test]
async fn a_secret_that_does_not_open_is_reported_and_left_alone() {
    let pool = db_or_return!();
    let _serial = DB_LOCK.lock().await;
    reset(&pool).await;
    let cfg = test_config();
    let tenant = seed_tenant("broken").await;
    // Sealed for another scope, so it cannot be opened with this row's scope.
    let (ct, nonce) = legacy(AI_KEY, "some|other|scope", "sk-x");
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_credentials (tenant_id, provider, ciphertext, nonce, key_hint) \
         VALUES ($1, 'openai', $2, $3, 'hint') RETURNING id",
    )
    .bind(tenant.tenant_id)
    .bind(&ct)
    .bind(&nonce)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();

    let report = secret_maintenance::run(&pool, &cfg, Op::Reencrypt)
        .await
        .unwrap();
    assert_eq!((report.scanned, report.changed, report.failed), (1, 0, 1));
    let row = ai_secret(id).await;
    assert!(row.envelope.is_none());
    assert_eq!(row.ciphertext, ct, "the row is exactly as it was");
}

#[tokio::test]
async fn a_key_rotation_is_finished_by_rewrapping_without_touching_the_secrets() {
    let pool = db_or_return!();
    let _serial = DB_LOCK.lock().await;
    reset(&pool).await;
    let cfg = test_config();
    let a = seed_tenant("rot-a").await;
    let b = seed_tenant("rot-b").await;
    let rows_a = seed_legacy_secrets(&a, &PLAIN_A).await;
    let rows_b = seed_legacy_secrets(&b, &PLAIN_B).await;
    secret_maintenance::run(&pool, &cfg, Op::Reencrypt)
        .await
        .unwrap();
    let before = all_secrets(&rows_a).await;

    assert_eq!(
        secret_maintenance::rotate_kek(&pool, &cfg, Some(a.tenant_id))
            .await
            .unwrap(),
        2
    );
    // Old secrets keep opening under the old version until they are re-wrapped.
    assert_opens(&pool, &a, &rows_a, &PLAIN_A).await;

    let report = secret_maintenance::run(&pool, &cfg, Op::Rewrap)
        .await
        .unwrap();
    assert_eq!((report.failed, report.changed), (0, 4));

    let after = all_secrets(&rows_a).await;
    for (old, new) in before.iter().zip(&after) {
        assert_eq!(new.envelope.as_ref().unwrap().kek_version, 2);
        assert_eq!(
            new.ciphertext, old.ciphertext,
            "the secret itself is not re-encrypted"
        );
        assert_eq!(new.nonce, old.nonce);
        assert_ne!(
            new.envelope.as_ref().unwrap().wrapped_dek,
            old.envelope.as_ref().unwrap().wrapped_dek
        );
    }
    assert_opens(&pool, &a, &rows_a, &PLAIN_A).await;
    // The other tenant's key was not rotated.
    for secret in all_secrets(&rows_b).await {
        assert_eq!(secret.envelope.unwrap().kek_version, 1);
    }
    // Nothing more to do.
    let again = secret_maintenance::run(&pool, &cfg, Op::Rewrap)
        .await
        .unwrap();
    assert_eq!(again.changed, 0);
}

#[tokio::test]
async fn deleting_a_tenant_destroys_its_key_and_only_its_key() {
    let pool = db_or_return!();
    let _serial = DB_LOCK.lock().await;
    reset(&pool).await;
    let cfg = test_config();
    let a = seed_tenant("gone").await;
    let b = seed_tenant("kept").await;
    let rows_a = seed_legacy_secrets(&a, &PLAIN_A).await;
    let rows_b = seed_legacy_secrets(&b, &PLAIN_B).await;
    secret_maintenance::run(&pool, &cfg, Op::Reencrypt)
        .await
        .unwrap();

    sqlx::query("UPDATE tenants SET deleted_at = now() WHERE id = $1")
        .bind(a.tenant_id)
        .execute(&db::testing::owner_pool())
        .await
        .unwrap();

    let kek: (String, Option<Vec<u8>>) =
        sqlx::query_as("SELECT state, wrapped_kek FROM secret_keks WHERE kek_id = $1")
            .bind(format!("tenant:{}", a.tenant_id))
            .fetch_one(&db::testing::seed_pool())
            .await
            .unwrap();
    assert_eq!(kek.0, "destroyed");
    assert!(kek.1.is_none(), "the wrapped key is gone, not just flagged");

    // A process that starts afterwards (no cache) cannot open any of the tenant's secrets.
    let fresh = ai_vault(&pool);
    let stored = ai_secret(rows_a.ai_credential).await;
    let error = fresh
        .open(&ai::crypto::aad(Some(a.tenant_id), "openai"), &stored)
        .await
        .unwrap_err();
    assert_eq!(error, SecretError::KekDestroyed);
    let integrations = integration_vault(&pool);
    assert!(integrations
        .open(
            &integrations::crypto::aad(a.tenant_id, "whatsapp", WEBHOOK_FIELD),
            &webhook_token(rows_a.connection).await
        )
        .await
        .is_err());

    // The other tenant is unaffected.
    assert_opens(&pool, &b, &rows_b, &PLAIN_B).await;

    // Bringing the tenant back does not bring the key back.
    sqlx::query("UPDATE tenants SET deleted_at = NULL WHERE id = $1")
        .bind(a.tenant_id)
        .execute(&db::testing::owner_pool())
        .await
        .unwrap();
    assert!(ai_vault(&pool)
        .open(&ai::crypto::aad(Some(a.tenant_id), "openai"), &stored)
        .await
        .is_err());
}

#[tokio::test]
async fn a_sealed_secret_does_not_open_for_another_tenant_or_field() {
    let pool = db_or_return!();
    let _serial = DB_LOCK.lock().await;
    reset(&pool).await;
    let a = seed_tenant("scope-a").await;
    let b = seed_tenant("scope-b").await;
    let vault = ai_vault(&pool);
    let aad_a = ai::crypto::aad(Some(a.tenant_id), "openai");
    let sealed = vault
        .seal(Some(a.tenant_id), &aad_a, b"sk-secret")
        .await
        .unwrap();

    assert_eq!(open(&vault, &aad_a, &sealed).await, "sk-secret");
    // Another tenant's scope, or another provider's.
    assert!(vault
        .open(&ai::crypto::aad(Some(b.tenant_id), "openai"), &sealed)
        .await
        .is_err());
    assert!(vault
        .open(&ai::crypto::aad(Some(a.tenant_id), "anthropic"), &sealed)
        .await
        .is_err());
    // Moved under the other tenant's key, with that tenant's scope: still refused.
    vault
        .seal(
            Some(b.tenant_id),
            &ai::crypto::aad(Some(b.tenant_id), "openai"),
            b"x",
        )
        .await
        .unwrap();
    let mut moved = sealed.clone();
    moved.envelope.as_mut().unwrap().kek_id = format!("tenant:{}", b.tenant_id);
    assert!(vault
        .open(&ai::crypto::aad(Some(b.tenant_id), "openai"), &moved)
        .await
        .is_err());
}

#[tokio::test]
async fn a_credential_saved_through_the_api_is_sealed_with_envelope_encryption() {
    let pool = db_or_return!();
    let _serial = DB_LOCK.lock().await;
    reset(&pool).await;
    let openai_mock = wiremock::MockServer::start().await;
    let state = wiremock_state(pool.clone(), &openai_mock.uri());
    let tenant = seed_tenant("api").await;
    let user_id: Uuid = sqlx::query_scalar("SELECT user_id FROM tenant_memberships WHERE id = $1")
        .bind(tenant.membership_id)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap();

    let request = Request::builder()
        .uri("/api/v1/tenant/ai/credentials/openai")
        .method(Method::PUT)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant.tenant_id.to_string())
        .header("content-type", "application/json")
        .body(Body::from(r#"{"api_key":"sk-from-the-api-1234"}"#))
        .unwrap();
    let response = router::app_with_test_routes(state.clone())
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let (id,): (Uuid,) = sqlx::query_as(
        "SELECT id FROM ai_credentials WHERE tenant_id = $1 AND provider = 'openai'",
    )
    .bind(tenant.tenant_id)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap();
    let stored = ai_secret(id).await;
    let envelope = stored
        .envelope
        .as_ref()
        .expect("new secrets get a data key");
    assert_eq!(envelope.kek_id, format!("tenant:{}", tenant.tenant_id));
    // The plaintext is not in the row, and the service resolves it.
    assert!(!String::from_utf8_lossy(&stored.ciphertext).contains("sk-from-the-api"));
    let (key, from_tenant) = ai::resolution::resolve_credential(
        &pool,
        state.ai.vault().expect("keys are configured"),
        ai::resolution::Scope::Tenant(tenant.tenant_id),
        "openai",
    )
    .await
    .unwrap()
    .expect("the credential resolves");
    assert!(from_tenant);
    assert_eq!(key.expose(), "sk-from-the-api-1234");
}

#[tokio::test]
async fn an_earlier_row_still_resolves_without_the_job() {
    let pool = db_or_return!();
    let _serial = DB_LOCK.lock().await;
    reset(&pool).await;
    let tenant = seed_tenant("old").await;
    seed_legacy_secrets(&tenant, &PLAIN_A).await;
    let service = ai::AiService::from_config(pool.clone(), &test_config()).unwrap();
    let (key, _) = ai::resolution::resolve_credential(
        &pool,
        service.vault().unwrap(),
        ai::resolution::Scope::Tenant(tenant.tenant_id),
        "openai",
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(key.expose(), PLAIN_A.ai);
}

#[tokio::test]
async fn a_root_key_rotation_moves_the_tenant_keys_and_leaves_the_secrets_alone() {
    let pool = db_or_return!();
    let _serial = DB_LOCK.lock().await;
    reset(&pool).await;
    let cfg = test_config();
    let tenant = seed_tenant("root").await;
    let rows = seed_legacy_secrets(&tenant, &PLAIN_A).await;
    secret_maintenance::run(&pool, &cfg, Op::Reencrypt)
        .await
        .unwrap();
    let before = all_secrets(&rows).await;
    let aad = ai::crypto::aad(Some(tenant.tenant_id), "openai");

    // The operator sets a new root key and keeps the old one for unwrapping.
    let mut rotated = test_config();
    rotated.secrets_root_key = Some(NEW_ROOT_KEY.into());
    rotated.secrets_root_key_previous = Some(AI_KEY.into());
    let both = Vault::from_config(&pool, Some(AI_KEY), Some(NEW_ROOT_KEY), Some(AI_KEY)).unwrap();
    assert_eq!(open(&both, &aad, &before[0]).await, PLAIN_A.ai);

    // The new root key alone cannot open the keys the old one still wraps.
    let only_new = Vault::from_config(&pool, Some(AI_KEY), Some(NEW_ROOT_KEY), None).unwrap();
    assert!(only_new.open(&aad, &before[0]).await.is_err());

    // Re-wrapping the tenant key (one key, not one per secret) frees the old root key.
    assert_eq!(
        secret_maintenance::rewrap_keks(&pool, &rotated)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        secret_maintenance::rewrap_keks(&pool, &rotated)
            .await
            .unwrap(),
        0
    );
    let fresh = Vault::from_config(&pool, Some(AI_KEY), Some(NEW_ROOT_KEY), None).unwrap();
    assert_eq!(open(&fresh, &aad, &before[0]).await, PLAIN_A.ai);

    // No secret was rewritten.
    let after = all_secrets(&rows).await;
    assert_eq!(before.to_vec(), after.to_vec());
}
