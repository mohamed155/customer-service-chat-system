#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // integration test assertions

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use axum::Router;
use config::Environment;
use http_body_util::BodyExt;
use identity::sessions::Issued;
use serde_json::Value;
use server::{router, state::AppState};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

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

async fn database() -> Option<PgPool> {
    let required = std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1");
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        assert!(!required, "REQUIRE_DB_TESTS=1 requires DATABASE_URL");
        eprintln!("SSE revocation tests need DATABASE_URL");
        return None;
    };
    let pool = db::lazy_pool(&database_url, 6, Duration::from_secs(5));
    if let Err(error) = sqlx::query("SELECT 1").execute(&pool).await {
        assert!(
            !required,
            "SSE revocation tests require PostgreSQL: {error}"
        );
        eprintln!("SSE revocation tests need live PostgreSQL");
        return None;
    }
    db::testing::migrate().await;

    Some(pool)
}

fn app(pool: &PgPool) -> Router {
    app_with_presence(pool).0
}

fn app_with_presence(pool: &PgPool) -> (Router, Arc<escalations::presence::Runtime>) {
    app_with_environment(pool, Environment::Test)
}

fn app_with_environment(
    pool: &PgPool,
    environment: Environment,
) -> (Router, Arc<escalations::presence::Runtime>) {
    session_fixture::configure_keys();
    let mut config =
        test_config(std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into()));
    config.environment = environment;
    let cache = cache::Cache::new(&config.redis_url).unwrap();
    let runtime = escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45));
    let app = router::app(AppState {
        config: Arc::new(config.clone()),
        db: pool.clone(),
        cache: Arc::new(cache),
        health_checks: vec![],
        escalations: runtime.clone(),
        ai: ai::AiService::from_config(pool.clone(), &config).unwrap(),
        budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
    });
    (app, runtime)
}

async fn send(
    app: &Router,
    method: Method,
    path: &str,
    sign_in: &Issued,
    tenant: Option<Uuid>,
    payload: Option<Value>,
) -> Response {
    let mut request = Request::builder()
        .uri(path)
        .method(method)
        .header(
            "cookie",
            format!("__Host-app_session={}", sign_in.access_token),
        )
        .header("accept", "application/problem+json")
        .header("origin", ORIGIN)
        .header("x-requested-with", "XMLHttpRequest");
    if let Some(tenant) = tenant {
        request = request.header("x-tenant-id", tenant.to_string());
    }
    let body = match payload {
        Some(payload) => {
            request = request.header("content-type", "application/json");
            Body::from(payload.to_string())
        }
        None => Body::empty(),
    };
    app.clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap()
}

async fn seed_user(pool: &PgPool, role: Option<&str>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name, platform_role)
         VALUES ($1, 'Stream user', $2) RETURNING id",
    )
    .bind(format!("sse-revocation-{}@example.com", Uuid::new_v4()))
    .bind(role)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_tenant(pool: &PgPool) -> Uuid {
    db::testing::insert_tenant(
        pool,
        "Stream tenant",
        &format!("sse-revocation-{}", Uuid::new_v4().simple()),
    )
    .await
}

async fn seed_member(pool: &PgPool, tenant: Uuid, role: &str) -> (Uuid, Issued) {
    let user = seed_user(pool, None).await;
    sqlx::query("INSERT INTO tenant_memberships (tenant_id,user_id,role) VALUES ($1,$2,$3)")
        .bind(tenant)
        .bind(user)
        .bind(role)
        .execute(&db::testing::seed_pool())
        .await
        .unwrap();
    (
        user,
        session_fixture::start(pool, &Environment::Test, user).await,
    )
}

async fn open(
    app: &Router,
    tenant: Uuid,
    session: &Issued,
) -> tokio::task::JoinHandle<Result<http_body_util::Collected<axum::body::Bytes>, axum::Error>> {
    let response = send(
        app,
        Method::GET,
        "/api/v1/tenant/events",
        session,
        Some(tenant),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    tokio::spawn(response.into_body().collect())
}

#[tokio::test]
async fn logout_closes_the_matching_stream_across_redis_instances() {
    let Some(pool) = database().await else { return };
    let tenant = seed_tenant(&pool).await;
    let (_, session) = seed_member(&pool, tenant, "owner").await;
    let (_, other) = seed_member(&pool, tenant, "viewer").await;
    let cache = cache::Cache::new(
        &std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into()),
    )
    .unwrap();
    cache.ping().await.unwrap();
    let mut tasks = tokio::task::JoinSet::new();
    let left = cache.stream_revocation_bus(&mut tasks);
    let right = cache.stream_revocation_bus(&mut tasks);
    let mut probe = right.subscribe();
    let probe_id = Uuid::new_v4();
    let probe_signal = kernel::stream_revocation::Signal::Session {
        session_id: probe_id,
    };
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            left.publish(probe_signal);
            if matches!(tokio::time::timeout(Duration::from_millis(100), probe.recv()).await, Ok(Ok(kernel::stream_revocation::Signal::Session { session_id })) if session_id == probe_id)
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    let writer = app(&pool).layer(axum::Extension(left));
    let (reader, runtime) = app_with_presence(&pool);
    let reader = reader.layer(axum::Extension(right));
    let response = send(
        &reader,
        Method::GET,
        "/api/v1/tenant/events",
        &session,
        Some(tenant),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body();
    let mut unaffected = open(&reader, tenant, &other).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(200), body.frame())
            .await
            .is_err()
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    assert_eq!(
        send(
            &writer,
            Method::POST,
            "/api/v1/auth/logout",
            &session,
            None,
            None
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    // Buffer an event only after the reader's process has received the revocation hint. Its
    // stream must finish without pulling that event while the database check is pending.
    tokio::time::timeout_at(deadline, async {
        loop {
            if matches!(probe.recv().await, Ok(kernel::stream_revocation::Signal::Session { session_id }) if session_id == session.sid) {
                break;
            }
        }
    }).await.unwrap();
    runtime.broadcast(
        tenant,
        escalations::presence::Event::ConversationMessageStatus(
            serde_json::json!({"marker":"revoked"}),
        ),
    );
    assert!(tokio::time::timeout_at(deadline, body.frame())
        .await
        .unwrap()
        .is_none());
    assert!(
        tokio::time::timeout(Duration::from_millis(250), &mut unaffected)
            .await
            .is_err()
    );
    unaffected.abort();
}

#[tokio::test]
async fn periodic_recheck_closes_a_directly_revoked_session() {
    let Some(pool) = database().await else { return };
    let tenant = seed_tenant(&pool).await;
    let (_, session) = seed_member(&pool, tenant, "owner").await;
    let app = app(&pool).layer(axum::Extension(config::SseRevalidation(
        Duration::from_secs(5),
    )));
    let mut stream = open(&app, tenant, &session).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(200), &mut stream)
            .await
            .is_err()
    );
    assert!(identity::sessions::revoke_session(
        &pool,
        session.sid,
        identity::sessions::reason::LOGOUT
    )
    .await
    .unwrap());
    tokio::time::timeout(Duration::from_secs(6), stream)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn member_changes_close_only_streams_that_lose_access() {
    let Some(pool) = database().await else { return };
    let tenant = seed_tenant(&pool).await;
    let other_tenant = seed_tenant(&pool).await;
    let (_, owner) = seed_member(&pool, tenant, "owner").await;
    let (user, member) = seed_member(&pool, tenant, "agent").await;
    let (_, other) = seed_member(&pool, tenant, "viewer").await;
    let seed = db::testing::seed_pool();
    sqlx::query("INSERT INTO tenant_memberships (tenant_id,user_id,role) VALUES ($1,$2,'viewer')")
        .bind(other_tenant)
        .bind(user)
        .execute(&seed)
        .await
        .unwrap();
    let id: Uuid =
        sqlx::query_scalar("SELECT id FROM tenant_memberships WHERE tenant_id=$1 AND user_id=$2")
            .bind(tenant)
            .bind(user)
            .fetch_one(&seed)
            .await
            .unwrap();
    let app = app(&pool);
    let mut stream = open(&app, tenant, &member).await;
    let mut other_stream = open(&app, tenant, &other).await;
    let mut other_tenant_stream = open(&app, other_tenant, &member).await;
    let path = format!("/api/v1/tenant/members/{id}");
    assert_eq!(
        send(
            &app,
            Method::PATCH,
            &path,
            &owner,
            Some(tenant),
            Some(serde_json::json!({"role":"viewer"}))
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(250), &mut stream)
            .await
            .is_err()
    );
    assert_eq!(
        send(
            &app,
            Method::PATCH,
            &path,
            &owner,
            Some(tenant),
            Some(serde_json::json!({"status":"disabled"}))
        )
        .await
        .status(),
        StatusCode::OK
    );
    tokio::time::timeout(Duration::from_secs(3), stream)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(250), &mut other_stream)
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(250), &mut other_tenant_stream)
            .await
            .is_err()
    );
    other_stream.abort();
    other_tenant_stream.abort();
}

#[tokio::test]
async fn replayed_refresh_closes_its_stream() {
    use sha2::{Digest, Sha256};
    let Some(pool) = database().await else { return };
    let tenant = seed_tenant(&pool).await;
    let (_, session) = seed_member(&pool, tenant, "owner").await;
    let app = app(&pool);
    let mut stream = open(&app, tenant, &session).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(200), &mut stream)
            .await
            .is_err()
    );
    let refresh = || {
        let app = app.clone();
        let req = Request::post("/api/v1/auth/refresh")
            .header("origin", ORIGIN)
            .header("x-requested-with", "XMLHttpRequest")
            .header(
                "cookie",
                format!("__Secure-app_refresh={}", session.refresh_token),
            )
            .body(Body::empty())
            .unwrap();
        async move { app.oneshot(req).await.unwrap() }
    };
    assert_eq!(refresh().await.status(), StatusCode::NO_CONTENT);
    sqlx::query(
        "UPDATE refresh_tokens SET used_at = now() - interval '30 seconds' WHERE token_hash = $1",
    )
    .bind(Sha256::digest(session.refresh_token.as_bytes()).to_vec())
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(refresh().await.status(), StatusCode::UNAUTHORIZED);
    tokio::time::timeout(Duration::from_secs(3), stream)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn password_reset_closes_the_users_stream() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use sha2::{Digest, Sha256};
    let Some(pool) = database().await else { return };
    let tenant = seed_tenant(&pool).await;
    let (user, session) = seed_member(&pool, tenant, "owner").await;
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(identity::password::hash_password("SseOriginalPassword1!").unwrap())
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    let mut bytes = [0_u8; 32];
    bytes[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    bytes[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    let token = URL_SAFE_NO_PAD.encode(bytes);
    sqlx::query(
        "INSERT INTO password_reset_tokens
          (user_id, request_id, requested_at, token_hash, created_at, expires_at)
         VALUES ($1, $2, now(), $3, now(), now() + interval '30 minutes')",
    )
    .bind(user)
    .bind(kernel::ids::new_id())
    .bind(hex::encode(Sha256::digest(token.as_bytes())))
    .execute(&pool)
    .await
    .unwrap();
    let app = app(&pool);
    let mut stream = open(&app, tenant, &session).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(200), &mut stream)
            .await
            .is_err()
    );
    assert_eq!(
        send(
            &app,
            Method::POST,
            "/api/v1/auth/password/reset",
            &session,
            None,
            Some(serde_json::json!({"token": token, "password": "SseReplacementPassword1!"})),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    tokio::time::timeout(Duration::from_secs(3), stream)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn healthy_stream_delivers_events_after_multiple_periodic_checks() {
    let Some(pool) = database().await else { return };
    let tenant = seed_tenant(&pool).await;
    let (_, session) = seed_member(&pool, tenant, "owner").await;
    let (app, runtime) = app_with_presence(&pool);
    let app = app.layer(axum::Extension(config::SseRevalidation(
        Duration::from_secs(5),
    )));
    let response = send(
        &app,
        Method::GET,
        "/api/v1/tenant/events",
        &session,
        Some(tenant),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body();
    assert!(tokio::time::timeout(Duration::from_secs(11), body.frame())
        .await
        .is_err());
    runtime.broadcast(
        tenant,
        escalations::presence::Event::ConversationMessageStatus(
            serde_json::json!({"marker":"healthy"}),
        ),
    );
    let frame = tokio::time::timeout(Duration::from_secs(2), body.frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(String::from_utf8_lossy(&frame.into_data().unwrap()).contains("healthy"));
}

#[tokio::test]
async fn a_deleted_membership_closes_only_its_tenant_stream_on_recheck() {
    let Some(pool) = database().await else { return };
    let tenant = seed_tenant(&pool).await;
    let another = seed_tenant(&pool).await;
    let (user, session) = seed_member(&pool, tenant, "agent").await;
    let seed = db::testing::seed_pool();
    sqlx::query("INSERT INTO tenant_memberships (tenant_id,user_id,role) VALUES ($1,$2,'viewer')")
        .bind(another)
        .bind(user)
        .execute(&seed)
        .await
        .unwrap();
    let bus = kernel::stream_revocation::Bus::default();
    let app = app(&pool).layer(axum::Extension(bus.clone()));
    let mut removed = open(&app, tenant, &session).await;
    let mut unaffected = open(&app, another, &session).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(200), &mut removed)
            .await
            .is_err()
    );
    bus.publish(kernel::stream_revocation::Signal::Membership {
        tenant_id: tenant,
        user_id: user,
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(200), &mut removed)
            .await
            .is_err()
    );
    sqlx::query(
        "UPDATE tenant_memberships SET deleted_at = now() WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant)
    .bind(user)
    .execute(&seed)
    .await
    .unwrap();
    bus.publish(kernel::stream_revocation::Signal::Membership {
        tenant_id: tenant,
        user_id: user,
    });
    tokio::time::timeout(Duration::from_secs(3), removed)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(250), &mut unaffected)
            .await
            .is_err()
    );
    unaffected.abort();
}

#[tokio::test]
async fn a_platform_role_change_that_removes_conversation_access_closes_the_stream() {
    let Some(pool) = database().await else { return };
    let tenant = seed_tenant(&pool).await;
    let staff = seed_user(&pool, Some("support")).await;
    let bus = kernel::stream_revocation::Bus::default();
    let app = app_with_environment(&pool, Environment::Production)
        .0
        .layer(axum::Extension(bus.clone()));
    let session =
        session_fixture::PlatformAccess::sign_in(&pool, &Environment::Production, staff).await;
    let switched = app.clone().oneshot(session.switch(tenant)).await.unwrap();
    assert_eq!(switched.status(), StatusCode::OK);

    let request = session.scoped_request(Method::GET, "/api/v1/tenant/events", tenant, None);
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = tokio::spawn(response.into_body().collect());
    assert!(
        tokio::time::timeout(Duration::from_millis(200), &mut stream)
            .await
            .is_err()
    );
    sqlx::query("UPDATE users SET platform_role = 'sales' WHERE id = $1")
        .bind(staff)
        .execute(&pool)
        .await
        .unwrap();
    bus.publish(kernel::stream_revocation::Signal::User { user_id: staff });
    tokio::time::timeout(Duration::from_secs(3), stream)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
