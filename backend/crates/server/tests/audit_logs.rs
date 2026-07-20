use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use config::Environment;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use server::router;
use server::state::AppState;
use cache::Cache;

fn test_config(environment: Environment) -> config::AppConfig {
    config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: "redis://127.0.0.1:6379".into(),
        auth_jwt_secret: "test-auth-jwt-secret-at-least-32-bytes".into(),
        auth_session_ttl_seconds: 28_800,
        port: 0,
        bind_address: "0.0.0.0".into(),
        environment,
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

fn app_state(pool: sqlx::PgPool, environment: Environment) -> AppState {
    let cfg = test_config(environment);
    AppState {
        config: Arc::new(cfg.clone()),
        db: pool.clone(),
        cache: Arc::new(Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool, &cfg).unwrap(),
    }
}

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("skipping audit_logs live tests: DATABASE_URL not set");
            if require_db_tests() {
                panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is not set");
            }
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping audit_logs live tests: DATABASE_URL is unreachable");
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        return None;
    }
    Some(pool)
}

async fn send(pool: sqlx::PgPool, environment: Environment, request: Request<Body>) -> Response {
    router::app_with_test_routes(app_state(pool, environment))
        .oneshot(request)
        .await
        .expect("request should complete")
}

fn authenticated_request(
    uri: &str,
    method: Method,
    user_id: Uuid,
    tenant_id: Option<Uuid>,
    environment: Environment,
) -> Request<Body> {
    let mut builder = Request::builder().uri(uri).method(method);
    if environment == Environment::Production {
        let config = test_config(environment.clone());
        let (token, _, _) = identity::session::issue_token(
            &config.auth_jwt_secret,
            config.auth_session_ttl_seconds,
            user_id,
        )
        .unwrap();
        builder = builder.header("cookie", format!("app_session={token}"));
    } else {
        builder = builder.header("X-Dev-User-Id", user_id.to_string());
    }
    if let Some(tenant_id) = tenant_id {
        builder = builder.header("X-Tenant-ID", tenant_id.to_string());
    }
    builder.body(Body::empty()).unwrap()
}

async fn body_json(response: Response) -> Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, platform_role: Option<&str>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name, platform_role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(format!("audit_{}@example.com", Uuid::new_v4()))
    .bind("Audit User")
    .bind(platform_role)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Audit Tenant")
        .bind(format!("audit-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_membership(pool: &sqlx::PgPool, tenant_id: Uuid, user_id: Uuid, role: &str) {
    sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_audit_row(
    pool: &sqlx::PgPool,
    actor_user_id: Option<Uuid>,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    tenant_id: Option<Uuid>,
    details: &str,
    created_at: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details, created_at) VALUES ($1, $2, $3, $4, $5, $6::jsonb, $7::timestamptz) RETURNING id",
    )
    .bind(actor_user_id)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(tenant_id)
    .bind(details)
    .bind(created_at)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn tenant_isolation_tenant_a_does_not_see_tenant_b_rows() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let admin = seed_user(&pool, None).await;
    seed_membership(&pool, tenant_a, admin, "admin").await;

    seed_audit_row(&pool, Some(admin), "member.role_changed", "membership", "id-1", Some(tenant_a), r#"{"from":"agent","to":"manager"}"#, "2026-07-18T14:00:00Z").await;
    seed_audit_row(&pool, Some(admin), "member.role_changed", "membership", "id-2", Some(tenant_b), r#"{"from":"agent","to":"admin"}"#, "2026-07-18T14:01:00Z").await;

    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/audit-logs",
            Method::GET,
            admin,
            Some(tenant_a),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let entries = json["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "should see only tenant A's row");
    assert_eq!(
        entries[0]["tenant_id"].as_str().unwrap(),
        tenant_a.to_string()
    );
}

#[tokio::test]
async fn platform_level_rows_are_excluded_from_tenant_endpoint() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool, None).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    seed_audit_row(&pool, None, "auth.login_failed", "auth", "test@example.com", None, r#"{}"#, "2026-07-18T14:00:00Z").await;
    seed_audit_row(&pool, Some(admin), "member.role_changed", "membership", "id-1", Some(tenant), r#"{}"#, "2026-07-18T14:01:00Z").await;

    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/audit-logs",
            Method::GET,
            admin,
            Some(tenant),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let entries = json["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["action"], "member.role_changed");
}

#[tokio::test]
async fn ordering_is_newest_first() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool, None).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    seed_audit_row(&pool, Some(admin), "member.role_changed", "membership", "id-1", Some(tenant), r#"{}"#, "2026-07-18T14:00:00Z").await;
    seed_audit_row(&pool, Some(admin), "member.role_changed", "membership", "id-2", Some(tenant), r#"{}"#, "2026-07-18T15:00:00Z").await;

    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/audit-logs",
            Method::GET,
            admin,
            Some(tenant),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let entries = json["data"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    let t0 = entries[0]["created_at"].as_str().unwrap();
    let t1 = entries[1]["created_at"].as_str().unwrap();
    assert!(t0 > t1, "newest first");
}

#[tokio::test]
async fn cursor_pagination_returns_all_pages() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool, None).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    for i in 0..5 {
        seed_audit_row(
            &pool,
            Some(admin),
            "member.role_changed",
            "membership",
            &format!("id-{i}"),
            Some(tenant),
            r#"{}"#,
            &format!("2026-07-18T{:02}:00:00Z", 14 + i),
        ).await;
    }

    let all_ids = {
        let response = send(
            pool.clone(),
            Environment::Test,
            authenticated_request(
                "/api/v1/tenant/audit-logs?limit=5",
                Method::GET,
                admin,
                Some(tenant),
                Environment::Test,
            ),
        )
        .await;
        let json = body_json(response).await;
        json["data"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap().to_string()).collect::<Vec<_>>()
    };

    let mut collected = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let uri = match &cursor {
            Some(c) => format!("/api/v1/tenant/audit-logs?limit=2&cursor={c}"),
            None => "/api/v1/tenant/audit-logs?limit=2".to_string(),
        };
        let response = send(
            pool.clone(),
            Environment::Test,
            authenticated_request(&uri, Method::GET, admin, Some(tenant), Environment::Test),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        let entries = json["data"].as_array().unwrap();
        for entry in entries {
            let id = entry["id"].as_str().unwrap();
            assert!(!collected.contains(&id.to_string()), "duplicate id: {id}");
            collected.push(id.to_string());
        }
        let has_more = json["pagination"]["has_more"].as_bool().unwrap();
        cursor = json["pagination"]["next_cursor"].as_str().map(|s| s.to_string());
        if !has_more {
            break;
        }
    }

    assert_eq!(collected.len(), 5);
    assert_eq!(collected, all_ids);
    assert!(cursor.is_none(), "last page must return next_cursor: null");
}

#[tokio::test]
async fn category_filter_returns_only_matching_rows() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool, None).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    seed_audit_row(&pool, Some(admin), "member.role_changed", "membership", "id-1", Some(tenant), r#"{}"#, "2026-07-18T14:00:00Z").await;
    seed_audit_row(&pool, Some(admin), "auth.login_succeeded", "auth", "id-2", Some(tenant), r#"{}"#, "2026-07-18T14:01:00Z").await;

    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/audit-logs?category=members",
            Method::GET,
            admin,
            Some(tenant),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let entries = json["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["action"], "member.role_changed");
}

#[tokio::test]
async fn system_actor_returns_kind_system() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool, None).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    seed_audit_row(&pool, None, "auth.login_failed", "auth", "test@example.com", Some(tenant), r#"{}"#, "2026-07-18T14:00:00Z").await;

    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/audit-logs",
            Method::GET,
            admin,
            Some(tenant),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let actor = &json["data"][0]["actor"];
    assert_eq!(actor["kind"], "system");
}

#[tokio::test]
async fn deleted_actor_has_deleted_flag() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let actor_user = seed_user(&pool, None).await;
    seed_membership(&pool, tenant, actor_user, "admin").await;
    sqlx::query("UPDATE users SET deleted_at = now() WHERE id = $1")
        .bind(actor_user)
        .execute(&pool)
        .await
        .unwrap();

    seed_audit_row(&pool, Some(actor_user), "member.role_changed", "membership", "id-1", Some(tenant), r#"{}"#, "2026-07-18T14:00:00Z").await;

    let admin = seed_user(&pool, None).await;
    seed_membership(&pool, tenant, admin, "admin").await;
    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/audit-logs",
            Method::GET,
            admin,
            Some(tenant),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let actor = &json["data"][0]["actor"];
    assert_eq!(actor["deleted"], true);
}

#[tokio::test]
async fn platform_staff_actor_has_is_platform_staff_flag() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let staff_user = seed_user(&pool, Some("super_admin")).await;
    let admin = seed_user(&pool, None).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    seed_audit_row(&pool, Some(staff_user), "member.role_changed", "membership", "id-1", Some(tenant), r#"{}"#, "2026-07-18T14:00:00Z").await;

    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/audit-logs",
            Method::GET,
            admin,
            Some(tenant),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let actor = &json["data"][0]["actor"];
    assert_eq!(actor["is_platform_staff"], true);
}

#[tokio::test]
async fn invalid_category_returns_422() {
    let Some(pool) = get_pool().await else { return };
    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool, None).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/audit-logs?category=bogus",
            Method::GET,
            admin,
            Some(tenant),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn invalid_date_returns_422() {
    let Some(pool) = get_pool().await else { return };
    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool, None).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/audit-logs?from=not-a-date",
            Method::GET,
            admin,
            Some(tenant),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn invalid_cursor_returns_422() {
    let Some(pool) = get_pool().await else { return };
    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool, None).await;
    seed_membership(&pool, tenant, admin, "admin").await;

    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/tenant/audit-logs?cursor=zzz",
            Method::GET,
            admin,
            Some(tenant),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn immutability_update_and_delete_are_rejected_by_trigger() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant = seed_tenant(&pool).await;
    let admin = seed_user(&pool, None).await;
    let id = seed_audit_row(&pool, Some(admin), "member.role_changed", "membership", "id-1", Some(tenant), r#"{}"#, "2026-07-18T14:00:00Z").await;

    let update_err = sqlx::query("UPDATE audit_logs SET action = 'tampered' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap_err();
    let msg = update_err.to_string().to_lowercase();
    assert!(
        msg.contains("append-only") || msg.contains("audit_logs_append_only") || msg.contains("cannot update"),
        "expected append-only error, got: {msg}"
    );

    let delete_err = sqlx::query("DELETE FROM audit_logs WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap_err();
    let msg = delete_err.to_string().to_lowercase();
    assert!(
        msg.contains("append-only") || msg.contains("audit_logs_append_only") || msg.contains("cannot delete"),
        "expected append-only error, got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// T033 — Platform audit endpoint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn platform_user_sees_all_tenants_and_platform_rows() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let platform_user = seed_user(&pool, Some("super_admin")).await;

    seed_audit_row(&pool, None, "auth.login_failed", "auth", "a@example.com", None, r#"{}"#, "2026-07-18T14:00:00Z").await;
    seed_audit_row(&pool, None, "member.role_changed", "membership", "id-a", Some(tenant_a), r#"{}"#, "2026-07-18T14:01:00Z").await;
    seed_audit_row(&pool, None, "member.role_changed", "membership", "id-b", Some(tenant_b), r#"{}"#, "2026-07-18T14:02:00Z").await;

    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/platform/audit-logs",
            Method::GET,
            platform_user,
            None,
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let entries = json["data"].as_array().unwrap();
    assert_eq!(entries.len(), 3, "platform user should see all 3 rows");
    let tenant_ids: Vec<Option<String>> = entries
        .iter()
        .map(|e| e["tenant_id"].as_str().map(|s| s.to_string()))
        .collect();
    assert!(tenant_ids.contains(&None));
    assert!(tenant_ids.contains(&Some(tenant_a.to_string())));
    assert!(tenant_ids.contains(&Some(tenant_b.to_string())));
}

#[tokio::test]
async fn platform_filter_by_tenant_narrows_results() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_a = seed_tenant(&pool).await;
    let tenant_b = seed_tenant(&pool).await;
    let platform_user = seed_user(&pool, Some("super_admin")).await;

    seed_audit_row(&pool, None, "member.role_changed", "membership", "id-a", Some(tenant_a), r#"{}"#, "2026-07-18T14:01:00Z").await;
    seed_audit_row(&pool, None, "member.role_changed", "membership", "id-b", Some(tenant_b), r#"{}"#, "2026-07-18T14:02:00Z").await;

    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            &format!("/api/v1/platform/audit-logs?tenant_id={tenant_a}"),
            Method::GET,
            platform_user,
            None,
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;
    let entries = json["data"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "filtered should return only tenant A");
    assert_eq!(
        entries[0]["tenant_id"].as_str().unwrap(),
        tenant_a.to_string()
    );
}

#[tokio::test]
async fn tenant_only_user_is_denied_platform_endpoint() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, None).await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/platform/audit-logs",
            Method::GET,
            user_id,
            None,
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn all_five_platform_roles_can_access_platform_audit() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    for role in ["super_admin", "developer", "support", "sales", "finance"] {
        let user_id = seed_user(&pool, Some(role)).await;
        let response = send(
            pool.clone(),
            Environment::Test,
            authenticated_request(
                "/api/v1/platform/audit-logs",
                Method::GET,
                user_id,
                None,
                Environment::Test,
            ),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "platform role {role} expected 200, got {}",
            response.status()
        );
    }
}

// ---------------------------------------------------------------------------
// T041 — Tool execution audit row
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tool_execution_emits_audit_row() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, None).await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let customer_id: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Test Customer")
    .fetch_one(&pool)
    .await
    .unwrap();

    let conversation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) VALUES ($1, $2, 'chat', 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let ctx = tools::registry::ToolExecutionCtx {
        tenant_id,
        conversation_id,
        pool: pool.clone(),
        master_key: None,
    };

    let resolved = tools::policy::ResolvedTool {
        spec: ai_providers::ToolSpec {
            name: "lookup_customer".into(),
            description: "Look up customer profile".into(),
            input_schema: json!({"type": "object", "properties": {}}),
        },
        source: tools::model::ToolSource::Builtin,
        approval_required: false,
        tenant_tool_id: None,
    };

    let tool_request_id = Uuid::new_v4();
    let outcome = tools::executor::execute(&ctx, &resolved, json!({}), tool_request_id).await;
    assert!(
        matches!(outcome, tools::executor::ExecutionOutcome::Succeeded(_)),
        "expected succeeded, got {:?}",
        outcome
    );

    let row: (Value,) = sqlx::query_as(
        "SELECT details FROM audit_logs \
         WHERE action = 'tool.executed' AND tenant_id = $1 AND resource_id = $2 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .bind("lookup_customer")
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(row.0["tool_name"], "lookup_customer");
    assert_eq!(row.0["outcome"], "succeeded");
    assert_eq!(row.0["conversation_id"], conversation_id.to_string());
    assert_eq!(row.0["request_id"], tool_request_id.to_string());

    let actor_null: bool = sqlx::query_scalar(
        "SELECT COUNT(*) = 0 FROM audit_logs \
         WHERE action = 'tool.executed' AND actor_user_id IS NOT NULL \
         AND tenant_id = $1 AND resource_id = $2",
    )
    .bind(tenant_id)
    .bind("lookup_customer")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(actor_null, "tool.executed must have actor_user_id IS NULL");
}

#[tokio::test]
async fn failed_tool_execution_emits_audit_row() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, None).await;
    seed_membership(&pool, tenant_id, user_id, "admin").await;

    let customer_id: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Test Customer")
    .fetch_one(&pool)
    .await
    .unwrap();

    let conversation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) VALUES ($1, $2, 'chat', 'active') RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let ctx = tools::registry::ToolExecutionCtx {
        tenant_id,
        conversation_id,
        pool: pool.clone(),
        master_key: None,
    };

    let resolved = tools::policy::ResolvedTool {
        spec: ai_providers::ToolSpec {
            name: "non_existent_tool".into(),
            description: "does not exist".into(),
            input_schema: json!({"type": "object", "properties": {}}),
        },
        source: tools::model::ToolSource::Builtin,
        approval_required: false,
        tenant_tool_id: None,
    };

    let tool_request_id = Uuid::new_v4();
    let outcome = tools::executor::execute(&ctx, &resolved, json!({}), tool_request_id).await;
    assert!(
        matches!(outcome, tools::executor::ExecutionOutcome::Failed(_)),
        "expected failed, got {:?}",
        outcome
    );

    let row: (Value,) = sqlx::query_as(
        "SELECT details FROM audit_logs \
         WHERE action = 'tool.executed' AND tenant_id = $1 AND resource_id = $2 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .bind("non_existent_tool")
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(row.0["tool_name"], "non_existent_tool");
    assert_eq!(row.0["outcome"], "failed");

    let actor_null: bool = sqlx::query_scalar(
        "SELECT COUNT(*) = 0 FROM audit_logs \
         WHERE action = 'tool.executed' AND actor_user_id IS NOT NULL \
         AND tenant_id = $1 AND resource_id = $2",
    )
    .bind(tenant_id)
    .bind("non_existent_tool")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(actor_null, "tool.executed must have actor_user_id IS NULL");
}

// ---------------------------------------------------------------------------
// T046 — Performance: list under 2 seconds (manual, #[ignore])
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore]
async fn performance_list_under_two_seconds() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let actor_id = seed_user(&pool, None).await;
    seed_membership(&pool, tenant_id, actor_id, "admin").await;

    // Bulk insert ~50,000 audit rows
    let batch_size = 500;
    let total = 50_000;
    let mut inserted = 0;
    while inserted < total {
        let mut builder = sqlx::QueryBuilder::new(
            "INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id, details, created_at) ",
        );
        builder.push_values(0..batch_size.min(total - inserted), |mut b, i| {
            let ts = format!("2026-07-18T{:02}:{:02}:{:02}Z", 0, 0, inserted + i);
            b.push_bind(Some(actor_id))
                .push_bind("member.role_changed")
                .push_bind("membership")
                .push_bind(format!("perf-{}", inserted + i))
                .push_bind(tenant_id)
                .push_bind(json!({}))
                .push_bind(ts);
        });
        builder.build().execute(&pool).await.unwrap();
        inserted += batch_size;
    }

    let actor = seed_user(&pool, None).await;
    seed_membership(&pool, tenant_id, actor, "admin").await;
    let base = |query: &str| {
        format!("/api/v1/tenant/audit-logs{query}")
    };

    // (a) Unfiltered first page
    let start = std::time::Instant::now();
    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(&base(""), Method::GET, actor, Some(tenant_id), Environment::Test),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let elapsed_a = start.elapsed();
    assert!(
        elapsed_a < Duration::from_secs(2),
        "unfiltered first page took {:?}",
        elapsed_a
    );

    // (b) category=members filtered page
    let start = std::time::Instant::now();
    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            &base("?category=members"),
            Method::GET,
            actor,
            Some(tenant_id),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let elapsed_b = start.elapsed();
    assert!(
        elapsed_b < Duration::from_secs(2),
        "category=members took {:?}",
        elapsed_b
    );

    // (c) Deep cursor page — fetch after paginating to ~row 40,000
    let mut cursor: Option<String> = None;
    for _ in 0..40 {
        let uri = match &cursor {
            Some(c) => base(&format!("?limit=1000&cursor={c}")),
            None => base("?limit=1000"),
        };
        let response = send(
            pool.clone(),
            Environment::Test,
            authenticated_request(&uri, Method::GET, actor, Some(tenant_id), Environment::Test),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        cursor = json["pagination"]["next_cursor"]
            .as_str()
            .map(|s| s.to_string());
        if cursor.is_none() || json["data"].as_array().unwrap().is_empty() {
            break;
        }
    }
    let start = std::time::Instant::now();
    let uri = match &cursor {
        Some(c) => base(&format!("?limit=50&cursor={c}")),
        None => base("?limit=50"),
    };
    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(&uri, Method::GET, actor, Some(tenant_id), Environment::Test),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let elapsed_c = start.elapsed();
    assert!(
        elapsed_c < Duration::from_secs(2),
        "deep cursor page took {:?}",
        elapsed_c
    );

    // (d) actor_id filtered page
    let start = std::time::Instant::now();
    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            &base(&format!("?actor_id={actor_id}")),
            Method::GET,
            actor,
            Some(tenant_id),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let elapsed_d = start.elapsed();
    assert!(
        elapsed_d < Duration::from_secs(2),
        "actor_id filter took {:?}",
        elapsed_d
    );
}
