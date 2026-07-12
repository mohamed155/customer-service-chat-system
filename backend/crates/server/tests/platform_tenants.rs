use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use axum::response::Response;
use http_body_util::BodyExt;
use server::router;
use server::state::AppState;
use tower::ServiceExt;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Live-gated helpers (mirror tenancy.rs).
// ---------------------------------------------------------------------------

pub async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("skipping platform_tenants test: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        eprintln!("skipping platform_tenants test: could not connect to DATABASE_URL");
        return None;
    }
    Some(pool)
}

pub fn test_config() -> config::AppConfig {
    config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: "redis://127.0.0.1:6379".into(),
        auth_jwt_secret: "test-auth-jwt-secret-at-least-32-bytes".into(),
        auth_session_ttl_seconds: 28_800,
        port: 0,
        bind_address: "0.0.0.0".into(),
        environment: config::Environment::Test,
        cors_allowed_origins: vec![],
        log_format: config::LogFormat::Pretty,
        db_max_connections: 2,
        db_acquire_timeout_ms: 5000,
        ready_probe_timeout_ms: 5000,
        shutdown_grace_seconds: 1,
    }
}

pub fn test_app_state(pool: sqlx::PgPool) -> AppState {
    AppState {
        config: Arc::new(test_config()),
        db: pool,
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
    }
}

pub async fn send_request(pool: sqlx::PgPool, req: Request<Body>) -> Response {
    let state = test_app_state(pool);
    let app = router::app(state);
    app.oneshot(req).await.expect("request should succeed")
}

pub async fn seed_user(pool: &sqlx::PgPool, platform_role: Option<&str>) -> Uuid {
    let email = format!("pt_{}@example.com", Uuid::new_v4());
    match platform_role {
        Some(role) => sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO users (email, display_name, platform_role) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(&email)
        .bind("PT Seed")
        .bind(role)
        .fetch_one(pool)
        .await
        .expect("seed user with role"),
        None => sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id",
        )
        .bind(&email)
        .bind("PT Seed")
        .fetch_one(pool)
        .await
        .expect("seed user"),
    }
}

async fn body_bytes(res: &mut Response) -> Vec<u8> {
    BodyExt::collect(res.body_mut())
        .await
        .unwrap()
        .to_bytes()
        .to_vec()
}

pub async fn body_json(res: &mut Response) -> serde_json::Value {
    serde_json::from_slice(&body_bytes(res).await).unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn create_tenant_succeeds_with_default_plan() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let name = format!("Acme {unique}");
        let slug = format!("acme-{unique}");

        let mut res = send_request(
            pool.clone(),
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": name,
                        "slug": slug,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 201, "expected 201 Created");
        let body = body_json(&mut res).await;
        let created_id = body["id"].as_str().expect("id in response").to_owned();
        assert_eq!(body["name"], name);
        assert_eq!(body["slug"], slug);
        assert_eq!(body["status"], "active");
        assert_eq!(body["plan"], "trial");
        assert!(body["createdAt"].is_string());
        assert!(body["updatedAt"].is_string());
        assert!(body["contactName"].is_null());
        assert!(body["contactEmail"].is_null());

        // Confirm the new tenant is visible to the list endpoint through the
        // public `?q=<slug-fragment>` API. The fragment is the unique portion
        // of the slug, which is `ILIKE`-matched against both `name` and
        // `slug` and uniquely identifies this tenant.
        let mut list_res = send_request(
            pool,
            Request::get(format!("/api/v1/platform/tenants?q={unique}"))
                .header("X-Dev-User-Id", admin.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(list_res.status(), 200, "expected 200 from list endpoint");
        let list_body = body_json(&mut list_res).await;
        let items = list_body["items"]
            .as_array()
            .expect("items array in list response");
        assert!(
            items
                .iter()
                .any(|item| item["id"].as_str() == Some(created_id.as_str())),
            "expected new tenant {created_id} in list filtered by q={unique}, got: {items:?}"
        );
    }

    #[tokio::test]
    async fn create_tenant_audit_row_records_actor() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let slug = format!("audit-{unique}");

        let mut res = send_request(
            pool.clone(),
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "Audit Tenant",
                        "slug": slug,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;
        assert_eq!(res.status(), 201);
        let body = body_json(&mut res).await;
        let tenant_id = body["id"].as_str().unwrap().to_owned();
        drop(res);

        // Give the async write a moment to flush.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let row: Option<(Uuid, String, Option<Uuid>, serde_json::Value)> = sqlx::query_as(
            r#"
            SELECT id, action, tenant_id, details
            FROM audit_logs
            WHERE actor_user_id = $1
              AND action = 'platform.tenant_created'
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(admin)
        .fetch_optional(&pool)
        .await
        .unwrap();

        let (_, action, audited_tenant_id, details) =
            row.expect("expected a platform.tenant_created audit row");
        assert_eq!(action, "platform.tenant_created");
        assert_eq!(
            audited_tenant_id,
            Some(Uuid::parse_str(&tenant_id).unwrap())
        );
        assert_eq!(details["name"], "Audit Tenant");
        assert_eq!(details["slug"], slug);
        assert_eq!(details["plan"], "trial");
    }

    #[tokio::test]
    async fn create_tenant_missing_name_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "slug": format!("noname-{unique}"),
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 422);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "name"),
            "expected a name error detail, got: {details:?}"
        );
    }

    #[tokio::test]
    async fn create_tenant_uppercase_slug_returns_422() {
        // T044: Per spec contract, the supplied slug must match
        // `^[a-z0-9](-?[a-z0-9])*$` exactly. The handler MUST NOT
        // lowercase the slug before validation; an uppercase character
        // is an invalid format and must produce 422.
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "Acme Co",
                        "slug": "Acme",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "uppercase slug must be rejected with 422, not silently lowercased"
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        let slug_detail = details
            .iter()
            .find(|d| d["field"] == "slug")
            .unwrap_or_else(|| panic!("expected a slug error detail, got: {details:?}"));
        assert_eq!(slug_detail["code"], "invalid_format");
    }

    #[tokio::test]
    async fn update_tenant_uppercase_slug_returns_422() {
        // T044: Same contract on PATCH — supplied uppercase slug is rejected
        // with 422, never silently rewritten.
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("Uppercase PATCH Co {unique}"),
            &format!("uc-patch-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let mut res = send_request(
            pool,
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "slug": "Acme" }),
            ),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "uppercase slug on PATCH must be rejected with 422, not silently lowercased"
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        let slug_detail = details
            .iter()
            .find(|d| d["field"] == "slug")
            .unwrap_or_else(|| panic!("expected a slug error detail, got: {details:?}"));
        assert_eq!(slug_detail["code"], "invalid_format");
    }

    #[tokio::test]
    async fn create_tenant_malformed_slug_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "Bad Slug Co",
                        "slug": "Bad Slug!",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 422);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "slug"),
            "expected a slug error detail, got: {details:?}"
        );
    }

    #[tokio::test]
    async fn create_tenant_duplicate_live_slug_returns_409() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let slug = format!("dup-{unique}");

        // Pre-existing live tenant with this slug.
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id",
        )
        .bind("Existing Tenant")
        .bind(&slug)
        .fetch_one(&pool)
        .await
        .unwrap();

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "Duplicate Tenant",
                        "slug": slug,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 409);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "conflict");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details
                .iter()
                .any(|d| d["field"] == "slug" && d["code"] == "conflict"),
            "expected a slug conflict detail, got: {details:?}"
        );
    }

    #[tokio::test]
    async fn create_tenant_malformed_contact_email_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "Bad Email Co",
                        "slug": format!("bademail-{unique}"),
                        "contactEmail": "not-an-email",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 422);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "contactEmail"),
            "expected a contactEmail error detail, got: {details:?}"
        );
    }

    #[tokio::test]
    async fn create_tenant_switches_in_successfully() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let slug = format!("switch-{unique}");

        // Create the new tenant.
        let mut create_res = send_request(
            pool.clone(),
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "Switchable Tenant",
                        "slug": slug,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;
        assert_eq!(create_res.status(), 201);
        let body = body_json(&mut create_res).await;
        let new_id: Uuid = body["id"].as_str().unwrap().parse().unwrap();
        drop(create_res);

        // Switch to the new tenant.
        let mut switch_res = send_request(
            pool,
            Request::post(format!("/api/v1/platform/tenants/{new_id}/switch"))
                .header("X-Dev-User-Id", admin.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(switch_res.status(), 200);
        let switch_body = body_json(&mut switch_res).await;
        assert_eq!(switch_body["id"], serde_json::json!(new_id));
    }

    // -----------------------------------------------------------------------
    // US2 (T024) — list: status filter, combined q+status, multi-page traversal
    // -----------------------------------------------------------------------

    async fn collect_tenant_ids(pool: sqlx::PgPool, admin: Uuid, query: &str) -> Vec<Uuid> {
        let mut ids: Vec<Uuid> = Vec::new();
        let mut next_cursor: Option<String> = None;
        let mut guard = 0;
        loop {
            guard += 1;
            assert!(guard < 100, "runaway pagination guard");
            let mut url = format!("/api/v1/platform/tenants?{query}&limit=10");
            if let Some(cursor) = next_cursor.take() {
                url.push_str(&format!("&cursor={cursor}"));
            }
            let mut res = send_request(
                pool.clone(),
                Request::get(url)
                    .header("X-Dev-User-Id", admin.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(res.status(), 200, "expected 200 from list endpoint");
            let body = body_json(&mut res).await;
            let items = body["items"].as_array().expect("items array");
            for item in items {
                let id_str = item["id"].as_str().expect("id string");
                ids.push(Uuid::parse_str(id_str).expect("valid uuid"));
            }
            let has_more = body["hasMore"].as_bool().unwrap_or(false);
            if has_more {
                next_cursor = body["nextCursor"].as_str().map(|s| s.to_owned());
            } else {
                break;
            }
        }
        ids
    }

    #[tokio::test]
    async fn list_tenants_status_filter_returns_matching_subset() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        // Names that DO NOT collide with the q=alpha scenarios so the
        // status filter assertions are exact.
        let mut active_ids = Vec::new();
        for i in 0..3 {
            let id = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO tenants (name, slug, status) VALUES ($1, $2, 'active') RETURNING id",
            )
            .bind(format!("US2 Active {unique} {i}"))
            .bind(format!("us2-active-{unique}-{i}"))
            .fetch_one(&pool)
            .await
            .unwrap();
            active_ids.push(id);
        }
        let mut suspended_ids = Vec::new();
        for i in 0..2 {
            let id = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO tenants (name, slug, status) VALUES ($1, $2, 'suspended') RETURNING id",
            )
            .bind(format!("US2 Suspended {unique} {i}"))
            .bind(format!("us2-sus-{unique}-{i}"))
            .fetch_one(&pool)
            .await
            .unwrap();
            suspended_ids.push(id);
        }

        // status=active: must include the 3 we seeded with that status; must
        // NOT include any of the 2 we seeded as suspended.
        // `+` decodes to a space, matching the space-separated seeded names.
        let active_observed = collect_tenant_ids(
            pool.clone(),
            admin,
            &format!("q=US2+Active+{unique}&status=active"),
        )
        .await;
        for id in &active_ids {
            assert!(
                active_observed.contains(id),
                "expected active id {id} in active-filtered list, got {active_observed:?}"
            );
        }
        for id in &suspended_ids {
            assert!(
                !active_observed.contains(id),
                "did not expect suspended id {id} in active-filtered list, got {active_observed:?}"
            );
        }

        // status=suspended: must include the 2 we seeded with that status; must
        // NOT include any of the 3 we seeded as active.
        // `+` decodes to a space, matching the space-separated seeded names.
        let suspended_observed = collect_tenant_ids(
            pool,
            admin,
            &format!("q=US2+Suspended+{unique}&status=suspended"),
        )
        .await;
        for id in &suspended_ids {
            assert!(
                suspended_observed.contains(id),
                "expected suspended id {id} in suspended-filtered list, got {suspended_observed:?}"
            );
        }
        for id in &active_ids {
            assert!(
                !suspended_observed.contains(id),
                "did not expect active id {id} in suspended-filtered list, got {suspended_observed:?}"
            );
        }
    }

    #[tokio::test]
    async fn list_tenants_q_and_status_combine() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        // Mixed: 2 alpha active, 1 alpha suspended, 2 beta active.
        let mut alpha_active = Vec::new();
        for i in 0..2 {
            let id = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO tenants (name, slug, status) VALUES ($1, $2, 'active') RETURNING id",
            )
            .bind(format!("AlphaCombine {unique} {i}"))
            .bind(format!("alpha-combine-{unique}-{i}"))
            .fetch_one(&pool)
            .await
            .unwrap();
            alpha_active.push(id);
        }
        let alpha_suspended = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tenants (name, slug, status) VALUES ($1, $2, 'suspended') RETURNING id",
        )
        .bind(format!("AlphaSuspended {unique}"))
        .bind(format!("alpha-sus-{unique}"))
        .fetch_one(&pool)
        .await
        .unwrap();
        let mut beta_active = Vec::new();
        for i in 0..2 {
            let id = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO tenants (name, slug, status) VALUES ($1, $2, 'active') RETURNING id",
            )
            .bind(format!("BetaCombine {unique} {i}"))
            .bind(format!("beta-combine-{unique}-{i}"))
            .fetch_one(&pool)
            .await
            .unwrap();
            beta_active.push(id);
        }

        let observed = collect_tenant_ids(
            pool,
            admin,
            &format!("q=AlphaCombine+{unique}&status=active"),
        )
        .await;

        for id in &alpha_active {
            assert!(
                observed.contains(id),
                "expected alpha+active id {id} in intersection list, got {observed:?}"
            );
        }
        assert!(
            !observed.contains(&alpha_suspended),
            "alpha+suspended must not appear under status=active, got {observed:?}"
        );
        for id in &beta_active {
            assert!(
                !observed.contains(id),
                "beta+active must not appear under q=AlphaCombine, got {observed:?}"
            );
        }
    }

    #[tokio::test]
    async fn list_tenants_pagination_traverses_full_set() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        // Seed 30 tenants tagged with this unique; we'll page through with
        // limit=10 and assert every tenant appears exactly once.
        let mut seeded: Vec<Uuid> = Vec::with_capacity(30);
        for i in 0..30 {
            let id = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO tenants (name, slug, status) VALUES ($1, $2, 'active') RETURNING id",
            )
            .bind(format!("PageTenant {unique} {i}"))
            .bind(format!("page-{unique}-{i:02}"))
            .fetch_one(&pool)
            .await
            .unwrap();
            seeded.push(id);
        }

        let observed = collect_tenant_ids(pool, admin, &format!("q=PageTenant+{unique}")).await;

        // Every seeded id should appear exactly once.
        let mut seeded_sorted = seeded.clone();
        seeded_sorted.sort();
        let mut observed_sorted = observed.clone();
        observed_sorted.sort();
        assert_eq!(
            seeded_sorted, observed_sorted,
            "paginated traversal must visit every seeded tenant exactly once"
        );
    }

    // -----------------------------------------------------------------------
    // US2 (T024) — detail: 200 for live, 404 for unknown/soft-deleted
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn get_tenant_detail_returns_200_for_live_tenant() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tenants (name, slug, plan, contact_name, contact_email) \
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(format!("Detail Co {unique}"))
        .bind(format!("detail-{unique}"))
        .bind("professional")
        .bind("Detail Contact")
        .bind("contact@example.test")
        .fetch_one(&pool)
        .await
        .unwrap();

        let mut res = send_request(
            pool,
            Request::get(format!("/api/v1/platform/tenants/{tenant_id}"))
                .header("X-Dev-User-Id", admin.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(res.status(), 200);
        let body = body_json(&mut res).await;
        assert_eq!(body["id"], serde_json::json!(tenant_id));
        assert_eq!(body["name"], format!("Detail Co {unique}"));
        assert_eq!(body["slug"], format!("detail-{unique}"));
        assert_eq!(body["status"], "active");
        assert_eq!(body["plan"], "professional");
        assert_eq!(body["contactName"], "Detail Contact");
        assert_eq!(body["contactEmail"], "contact@example.test");
        assert!(body["createdAt"].is_string());
        assert!(body["updatedAt"].is_string());
    }

    #[tokio::test]
    async fn get_tenant_detail_returns_404_for_unknown() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unknown_id = Uuid::new_v4();
        let mut res = send_request(
            pool,
            Request::get(format!("/api/v1/platform/tenants/{unknown_id}"))
                .header("X-Dev-User-Id", admin.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(res.status(), 404);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "not_found");
    }

    #[tokio::test]
    async fn get_tenant_detail_returns_404_for_soft_deleted() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id",
        )
        .bind(format!("Soft Deleted Co {unique}"))
        .bind(format!("soft-deleted-{unique}"))
        .fetch_one(&pool)
        .await
        .unwrap();

        // Soft-delete it.
        sqlx::query("UPDATE tenants SET deleted_at = now() WHERE id = $1")
            .bind(tenant_id)
            .execute(&pool)
            .await
            .unwrap();

        let mut res = send_request(
            pool,
            Request::get(format!("/api/v1/platform/tenants/{tenant_id}"))
                .header("X-Dev-User-Id", admin.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(res.status(), 404);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "not_found");
    }

    // -----------------------------------------------------------------------
    // US3 (T032) — update_tenant
    // -----------------------------------------------------------------------

    async fn seed_tenant_full(
        pool: &sqlx::PgPool,
        name: &str,
        slug: &str,
        plan: &str,
        contact_name: Option<&str>,
        contact_email: Option<&str>,
    ) -> Uuid {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tenants (name, slug, plan, contact_name, contact_email) \
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(name)
        .bind(slug)
        .bind(plan)
        .bind(contact_name)
        .bind(contact_email)
        .fetch_one(pool)
        .await
        .expect("seed tenant full")
    }

    fn patch_request(uri: String, user_id: Uuid, body: serde_json::Value) -> Request<Body> {
        Request::patch(uri)
            .header("X-Dev-User-Id", user_id.to_string())
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    }

    #[tokio::test]
    async fn update_tenant_persists_name_plan_contact() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("Original Co {unique}"),
            &format!("orig-{unique}"),
            "trial",
            Some("Original Contact"),
            Some("orig@example.test"),
        )
        .await;

        // PATCH {name, plan, contactName, contactEmail}
        let mut res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({
                    "name": format!("Renamed Co {unique}"),
                    "plan": "enterprise",
                    "contactName": "Renamed Contact",
                    "contactEmail": "renamed@example.test",
                }),
            ),
        )
        .await;
        assert_eq!(res.status(), 200, "expected 200 from PATCH");
        let body = body_json(&mut res).await;
        assert_eq!(body["id"], serde_json::json!(tenant_id));
        assert_eq!(body["name"], format!("Renamed Co {unique}"));
        assert_eq!(body["slug"], format!("orig-{unique}"));
        assert_eq!(body["status"], "active");
        assert_eq!(body["plan"], "enterprise");
        assert_eq!(body["contactName"], "Renamed Contact");
        assert_eq!(body["contactEmail"], "renamed@example.test");

        // GET it back to confirm persistence.
        let mut get_res = send_request(
            pool,
            Request::get(format!("/api/v1/platform/tenants/{tenant_id}"))
                .header("X-Dev-User-Id", admin.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(get_res.status(), 200);
        let get_body = body_json(&mut get_res).await;
        assert_eq!(get_body["name"], format!("Renamed Co {unique}"));
        assert_eq!(get_body["plan"], "enterprise");
        assert_eq!(get_body["contactName"], "Renamed Contact");
        assert_eq!(get_body["contactEmail"], "renamed@example.test");
    }

    #[tokio::test]
    async fn update_tenant_slug_change_writes_audit_via_trigger() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let old_slug = format!("slug-old-{unique}");
        let new_slug = format!("slug-new-{unique}");
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("Slug Co {unique}"),
            &old_slug,
            "trial",
            None,
            None,
        )
        .await;

        let mut res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "slug": new_slug }),
            ),
        )
        .await;
        assert_eq!(res.status(), 200);
        let body = body_json(&mut res).await;
        assert_eq!(body["slug"], new_slug);
        drop(res);

        // The DB trigger must have written a row with the actor we set.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let row: Option<(Uuid, Uuid, String, serde_json::Value)> = sqlx::query_as(
            "SELECT actor_user_id, tenant_id, resource_id::text, details \
             FROM audit_logs \
             WHERE action = 'tenant.slug_changed' AND tenant_id = $1 \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
        let (actor, audited_tenant, resource_id, details) =
            row.expect("expected a tenant.slug_changed audit row from the trigger");
        assert_eq!(actor, admin, "audit actor must be the patching user");
        assert_eq!(audited_tenant, tenant_id);
        assert_eq!(resource_id, tenant_id.to_string());
        assert_eq!(details["old_slug"], old_slug);
        assert_eq!(details["new_slug"], new_slug);
    }

    #[tokio::test]
    async fn update_tenant_slug_collision_returns_409() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let collision_slug = format!("col-{unique}");

        let first_id = seed_tenant_full(
            &pool,
            &format!("First Co {unique}"),
            &format!("first-{unique}"),
            "trial",
            None,
            None,
        )
        .await;
        let _second_id = seed_tenant_full(
            &pool,
            &format!("Second Co {unique}"),
            &collision_slug,
            "trial",
            None,
            None,
        )
        .await;

        // PATCH first tenant's slug → second's slug.
        let mut res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{first_id}"),
                admin,
                json!({ "slug": collision_slug }),
            ),
        )
        .await;
        assert_eq!(res.status(), 409, "expected 409 on slug collision");
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "conflict");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details
                .iter()
                .any(|d| d["field"] == "slug" && d["code"] == "conflict"),
            "expected a slug conflict detail, got: {details:?}"
        );
        drop(res);

        // The first tenant's slug must be unchanged in the DB.
        let (current_slug,): (String,) = sqlx::query_as("SELECT slug FROM tenants WHERE id = $1")
            .bind(first_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(current_slug, format!("first-{unique}"));
    }

    #[tokio::test]
    async fn update_tenant_status_change_to_suspended_blocks_member_next_request() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("Suspend Co {unique}"),
            &format!("suspend-{unique}"),
            "trial",
            None,
            None,
        )
        .await;
        let member = seed_user(&pool, None).await;
        sqlx::query(
            "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, 'agent')",
        )
        .bind(tenant_id)
        .bind(member)
        .execute(&pool)
        .await
        .unwrap();

        // Baseline: GET /api/v1/tenant as the member works while active.
        let before = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant")
                .header("X-Dev-User-Id", member.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(
            before.status(),
            200,
            "member must reach /tenant while tenant is active"
        );

        // PATCH tenant to suspended.
        let mut patch_res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "status": "suspended" }),
            ),
        )
        .await;
        assert_eq!(patch_res.status(), 200);
        let body = body_json(&mut patch_res).await;
        assert_eq!(body["status"], "suspended");
        drop(patch_res);

        // Member is now blocked on the next tenant-scoped request.
        let mut blocked = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant")
                .header("X-Dev-User-Id", member.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(
            blocked.status(),
            403,
            "member must be blocked when tenant is suspended"
        );
        let body = body_json(&mut blocked).await;
        assert_eq!(body["error"]["code"], "unauthorized");
        drop(blocked);

        // Re-activate → member regains access.
        let reactivate = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "status": "active" }),
            ),
        )
        .await;
        assert_eq!(reactivate.status(), 200);
        drop(reactivate);

        let after = send_request(
            pool,
            Request::builder()
                .uri("/api/v1/tenant")
                .header("X-Dev-User-Id", member.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(
            after.status(),
            200,
            "member must regain access after re-activation"
        );
    }

    #[tokio::test]
    async fn update_tenant_status_change_audit_asserts_old_and_new() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("Status Audit Co {unique}"),
            &format!("status-audit-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "status": "suspended" }),
            ),
        )
        .await;
        assert_eq!(res.status(), 200);
        drop(res);

        tokio::time::sleep(Duration::from_millis(50)).await;
        let row: Option<(Uuid, serde_json::Value)> = sqlx::query_as(
            "SELECT actor_user_id, details \
             FROM audit_logs \
             WHERE action = 'platform.tenant_status_changed' AND tenant_id = $1 \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
        let (actor, details) = row.expect("expected a platform.tenant_status_changed audit row");
        assert_eq!(actor, admin);
        assert_eq!(details["old_status"], "active");
        assert_eq!(details["new_status"], "suspended");
    }

    #[tokio::test]
    async fn update_tenant_field_change_audit_asserts_old_and_new() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("Field Audit Co {unique}"),
            &format!("field-audit-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let new_name = format!("Field Audit Co Renamed {unique}");
        let res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({
                    "name": new_name,
                    "plan": "enterprise",
                }),
            ),
        )
        .await;
        assert_eq!(res.status(), 200);
        drop(res);

        tokio::time::sleep(Duration::from_millis(50)).await;
        let row: Option<(Uuid, serde_json::Value)> = sqlx::query_as(
            "SELECT actor_user_id, details \
             FROM audit_logs \
             WHERE action = 'platform.tenant_updated' AND tenant_id = $1 \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
        let (actor, details) = row.expect("expected a platform.tenant_updated audit row");
        assert_eq!(actor, admin);
        let changes = &details["changes"];
        assert_eq!(changes["name"]["old"], format!("Field Audit Co {unique}"));
        assert_eq!(changes["name"]["new"], new_name);
        assert_eq!(changes["plan"]["old"], "trial");
        assert_eq!(changes["plan"]["new"], "enterprise");
    }

    #[tokio::test]
    async fn create_tenant_atomic_with_audit() {
        // T042: After a successful POST, the audit row must be visible
        // without any sleep. Inserting the audit row inside the same
        // transaction as the tenant row guarantees they commit together.
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let slug = format!("atomic-create-{unique}");

        let mut res = send_request(
            pool.clone(),
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": format!("Atomic Create {unique}"),
                        "slug": slug,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;
        assert_eq!(res.status(), 201, "expected 201 Created");
        let body = body_json(&mut res).await;
        let tenant_id = body["id"].as_str().unwrap().to_owned();
        drop(res);

        // No `tokio::time::sleep` here — atomicity is the whole point. The
        // audit row must already be queryable the moment the response
        // returns 201.
        let row: Option<(Uuid, String, serde_json::Value)> = sqlx::query_as(
            r#"
            SELECT actor_user_id, action, details
            FROM audit_logs
            WHERE action = 'platform.tenant_created'
              AND tenant_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(Uuid::parse_str(&tenant_id).unwrap())
        .fetch_optional(&pool)
        .await
        .unwrap();
        let (actor, action, details) = row
            .expect("platform.tenant_created audit row must be committed together with the tenant");
        assert_eq!(action, "platform.tenant_created");
        assert_eq!(actor, admin);
        assert_eq!(details["slug"], slug);
    }

    #[tokio::test]
    async fn update_tenant_audits_are_atomic_with_update() {
        // T042: A PATCH that fails mid-transaction must NOT leave behind an
        // audit row. The slug-collision path is the cleanest way to force a
        // sqlx error inside the transaction: the UPDATE itself fails with
        // 23505, the transaction drops, and nothing — neither the tenant
        // update nor the audit insert — may be observable afterwards.
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let collision_slug = format!("atomic-coll-{unique}");

        // The tenant that will receive a (failing) PATCH.
        let target_id = seed_tenant_full(
            &pool,
            &format!("Atomic Target {unique}"),
            &format!("atomic-target-{unique}"),
            "trial",
            None,
            None,
        )
        .await;
        // A pre-existing live tenant whose slug is the collision target.
        let _other_id = seed_tenant_full(
            &pool,
            &format!("Atomic Other {unique}"),
            &collision_slug,
            "trial",
            None,
            None,
        )
        .await;

        // Baseline counts so we can compare absolute values too (the table
        // is shared with parallel/prior tests).
        let before_status_changed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs \
             WHERE tenant_id = $1 AND action = 'platform.tenant_status_changed'",
        )
        .bind(target_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let before_updated: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs \
             WHERE tenant_id = $1 AND action = 'platform.tenant_updated'",
        )
        .bind(target_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        // PATCH the target with a slug that collides → 409, no audit row.
        let mut res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{target_id}"),
                admin,
                json!({
                    "name": format!("Renamed {unique}"),
                    "slug": collision_slug,
                }),
            ),
        )
        .await;
        assert_eq!(res.status(), 409, "expected 409 on slug collision");
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "conflict");
        drop(res);

        // The target's row is unchanged (name + slug), and NO new audit
        // row of any platform-tenant action exists for it.
        let (current_name, current_slug): (String, String) =
            sqlx::query_as("SELECT name, slug FROM tenants WHERE id = $1")
                .bind(target_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(current_name, format!("Atomic Target {unique}"));
        assert_eq!(current_slug, format!("atomic-target-{unique}"));

        let after_status_changed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs \
             WHERE tenant_id = $1 AND action = 'platform.tenant_status_changed'",
        )
        .bind(target_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let after_updated: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs \
             WHERE tenant_id = $1 AND action = 'platform.tenant_updated'",
        )
        .bind(target_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            after_status_changed, before_status_changed,
            "no platform.tenant_status_changed row must be persisted for a failed PATCH"
        );
        assert_eq!(
            after_updated, before_updated,
            "no platform.tenant_updated row must be persisted for a failed PATCH"
        );

        // The trigger-owned slug audit must also be absent.
        let slug_changed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs \
             WHERE tenant_id = $1 AND action = 'tenant.slug_changed'",
        )
        .bind(target_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            slug_changed, 0,
            "trigger-owned tenant.slug_changed must also roll back with the failed tx"
        );
    }

    #[tokio::test]
    async fn update_tenant_concurrent_patches_audit_distinct_old_values() {
        // T042: Two concurrent PATCHes on the same tenant must each
        // observe a serialised view of the row (the one whose `SELECT FOR
        // UPDATE` acquired the lock first). The audit row written by each
        // PATCH must carry that PATCH's own `old` value, not the latest
        // committed value at the time the audit insert runs.
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("Concurrent {unique}"),
            &format!("conc-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        // Each patch changes the name to a unique value. Both target the
        // same row, so the row's `name` will be serialised by the
        // `SELECT ... FOR UPDATE` lock: whichever transaction commits
        // first sees the original name, the second sees the first's new
        // name as its `old`.
        let first_new = format!("First new name {unique}");
        let second_new = format!("Second new name {unique}");
        let original_name = format!("Concurrent {unique}");

        let pool_a = pool.clone();
        let admin_a = admin;
        let tenant_id_a = tenant_id;
        let first_new_a = first_new.clone();
        let first_fut = tokio::spawn(async move {
            send_request(
                pool_a,
                patch_request(
                    format!("/api/v1/platform/tenants/{tenant_id_a}"),
                    admin_a,
                    json!({ "name": first_new_a }),
                ),
            )
            .await
        });

        let pool_b = pool.clone();
        let admin_b = admin;
        let tenant_id_b = tenant_id;
        let second_new_b = second_new.clone();
        let second_fut = tokio::spawn(async move {
            send_request(
                pool_b,
                patch_request(
                    format!("/api/v1/platform/tenants/{tenant_id_b}"),
                    admin_b,
                    json!({ "name": second_new_b }),
                ),
            )
            .await
        });

        let (res1, res2) = tokio::join!(first_fut, second_fut);
        let res1 = res1.expect("first task should not panic");
        let res2 = res2.expect("second task should not panic");
        assert_eq!(res1.status(), 200, "first PATCH must succeed");
        assert_eq!(res2.status(), 200, "second PATCH must succeed");

        // The final stored name must be one of the two new values
        // (we don't know which won the race).
        let final_name: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(
            final_name == first_new || final_name == second_new,
            "final name must be one of the new values, got {final_name}"
        );

        // Each patch must have produced exactly one audit row.
        let rows: Vec<(Uuid, serde_json::Value)> = sqlx::query_as(
            "SELECT actor_user_id, details \
             FROM audit_logs \
             WHERE tenant_id = $1 AND action = 'platform.tenant_updated' \
             ORDER BY created_at ASC",
        )
        .bind(tenant_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            rows.len(),
            2,
            "expected exactly two platform.tenant_updated rows, got {}: {rows:?}",
            rows.len()
        );

        // Collect (old, new) pairs from each row.
        let pairs: Vec<(String, String)> = rows
            .iter()
            .map(|(_, details)| {
                let changes = &details["changes"];
                (
                    changes["name"]["old"].as_str().unwrap().to_string(),
                    changes["name"]["new"].as_str().unwrap().to_string(),
                )
            })
            .collect();

        // The two `new` values must be exactly the two patches' new names.
        let mut new_values: Vec<String> = pairs.iter().map(|(_, n)| n.clone()).collect();
        new_values.sort();
        let mut expected_new = vec![first_new.clone(), second_new.clone()];
        expected_new.sort();
        assert_eq!(
            new_values, expected_new,
            "audit `new` values must match the two patches' new names; got {pairs:?}"
        );

        // The two `old` values must form a chain: one is the original
        // name, the other is one of the PATCHes' `new` values. This is the
        // serialisation guarantee — the lock-aware read means each PATCH
        // observes the value committed before it. The set of `old` values
        // is exactly {original_name} ∪ {some_new_value}: one of the
        // PATCHes (the one that committed first) saw the original; the
        // other saw the first PATCH's new value. Equivalently: of the
        // two `old` values, exactly one is NOT a `new` value — and that
        // one must be the original name.
        let new_set: std::collections::HashSet<&String> = new_values.iter().collect();
        let non_new_olds: Vec<&String> = pairs
            .iter()
            .map(|(o, _)| o)
            .filter(|o| !new_set.contains(o))
            .collect();
        assert_eq!(
            non_new_olds,
            vec![&original_name],
            "the only `old` value that isn't also a `new` value must be the original name; got {pairs:?}"
        );

        // The `new` of one audit row must equal the `old` of the other
        // (the chain link). This is the strongest serialisation proof:
        // it means one PATCH observed the other PATCH's value before
        // recording its own audit row.
        let first_pair = &pairs[0];
        let second_pair = &pairs[1];
        let linked = (first_pair.0 == second_pair.1) || (second_pair.0 == first_pair.1);
        assert!(
            linked,
            "the two audit rows must chain (one's `new` = the other's `old`); got {pairs:?}"
        );
    }

    #[tokio::test]
    async fn update_tenant_two_sequential_patches_both_audit() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("Sequential Co {unique}"),
            &format!("seq-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        for i in 0..2 {
            let res = send_request(
                pool.clone(),
                patch_request(
                    format!("/api/v1/platform/tenants/{tenant_id}"),
                    admin,
                    json!({ "name": format!("Sequential Co {unique} patch {i}") }),
                ),
            )
            .await;
            assert_eq!(res.status(), 200);
            drop(res);
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs \
             WHERE action = 'platform.tenant_updated' AND tenant_id = $1 AND actor_user_id = $2",
        )
        .bind(tenant_id)
        .bind(admin)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            count, 2,
            "expected two platform.tenant_updated rows, got {count}"
        );
    }

    // -----------------------------------------------------------------------
    // T047 — Convergence coverage: list status filter (no q), anonymous/
    // tenant-role denial on create+detail, create validation, PATCH
    // validation/mixed-audit, soft-deleted slug reuse, deny-by-default sweep.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn list_tenants_status_filter_alone_returns_matching_subset() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        // Seed 3 active and 2 suspended tenants with a unique tag.
        let mut active_ids = Vec::new();
        for i in 0..3 {
            let id = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO tenants (name, slug, status) VALUES ($1, $2, 'active') RETURNING id",
            )
            .bind(format!("T047Active {unique} {i}"))
            .bind(format!("t047-active-{unique}-{i}"))
            .fetch_one(&pool)
            .await
            .unwrap();
            active_ids.push(id);
        }
        let mut suspended_ids = Vec::new();
        for i in 0..2 {
            let id = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO tenants (name, slug, status) VALUES ($1, $2, 'suspended') RETURNING id",
            )
            .bind(format!("T047Suspended {unique} {i}"))
            .bind(format!("t047-sus-{unique}-{i}"))
            .fetch_one(&pool)
            .await
            .unwrap();
            suspended_ids.push(id);
        }

        // status=active with no `q` must return every live active tenant and
        // exclude every suspended tenant. We sweep every page at the maximum
        // page size (limit=100, the kernel clamp) and accumulate the observed
        // ids. The guard is sized to cover the worst-case database state
        // observed during convergence.
        let mut observed_active_ids: Vec<Uuid> = Vec::new();
        let mut next_cursor: Option<String> = None;
        let mut guard = 0;
        loop {
            guard += 1;
            assert!(guard < 5000, "runaway pagination guard for status=active");
            let mut url = String::from("/api/v1/platform/tenants?status=active&limit=100");
            if let Some(cursor) = next_cursor.take() {
                url.push_str(&format!("&cursor={cursor}"));
            }
            let mut res = send_request(
                pool.clone(),
                Request::get(&url)
                    .header("X-Dev-User-Id", admin.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(res.status(), 200, "expected 200 from list endpoint");
            let body = body_json(&mut res).await;
            let items = body["items"].as_array().expect("items array");
            for item in items {
                let id_str = item["id"].as_str().expect("id string");
                observed_active_ids.push(Uuid::parse_str(id_str).expect("valid uuid"));
            }
            let has_more = body["hasMore"].as_bool().unwrap_or(false);
            if has_more {
                next_cursor = body["nextCursor"].as_str().map(|s| s.to_owned());
            } else {
                break;
            }
        }
        for id in &active_ids {
            assert!(
                observed_active_ids.contains(id),
                "expected active id {id} in status=active list, got {} ids",
                observed_active_ids.len()
            );
        }
        for id in &suspended_ids {
            assert!(
                !observed_active_ids.contains(id),
                "did not expect suspended id {id} in status=active list, got {} ids",
                observed_active_ids.len()
            );
        }

        // The same sweep with status=suspended must NOT include any active id.
        // The active filter and the suspended filter must be disjoint.
        let mut observed_suspended_ids: Vec<Uuid> = Vec::new();
        let mut next_cursor: Option<String> = None;
        let mut guard = 0;
        loop {
            guard += 1;
            assert!(
                guard < 5000,
                "runaway pagination guard for status=suspended"
            );
            let mut url = String::from("/api/v1/platform/tenants?status=suspended&limit=100");
            if let Some(cursor) = next_cursor.take() {
                url.push_str(&format!("&cursor={cursor}"));
            }
            let mut res = send_request(
                pool.clone(),
                Request::get(&url)
                    .header("X-Dev-User-Id", admin.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(res.status(), 200, "expected 200 from list endpoint");
            let body = body_json(&mut res).await;
            let items = body["items"].as_array().expect("items array");
            for item in items {
                let id_str = item["id"].as_str().expect("id string");
                observed_suspended_ids.push(Uuid::parse_str(id_str).expect("valid uuid"));
            }
            let has_more = body["hasMore"].as_bool().unwrap_or(false);
            if has_more {
                next_cursor = body["nextCursor"].as_str().map(|s| s.to_owned());
            } else {
                break;
            }
        }
        for id in &suspended_ids {
            assert!(
                observed_suspended_ids.contains(id),
                "expected suspended id {id} in status=suspended list, got {} ids",
                observed_suspended_ids.len()
            );
        }
        for id in &active_ids {
            assert!(
                !observed_suspended_ids.contains(id),
                "did not expect active id {id} in status=suspended list, got {} ids",
                observed_suspended_ids.len()
            );
        }
    }

    #[tokio::test]
    async fn create_tenant_anonymous_returns_401() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let unique = Uuid::new_v4().simple().to_string();
        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "Anonymous Co",
                        "slug": format!("t047-anon-{unique}"),
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 401, "expected 401 for anonymous create");
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "unauthenticated");
    }

    #[tokio::test]
    async fn create_tenant_each_tenant_role_returns_403() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        // Seed a tenant so the role-bearing users have a valid scope.
        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id",
        )
        .bind(format!("T047 Role Scope {unique}"))
        .bind(format!("t047-role-scope-{unique}"))
        .fetch_one(&pool)
        .await
        .unwrap();

        let roles = ["owner", "admin", "manager", "agent", "viewer"];
        for role in roles {
            let user_id = seed_user(&pool, None).await;
            sqlx::query(
                "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)",
            )
            .bind(tenant_id)
            .bind(user_id)
            .bind(role)
            .execute(&pool)
            .await
            .unwrap();

            let slug = format!("t047-{role}-{unique}");
            let mut res = send_request(
                pool.clone(),
                Request::post("/api/v1/platform/tenants")
                    .header("X-Dev-User-Id", user_id.to_string())
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "name": format!("T047 {role}"),
                            "slug": slug,
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await;
            assert_eq!(
                res.status(),
                403,
                "expected 403 for tenant role={role} on POST /platform/tenants, got {}",
                res.status()
            );
            let body = body_json(&mut res).await;
            assert_eq!(body["error"]["code"], "unauthorized");
        }

        // Sanity: the super_admin (who can do everything) still creates fine.
        let res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "T047 Sanity Admin",
                        "slug": format!("t047-sanity-{unique}"),
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;
        assert_eq!(res.status(), 201);
    }

    #[tokio::test]
    async fn create_tenant_invalid_plan_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "Premium Co",
                        "slug": format!("t047-premium-{unique}"),
                        "plan": "premium",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 422);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            !details.is_empty(),
            "expected at least one detail entry, got: {details:?}"
        );
        assert_eq!(
            details[0]["field"], "plan",
            "expected details[0].field == plan, got: {details:?}"
        );
    }

    #[tokio::test]
    async fn create_tenant_invalid_email_format_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "Bad Email Co",
                        "slug": format!("t047-bademail-{unique}"),
                        "contactEmail": "not-an-email",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 422);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            !details.is_empty(),
            "expected at least one detail entry, got: {details:?}"
        );
        assert_eq!(
            details[0]["field"], "contactEmail",
            "expected details[0].field == contactEmail, got: {details:?}"
        );
    }

    #[tokio::test]
    async fn create_tenant_soft_deleted_slug_reuse_succeeds() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let slug = format!("t047-reuse-{unique}");

        // Seed a tenant, then soft-delete it. The unique partial index
        // excludes soft-deleted rows, so the slug is reusable.
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id",
        )
        .bind(format!("T047 Reuse Original {unique}"))
        .bind(&slug)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE tenants SET deleted_at = now() WHERE slug = $1")
            .bind(&slug)
            .execute(&pool)
            .await
            .unwrap();

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": format!("T047 Reuse Reused {unique}"),
                        "slug": slug,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(
            res.status(),
            201,
            "expected 201 for soft-deleted slug reuse, got {}",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["slug"], slug);
        assert_eq!(body["status"], "active");
    }

    #[tokio::test]
    async fn update_tenant_invalid_plan_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T047 PatchPlan {unique}"),
            &format!("t047-patchplan-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let mut res = send_request(
            pool,
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "plan": "premium" }),
            ),
        )
        .await;

        assert_eq!(res.status(), 422);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            !details.is_empty(),
            "expected at least one detail entry, got: {details:?}"
        );
        assert_eq!(
            details[0]["field"], "plan",
            "expected details[0].field == plan, got: {details:?}"
        );
    }

    /// T056: PATCH `{"plan": ""}` must be 422 with a `plan` detail. A blank
    /// plan is not a clearing signal — `plan` is non-nullable.
    #[tokio::test]
    async fn update_tenant_blank_plan_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T056 BlankPlan {unique}"),
            &format!("t056-blankplan-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let mut res = send_request(
            pool,
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "plan": "" }),
            ),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "PATCH with blank plan must be 422, got {}",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "plan"),
            "expected a plan error detail, got: {details:?}"
        );
    }

    #[tokio::test]
    async fn update_tenant_invalid_status_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T047 PatchStatus {unique}"),
            &format!("t047-patchstatus-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let mut res = send_request(
            pool,
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "status": "deleted" }),
            ),
        )
        .await;

        assert_eq!(res.status(), 422);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            !details.is_empty(),
            "expected at least one detail entry, got: {details:?}"
        );
        assert_eq!(
            details[0]["field"], "status",
            "expected details[0].field == status, got: {details:?}"
        );
    }

    #[tokio::test]
    async fn update_tenant_combined_status_and_field_change_emits_both_audits() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let original_name = format!("T047 Mixed {unique}");
        let tenant_id = seed_tenant_full(
            &pool,
            &original_name,
            &format!("t047-mixed-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let new_name = format!("T047 Mixed Renamed {unique}");
        let res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({
                    "status": "suspended",
                    "name": new_name,
                }),
            ),
        )
        .await;
        assert_eq!(res.status(), 200);
        drop(res);

        // Allow the async audit insert to flush.
        tokio::time::sleep(Duration::from_millis(50)).await;

        // platform.tenant_status_changed: old=active, new=suspended, actor=admin
        let status_row: Option<(Uuid, serde_json::Value)> = sqlx::query_as(
            "SELECT actor_user_id, details FROM audit_logs \
             WHERE action = 'platform.tenant_status_changed' AND tenant_id = $1 \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
        let (status_actor, status_details) =
            status_row.expect("expected a platform.tenant_status_changed audit row");
        assert_eq!(status_actor, admin);
        assert_eq!(status_details["old_status"], "active");
        assert_eq!(status_details["new_status"], "suspended");

        // platform.tenant_updated: changes.name.old/new for the rename
        let updated_row: Option<(Uuid, serde_json::Value)> = sqlx::query_as(
            "SELECT actor_user_id, details FROM audit_logs \
             WHERE action = 'platform.tenant_updated' AND tenant_id = $1 \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(&pool)
        .await
        .unwrap();
        let (updated_actor, updated_details) =
            updated_row.expect("expected a platform.tenant_updated audit row");
        assert_eq!(updated_actor, admin);
        let changes = &updated_details["changes"];
        assert_eq!(changes["name"]["old"], original_name);
        assert_eq!(changes["name"]["new"], new_name);
    }

    #[tokio::test]
    async fn get_tenant_detail_anonymous_returns_401() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id",
        )
        .bind(format!("T047 Detail Anon {unique}"))
        .bind(format!("t047-detail-anon-{unique}"))
        .fetch_one(&pool)
        .await
        .unwrap();

        let mut res = send_request(
            pool,
            Request::get(format!("/api/v1/platform/tenants/{tenant_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 401, "expected 401 for anonymous detail");
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "unauthenticated");
    }

    #[tokio::test]
    async fn get_tenant_detail_each_tenant_role_returns_403() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id",
        )
        .bind(format!("T047 Detail Role {unique}"))
        .bind(format!("t047-detail-role-{unique}"))
        .fetch_one(&pool)
        .await
        .unwrap();

        let roles = ["owner", "admin", "manager", "agent", "viewer"];
        for role in roles {
            let user_id = seed_user(&pool, None).await;
            sqlx::query(
                "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, $3)",
            )
            .bind(tenant_id)
            .bind(user_id)
            .bind(role)
            .execute(&pool)
            .await
            .unwrap();

            let mut res = send_request(
                pool.clone(),
                Request::get(format!("/api/v1/platform/tenants/{tenant_id}"))
                    .header("X-Dev-User-Id", user_id.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(
                res.status(),
                403,
                "expected 403 for tenant role={role} on GET /platform/tenants/{{id}}, got {}",
                res.status()
            );
            let body = body_json(&mut res).await;
            assert_eq!(body["error"]["code"], "unauthorized");
        }
    }

    // -----------------------------------------------------------------------
    // T057 / T059 — blank rejection + accumulated validation errors
    //
    // The platform-tenant-management contract requires:
    //   * Supplied blank strings for `plan`, `contactName`, `contactEmail` on
    //     create are rejected with 422 (not silently treated as absent).
    //   * All semantic field validation failures are accumulated into one 422
    //     response so the UI can surface every problem at once.
    //   * Truly malformed JSON still returns 400 (the boundary layer stays
    //     distinct from per-field validation).
    //   * Unknown / wrong-typed fields in otherwise-valid JSON return 422 with
    //     the offending field named in `details`, not a generic 400.
    // -----------------------------------------------------------------------

    /// T057: A supplied `plan: ""` must surface as 422 with a per-field
    /// detail — it is NOT a clearing signal and must NOT default to "trial".
    #[tokio::test]
    async fn create_tenant_blank_plan_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "Blank Plan Co",
                        "slug": format!("t057-blankplan-{unique}"),
                        "plan": "",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "blank plan must be 422, not silently defaulted to trial"
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "plan"),
            "expected a plan error detail, got: {details:?}"
        );
    }

    /// T057: A supplied `contactName: ""` must surface as 422 with a per-field
    /// detail (camelCase field name in the error envelope).
    #[tokio::test]
    async fn create_tenant_blank_contact_name_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "Blank ContactName Co",
                        "slug": format!("t057-blankcn-{unique}"),
                        "contactName": "",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "blank contactName must be 422, not silently cleared"
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "contactName"),
            "expected a contactName error detail, got: {details:?}"
        );
    }

    /// T057: A supplied `contactEmail: ""` must surface as 422 with a
    /// per-field detail (camelCase field name in the error envelope).
    #[tokio::test]
    async fn create_tenant_blank_contact_email_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "Blank ContactEmail Co",
                        "slug": format!("t057-blankce-{unique}"),
                        "contactEmail": "",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "blank contactEmail must be 422, not silently cleared"
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "contactEmail"),
            "expected a contactEmail error detail, got: {details:?}"
        );
    }

    /// T059: Multiple invalid fields on POST must produce one 422 with a
    /// `details` entry for EACH offending field, not the first one only.
    #[tokio::test]
    async fn create_tenant_multiple_invalid_fields_returns_all_details() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "",
                        "slug": "BadSlug!",
                        "contactEmail": "not-an-email",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "expected 422 for multiple invalid fields"
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        let fields: Vec<&str> = details.iter().filter_map(|d| d["field"].as_str()).collect();
        assert!(
            fields.contains(&"name"),
            "expected a name error detail, got fields={fields:?}"
        );
        assert!(
            fields.contains(&"slug"),
            "expected a slug error detail, got fields={fields:?}"
        );
        assert!(
            fields.contains(&"contactEmail"),
            "expected a contactEmail error detail, got fields={fields:?}"
        );
    }

    /// T059: Same accumulation contract on PATCH — multiple invalid fields
    /// yield one 422 with one detail per offending field.
    #[tokio::test]
    async fn update_tenant_multiple_invalid_fields_returns_all_details() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T059 Mixed {unique}"),
            &format!("t059-mixed-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let mut res = send_request(
            pool,
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({
                    "name": "",
                    "slug": "BadSlug!",
                    "status": "deleted",
                    "contactEmail": "not-an-email",
                }),
            ),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "expected 422 for multiple invalid PATCH fields"
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        let fields: Vec<&str> = details.iter().filter_map(|d| d["field"].as_str()).collect();
        assert!(
            fields.contains(&"name"),
            "expected a name error detail, got fields={fields:?}"
        );
        assert!(
            fields.contains(&"slug"),
            "expected a slug error detail, got fields={fields:?}"
        );
        assert!(
            fields.contains(&"status"),
            "expected a status error detail, got fields={fields:?}"
        );
        assert!(
            fields.contains(&"contactEmail"),
            "expected a contactEmail error detail, got fields={fields:?}"
        );
    }

    /// T059: An unknown field in otherwise-valid JSON must produce 422 (not
    /// the generic 400 the previous kernel extractor emitted) and must
    /// surface the offending field name in `details`.
    #[tokio::test]
    async fn create_tenant_unknown_field_returns_422_with_detail() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "Unknown Field Co",
                        "slug": format!("t059-unknown-{unique}"),
                        "extraneous": "y",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "valid JSON with an unknown field must be 422, not 400"
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "extraneous"),
            "expected the unknown field name in details, got: {details:?}"
        );
    }

    /// T059: Truly malformed JSON (parser / EOF error) must still return 400
    /// — the boundary between "the body is not even valid JSON" and "the JSON
    /// is well-formed but the type/shape is wrong" must stay distinct.
    #[tokio::test]
    async fn create_tenant_truly_malformed_json_returns_400() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from("{ not valid json"))
                .unwrap(),
        )
        .await;

        assert_eq!(
            res.status(),
            400,
            "truly malformed JSON must remain 400 (not 422)"
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
    }

    /// T057: An *omitted* `plan` (not supplied at all) must still default to
    /// "trial". This is the half of the contract that this task explicitly
    /// preserves — only a *supplied blank* is rejected, not a missing one.
    #[tokio::test]
    async fn create_tenant_omitted_plan_defaults_to_trial() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": format!("Omitted Plan Co {unique}"),
                        "slug": format!("t057-omittedplan-{unique}"),
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(
            res.status(),
            201,
            "omitted plan must still 201, defaulting to trial"
        );
        let body = body_json(&mut res).await;
        assert_eq!(
            body["plan"], "trial",
            "omitted plan must default to trial, got: {body:?}"
        );
    }

    // -----------------------------------------------------------------------
    // T056 — PATCH absent-vs-null field semantics.
    //
    // The contract requires:
    //   * Absent (field not in body) → do not touch the column.
    //   * `null` (explicit JSON null) on a non-nullable field
    //     (`name`, `slug`, `plan`, `status`) → 422.
    //   * `""` (blank) on a non-nullable field → 422.
    //   * `null` or `""` on a nullable field (`contactName`, `contactEmail`)
    //     → clear the column.
    // -----------------------------------------------------------------------

    /// T056: PATCH `{"name": null}` must be 422 with a `name` detail. JSON
    /// `null` is a distinct value from field omission; the contract forbids
    /// nulling the non-nullable `name` column.
    #[tokio::test]
    async fn update_tenant_null_name_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T056 NullName {unique}"),
            &format!("t056-nullname-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let mut res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "name": null }),
            ),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "PATCH with explicit null name must be 422, got {}",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        let name_detail = details
            .iter()
            .find(|d| d["field"] == "name")
            .unwrap_or_else(|| panic!("expected a name error detail, got: {details:?}"));
        assert_eq!(name_detail["code"], "invalid_value");

        // The stored name must be unchanged.
        let stored: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stored, format!("T056 NullName {unique}"));
    }

    /// T056: PATCH `{"name": ""}` must be 422 with a `name` detail. A blank
    /// string on a non-nullable field is not a clearing signal — it is an
    /// invalid value.
    #[tokio::test]
    async fn update_tenant_blank_name_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T056 BlankName {unique}"),
            &format!("t056-blankname-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let mut res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "name": "" }),
            ),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "PATCH with blank name must be 422, got {}",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "name"),
            "expected a name error detail, got: {details:?}"
        );

        // The stored name must be unchanged.
        let stored: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stored, format!("T056 BlankName {unique}"));
    }

    /// T056: PATCH `{"plan": null}` must be 422 with a `plan` detail. `plan`
    /// is non-nullable in the schema and the contract does not permit
    /// `null` as a clearing signal for it.
    #[tokio::test]
    async fn update_tenant_null_plan_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T056 NullPlan {unique}"),
            &format!("t056-nullplan-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let mut res = send_request(
            pool,
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "plan": null }),
            ),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "PATCH with explicit null plan must be 422, got {}",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "plan"),
            "expected a plan error detail, got: {details:?}"
        );
    }

    /// T056: PATCH `{"slug": null}` must be 422 with a `slug` detail. `slug`
    /// is non-nullable and the contract does not permit nulling it.
    #[tokio::test]
    async fn update_tenant_null_slug_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T056 NullSlug {unique}"),
            &format!("t056-nullslug-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let mut res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "slug": null }),
            ),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "PATCH with explicit null slug must be 422, got {}",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "slug"),
            "expected a slug error detail, got: {details:?}"
        );

        // The stored slug must be unchanged.
        let stored: String = sqlx::query_scalar("SELECT slug FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stored, format!("t056-nullslug-{unique}"));
    }

    /// T056: PATCH `{"slug": ""}` must be 422 with a `slug` detail. A blank
    /// slug is invalid — not a clearing signal.
    #[tokio::test]
    async fn update_tenant_blank_slug_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T056 BlankSlug {unique}"),
            &format!("t056-blankslug-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let mut res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "slug": "" }),
            ),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "PATCH with blank slug must be 422, got {}",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "slug"),
            "expected a slug error detail, got: {details:?}"
        );

        // The stored slug must be unchanged.
        let stored: String = sqlx::query_scalar("SELECT slug FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stored, format!("t056-blankslug-{unique}"));
    }

    /// T056: PATCH `{"status": null}` must be 422 with a `status` detail.
    /// `status` is non-nullable and null is not a valid value.
    #[tokio::test]
    async fn update_tenant_null_status_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T056 NullStatus {unique}"),
            &format!("t056-nullstatus-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let mut res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "status": null }),
            ),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "PATCH with explicit null status must be 422, got {}",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "status"),
            "expected a status error detail, got: {details:?}"
        );

        // The stored status must be unchanged.
        let stored: String = sqlx::query_scalar("SELECT status FROM tenants WHERE id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stored, "active");
    }

    /// T056: PATCH with only `{"plan": "starter"}` — absent fields (`name`,
    /// `slug`, `status`, `contactName`, `contactEmail`) must NOT be touched.
    #[tokio::test]
    async fn update_tenant_absent_fields_preserved() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let original_name = format!("T056 Absent {unique}");
        let original_slug = format!("t056-absent-{unique}");
        let tenant_id = seed_tenant_full(
            &pool,
            &original_name,
            &original_slug,
            "trial",
            Some("Original Contact"),
            Some("original@example.test"),
        )
        .await;

        // PATCH only `plan` — everything else must be untouched.
        let mut res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "plan": "starter" }),
            ),
        )
        .await;
        assert_eq!(res.status(), 200);
        let body = body_json(&mut res).await;
        assert_eq!(body["name"], original_name);
        assert_eq!(body["slug"], original_slug);
        assert_eq!(body["status"], "active");
        assert_eq!(body["plan"], "starter");
        assert_eq!(body["contactName"], "Original Contact");
        assert_eq!(body["contactEmail"], "original@example.test");

        // Confirm with a fresh GET.
        let mut get_res = send_request(
            pool,
            Request::get(format!("/api/v1/platform/tenants/{tenant_id}"))
                .header("X-Dev-User-Id", admin.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(get_res.status(), 200);
        let get_body = body_json(&mut get_res).await;
        assert_eq!(get_body["name"], original_name);
        assert_eq!(get_body["slug"], original_slug);
        assert_eq!(get_body["status"], "active");
        assert_eq!(get_body["plan"], "starter");
        assert_eq!(get_body["contactName"], "Original Contact");
        assert_eq!(get_body["contactEmail"], "original@example.test");
    }

    /// T056: PATCH with an empty body `{}` must be a no-op (200, all
    /// existing values preserved).
    #[tokio::test]
    async fn update_tenant_empty_body_preserves_all_fields() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let original_name = format!("T056 EmptyBody {unique}");
        let original_slug = format!("t056-emptybody-{unique}");
        let tenant_id = seed_tenant_full(
            &pool,
            &original_name,
            &original_slug,
            "professional",
            Some("Contact Name"),
            Some("contact@test.example"),
        )
        .await;

        let mut res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({}),
            ),
        )
        .await;
        assert_eq!(res.status(), 200);
        let body = body_json(&mut res).await;
        assert_eq!(body["name"], original_name);
        assert_eq!(body["slug"], original_slug);
        assert_eq!(body["status"], "active");
        assert_eq!(body["plan"], "professional");
        assert_eq!(body["contactName"], "Contact Name");
        assert_eq!(body["contactEmail"], "contact@test.example");
    }

    /// T056: PATCH `{"status": ""}` must be 422 with a `status` detail.
    #[tokio::test]
    async fn update_tenant_blank_status_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T056 BlankStatus {unique}"),
            &format!("t056-blankstatus-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let mut res = send_request(
            pool,
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "status": "" }),
            ),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "PATCH with blank status must be 422, got {}",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "status"),
            "expected a status error detail, got: {details:?}"
        );
    }

    /// T056: PATCH `{"contactName": null, "contactEmail": null}` must clear
    /// both contact columns (the nullable-field half of the contract).
    #[tokio::test]
    async fn update_tenant_explicit_null_contact_clears_field() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T056 NullContact {unique}"),
            &format!("t056-nullcontact-{unique}"),
            "trial",
            Some("Initial Contact"),
            Some("initial@example.test"),
        )
        .await;

        let mut res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "contactName": null, "contactEmail": null }),
            ),
        )
        .await;
        assert_eq!(
            res.status(),
            200,
            "explicit null contacts must clear the columns"
        );
        let body = body_json(&mut res).await;
        assert!(body["contactName"].is_null());
        assert!(body["contactEmail"].is_null());
        drop(res);

        // Verify with a fresh GET that the columns are null in the DB.
        let mut get_res = send_request(
            pool,
            Request::get(format!("/api/v1/platform/tenants/{tenant_id}"))
                .header("X-Dev-User-Id", admin.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(get_res.status(), 200);
        let get_body = body_json(&mut get_res).await;
        assert!(get_body["contactName"].is_null());
        assert!(get_body["contactEmail"].is_null());
    }

    /// T069: PATCH `{"contactName": "", "contactEmail": ""}` must return 422
    /// with field-level error details — blank values on a nullable field are
    /// invalid; use explicit JSON `null` to clear the column instead.
    #[tokio::test]
    async fn update_tenant_blank_contact_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T069 BlankContact {unique}"),
            &format!("t069-blankcontact-{unique}"),
            "trial",
            Some("Initial Contact"),
            Some("initial@example.test"),
        )
        .await;

        let mut res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "contactName": "", "contactEmail": "" }),
            ),
        )
        .await;
        assert_eq!(res.status(), 422);
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");

        // Both fields should have their own error detail.
        let name_detail = details
            .iter()
            .find(|d| d["field"] == "contactName")
            .unwrap_or_else(|| panic!("expected contactName detail, got: {details:?}"));
        assert_eq!(name_detail["code"], "invalid_value");

        let email_detail = details
            .iter()
            .find(|d| d["field"] == "contactEmail")
            .unwrap_or_else(|| panic!("expected contactEmail detail, got: {details:?}"));
        assert_eq!(email_detail["code"], "invalid_value");
    }

    /// T058: POST `{"name": ""}` must be 422 with a `name` detail. A blank
    /// name on create is an invalid value, not a "use default" signal.
    #[tokio::test]
    async fn create_tenant_blank_name_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": "",
                        "slug": format!("t058-blankname-{unique}"),
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 422, "blank name on create must be 422");
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "name"),
            "expected a name error detail, got: {details:?}"
        );
    }

    /// T058: A 200-character name composed entirely of 3-byte UTF-8 chars
    /// ("日" = 3 bytes each, 600 bytes total) MUST be accepted on create.
    /// PostgreSQL `length()` counts characters (200), not bytes (600), and
    /// the Rust handler must match — currently `str::len()` rejects this.
    #[tokio::test]
    async fn create_tenant_multibyte_name_under_200_chars_accepted() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let name: String = "日".repeat(200);
        assert_eq!(name.chars().count(), 200, "name should be 200 chars");
        assert!(
            name.len() > 200,
            "name should be more than 200 bytes (multibyte)"
        );

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": name,
                        "slug": format!("t058-mb200-{unique}"),
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(
            res.status(),
            201,
            "200-char multibyte name must be accepted (got {}); bytes: {}",
            res.status(),
            name.len()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["name"].as_str().unwrap().chars().count(), 200);
    }

    /// T058: Same multibyte boundary on PATCH.
    #[tokio::test]
    async fn update_tenant_multibyte_name_under_200_chars_accepted() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T058 Multibyte {unique}"),
            &format!("t058-mb-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let new_name: String = "日".repeat(200);
        assert_eq!(new_name.chars().count(), 200);
        assert!(
            new_name.len() > 200,
            "new_name should be more than 200 bytes"
        );

        let mut res = send_request(
            pool,
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "name": new_name }),
            ),
        )
        .await;

        assert_eq!(
            res.status(),
            200,
            "200-char multibyte name on PATCH must be accepted (got {})",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["name"].as_str().unwrap().chars().count(), 200);
    }

    // -----------------------------------------------------------------------
    // T054 — SC-005: Authenticated member context for suspend/refuse/recover
    //
    // Exercises the full lifecycle with a real signed-in tenant-member
    // (no platform role) using authenticated requests via X-Dev-User-Id +
    // X-Tenant-ID. Verifies that:
    //   1. The member can access tenant-scoped endpoints while active.
    //   2. The member's very next request is refused after suspension.
    //   3. The member regains access on the next request after reactivation.
    //   4. Access-denied audit rows are written when the member is refused.
    //   5. Status-change audit rows carry the correct actor and old/new.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn t054_member_suspend_refuse_reactivate_recover_with_audits() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        // Step 1: Create tenant via the platform API (realistic flow).
        let mut create_res = send_request(
            pool.clone(),
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": format!("T054 Suspend Co {unique}"),
                        "slug": format!("t054-suspend-{unique}"),
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;
        assert_eq!(create_res.status(), 201);
        let create_body = body_json(&mut create_res).await;
        let tenant_id: Uuid = create_body["id"].as_str().unwrap().parse().unwrap();
        assert_eq!(create_body["status"], "active");

        // Step 2: Seed a tenant-member (no platform_role) and add membership.
        let member = seed_user(&pool, None).await;
        sqlx::query(
            "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, 'agent')",
        )
        .bind(tenant_id)
        .bind(member)
        .execute(&pool)
        .await
        .unwrap();

        // Step 3: Member's request succeeds while tenant is active.
        let mut active_res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant")
                .header("X-Dev-User-Id", member.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(
            active_res.status(),
            200,
            "member must reach /tenant while active"
        );
        let active_body = body_json(&mut active_res).await;
        assert_eq!(active_body["id"], serde_json::json!(tenant_id));

        // Step 4: Admin suspends the tenant.
        let mut suspend_res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "status": "suspended" }),
            ),
        )
        .await;
        assert_eq!(suspend_res.status(), 200, "admin suspend must succeed");
        let suspend_body = body_json(&mut suspend_res).await;
        assert_eq!(suspend_body["status"], "suspended");

        // Step 5: Member's very next request is refused.
        let mut blocked_res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant")
                .header("X-Dev-User-Id", member.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(
            blocked_res.status(),
            403,
            "member must be refused immediately after suspension"
        );
        let blocked_body = body_json(&mut blocked_res).await;
        assert_eq!(blocked_body["error"]["code"], "unauthorized");

        // Step 6: Admin reactivates the tenant.
        let mut reactivate_res = send_request(
            pool.clone(),
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "status": "active" }),
            ),
        )
        .await;
        assert_eq!(
            reactivate_res.status(),
            200,
            "admin reactivate must succeed"
        );
        let reactivate_body = body_json(&mut reactivate_res).await;
        assert_eq!(reactivate_body["status"], "active");

        // Step 7: Member regains access on the next request.
        let mut recovered_res = send_request(
            pool.clone(),
            Request::builder()
                .uri("/api/v1/tenant")
                .header("X-Dev-User-Id", member.to_string())
                .header("X-Tenant-ID", tenant_id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(
            recovered_res.status(),
            200,
            "member must regain access after reactivation"
        );
        let recovered_body = body_json(&mut recovered_res).await;
        assert_eq!(recovered_body["id"], serde_json::json!(tenant_id));

        // Step 8: Verify audit rows — access_denied written when member was refused.
        let access_denied_row: Option<(Uuid, String, serde_json::Value)> = sqlx::query_as(
            r#"
            SELECT id, action, details
            FROM audit_logs
            WHERE actor_user_id = $1
              AND action = 'tenant.access_denied'
              AND details->>'requested_tenant_id' = $2
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(member)
        .bind(tenant_id.to_string())
        .fetch_optional(&pool)
        .await
        .unwrap();
        let (_, action, details) = access_denied_row
            .expect("expected a tenant.access_denied audit row for the blocked member");
        assert_eq!(action, "tenant.access_denied");
        assert!(
            details.get("reason").is_some(),
            "access_denied details must include a reason"
        );

        // Step 9: Verify status-change audit rows for both transitions.
        let status_rows: Vec<(Uuid, serde_json::Value)> = sqlx::query_as(
            r#"
            SELECT actor_user_id, details
            FROM audit_logs
            WHERE action = 'platform.tenant_status_changed'
              AND tenant_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(
            status_rows.len() >= 2,
            "expected at least 2 platform.tenant_status_changed rows (suspend + reactivate), got {}",
            status_rows.len()
        );

        // The first transition: active → suspended, by admin.
        let (actor1, details1) = &status_rows[0];
        assert_eq!(*actor1, admin, "suspend must be attributed to admin");
        assert_eq!(details1["old_status"], "active");
        assert_eq!(details1["new_status"], "suspended");

        // The second transition: suspended → active, by admin.
        let (actor2, details2) = &status_rows[1];
        assert_eq!(*actor2, admin, "reactivate must be attributed to admin");
        assert_eq!(details2["old_status"], "suspended");
        assert_eq!(details2["new_status"], "active");
    }

    // -----------------------------------------------------------------------
    // T096 — Slug validation: reject trailing hyphens and surrounding whitespace
    // (validate the slug exactly as supplied, no mutation).
    // -----------------------------------------------------------------------

    /// T096: A slug with leading whitespace must be rejected with 422 — the
    /// handler must NOT trim the slug before validation.
    #[tokio::test]
    async fn create_tenant_slug_with_leading_space_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": format!("T096 Leading Space {unique}"),
                        "slug": format!(" leading-space-{unique}"),
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "slug with leading space must be 422, not silently trimmed"
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "slug"),
            "expected a slug error detail, got: {details:?}"
        );
    }

    /// T096: A slug ending with a trailing hyphen must be rejected with 422.
    #[tokio::test]
    async fn create_tenant_slug_trailing_hyphen_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": format!("T096 Trailing Hyphen {unique}"),
                        "slug": format!("trailing-hyphen-{unique}-"),
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(res.status(), 422, "slug with trailing hyphen must be 422");
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "slug"),
            "expected a slug error detail, got: {details:?}"
        );
    }

    /// T096: Same trailing-hyphen rejection on PATCH.
    #[tokio::test]
    async fn update_tenant_slug_trailing_hyphen_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T096 PATCH Trailing {unique}"),
            &format!("t096-patch-trailing-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let mut res = send_request(
            pool,
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "slug": format!("acme-{unique}-") }),
            ),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "slug with trailing hyphen on PATCH must be 422"
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "slug"),
            "expected a slug error detail, got: {details:?}"
        );
    }

    /// T096: Slug with leading whitespace on PATCH must be 422.
    #[tokio::test]
    async fn update_tenant_slug_with_leading_space_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T096 PATCH Space {unique}"),
            &format!("t096-patch-space-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let mut res = send_request(
            pool,
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "slug": format!(" acme-{unique}") }),
            ),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "slug with leading space on PATCH must be 422, not silently trimmed"
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "slug"),
            "expected a slug error detail, got: {details:?}"
        );
    }

    // -----------------------------------------------------------------------
    // T110 — Validate plan/status/contact exactly without trimming
    // -----------------------------------------------------------------------

    /// T110: plan " trial" (with leading space) on create must be 422 —
    /// leading whitespace is not a valid plan value.
    #[tokio::test]
    async fn create_tenant_plan_with_leading_space_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": format!("T110 Leading Plan {unique}"),
                        "slug": format!("t110-leading-plan-{unique}"),
                        "plan": " trial",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "plan with leading space on create must be 422, got {}",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "plan"),
            "expected a plan error detail, got: {details:?}"
        );
    }

    /// T110: plan " trial" (with leading space) on PATCH must be 422.
    #[tokio::test]
    async fn update_tenant_plan_with_leading_space_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T110 Leading Plan PATCH {unique}"),
            &format!("t110-leading-plan-patch-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let mut res = send_request(
            pool,
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "plan": " trial" }),
            ),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "plan with leading space on PATCH must be 422, got {}",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "plan"),
            "expected a plan error detail, got: {details:?}"
        );
    }

    /// T110: status "active " (with trailing space) on PATCH must be 422 —
    /// trailing whitespace is not a valid status value.
    #[tokio::test]
    async fn update_tenant_status_trailing_space_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T110 Trailing Status {unique}"),
            &format!("t110-trailing-status-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let mut res = send_request(
            pool,
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "status": "active " }),
            ),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "status with trailing space on PATCH must be 422, got {}",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "status"),
            "expected a status error detail, got: {details:?}"
        );
    }

    /// T110/T111: contact email " user@test.com" (with leading space) on
    /// create must be 422 — without trimming, the leading space causes the
    /// email validation to reject it.
    #[tokio::test]
    async fn create_tenant_contact_email_with_leading_space_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": format!("T110 Leading Email {unique}"),
                        "slug": format!("t110-leading-email-{unique}"),
                        "contactEmail": " user@test.com",
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "contact email with leading space on create must be 422, got {}",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "contactEmail"),
            "expected a contactEmail error detail, got: {details:?}"
        );
    }

    // -----------------------------------------------------------------------
    // T101 — Create plan omitted-vs-null semantics
    // -----------------------------------------------------------------------

    /// T101: Explicit `plan: null` on create must be rejected with 422
    /// and a field-level detail — null is not the same as omitted.
    #[tokio::test]
    async fn create_tenant_null_plan_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();

        let mut res = send_request(
            pool.clone(),
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": format!("T101 Null Plan {unique}"),
                        "slug": format!("t101-nullplan-{unique}"),
                        "plan": null,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "explicit null plan on create must be 422"
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "plan"),
            "expected a plan error detail, got: {details:?}"
        );

        // Confirm no tenant was created.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tenants WHERE slug = $1")
            .bind(format!("t101-nullplan-{unique}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "no tenant should be created for null plan");

        // Confirm no audit row was written for the attempted create.
        let audit_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM audit_logs WHERE action = 'platform.tenant_created' AND details->>'slug' = $1",
        )
            .bind(format!("t101-nullplan-{unique}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            audit_count.0, 0,
            "no audit row should be written for null plan"
        );
    }

    /// T058: 201 multibyte chars (well over 200 bytes) MUST be rejected with
    /// 422 — the limit is 200 characters, not 200 bytes.
    #[tokio::test]
    async fn create_tenant_multibyte_name_over_200_chars_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let name: String = "日".repeat(201);
        assert_eq!(name.chars().count(), 201);

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": name,
                        "slug": format!("t058-mb201-{unique}"),
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "201-char multibyte name must be 422, got {}",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "name"),
            "expected a name error detail, got: {details:?}"
        );
    }

    /// T058: 201 multibyte chars on PATCH MUST be rejected with 422.
    #[tokio::test]
    async fn update_tenant_multibyte_name_over_200_chars_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T058 Multibyte Update {unique}"),
            &format!("t058-mb-upd-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let new_name: String = "日".repeat(201);
        assert_eq!(new_name.chars().count(), 201);

        let mut res = send_request(
            pool,
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "name": new_name }),
            ),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "201-char multibyte name on PATCH must be 422, got {}",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "name"),
            "expected a name error detail, got: {details:?}"
        );
    }

    /// T058: A 200-character contact name composed entirely of 3-byte UTF-8
    /// chars MUST be accepted on create.
    #[tokio::test]
    async fn create_tenant_multibyte_contact_name_under_200_chars_accepted() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let contact_name: String = "名".repeat(200);
        assert_eq!(contact_name.chars().count(), 200);
        assert!(
            contact_name.len() > 200,
            "contact_name should be >200 bytes"
        );

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": format!("T058 Contact MB {unique}"),
                        "slug": format!("t058-mb-contact-{unique}"),
                        "contactName": contact_name,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(
            res.status(),
            201,
            "200-char multibyte contactName must be accepted (got {})",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["contactName"].as_str().unwrap().chars().count(), 200);
    }

    /// T058: 201 multibyte contact name chars on create MUST be rejected with 422.
    #[tokio::test]
    async fn create_tenant_multibyte_contact_name_over_200_chars_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let contact_name: String = "名".repeat(201);
        assert_eq!(contact_name.chars().count(), 201);

        let mut res = send_request(
            pool,
            Request::post("/api/v1/platform/tenants")
                .header("X-Dev-User-Id", admin.to_string())
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "name": format!("T058 Contact MB Over {unique}"),
                        "slug": format!("t058-mb-contact-over-{unique}"),
                        "contactName": contact_name,
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "201-char multibyte contactName must be 422, got {}",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "contactName"),
            "expected a contactName error detail, got: {details:?}"
        );
    }

    /// T058: A 200-character contact name on PATCH MUST be accepted.
    #[tokio::test]
    async fn update_tenant_multibyte_contact_name_under_200_chars_accepted() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T058 Contact MB Update {unique}"),
            &format!("t058-mb-contact-upd-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let contact_name: String = "名".repeat(200);
        assert_eq!(contact_name.chars().count(), 200);

        let mut res = send_request(
            pool,
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "contactName": contact_name }),
            ),
        )
        .await;

        assert_eq!(
            res.status(),
            200,
            "200-char multibyte contactName on PATCH must be accepted (got {})",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["contactName"].as_str().unwrap().chars().count(), 200);
    }

    /// T058: 201 multibyte contact name chars on PATCH MUST be rejected with 422.
    #[tokio::test]
    async fn update_tenant_multibyte_contact_name_over_200_chars_returns_422() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let tenant_id = seed_tenant_full(
            &pool,
            &format!("T058 Contact MB Update Over {unique}"),
            &format!("t058-mb-contact-upd-over-{unique}"),
            "trial",
            None,
            None,
        )
        .await;

        let contact_name: String = "名".repeat(201);
        assert_eq!(contact_name.chars().count(), 201);

        let mut res = send_request(
            pool,
            patch_request(
                format!("/api/v1/platform/tenants/{tenant_id}"),
                admin,
                json!({ "contactName": contact_name }),
            ),
        )
        .await;

        assert_eq!(
            res.status(),
            422,
            "201-char multibyte contactName on PATCH must be 422, got {}",
            res.status()
        );
        let body = body_json(&mut res).await;
        assert_eq!(body["error"]["code"], "validation_failed");
        let details = body["error"]["details"].as_array().expect("details array");
        assert!(
            details.iter().any(|d| d["field"] == "contactName"),
            "expected a contactName error detail, got: {details:?}"
        );
    }

    #[tokio::test]
    async fn t074_pagination_traverses_500_tenants_without_gaps() {
        let Some(pool) = get_pool().await else { return };
        db::run_migrations(&pool).await.unwrap();

        let admin = seed_user(&pool, Some("super_admin")).await;
        let unique = Uuid::new_v4().simple().to_string();
        let count = 505usize;

        // Insert 500+ tenants with a unique tag via direct SQL.
        let mut seeded: Vec<Uuid> = Vec::with_capacity(count);
        for i in 0..count {
            let id = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO tenants (name, slug, status) VALUES ($1, $2, 'active') RETURNING id",
            )
            .bind(format!("T074 Tenant {unique} {i}"))
            .bind(format!("t074-{unique}-{i:03}"))
            .fetch_one(&pool)
            .await
            .unwrap();
            seeded.push(id);
        }

        assert!(
            seeded.len() >= 500,
            "must insert at least 500 tenants, got {}",
            seeded.len()
        );

        // Traverse all tenants matching the unique tag, recording timing.
        let start = std::time::Instant::now();
        let mut observed_ids: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
        let mut next_cursor: Option<String> = None;
        let mut page_count = 0usize;
        loop {
            page_count += 1;
            assert!(page_count < 200, "runaway pagination guard");

            let mut url = format!("/api/v1/platform/tenants?q=T074+Tenant+{unique}&limit=25");
            if let Some(ref cursor) = next_cursor {
                url.push_str(&format!("&cursor={cursor}"));
            }

            let mut res = send_request(
                pool.clone(),
                Request::get(url)
                    .header("X-Dev-User-Id", admin.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(res.status(), 200, "expected 200 on page {page_count}");
            let body = body_json(&mut res).await;

            let items = body["items"].as_array().expect("items array");
            for item in items {
                let id_str = item["id"].as_str().expect("id string");
                let id = Uuid::parse_str(id_str).expect("valid uuid");
                assert!(
                    observed_ids.insert(id),
                    "duplicate id {id} on page {page_count}"
                );
            }

            let has_more = body["hasMore"].as_bool().unwrap_or(false);
            if has_more {
                let nc = body["nextCursor"]
                    .as_str()
                    .expect("nextCursor must be present when hasMore is true")
                    .to_owned();
                assert!(!nc.is_empty(), "nextCursor must not be empty");
                next_cursor = Some(nc);
            } else {
                assert!(
                    body["nextCursor"].is_null(),
                    "nextCursor must be null/absent when hasMore is false"
                );
                break;
            }
        }

        let elapsed = start.elapsed();

        // Every seeded tenant is returned exactly once (no gaps, no duplicates).
        assert_eq!(
            observed_ids.len(),
            seeded.len(),
            "must see every seeded tenant exactly once"
        );
        for id in &seeded {
            assert!(
                observed_ids.contains(id),
                "seeded tenant {id} was not found in paginated traversal"
            );
        }

        // Timing: 505 tenants at limit=25 is ~21 pages; well under 5 seconds.
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "pagination traversal took {elapsed:?}, expected < 5s"
        );

        println!(
            "T074: {} tenants traversed in {} pages, elapsed {:?}",
            observed_ids.len(),
            page_count,
            elapsed
        );
    }
}
