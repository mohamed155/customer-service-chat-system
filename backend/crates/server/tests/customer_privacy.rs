#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use storage::ObjectStorage;
use uuid::Uuid;

async fn pool() -> Option<PgPool> {
    let required = std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1");
    let Ok(url) = std::env::var("DATABASE_URL") else {
        assert!(!required, "DATABASE_URL is required");
        return None;
    };
    let pool = db::lazy_pool(&url, 6, Duration::from_secs(5));
    if let Err(error) = sqlx::query("SELECT 1").execute(&pool).await {
        assert!(!required, "{error}");
        return None;
    }
    db::testing::migrate().await;
    Some(pool)
}

static PRIVACY_QUEUE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn redis() -> Option<(String, Arc<cache::Cache>)> {
    let required = std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1");
    let Ok(url) = std::env::var("REDIS_URL") else {
        assert!(!required, "REDIS_URL is required for privacy worker tests");
        return None;
    };
    let result = async {
        let cache = cache::Cache::new(&url)?;
        tokio::time::timeout(Duration::from_secs(5), cache.ping())
            .await
            .map_err(|_| "Redis ping timed out".to_owned())??;
        Ok::<_, String>(cache)
    }
    .await;
    match result {
        Ok(cache) => Some((url, Arc::new(cache))),
        Err(error) => {
            assert!(
                !required,
                "REDIS_URL is unavailable for privacy worker tests: {error}"
            );
            None
        }
    }
}

async fn process_request(worker: &privacy::Worker, request: Uuid, expected_status: &str) {
    let seed = db::testing::seed_pool();
    let attempts = || {
        sqlx::query_scalar::<_, i32>("SELECT attempts FROM privacy_requests WHERE id = $1")
            .bind(request)
            .fetch_one(&seed)
    };
    let before = attempts().await.unwrap();
    assert!(worker.process_one().await.unwrap());
    let (status, after): (String, i32) =
        sqlx::query_as("SELECT status, attempts FROM privacy_requests WHERE id = $1")
            .bind(request)
            .fetch_one(&seed)
            .await
            .unwrap();
    assert_eq!(
        (status.as_str(), after),
        (expected_status, before + 1),
        "worker must advance exactly the expected request {request}"
    );
}

#[test]
fn documented_inventory_matches_code() {
    let documented: BTreeSet<_> =
        include_str!("../../../../docs/security/personal-data-inventory.md")
            .lines()
            .filter_map(|line| line.strip_prefix("| `"))
            .filter_map(|line| line.split_once("` |"))
            .map(|(name, _)| name)
            .filter(|name| !name.contains([':', '/', '{']))
            .collect();
    assert_eq!(
        documented,
        privacy::inventory::TABLES.iter().map(|(t, _)| *t).collect()
    );
}

#[tokio::test]
async fn catalog_subject_tables_are_in_inventory() {
    let Some(pool) = pool().await else { return };
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'public' AND c.relkind IN ('r', 'p') AND c.relname <> '_sqlx_migrations'",
    ).fetch_all(&pool).await.unwrap();
    assert_eq!(
        tables.iter().map(String::as_str).collect::<BTreeSet<_>>(),
        privacy::inventory::TABLES
            .iter()
            .map(|(name, _)| *name)
            .collect()
    );
}

async fn seed(pool: &PgPool, marker: &str) -> (Uuid, Uuid, Uuid, Uuid) {
    let tenant = db::testing::insert_tenant(
        pool,
        marker,
        &format!("privacy-{}", Uuid::new_v4().simple()),
    )
    .await;
    let owner = db::testing::seed_pool();
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email,display_name) VALUES ($1,'Privacy Operator') RETURNING id",
    )
    .bind(format!("{}@privacy.test", Uuid::new_v4().simple()))
    .fetch_one(&owner)
    .await
    .unwrap();
    let customer: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id,display_name,email) VALUES ($1,$2,$3) RETURNING id",
    )
    .bind(tenant)
    .bind(marker)
    .bind(format!("{marker}@privacy.test"))
    .fetch_one(&owner)
    .await
    .unwrap();
    let conversation: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id,customer_id,channel,status)
         VALUES ($1,$2,'web_chat','open') RETURNING id",
    )
    .bind(tenant)
    .bind(customer)
    .fetch_one(&owner)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO messages (tenant_id,conversation_id,kind,body) VALUES ($1,$2,'customer',$3)",
    )
    .bind(tenant)
    .bind(conversation)
    .bind(marker)
    .execute(&owner)
    .await
    .unwrap();
    sqlx::query("INSERT INTO customer_channel_identifiers (tenant_id,customer_id,channel,identifier) VALUES ($1,$2,'email',$3)")
        .bind(tenant).bind(customer).bind(format!("{marker}@privacy.test")).execute(&owner).await.unwrap();
    (tenant, user, customer, conversation)
}

#[tokio::test]
async fn export_then_erase_preserves_history_and_other_tenant_data() {
    let Some((_, redis_cache)) = redis().await else {
        return;
    };
    let _queue = PRIVACY_QUEUE.lock().await;
    let Some(pool) = pool().await else { return };
    let marker = format!("privacy-{}", Uuid::new_v4().simple());
    let (tenant, actor, customer, conversation) = seed(&pool, &marker).await;
    let other_marker = format!("foreign-{}", Uuid::new_v4().simple());
    let same_tenant_marker = format!("other-{}", Uuid::new_v4().simple());
    let (other_tenant, _, other_customer, other_conversation) = seed(&pool, &other_marker).await;
    let owner = db::testing::seed_pool();
    let same_tenant_customer: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id,display_name) VALUES ($1,$2) RETURNING id",
    )
    .bind(tenant)
    .bind(&same_tenant_marker)
    .fetch_one(&owner)
    .await
    .unwrap();
    let storage = Arc::new(storage::InMemoryStorage::default());
    let attachment =
        seed_personal_content(storage.as_ref(), tenant, customer, conversation, &marker).await;
    let other_attachment = seed_personal_content(
        storage.as_ref(),
        other_tenant,
        other_customer,
        other_conversation,
        &other_marker,
    )
    .await;
    // External producer IDs are plain text. Seed a conflicting reference in the other
    // tenant to prove that subject matching cannot scrub the other tenant's event.
    sqlx::query(
        "INSERT INTO outbox_events (id,tenant_id,aggregate_type,aggregate_id,event_type,payload,created_at)
         VALUES ($1,$2,'conversation',$3,'test.privacy',jsonb_build_object('text',$4::text),now())",
    ).bind(Uuid::new_v4()).bind(other_tenant.to_string()).bind(conversation.to_string())
        .bind(&other_marker).execute(&owner).await.unwrap();
    let same_tenant_conversation: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id,customer_id,channel,status)
         VALUES ($1,$2,'web_chat','open') RETURNING id",
    )
    .bind(tenant)
    .bind(same_tenant_customer)
    .fetch_one(&owner)
    .await
    .unwrap();
    let mixed_payload = serde_json::json!({
        "customer_id": customer,
        "conversationId": same_tenant_conversation,
        "text": same_tenant_marker,
    });
    let mixed = seed_delivery(tenant, &mixed_payload).await;
    let unrelated = seed_delivery(
        tenant,
        &serde_json::json!({"customer_id": same_tenant_customer, "text": same_tenant_marker}),
    )
    .await;
    let unattributed = seed_delivery(tenant, &serde_json::json!({"text": "opaque-webhook"})).await;
    let other_before = personal_table_snapshot(other_tenant).await;
    let to = chrono::Utc::now() + chrono::Duration::hours(1);
    let from = to - chrono::Duration::hours(2);
    let counts = analytics::queries::conversation_counts(&pool, tenant, from, to, None)
        .await
        .unwrap();
    let feedback = analytics::queries::satisfaction(&pool, tenant, from, to, None)
        .await
        .unwrap();
    let tokens = analytics::queries::token_totals(&pool, tenant, from, to)
        .await
        .unwrap();
    let worker = privacy::Worker {
        pool: pool.clone(),
        storage: storage.clone(),
        cache: redis_cache,
        notifications: Arc::new(server::privacy_ports::NotificationPort),
    };
    assert!(matches!(
        privacy::request(&pool, other_tenant, customer, actor, privacy::Kind::Erase).await,
        Err(privacy::RequestError::NotFound)
    ));
    let export = privacy::request(&pool, tenant, customer, actor, privacy::Kind::Export)
        .await
        .unwrap();
    process_request(&worker, export.request_id, "completed").await;
    let export_key = format!("privacy-exports/{tenant}/{}.zip", export.request_id);
    let (bytes, content_type) = storage.get(&export_key).await.unwrap();
    assert_eq!(content_type, "application/zip");
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut file = String::new();
    use std::io::Read;
    for name in [
        "customer.json",
        "conversations.json",
        "messages.json",
        "channel_identifiers.json",
        "conversation_feedback.json",
        "manifest.json",
    ] {
        file.clear();
        archive
            .by_name(name)
            .unwrap()
            .read_to_string(&mut file)
            .unwrap();
        assert!(!file.contains(&other_marker));
        assert!(!file.contains(&same_tenant_marker));
        assert!(serde_json::from_str::<serde_json::Value>(&file).is_ok());
        if name == "customer.json" || name == "messages.json" {
            assert!(file.contains(&marker));
        }
    }
    file.clear();
    archive
        .by_name("integration_webhook_deliveries.json")
        .unwrap()
        .read_to_string(&mut file)
        .unwrap();
    let deliveries: Vec<serde_json::Value> = serde_json::from_str(&file).unwrap();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0]["payload"]["text"], marker);
    assert_eq!(
        deliveries[0]["payload"]["conversation_id"],
        conversation.to_string()
    );

    let uppercase = seed_delivery(
        tenant,
        &serde_json::json!({
            "customer_id": customer.to_string().to_uppercase(),
            "text": marker,
        }),
    )
    .await;
    let uppercase_alias = seed_delivery(
        tenant,
        &serde_json::json!({
            "customerId": customer.to_string().to_uppercase(),
            "conversation_id": conversation,
            "text": marker,
        }),
    )
    .await;
    let attachment_id = attachment.rsplit('/').next().unwrap();
    file.clear();
    archive
        .by_name(&format!("attachments/{attachment_id}"))
        .unwrap()
        .read_to_string(&mut file)
        .unwrap();
    assert_eq!(file, marker);
    let first = privacy::request(&pool, tenant, customer, actor, privacy::Kind::Erase)
        .await
        .unwrap();
    let repeated = privacy::request(&pool, tenant, customer, actor, privacy::Kind::Erase)
        .await
        .unwrap();
    assert_eq!(first.request_id, repeated.request_id);
    assert!(matches!(
        privacy::request(&pool, tenant, customer, actor, privacy::Kind::Export).await,
        Err(privacy::RequestError::Erasing)
    ));
    process_request(&worker, first.request_id, "completed").await;
    assert_eq!(delivery_payload(uppercase).await, serde_json::json!({}));
    assert_eq!(
        delivery_payload(uppercase_alias).await,
        serde_json::json!({})
    );
    assert_eq!(delivery_payload(mixed).await, mixed_payload);
    assert_eq!(
        delivery_payload(unrelated).await["text"],
        same_tenant_marker
    );
    assert_eq!(
        delivery_payload(unattributed).await["text"],
        "opaque-webhook"
    );

    assert!(matches!(
        storage.get(&export_key).await,
        Err(storage::StorageError::NotFound)
    ));
    assert_eq!(
        privacy::request(&pool, tenant, customer, actor, privacy::Kind::Erase)
            .await
            .unwrap()
            .status,
        "completed"
    );
    let mut work = db::UnitOfWork::begin_for_tenant(&pool, tenant)
        .await
        .unwrap();
    assert!(!customers::queries::update_contact_field_in_tx(
        &mut work, tenant, customer, "phone", &marker,
    )
    .await
    .unwrap());
    let tool: Uuid = sqlx::query_scalar("SELECT id FROM tool_requests WHERE tenant_id = $1")
        .bind(tenant)
        .fetch_one(work.conn())
        .await
        .unwrap();
    sqlx::query("UPDATE tool_requests SET result = jsonb_build_object('text',$3::text), error = $3 WHERE tenant_id = $1 AND id = $2")
        .bind(tenant).bind(tool).bind(&marker).execute(work.conn()).await.unwrap();
    let recipient: Uuid =
        sqlx::query_scalar("SELECT id FROM tenant_memberships WHERE tenant_id = $1 LIMIT 1")
            .bind(tenant)
            .fetch_one(work.conn())
            .await
            .unwrap();
    let late = sqlx::query("INSERT INTO notifications (tenant_id,recipient_membership_id,kind,title,body,subject_type,subject_id,dedupe_key)
                            VALUES ($1,$2,'tool.approval_required',$4,$4,'tool_request',$3,'late')")
        .bind(tenant).bind(recipient).bind(tool).bind(&marker).execute(work.conn()).await.unwrap();
    assert_eq!(late.rows_affected(), 0);
    work.commit().await.unwrap();
    for (table, rows) in personal_table_snapshot(tenant).await {
        assert!(
            !rows.contains(&marker),
            "personal marker remains in {table}"
        );
    }
    assert_eq!(personal_table_snapshot(other_tenant).await, other_before);
    assert_eq!(
        analytics::queries::conversation_counts(&pool, tenant, from, to, None)
            .await
            .unwrap(),
        counts
    );
    assert_eq!(
        analytics::queries::satisfaction(&pool, tenant, from, to, None)
            .await
            .unwrap(),
        feedback
    );
    assert_eq!(
        analytics::queries::token_totals(&pool, tenant, from, to)
            .await
            .unwrap(),
        tokens
    );
    assert!(matches!(
        storage.get(&attachment).await,
        Err(storage::StorageError::NotFound)
    ));
    assert_eq!(
        storage.get(&other_attachment).await.unwrap().0,
        other_marker.as_bytes()
    );
    let owner = db::testing::seed_pool();
    let mut audit: Vec<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT action, details FROM audit_logs WHERE tenant_id = $1 AND resource_id = $2
         AND action IN ('customer.export_requested','customer.export_completed','customer.erase_requested','customer.erased')
         ORDER BY action",
    ).bind(tenant).bind(customer.to_string()).fetch_all(&owner).await.unwrap();
    audit.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        audit,
        vec![
            (
                "customer.erase_requested".into(),
                serde_json::json!({"request_id": first.request_id})
            ),
            (
                "customer.erased".into(),
                serde_json::json!({"request_id": first.request_id})
            ),
            (
                "customer.export_completed".into(),
                serde_json::json!({"request_id": export.request_id})
            ),
            (
                "customer.export_requested".into(),
                serde_json::json!({"request_id": export.request_id})
            ),
        ]
    );
    let bodies: Vec<String> = sqlx::query_scalar(
        "SELECT body FROM messages WHERE conversation_id = ANY($1) ORDER BY (conversation_id = $2) DESC",
    ).bind(vec![conversation, other_conversation]).bind(conversation).fetch_all(&owner).await.unwrap();
    assert_eq!(bodies, vec!["[erased]".to_owned(), other_marker.clone()]);
    let email: String = sqlx::query_scalar("SELECT email::text FROM customers WHERE id = $1")
        .bind(other_customer)
        .fetch_one(&owner)
        .await
        .unwrap();
    assert_eq!(email, format!("{other_marker}@privacy.test"));
}

struct FailFirstDelete {
    inner: storage::InMemoryStorage,
    fail: std::sync::atomic::AtomicBool,
}

#[async_trait::async_trait]
impl ObjectStorage for FailFirstDelete {
    async fn put(
        &self,
        key: &str,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<(), storage::StorageError> {
        self.inner.put(key, content_type, bytes).await
    }

    async fn get(&self, key: &str) -> Result<(Vec<u8>, String), storage::StorageError> {
        self.inner.get(key).await
    }

    async fn delete(&self, key: &str) -> Result<(), storage::StorageError> {
        if self.fail.swap(false, std::sync::atomic::Ordering::SeqCst) {
            return Err(storage::StorageError::Other(
                "injected storage failure".into(),
            ));
        }
        self.inner.delete(key).await
    }

    async fn probe(&self) -> Result<(), storage::StorageError> {
        self.inner.probe().await
    }
}

#[tokio::test]
async fn failed_storage_step_resumes_without_repeating_completed_steps() {
    let Some((_, redis_cache)) = redis().await else {
        return;
    };
    let _queue = PRIVACY_QUEUE.lock().await;
    let Some(pool) = pool().await else { return };
    let (tenant, actor, customer, conversation) = seed(&pool, "privacy-retry").await;
    let owner = db::testing::seed_pool();
    let message: Uuid = sqlx::query_scalar("SELECT id FROM messages WHERE conversation_id = $1")
        .bind(conversation)
        .fetch_one(&owner)
        .await
        .unwrap();
    let key = format!("whatsapp-media/{tenant}/{}", Uuid::new_v4());
    sqlx::query(
        "INSERT INTO message_attachments (tenant_id,message_id,kind,status,storage_key,mime_type)
                 VALUES ($1,$2,'document','stored',$3,'text/plain')",
    )
    .bind(tenant)
    .bind(message)
    .bind(&key)
    .execute(&owner)
    .await
    .unwrap();
    let storage = Arc::new(FailFirstDelete {
        inner: storage::InMemoryStorage::default(),
        fail: std::sync::atomic::AtomicBool::new(true),
    });
    storage
        .put(&key, "text/plain", b"attachment".to_vec())
        .await
        .unwrap();
    let worker = privacy::Worker {
        pool: pool.clone(),
        storage: storage.clone(),
        cache: redis_cache,
        notifications: Arc::new(server::privacy_ports::NotificationPort),
    };
    let (left, right) = tokio::join!(
        privacy::request(&pool, tenant, customer, actor, privacy::Kind::Erase),
        privacy::request(&pool, tenant, customer, actor, privacy::Kind::Erase)
    );
    let request = left.unwrap();
    assert_eq!(request.request_id, right.unwrap().request_id);
    process_request(&worker, request.request_id, "failed").await;
    assert_eq!(
        privacy::request(&pool, tenant, customer, actor, privacy::Kind::Erase)
            .await
            .unwrap()
            .status,
        "failed"
    );
    assert!(storage.get(&key).await.is_ok());
    sqlx::query("UPDATE privacy_requests SET available_at = now() WHERE id = $1")
        .bind(request.request_id)
        .execute(&owner)
        .await
        .unwrap();
    process_request(&worker, request.request_id, "completed").await;
    assert_eq!(
        privacy::request(&pool, tenant, customer, actor, privacy::Kind::Erase)
            .await
            .unwrap()
            .status,
        "completed"
    );
    assert!(matches!(
        storage.get(&key).await,
        Err(storage::StorageError::NotFound)
    ));
    let prior_steps: Vec<(String, i32)> = sqlx::query_as(
        "SELECT step_name, attempts FROM privacy_request_steps WHERE request_id = $1
         AND step_name IN ('attachments', 'content', 'identifiers') ORDER BY step_name",
    )
    .bind(request.request_id)
    .fetch_all(&owner)
    .await
    .unwrap();
    assert_eq!(
        prior_steps,
        vec![
            ("attachments".into(), 2),
            ("content".into(), 1),
            ("identifiers".into(), 1)
        ]
    );
}

mod session_fixture;

const ORIGIN: &str = "http://localhost:4200";

fn test_config(redis_url: String) -> config::AppConfig {
    config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url,
        port: 0,
        bind_address: "0.0.0.0".into(),
        environment: config::Environment::Test,
        cors_allowed_origins: vec![ORIGIN.into()],
        log_format: config::LogFormat::Pretty,
        smtp_url: None,
        smtp_from: None,
        public_dashboard_url: ORIGIN.into(),
        db_max_connections: 2,
        db_acquire_timeout_ms: 5000,
        ready_probe_timeout_ms: 5000,
        shutdown_grace_seconds: 1,
        docs_enabled: false,
        rate_limit_enabled: false,
        rate_limit_login_max: 10,
        rate_limit_login_window_secs: 60,
        password_reset_limits: Default::default(),
        login_lockout: Default::default(),
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
        ai_key_encryption_key: None,
        integration_secrets_key: None,
        secrets_root_key: None,
        secrets_root_key_previous: None,
        auth_mfa_required: false,
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
        trusted_proxy: false,
        outbound_http: config::OutboundHttpConfig::default(),
    }
}

#[tokio::test]
async fn privacy_routes_require_permissions_confirmation_tenant_and_step_up() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use server::state::AppState;
    use tower::ServiceExt;

    let Some((redis_url, redis_cache)) = redis().await else {
        return;
    };
    let _queue = PRIVACY_QUEUE.lock().await;
    let Some(pool) = pool().await else { return };
    let (tenant, owner, customer, _) = seed(&pool, "privacy-route").await;
    let (_, _, foreign, _) = seed(&pool, "privacy-foreign").await;
    let db_owner = db::testing::seed_pool();
    let viewer: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name) VALUES ($1,'Viewer') RETURNING id",
    )
    .bind(format!("{}@privacy.test", Uuid::new_v4().simple()))
    .fetch_one(&db_owner)
    .await
    .unwrap();
    for (user, role) in [(owner, "owner"), (viewer, "viewer")] {
        sqlx::query("INSERT INTO tenant_memberships (tenant_id,user_id,role) VALUES ($1,$2,$3)")
            .bind(tenant)
            .bind(user)
            .bind(role)
            .execute(&db_owner)
            .await
            .unwrap();
    }
    let store = Arc::new(storage::InMemoryStorage::default());
    let worker = privacy::Worker {
        pool: pool.clone(),
        storage: store.clone(),
        cache: redis_cache,
        notifications: Arc::new(server::privacy_ports::NotificationPort),
    };
    let make_router = |mfa_required| {
        let mut config = test_config(redis_url.clone());
        config.auth_mfa_required = mfa_required;
        server::router::app_with_storage(
            AppState {
                config: Arc::new(config.clone()),
                db: pool.clone(),
                cache: worker.cache.clone(),
                health_checks: vec![],
                escalations: escalations::presence::Runtime::new(
                    pool.clone(),
                    Duration::from_secs(45),
                ),
                ai: ai::AiService::from_config(pool.clone(), &config).unwrap(),
                budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
            },
            store.clone(),
        )
    };
    let owner_id = owner;
    let owner = session_fixture::start(&pool, &config::Environment::Test, owner).await;
    let viewer = session_fixture::start(&pool, &config::Environment::Test, viewer).await;
    let post = |id: Uuid, token: &str, payload: serde_json::Value| {
        Request::post(format!("/api/v1/tenant/customers/{id}/erase"))
            .header("x-tenant-id", tenant.to_string())
            .header("cookie", format!("__Host-app_session={token}"))
            .header("origin", ORIGIN)
            .header("x-requested-with", "XMLHttpRequest")
            .header("content-type", "application/json")
            .header("accept", "application/problem+json")
            .body(Body::from(payload.to_string()))
            .unwrap()
    };
    let app = make_router(false);
    let cases = [
        (
            customer,
            viewer.access_token.as_str(),
            serde_json::json!({"confirm":customer}),
            StatusCode::FORBIDDEN,
        ),
        (
            customer,
            owner.access_token.as_str(),
            serde_json::json!({}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            customer,
            owner.access_token.as_str(),
            serde_json::json!({"confirm":foreign}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            foreign,
            owner.access_token.as_str(),
            serde_json::json!({"confirm":foreign}),
            StatusCode::NOT_FOUND,
        ),
    ];
    for (id, token, body, expected) in cases {
        assert_eq!(
            app.clone()
                .oneshot(post(id, token, body))
                .await
                .unwrap()
                .status(),
            expected
        );
    }

    let export = privacy::request(&pool, tenant, customer, owner_id, privacy::Kind::Export)
        .await
        .unwrap();
    process_request(&worker, export.request_id, "completed").await;
    let download = |id: Uuid, token: &str| {
        Request::get(format!(
            "/api/v1/tenant/customers/{id}/export/{}/download",
            export.request_id
        ))
        .header("x-tenant-id", tenant.to_string())
        .header("cookie", format!("__Host-app_session={token}"))
        .body(Body::empty())
        .unwrap()
    };
    assert_eq!(
        app.clone()
            .oneshot(download(customer, &viewer.access_token))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.clone()
            .oneshot(download(foreign, &owner.access_token))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let response = app
        .clone()
        .oneshot(download(customer, &owner.access_token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/zip");
    assert!(response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .starts_with(b"PK"));
    sqlx::query("UPDATE privacy_requests SET expires_at = now() WHERE tenant_id = $1 AND id = $2")
        .bind(tenant)
        .bind(export.request_id)
        .execute(&db_owner)
        .await
        .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(download(customer, &owner.access_token))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    process_request(&worker, export.request_id, "expired").await;
    let expired: (String, Option<String>) =
        sqlx::query_as("SELECT status, object_key FROM privacy_requests WHERE id = $1")
            .bind(export.request_id)
            .fetch_one(&db_owner)
            .await
            .unwrap();
    assert_eq!(expired, ("expired".into(), None));
    assert!(matches!(
        store
            .get(&format!(
                "privacy-exports/{tenant}/{}.zip",
                export.request_id
            ))
            .await,
        Err(storage::StorageError::NotFound)
    ));
    let session = identity::sessions::start(
        &pool,
        identity::keys::keys_for(&config::Environment::Test),
        owner_id,
        identity::session::SessionAuth {
            amr: vec!["pwd".into(), "otp".into()],
            mfa_at: Some(chrono::Utc::now().timestamp() - 360),
        },
        &identity::sessions::ClientMeta::default(),
    )
    .await
    .unwrap();
    let response = make_router(true)
        .oneshot(post(
            customer,
            &session.access_token,
            serde_json::json!({"confirm":customer}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "mfa_step_up_required");
    let response = app
        .oneshot(post(
            customer,
            &owner.access_token,
            serde_json::json!({"confirm":customer}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let receipt: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let erase_id: Uuid = serde_json::from_value(receipt["request_id"].clone()).unwrap();
    process_request(&worker, erase_id, "completed").await;
}

async fn seed_delivery(tenant: Uuid, payload: &serde_json::Value) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO integration_webhook_deliveries (tenant_id, connection_id, payload)
         SELECT tenant_id, id, $2 FROM integration_connections WHERE tenant_id = $1
         RETURNING id",
    )
    .bind(tenant)
    .bind(payload)
    .fetch_one(&db::testing::seed_pool())
    .await
    .unwrap()
}

async fn delivery_payload(id: Uuid) -> serde_json::Value {
    sqlx::query_scalar("SELECT payload FROM integration_webhook_deliveries WHERE id = $1")
        .bind(id)
        .fetch_one(&db::testing::seed_pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn soft_deleted_customer_can_be_exported_and_erased() {
    let Some((_, redis_cache)) = redis().await else {
        return;
    };
    let _queue = PRIVACY_QUEUE.lock().await;
    let Some(pool) = pool().await else { return };
    let (tenant, actor, customer, _) = seed(&pool, "soft-deleted-privacy-customer").await;
    let seed = db::testing::seed_pool();
    sqlx::query("UPDATE customers SET deleted_at = now() WHERE tenant_id = $1 AND id = $2")
        .bind(tenant)
        .bind(customer)
        .execute(&seed)
        .await
        .unwrap();
    let store = Arc::new(storage::InMemoryStorage::default());
    let worker = privacy::Worker {
        pool: pool.clone(),
        storage: store.clone(),
        cache: redis_cache,
        notifications: Arc::new(server::privacy_ports::NotificationPort),
    };
    let export = privacy::request(&pool, tenant, customer, actor, privacy::Kind::Export)
        .await
        .unwrap();
    process_request(&worker, export.request_id, "completed").await;
    let key = format!("privacy-exports/{tenant}/{}.zip", export.request_id);
    let (bytes, _) = store.get(&key).await.unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let records: Vec<serde_json::Value> =
        serde_json::from_reader(archive.by_name("customer.json").unwrap()).unwrap();
    assert_eq!(records[0]["display_name"], "soft-deleted-privacy-customer");

    let erase = privacy::request(&pool, tenant, customer, actor, privacy::Kind::Erase)
        .await
        .unwrap();
    process_request(&worker, erase.request_id, "completed").await;
    let tombstone: (String, Option<String>, bool, bool) = sqlx::query_as(
        "SELECT display_name, email::text, erased_at IS NOT NULL, deleted_at IS NOT NULL
         FROM customers WHERE tenant_id = $1 AND id = $2",
    )
    .bind(tenant)
    .bind(customer)
    .fetch_one(&seed)
    .await
    .unwrap();
    assert_eq!(tombstone, ("[erased]".into(), None, true, true));
}

async fn seed_personal_content(
    storage: &dyn ObjectStorage,
    tenant: Uuid,
    customer: Uuid,
    conversation: Uuid,
    marker: &str,
) -> String {
    let db = db::testing::seed_pool();
    let message: Uuid = sqlx::query_scalar("SELECT id FROM messages WHERE conversation_id = $1")
        .bind(conversation)
        .fetch_one(&db)
        .await
        .unwrap();
    let usage: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_usage_records
         (tenant_id,provider,model,status,streamed,latency_ms,input_tokens,output_tokens,request_id,request_content,response_content)
         VALUES ($1,'openai','test','success',false,1,2,3,$2,jsonb_build_object('text',$2::text),$2)
         RETURNING id",
    ).bind(tenant).bind(marker).fetch_one(&db).await.unwrap();
    let generation: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_generations (tenant_id,conversation_id,trigger_message_id,usage_record_id,outcome,latency_ms,request_id)
         VALUES ($1,$2,$3,$4,'success',1,$5) RETURNING id",
    ).bind(tenant).bind(conversation).bind(message).bind(usage).bind(marker)
        .fetch_one(&db).await.unwrap();
    let tool: Uuid = sqlx::query_scalar(
        "INSERT INTO tool_requests (tenant_id,conversation_id,generation_id,tool_name,tool_source,arguments,status,approval_required,result,error,chain_index)
         VALUES ($1,$2,$3,'lookup','builtin',jsonb_build_object('text',$4::text),'failed',false,jsonb_build_object('text',$4::text),$4,0)
         RETURNING id",
    ).bind(tenant).bind(conversation).bind(generation).bind(marker).fetch_one(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO notifications (tenant_id,recipient_membership_id,kind,title,body,subject_type,subject_id,dedupe_key)
         VALUES ($1, $2, 'tool.approval_required', $4, $4, 'tool_request', $3, $4)",
    ).bind(tenant).bind(member(tenant).await).bind(tool).bind(marker).execute(&db).await.unwrap();
    sqlx::query("INSERT INTO escalations (tenant_id,conversation_id,reason,required_skill_names,matched_skill_names,status)
                VALUES ($1,$2,$3,ARRAY[$3],ARRAY[$3],'closed')")
        .bind(tenant).bind(conversation).bind(marker).execute(&db).await.unwrap();
    let widget: Uuid = sqlx::query_scalar(
        "INSERT INTO widget_instances (tenant_id,public_id,name) VALUES ($1,$2,'Support') RETURNING id",
    ).bind(tenant).bind(Uuid::new_v4().to_string()).fetch_one(&db).await.unwrap();
    let session: Uuid = sqlx::query_scalar(
        "INSERT INTO widget_sessions (tenant_id,widget_instance_id,token_hash,customer_id,expires_at)
         VALUES ($1,$2,$3,$4,now()+interval '1 day') RETURNING id",
    ).bind(tenant).bind(widget).bind([Uuid::new_v4().as_bytes().as_slice(), marker.as_bytes()].concat())
        .bind(customer).fetch_one(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO conversation_feedback (tenant_id,conversation_id,widget_session_id,channel,rating,comment)
         VALUES ($1,$2,$3,'web_chat',4,$4)",
    ).bind(tenant).bind(conversation).bind(session).bind(marker).execute(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO message_citations (tenant_id,message_id,knowledge_item_id,item_title,passage_text,relevance_score,ordinal)
         VALUES ($1,$2,$3,$4,$4,1,0)",
    ).bind(tenant).bind(message).bind(Uuid::new_v4()).bind(marker).execute(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO whatsapp_message_meta (tenant_id,message_id,conversation_id,direction,wamid,failure_reason)
         VALUES ($1,$2,$3,'inbound',$4,$4)",
    ).bind(tenant).bind(message).bind(conversation).bind(marker).execute(&db).await.unwrap();
    let attachment = Uuid::new_v4();
    let key = format!("whatsapp-media/{tenant}/{attachment}");
    sqlx::query(
        "INSERT INTO message_attachments (id,tenant_id,message_id,kind,status,storage_key,mime_type,file_name)
         VALUES ($1,$2,$3,'document','stored',$4,'text/plain',$5)",
    ).bind(attachment).bind(tenant).bind(message).bind(&key).bind(marker).execute(&db).await.unwrap();
    storage
        .put(&key, "text/plain", marker.as_bytes().to_vec())
        .await
        .unwrap();
    let connection: Uuid = sqlx::query_scalar(
        "INSERT INTO integration_connections (tenant_id,catalog_id,webhook_token_hash,webhook_token_ciphertext,webhook_token_nonce)
         SELECT $1,id,$2,$2,$2 FROM integration_catalog WHERE slug = 'generic-webhook' RETURNING id",
    ).bind(tenant).bind(Uuid::new_v4().as_bytes().as_slice()).fetch_one(&db).await.unwrap();
    sqlx::query(
        "INSERT INTO integration_webhook_deliveries (tenant_id,connection_id,payload)
                 VALUES ($1,$2,jsonb_build_object('conversation_id',$3::uuid,'text',$4::text))",
    )
    .bind(tenant)
    .bind(connection)
    .bind(conversation)
    .bind(marker)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO outbox_events (id,tenant_id,aggregate_type,aggregate_id,event_type,payload,created_at)
                 VALUES ($1,$2,'conversation',$3,'test.privacy',jsonb_build_object('title',$4::text),now())")
        .bind(Uuid::new_v4()).bind(tenant.to_string()).bind(conversation.to_string()).bind(marker)
        .execute(&db).await.unwrap();
    key
}

async fn member(tenant: Uuid) -> Uuid {
    let db = db::testing::seed_pool();
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email,display_name) VALUES ($1,'Privacy Agent') RETURNING id",
    )
    .bind(format!("{}@privacy.test", Uuid::new_v4().simple()))
    .fetch_one(&db)
    .await
    .unwrap();
    sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id,user_id,role) VALUES ($1,$2,'agent') RETURNING id",
    ).bind(tenant).bind(user).fetch_one(&db).await.unwrap()
}

async fn personal_table_snapshot(tenant: Uuid) -> Vec<(String, String)> {
    let db = db::testing::seed_pool();
    let mut snapshot = Vec::new();
    let tables = [
        "customers",
        "customer_channel_identifiers",
        "messages",
        "conversation_feedback",
        "ai_usage_records",
        "ai_generations",
        "tool_requests",
        "message_citations",
        "escalations",
        "notifications",
        "whatsapp_message_meta",
        "message_attachments",
        "widget_sessions",
        "integration_webhook_deliveries",
        "outbox_events",
    ];
    for table in tables {
        let sql = format!(
            "SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY id)::text, '') FROM {table} r WHERE tenant_id::text = $1"
        );
        let json: String = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .bind(tenant.to_string())
            .fetch_one(&db)
            .await
            .unwrap();
        snapshot.push((table.to_owned(), json));
    }
    snapshot
}
