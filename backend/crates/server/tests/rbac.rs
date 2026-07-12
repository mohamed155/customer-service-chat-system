use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use axum::{routing::get, Extension, Router};
use config::Environment;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use server::router;
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;

const TENANT_PERMISSIONS: &[&str] = &[
    "overview.view",
    "conversations.view",
    "conversations.manage",
    "customers.view",
    "customers.manage",
    "ai_agent.view",
    "ai_agent.manage",
    "knowledge_base.view",
    "knowledge_base.manage",
    "integrations.view",
    "integrations.manage",
    "analytics.view",
    "members.view",
    "members.manage",
    "settings.view",
    "settings.manage",
    "billing.view",
    "billing.manage",
    "tenant.delete",
    "owner.assign",
];

const VIEWER_PERMISSIONS: &[&str] = &[
    "overview.view",
    "conversations.view",
    "customers.view",
    "ai_agent.view",
    "knowledge_base.view",
    "integrations.view",
    "analytics.view",
];

const TENANT_OPERATIONS: &[(&str, &str)] = &[
    ("/api/v1/tenant", "overview.view"),
    (
        "/api/v1/test/tenant/conversations/manage",
        "conversations.manage",
    ),
    ("/api/v1/test/tenant/members/manage", "members.manage"),
    ("/api/v1/test/tenant/settings/manage", "settings.manage"),
    ("/api/v1/test/tenant/billing/view", "billing.view"),
    ("/api/v1/test/tenant/billing/manage", "billing.manage"),
];

const PLATFORM_OPERATIONS: &[&str] = &[
    "/api/v1/platform/tenants",
    "/api/v1/test/platform/admin",
    "/api/v1/test/platform/billing/view",
    "/api/v1/test/platform/diagnostics/view",
];

/// T047: deny-by-default sweep covering the create (POST), detail (GET),
/// and management write (PATCH) endpoints explicitly.  `__TENANT_DETAIL__`
/// is a sentinel expanded at runtime to
/// `/api/v1/platform/tenants/{seeded-tenant-id}` because the detail path
/// needs a real UUID.
const PLATFORM_OPERATIONS_DENY_BY_DEFAULT: &[(&str, Method)] = &[
    ("/api/v1/platform/tenants", Method::GET),
    ("/api/v1/platform/tenants", Method::POST),
    ("__TENANT_DETAIL__", Method::GET),
    ("__TENANT_DETAIL__", Method::PATCH),
    ("/api/v1/test/platform/admin", Method::GET),
    ("/api/v1/test/platform/billing/view", Method::GET),
    ("/api/v1/test/platform/diagnostics/view", Method::GET),
];

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
        db_max_connections: 2,
        db_acquire_timeout_ms: 5000,
        ready_probe_timeout_ms: 5000,
        shutdown_grace_seconds: 1,
    }
}

fn app_state(pool: sqlx::PgPool, environment: Environment) -> AppState {
    AppState {
        config: Arc::new(test_config(environment)),
        db: pool,
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
    }
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("skipping RBAC live tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping RBAC live tests: DATABASE_URL is unreachable");
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

fn session_cookie(user_id: Uuid, environment: Environment) -> String {
    let config = test_config(environment);
    let (token, _, _) = identity::session::issue_token(
        &config.auth_jwt_secret,
        config.auth_session_ttl_seconds,
        user_id,
    )
    .unwrap();
    format!("app_session={token}")
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
        builder = builder.header("cookie", session_cookie(user_id, environment));
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
    .bind(format!("rbac_{}@example.com", Uuid::new_v4()))
    .bind("RBAC User")
    .bind(platform_role)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("RBAC Tenant")
        .bind(format!("rbac-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_membership(pool: &sqlx::PgPool, tenant_id: Uuid, user_id: Uuid, role: &str) {
    sqlx::query("INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(tenant_id)
        .bind(user_id)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
}

async fn assert_status(
    pool: &sqlx::PgPool,
    environment: Environment,
    uri: &str,
    user_id: Uuid,
    tenant_id: Option<Uuid>,
    expected: StatusCode,
) {
    let response = send(
        pool.clone(),
        environment.clone(),
        authenticated_request(uri, Method::GET, user_id, tenant_id, environment),
    )
    .await;
    assert_eq!(response.status(), expected, "unexpected status for {uri}");
}

#[tokio::test]
async fn protected_routes_authenticate_before_authorizing_without_live_dependencies() {
    let pool = db::lazy_pool(
        "postgres://127.0.0.1:1/unreachable",
        1,
        Duration::from_millis(1),
    );
    let response = send(
        pool,
        Environment::Test,
        Request::get("/api/v1/test/platform/admin")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        body_json(response).await["error"]["code"],
        "unauthenticated"
    );
}

#[tokio::test]
async fn anonymous_foreign_origin_protected_request_returns_401_before_csrf() {
    let pool = db::lazy_pool(
        "postgres://127.0.0.1:1/unreachable",
        1,
        Duration::from_millis(1),
    );
    let mut config = test_config(Environment::Test);
    config.cors_allowed_origins = vec!["https://dashboard.example".into()];
    let response = router::app_with_test_routes(AppState {
        config: Arc::new(config),
        db: pool,
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
    })
    .oneshot(
        Request::post("/api/v1/auth/logout")
            .header("origin", "https://evil.example")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        body_json(response).await["error"]["code"],
        "unauthenticated"
    );
}

// Every tenant role except Owner has at least one role-derived allow and one
// role-derived deny in the same (tenant) scope.  Owner holds every tenant
// permission, making a same-scope role-derived denial impossible by
// definition; cross-scope denial (platform routes) and missing-effective-set
// fail-closed are verified separately (see
// [`full_access_roles_do_not_bypass_missing_effective_permissions`] and the
// cross-scope switch/platform assertions below).
#[tokio::test]
async fn all_tenant_roles_have_representative_allows_and_denies() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let cases = [
        ("owner", "/api/v1/test/tenant/settings/manage", None),
        (
            "admin",
            "/api/v1/test/tenant/settings/manage",
            Some("/api/v1/test/tenant/billing/manage"),
        ),
        (
            "manager",
            "/api/v1/test/tenant/members/manage",
            Some("/api/v1/test/tenant/settings/manage"),
        ),
        (
            "agent",
            "/api/v1/test/tenant/conversations/manage",
            Some("/api/v1/test/tenant/settings/manage"),
        ),
        (
            "viewer",
            "/api/v1/tenant",
            Some("/api/v1/test/tenant/conversations/manage"),
        ),
    ];

    for (role, allowed_uri, denied_uri) in cases {
        let user_id = seed_user(&pool, None).await;
        seed_membership(&pool, tenant_id, user_id, role).await;
        assert_status(
            &pool,
            Environment::Test,
            allowed_uri,
            user_id,
            Some(tenant_id),
            StatusCode::OK,
        )
        .await;
        let switch = send(
            pool.clone(),
            Environment::Test,
            authenticated_request(
                &format!("/api/v1/platform/tenants/{tenant_id}/switch"),
                Method::POST,
                user_id,
                None,
                Environment::Test,
            ),
        )
        .await;
        assert_eq!(switch.status(), StatusCode::FORBIDDEN);
        assert_status(
            &pool,
            Environment::Test,
            "/api/v1/platform/tenants",
            user_id,
            None,
            StatusCode::FORBIDDEN,
        )
        .await;
        if let Some(denied_uri) = denied_uri {
            assert_status(
                &pool,
                Environment::Test,
                denied_uri,
                user_id,
                Some(tenant_id),
                StatusCode::FORBIDDEN,
            )
            .await;
        }
    }
}

// Owner and Super Admin own every permission in their respective scopes,
// making a role-derived same-scope denial impossible (it would contradict the
// canonical matrix).  Their fail-closed coverage relies on:
//   - Cross-scope denial (see [`all_tenant_roles_have_representative_allows_and_denies`]
//     and [`all_platform_roles_have_representative_allows_and_denies`] for per-role
//     cross-scope assertions)
//   - Missing-effective-set denial (verified below — clear the PermissionSet
//     extension and confirm the guard still rejects)
#[tokio::test]
async fn full_access_roles_do_not_bypass_missing_effective_permissions() {
    let tenant_context = tenancy::TenantContext {
        tenant_id: Uuid::nil(),
        tenant_status: "active".into(),
        principal_kind: identity::PrincipalKind::Tenant,
        tenant_role: Some(authz::TenantRole::Owner),
        permissions: authz::PermissionSet::default(),
    };
    let tenant_response = Router::new()
        .route(
            "/",
            get(|| async { StatusCode::OK })
                .route_layer(authz::require_permission(authz::Permission::SettingsManage)),
        )
        .layer(Extension(tenant_context))
        .layer(Extension(authz::PermissionSet::default()))
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(tenant_response.status(), StatusCode::FORBIDDEN);

    let platform_response = Router::new()
        .route(
            "/",
            get(|| async { StatusCode::OK })
                .route_layer(authz::require_permission(authz::Permission::PlatformAdmin)),
        )
        .layer(Extension(identity::Principal {
            user_id: Uuid::nil(),
            email: "root@example.com".into(),
            display_name: "Root".into(),
            platform_role: Some(identity::PlatformRole::SuperAdmin),
            invalid_platform_role: false,
        }))
        .layer(Extension(authz::PermissionSet::default()))
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(platform_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn invalid_platform_role_is_authenticated_but_denied_on_protected_routes() {
    // Unrecognized platform role → platform_role = None → empty PermissionSet → 403
    let principal = identity::Principal {
        user_id: Uuid::nil(),
        email: "legacy@example.com".into(),
        display_name: "Legacy".into(),
        platform_role: None,
        invalid_platform_role: true,
    };
    let platform_response = Router::new()
        .route(
            "/",
            get(|| async { StatusCode::OK }).route_layer(authz::require_permission(
                authz::Permission::PlatformTenantsList,
            )),
        )
        .layer(axum::middleware::from_fn(
            authz::platform_permission_middleware,
        ))
        .layer(Extension(principal.clone()))
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(platform_response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn invalid_platform_role_can_authenticate_and_me_returns_empty_platform_permissions() {
    // Simulate a user whose stored platform_role is unrecognized:
    // principal_from_row parses it to None and sets invalid_platform_role=true.
    // build_me_response must no longer reject these users.
    let principal = identity::Principal {
        user_id: Uuid::nil(),
        email: "legacy@example.com".into(),
        display_name: "Legacy".into(),
        platform_role: None,
        invalid_platform_role: true,
    };
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    match tenancy::routes::build_me_response(&pool, principal, false).await {
        Ok(response) => {
            assert!(response.platform_role.is_none());
            assert!(response.platform_permissions.is_empty());
            assert!(response.staff_tenant_permissions.is_none());
        }
        Err(error) => panic!("unrecognized platform role should not be denied, got: {error:?}"),
    };
}

// Every platform role except Super Admin has at least one role-derived allow
// and one role-derived deny in the same (platform) scope.  Super Admin holds
// every platform permission, making a same-scope role-derived denial
// impossible; cross-scope denial (nonexistent tenant) and missing-effective-
// set fail-closed are verified separately (see
// [`full_access_roles_do_not_bypass_missing_effective_permissions`] and the
// else-branch below).
#[tokio::test]
async fn all_platform_roles_have_representative_allows_and_denies() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let cases = [
        ("super_admin", "/api/v1/test/platform/admin", None),
        (
            "developer",
            "/api/v1/test/platform/diagnostics/view",
            Some("/api/v1/test/platform/admin"),
        ),
        (
            "support",
            "/api/v1/platform/tenants",
            Some("/api/v1/test/platform/diagnostics/view"),
        ),
        (
            "sales",
            "/api/v1/platform/tenants",
            Some("/api/v1/test/platform/billing/view"),
        ),
        (
            "finance",
            "/api/v1/test/platform/billing/view",
            Some("/api/v1/test/platform/admin"),
        ),
    ];

    for (role, allowed_uri, denied_uri) in cases {
        let user_id = seed_user(&pool, Some(role)).await;
        assert_status(
            &pool,
            Environment::Test,
            allowed_uri,
            user_id,
            None,
            StatusCode::OK,
        )
        .await;
        let switch = send(
            pool.clone(),
            Environment::Test,
            authenticated_request(
                &format!("/api/v1/platform/tenants/{tenant_id}/switch"),
                Method::POST,
                user_id,
                None,
                Environment::Test,
            ),
        )
        .await;
        assert_eq!(switch.status(), StatusCode::OK);
        assert_status(
            &pool,
            Environment::Test,
            "/api/v1/tenant",
            user_id,
            Some(tenant_id),
            StatusCode::OK,
        )
        .await;
        if let Some(denied_uri) = denied_uri {
            assert_status(
                &pool,
                Environment::Test,
                denied_uri,
                user_id,
                None,
                StatusCode::FORBIDDEN,
            )
            .await;
        } else {
            // Cross-scope denial: Super Admin accessing a tenant route for
            // a nonexistent tenant still fails closed (tenant not found).
            assert_status(
                &pool,
                Environment::Test,
                "/api/v1/tenant",
                user_id,
                Some(Uuid::new_v4()),
                StatusCode::FORBIDDEN,
            )
            .await;
        }
    }
}

#[tokio::test]
async fn protected_routes_distinguish_anonymous_401_from_permission_403() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let viewer = seed_user(&pool, None).await;
    seed_membership(&pool, tenant_id, viewer, "viewer").await;

    let anonymous = send(
        pool.clone(),
        Environment::Test,
        Request::get("/api/v1/platform/tenants")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        body_json(anonymous).await["error"]["code"],
        "unauthenticated"
    );

    let denied = send(
        pool,
        Environment::Test,
        authenticated_request(
            "/api/v1/test/tenant/settings/manage",
            Method::GET,
            viewer,
            Some(tenant_id),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let body = body_json(denied).await;
    assert_eq!(body["error"]["code"], "unauthorized");
    assert_eq!(body["error"]["message"], "Access denied");
}

#[tokio::test]
async fn no_role_user_is_denied_by_every_protected_api_route() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let user_id = seed_user(&pool, None).await;
    let tenant_id = seed_tenant(&pool).await;

    for (uri, _) in TENANT_OPERATIONS {
        assert_status(
            &pool,
            Environment::Test,
            uri,
            user_id,
            Some(tenant_id),
            StatusCode::FORBIDDEN,
        )
        .await;
    }
    for uri in PLATFORM_OPERATIONS {
        assert_status(
            &pool,
            Environment::Test,
            uri,
            user_id,
            None,
            StatusCode::FORBIDDEN,
        )
        .await;
    }
    let switch = send(
        pool,
        Environment::Test,
        authenticated_request(
            &format!("/api/v1/platform/tenants/{tenant_id}/switch"),
            Method::POST,
            user_id,
            None,
            Environment::Test,
        ),
    )
    .await;
    assert!(!switch.status().is_success());
}

#[tokio::test]
async fn permission_denial_writes_audit_reason_without_exposing_permission() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let viewer = seed_user(&pool, None).await;
    seed_membership(&pool, tenant_id, viewer, "viewer").await;

    let response = send(
        pool.clone(),
        Environment::Test,
        authenticated_request(
            "/api/v1/test/tenant/settings/manage",
            Method::GET,
            viewer,
            Some(tenant_id),
            Environment::Test,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(!body_json(response)
        .await
        .to_string()
        .contains("settings.manage"));

    // Give the async audit write a moment to flush.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let details: Value = sqlx::query_scalar(
        "SELECT details FROM audit_logs WHERE actor_user_id = $1 AND action = 'tenant.access_denied' ORDER BY created_at DESC LIMIT 1",
    )
    .bind(viewer)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(details["reason"], "permission_denied");
    assert_eq!(details["requested_tenant_id"], json!(tenant_id));
}

#[tokio::test]
async fn me_returns_exact_tenant_permission_payloads() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    for (role, expected) in [
        ("owner", TENANT_PERMISSIONS),
        ("viewer", VIEWER_PERMISSIONS),
    ] {
        let user_id = seed_user(&pool, None).await;
        seed_membership(&pool, tenant_id, user_id, role).await;
        let response = send(
            pool.clone(),
            Environment::Test,
            authenticated_request("/api/v1/me", Method::GET, user_id, None, Environment::Test),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        let (email, display_name): (String, String) =
            sqlx::query_as("SELECT email, display_name FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let (tenant_name, tenant_slug): (String, String) =
            sqlx::query_as("SELECT name, slug FROM tenants WHERE id = $1")
                .bind(tenant_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            body,
            json!({
                "id": user_id,
                "email": email,
                "displayName": display_name,
                "platformRole": null,
                "platformPermissions": [],
                "staffTenantPermissions": null,
                "memberships": [{
                    "tenantId": tenant_id,
                    "tenantName": tenant_name,
                    "tenantSlug": tenant_slug,
                    "role": role,
                    "permissions": expected
                }]
            })
        );
    }
}

#[tokio::test]
async fn me_returns_exact_platform_support_payload_in_production() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let user_id = seed_user(&pool, Some("support")).await;
    let response = send(
        pool.clone(),
        Environment::Production,
        authenticated_request(
            "/api/v1/me",
            Method::GET,
            user_id,
            None,
            Environment::Production,
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let (email, display_name): (String, String) =
        sqlx::query_as("SELECT email, display_name FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        body,
        json!({
            "id": user_id,
            "email": email,
            "displayName": display_name,
            "platformRole": "support",
            "platformPermissions": [
                "platform.tenants.list",
                "platform.tenants.switch",
                "platform.tenants.manage"
            ],
            "staffTenantPermissions": [
                "overview.view",
                "conversations.view",
                "conversations.manage",
                "customers.view",
                "customers.manage",
                "knowledge_base.view"
            ],
            "memberships": []
        })
    );
}

#[tokio::test]
async fn tenant_role_change_takes_effect_on_the_next_request() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let user_id = seed_user(&pool, None).await;
    seed_membership(&pool, tenant_id, user_id, "owner").await;

    assert_status(
        &pool,
        Environment::Test,
        "/api/v1/test/tenant/settings/manage",
        user_id,
        Some(tenant_id),
        StatusCode::OK,
    )
    .await;
    sqlx::query(
        "UPDATE tenant_memberships SET role = 'viewer' WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
    assert_status(
        &pool,
        Environment::Test,
        "/api/v1/test/tenant/settings/manage",
        user_id,
        Some(tenant_id),
        StatusCode::FORBIDDEN,
    )
    .await;
}

#[tokio::test]
async fn platform_role_change_takes_effect_on_the_next_request() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let user_id = seed_user(&pool, Some("super_admin")).await;

    assert_status(
        &pool,
        Environment::Test,
        "/api/v1/test/platform/admin",
        user_id,
        None,
        StatusCode::OK,
    )
    .await;
    sqlx::query("UPDATE users SET platform_role = 'support' WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_status(
        &pool,
        Environment::Test,
        "/api/v1/test/platform/admin",
        user_id,
        None,
        StatusCode::FORBIDDEN,
    )
    .await;
}

#[tokio::test]
async fn database_constraints_reject_unknown_role_values() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let tenant_user = seed_user(&pool, None).await;
    seed_membership(&pool, tenant_id, tenant_user, "viewer").await;
    let platform_user = seed_user(&pool, Some("support")).await;

    let tenant_result = sqlx::query(
        "UPDATE tenant_memberships SET role = 'legacy_role' WHERE tenant_id = $1 AND user_id = $2",
    )
    .bind(tenant_id)
    .bind(tenant_user)
    .execute(&pool)
    .await;
    let platform_result =
        sqlx::query("UPDATE users SET platform_role = 'legacy_role' WHERE id = $1")
            .bind(platform_user)
            .execute(&pool)
            .await;

    assert!(tenant_result.is_err());
    assert!(platform_result.is_err());
}

#[tokio::test]
async fn staff_tenant_access_is_environment_aware() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let tenant_id = seed_tenant(&pool).await;
    let roles = ["super_admin", "developer", "support", "sales", "finance"];

    for role in roles {
        let user_id = seed_user(&pool, Some(role)).await;
        for (uri, _) in TENANT_OPERATIONS {
            assert_status(
                &pool,
                Environment::Development,
                uri,
                user_id,
                Some(tenant_id),
                StatusCode::OK,
            )
            .await;
        }
    }

    for role in roles {
        let user_id = seed_user(&pool, Some(role)).await;
        let cookie = session_cookie(user_id, Environment::Production);
        let expected = match role {
            "super_admin" => [true, true, true, true, true, true],
            "developer" => [true, false, false, false, false, false],
            "support" => [true, true, false, false, false, false],
            "sales" => [true, false, false, false, false, false],
            "finance" => [true, false, false, false, true, false],
            _ => unreachable!(),
        };
        for ((uri, _), allowed) in TENANT_OPERATIONS.iter().zip(expected) {
            let request = Request::get(*uri)
                .header("cookie", &cookie)
                .header("X-Tenant-ID", tenant_id.to_string())
                .body(Body::empty())
                .unwrap();
            let response = send(pool.clone(), Environment::Production, request).await;
            assert_eq!(
                response.status(),
                if allowed {
                    StatusCode::OK
                } else {
                    StatusCode::FORBIDDEN
                },
                "unexpected production result for {role} on {uri}"
            );
        }
    }
}

// PlatformTenantsManage is held by Super Admin and Support only.  Every
// other platform role (Developer, Sales, Finance) plus plain tenant users
// must be rejected with 403 on POST /api/v1/platform/tenants, and anonymous
// callers with 401 — independent of the create-tenant handler's own
// validation logic (which is exercised separately in platform_tenants.rs).
#[tokio::test]
async fn platform_tenants_create_allow_manage_deny_others() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let unique = Uuid::new_v4().simple().to_string();
    let allowed = ["super-admin", "support"];
    let denied = ["developer", "sales", "finance"];

    for role in allowed {
        let user_id = seed_user(&pool, Some(role.replace('-', "_").as_str())).await;
        let slug = format!("ok-{role}-{unique}");
        let body = serde_json::to_vec(&json!({
            "name": format!("Test {role}"),
            "slug": slug,
        }))
        .unwrap();
        let request = Request::post("/api/v1/platform/tenants")
            .header("X-Dev-User-Id", user_id.to_string())
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let response = send(pool.clone(), Environment::Test, request).await;
        assert_eq!(
            response.status(),
            StatusCode::CREATED,
            "expected 201 for role={role}, got {}",
            response.status()
        );
    }

    for role in denied {
        let user_id = seed_user(&pool, Some(role)).await;
        let slug = format!("no-{role}-{unique}");
        let body = serde_json::to_vec(&json!({
            "name": format!("Test {role}"),
            "slug": slug,
        }))
        .unwrap();
        let request = Request::post("/api/v1/platform/tenants")
            .header("X-Dev-User-Id", user_id.to_string())
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let response = send(pool.clone(), Environment::Test, request).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "expected 403 for role={role}, got {}",
            response.status()
        );
    }

    // Plain tenant user (no platform role) → 403 even with a valid body.
    let tenant_id = seed_tenant(&pool).await;
    let tenant_user = seed_user(&pool, None).await;
    seed_membership(&pool, tenant_id, tenant_user, "admin").await;
    let slug = format!("tenantuser-{unique}");
    let body = serde_json::to_vec(&json!({
        "name": "Tenant User Create",
        "slug": slug,
    }))
    .unwrap();
    let request = Request::post("/api/v1/platform/tenants")
        .header("X-Dev-User-Id", tenant_user.to_string())
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let response = send(pool.clone(), Environment::Test, request).await;
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "expected 403 for tenant user, got {}",
        response.status()
    );
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "unauthorized");

    // Anonymous → 401 (auth runs before authorization).
    let body = serde_json::to_vec(&json!({
        "name": "Anonymous Create",
        "slug": format!("anon-{unique}"),
    }))
    .unwrap();
    let request = Request::post("/api/v1/platform/tenants")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let response = send(pool, Environment::Test, request).await;
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "expected 401 for anonymous, got {}",
        response.status()
    );
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "unauthenticated");
}

#[tokio::test]
async fn me_staff_tenant_permissions_follow_environment_for_every_platform_role() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let production_expected = [
        ("super_admin", TENANT_PERMISSIONS),
        (
            "developer",
            &[
                "overview.view",
                "conversations.view",
                "customers.view",
                "ai_agent.view",
                "knowledge_base.view",
                "integrations.view",
                "analytics.view",
                "members.view",
                "settings.view",
            ][..],
        ),
        (
            "support",
            &[
                "overview.view",
                "conversations.view",
                "conversations.manage",
                "customers.view",
                "customers.manage",
                "knowledge_base.view",
            ][..],
        ),
        (
            "sales",
            &[
                "overview.view",
                "analytics.view",
                "members.view",
                "settings.view",
            ][..],
        ),
        (
            "finance",
            &[
                "overview.view",
                "analytics.view",
                "members.view",
                "settings.view",
                "billing.view",
            ][..],
        ),
    ];

    for (role, production_permissions) in production_expected {
        let user_id = seed_user(&pool, Some(role)).await;
        for (environment, expected) in [
            (Environment::Development, TENANT_PERMISSIONS),
            (Environment::Production, production_permissions),
        ] {
            let response = send(
                pool.clone(),
                environment.clone(),
                authenticated_request(
                    "/api/v1/me",
                    Method::GET,
                    user_id,
                    None,
                    environment.clone(),
                ),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                body_json(response).await["staffTenantPermissions"],
                json!(expected),
                "unexpected /me staff permissions for {role} in {environment:?}"
            );
        }
    }
}

// US2 (T023) — `GET /api/v1/platform/tenants/{id}` is read-only and gated by
// `platform.tenants.list` (every platform role) but rejected for tenant users.
#[tokio::test]
async fn platform_tenant_detail_allowed_for_every_platform_role() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let roles = ["super_admin", "developer", "support", "sales", "finance"];

    for role in roles {
        let user_id = seed_user(&pool, Some(role)).await;
        let response = send(
            pool.clone(),
            Environment::Test,
            authenticated_request(
                &format!("/api/v1/platform/tenants/{tenant_id}"),
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
            "expected 200 for role={role} on GET /platform/tenants/{{id}}"
        );
    }
}

#[tokio::test]
async fn platform_tenant_detail_denied_for_every_tenant_role() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let roles = ["owner", "admin", "manager", "agent", "viewer"];

    for role in roles {
        let user_id = seed_user(&pool, None).await;
        seed_membership(&pool, tenant_id, user_id, role).await;
        let response = send(
            pool.clone(),
            Environment::Test,
            authenticated_request(
                &format!("/api/v1/platform/tenants/{tenant_id}"),
                Method::GET,
                user_id,
                None,
                Environment::Test,
            ),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "expected 403 for tenant role={role} on GET /platform/tenants/{{id}}"
        );
        let body = body_json(response).await;
        assert_eq!(body["error"]["code"], "unauthorized");
    }
}

// US2 (T023) — bad `status` filter on the list endpoint is a 422
// `validation_failed` with a per-field `details` array, independent of the
// tenant-list RBAC tests above.
#[tokio::test]
async fn platform_tenants_list_invalid_status_filter_returns_422() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let user_id = seed_user(&pool, Some("super_admin")).await;
    let response = send(
        pool,
        Environment::Test,
        authenticated_request(
            "/api/v1/platform/tenants?status=invalid",
            Method::GET,
            user_id,
            None,
            Environment::Test,
        ),
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "expected 422 for status=invalid"
    );
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "validation_failed");
    let details = body["error"]["details"]
        .as_array()
        .expect("details must be an array");
    assert!(
        details
            .iter()
            .any(|d| d["field"] == "status" && d["code"] == "invalid_value"),
        "expected a status invalid_value detail, got: {details:?}"
    );
}

// US3 (T031) — PATCH /api/v1/platform/tenants/{id} is gated by
// `platform.tenants.manage`, held by Super Admin and Support only. Every
// other platform role (Developer, Sales, Finance) and every tenant role
// (Owner, Admin, Manager, Agent, Viewer) must be rejected with 403, and
// anonymous callers with 401 — independent of the update_tenant handler's
// own validation logic (which is exercised separately in platform_tenants.rs).
#[tokio::test]
async fn platform_tenants_update_allow_manage_deny_others() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();

    let tenant_id = seed_tenant(&pool).await;
    let unique = Uuid::new_v4().simple().to_string();

    // Allowed: super_admin + support must reach the handler (we accept any
    // non-401/non-403 response; the handler's own validation is tested in
    // platform_tenants.rs).
    for role in ["super_admin", "support"] {
        let user_id = seed_user(&pool, Some(role)).await;
        let body = serde_json::to_vec(&json!({
            "name": format!("Updated by {role} {unique}"),
        }))
        .unwrap();
        let request = Request::patch(format!("/api/v1/platform/tenants/{tenant_id}"))
            .header("X-Dev-User-Id", user_id.to_string())
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let response = send(pool.clone(), Environment::Test, request).await;
        assert_ne!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "allowed role {role} must not be 401"
        );
        assert_ne!(
            response.status(),
            StatusCode::FORBIDDEN,
            "allowed role {role} must not be 403, got {}",
            response.status()
        );
    }

    // Denied: other platform roles → 403.
    for role in ["developer", "sales", "finance"] {
        let user_id = seed_user(&pool, Some(role)).await;
        let body = serde_json::to_vec(&json!({ "name": format!("Updated by {role}") })).unwrap();
        let request = Request::patch(format!("/api/v1/platform/tenants/{tenant_id}"))
            .header("X-Dev-User-Id", user_id.to_string())
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let response = send(pool.clone(), Environment::Test, request).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "expected 403 for platform role={role} on PATCH /platform/tenants/{{id}}, got {}",
            response.status()
        );
        let body = body_json(response).await;
        assert_eq!(body["error"]["code"], "unauthorized");
    }

    // Denied: every tenant role → 403 (platform-scope route, no platform role).
    for role in ["owner", "admin", "manager", "agent", "viewer"] {
        let user_id = seed_user(&pool, None).await;
        seed_membership(&pool, tenant_id, user_id, role).await;
        let body =
            serde_json::to_vec(&json!({ "name": format!("Updated by tenant {role}") })).unwrap();
        let request = Request::patch(format!("/api/v1/platform/tenants/{tenant_id}"))
            .header("X-Dev-User-Id", user_id.to_string())
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let response = send(pool.clone(), Environment::Test, request).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "expected 403 for tenant role={role} on PATCH /platform/tenants/{{id}}, got {}",
            response.status()
        );
        let body = body_json(response).await;
        assert_eq!(body["error"]["code"], "unauthorized");
    }

    // Anonymous → 401 (auth runs before authorization).
    let body = serde_json::to_vec(&json!({ "name": "Anonymous Update" })).unwrap();
    let request = Request::patch(format!("/api/v1/platform/tenants/{tenant_id}"))
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let response = send(pool, Environment::Test, request).await;
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "expected 401 for anonymous on PATCH /platform/tenants/{{id}}"
    );
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "unauthenticated");
}

// T047: deny-by-default sweep extended to cover `POST /api/v1/platform/tenants`
// and `GET /api/v1/platform/tenants/{id}`. A no-platform-role user (no
// membership, no platform permission) must receive 403 on every operation in
// `PLATFORM_OPERATIONS_DENY_BY_DEFAULT` — the same fail-closed guarantee the
// existing sweep asserts for the read-only list and the synthetic test
// endpoints, but now also for the create and detail paths.
#[tokio::test]
async fn no_role_user_is_denied_on_every_platform_tenant_endpoint() {
    let Some(pool) = get_pool().await else { return };
    db::run_migrations(&pool).await.unwrap();
    let user_id = seed_user(&pool, None).await;
    let tenant_id = seed_tenant(&pool).await;

    for (uri, method) in PLATFORM_OPERATIONS_DENY_BY_DEFAULT {
        let actual_uri = if *uri == "__TENANT_DETAIL__" {
            format!("/api/v1/platform/tenants/{tenant_id}")
        } else {
            (*uri).to_string()
        };
        // POST /api/v1/platform/tenants and PATCH __TENANT_DETAIL__
        // require a body; others are empty.
        let request = if *method == Method::POST && *uri == "/api/v1/platform/tenants" {
            Request::post(&actual_uri)
                .header("X-Dev-User-Id", user_id.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "Deny By Default Sweep",
                        "slug": format!("deny-default-{}", Uuid::new_v4().simple()),
                    }))
                    .unwrap(),
                ))
                .unwrap()
        } else if *method == Method::PATCH && uri.contains("__TENANT_DETAIL__") {
            Request::patch(&actual_uri)
                .header("X-Dev-User-Id", user_id.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({ "name": "Deny PATCH Sweep" })).unwrap(),
                ))
                .unwrap()
        } else {
            authenticated_request(
                &actual_uri,
                method.clone(),
                user_id,
                None,
                Environment::Test,
            )
        };
        let response = send(pool.clone(), Environment::Test, request).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "expected 403 for {method} {actual_uri} with no role, got {}",
            response.status()
        );
        let body = body_json(response).await;
        assert_eq!(
            body["error"]["code"], "unauthorized",
            "expected error code unauthorized for {method} {actual_uri}, got {body:?}"
        );
    }
}
