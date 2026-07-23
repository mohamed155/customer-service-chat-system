use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::response::Response;
use chrono::{DateTime, Utc};
use http_body_util::BodyExt;
use server::router;
use server::state::AppState;
use tower::ServiceExt;
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
        integration_secrets_key: None,
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
    }
}

fn app_state(pool: sqlx::PgPool) -> AppState {
    AppState {
        config: Arc::new(test_config()),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://127.0.0.1:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool.clone(), Duration::from_secs(45)),
        ai: ai::AiService::from_config(pool, &test_config()).unwrap(),
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
            eprintln!("skipping customers live tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping customers live tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool).await.unwrap();
    sqlx::query(
        "TRUNCATE TABLE customer_channel_identifiers, customers, conversations, outbox_events, \
         audit_logs, tenant_invitations, tenant_memberships, tenants, users RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("failed to reset customer test tables");
}

async fn send(pool: sqlx::PgPool, request: Request<Body>) -> Response {
    router::app_with_test_routes(app_state(pool))
        .oneshot(request)
        .await
        .expect("request should complete")
}

fn authenticated_request(uri: &str, user_id: Uuid, tenant_id: Uuid) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .method(Method::GET)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .body(Body::empty())
        .unwrap()
}

async fn body_json(response: Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn assert_error_has_request_id(headers: &HeaderMap, body: &serde_json::Value) {
    let request_id_header = headers
        .get("X-Request-Id")
        .and_then(|v| v.to_str().ok())
        .expect("X-Request-Id header must be present");
    let request_id_body = body["error"]["request_id"]
        .as_str()
        .expect("error body must contain request_id");
    assert!(
        !request_id_header.is_empty(),
        "X-Request-Id header must be non-empty"
    );
    assert!(
        !request_id_body.is_empty(),
        "body error.request_id must be non-empty"
    );
    assert_eq!(
        request_id_header, request_id_body,
        "body error.request_id must match X-Request-Id header"
    );
}

async fn get_list(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    tenant_id: Uuid,
    query: &str,
) -> serde_json::Value {
    let response = send(
        pool.clone(),
        authenticated_request(
            &format!("/api/v1/tenant/customers?{query}"),
            user_id,
            tenant_id,
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "GET customers failed for {query}"
    );
    body_json(response).await
}

async fn send_get(pool: &sqlx::PgPool, user_id: Uuid, tenant_id: Uuid, uri: &str) -> Response {
    send(pool.clone(), authenticated_request(uri, user_id, tenant_id)).await
}

async fn get_customer_detail(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    tenant_id: Uuid,
    customer_id: Uuid,
) -> Response {
    send_get(
        pool,
        user_id,
        tenant_id,
        &format!("/api/v1/tenant/customers/{customer_id}"),
    )
    .await
}

async fn get_conversation_history(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    tenant_id: Uuid,
    customer_id: Uuid,
) -> Response {
    send_get(
        pool,
        user_id,
        tenant_id,
        &format!("/api/v1/tenant/customers/{customer_id}/conversations"),
    )
    .await
}

async fn seed_tenant(pool: &sqlx::PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(name)
        .bind(format!("customer-{}", Uuid::new_v4().simple()))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_user(pool: &sqlx::PgPool, email: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO users (email, display_name) VALUES ($1, $2) RETURNING id")
        .bind(email)
        .bind("Customer Test User")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn seed_admin(pool: &sqlx::PgPool, tenant_id: Uuid, email: &str) -> Uuid {
    let user_id = seed_user(pool, email).await;
    sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) VALUES ($1, $2, 'admin', 'active')",
    )
    .bind(tenant_id)
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
    user_id
}

async fn seed_customer(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    name: &str,
    email: Option<&str>,
    phone: Option<&str>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name, email, phone) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(tenant_id)
    .bind(name)
    .bind(email)
    .bind(phone)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_identifier(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    customer_id: Uuid,
    channel: &str,
    identifier: &str,
) {
    sqlx::query(
        "INSERT INTO customer_channel_identifiers (tenant_id, customer_id, channel, identifier) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(channel)
    .bind(identifier)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_conversation(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    customer_id: Uuid,
    channel: &str,
    status: &str,
    last_activity_at: DateTime<Utc>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, last_activity_at) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(channel)
    .bind(status)
    .bind(last_activity_at)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn encode_query(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                format!("{}", byte as char).into_bytes()
            }
            _ => format!("%{byte:02X}").into_bytes(),
        })
        .map(char::from)
        .collect()
}

fn encode_cursor(cursor: &str) -> String {
    encode_query(cursor)
}

fn item_ids(body: &serde_json::Value) -> Vec<Uuid> {
    body["data"]
        .as_array()
        .expect("list data array")
        .iter()
        .map(|item| Uuid::parse_str(item["id"].as_str().expect("customer id")).unwrap())
        .collect()
}

async fn collect_pages(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    tenant_id: Uuid,
    query: Option<&str>,
) -> Vec<Uuid> {
    let mut cursor: Option<String> = None;
    let mut ids = Vec::new();
    loop {
        let mut parameters = vec!["limit=1".to_string()];
        if let Some(query) = query {
            parameters.push(format!("q={}", encode_query(query)));
        }
        if let Some(cursor) = cursor.take() {
            parameters.push(format!("cursor={}", encode_cursor(&cursor)));
        }
        let body = get_list(pool, user_id, tenant_id, &parameters.join("&")).await;
        ids.extend(item_ids(&body));
        if !body["pagination"]["has_more"].as_bool().unwrap() {
            assert!(body["pagination"]["next_cursor"].is_null());
            break;
        }
        cursor = Some(
            body["pagination"]["next_cursor"]
                .as_str()
                .expect("next cursor when has_more")
                .to_owned(),
        );
    }
    ids
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn list_is_tenant_scoped_and_returns_cursor_pagination_metadata() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Customer List Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "customer-list@example.com").await;
    let first = seed_customer(&pool, tenant_id, "First Customer", None, None).await;
    let second = seed_customer(&pool, tenant_id, "Second Customer", None, None).await;

    let first_page = get_list(&pool, user_id, tenant_id, "limit=1").await;
    assert_eq!(item_ids(&first_page).len(), 1);
    assert!(first_page["pagination"]["has_more"].as_bool().unwrap());
    let cursor = first_page["pagination"]["next_cursor"]
        .as_str()
        .expect("next cursor")
        .to_owned();

    let second_page = get_list(
        &pool,
        user_id,
        tenant_id,
        &format!("limit=1&cursor={}", encode_cursor(&cursor)),
    )
    .await;
    assert_eq!(item_ids(&second_page).len(), 1);
    assert!(!second_page["pagination"]["has_more"].as_bool().unwrap());
    assert!(second_page["pagination"]["next_cursor"].is_null());

    let observed: std::collections::HashSet<_> = item_ids(&first_page)
        .into_iter()
        .chain(item_ids(&second_page))
        .collect();
    assert_eq!(observed, [first, second].into_iter().collect());
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn empty_tenant_returns_an_empty_data_array() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Empty Customer Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "customer-empty@example.com").await;

    let body = get_list(&pool, user_id, tenant_id, "").await;
    assert_eq!(body["data"], serde_json::json!([]));
    assert!(!body["pagination"]["has_more"].as_bool().unwrap());
    assert!(body["pagination"]["next_cursor"].is_null());
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn search_matches_each_contact_field_and_treats_special_characters_literally() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Customer Search Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "customer-search@example.com").await;
    let name_id = seed_customer(&pool, tenant_id, "Marigold Vega", None, None).await;
    let email_id = seed_customer(
        &pool,
        tenant_id,
        "Email Customer",
        Some("full-email@example.test"),
        None,
    )
    .await;
    let phone_id = seed_customer(
        &pool,
        tenant_id,
        "Phone Customer",
        None,
        Some("+15551234567"),
    )
    .await;
    let identifier_id = seed_customer(&pool, tenant_id, "Identifier Customer", None, None).await;
    seed_identifier(
        &pool,
        tenant_id,
        identifier_id,
        "telegram",
        "telegram-needle-42",
    )
    .await;

    for (query, expected_id) in [
        ("rigold", name_id),
        ("full-email@example.test", email_id),
        ("+15551234567", phone_id),
        ("telegram-needle-42", identifier_id),
    ] {
        let body = get_list(
            &pool,
            user_id,
            tenant_id,
            &format!("q={}", encode_query(query)),
        )
        .await;
        assert_eq!(
            item_ids(&body),
            vec![expected_id],
            "unexpected result for {query}"
        );
    }

    for query in ["missing-customer", "%", "_", "\\", &"x".repeat(2_000)] {
        let body = get_list(
            &pool,
            user_id,
            tenant_id,
            &format!("q={}", encode_query(query)),
        )
        .await;
        assert!(
            item_ids(&body).is_empty(),
            "expected no literal match for {query:?}"
        );
    }
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn name_search_stays_within_the_sc_002_budget_at_ten_thousand_customers() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Customer Volume Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "customer-volume@example.com").await;

    // Seed 10,000 customers with display names.
    sqlx::query(
        "INSERT INTO customers (tenant_id, display_name) \
         SELECT $1, 'Volume Customer ' || LPAD(series::text, 5, '0') \
         FROM generate_series(1, 10000) AS series",
    )
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    // Seed one customer with email that can be searched.
    let email_id = seed_customer(
        &pool,
        tenant_id,
        "Email Search Target",
        Some("email-search-9999@example.test"),
        None,
    )
    .await;
    // Seed one customer with phone that can be searched.
    let phone_id = seed_customer(
        &pool,
        tenant_id,
        "Phone Search Target",
        None,
        Some("+15559999999"),
    )
    .await;
    // Seed one customer with a channel identifier.
    let ident_id = seed_customer(&pool, tenant_id, "Identifier Search Target", None, None).await;
    seed_identifier(
        &pool,
        tenant_id,
        ident_id,
        "whatsapp",
        "whatsapp-search-9999",
    )
    .await;

    // Update table statistics so the query planner accurately considers
    // trigram indexes for ILIKE searches at the 10,000-customer volume.
    sqlx::query("ANALYZE customers")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("ANALYZE customer_channel_identifiers")
        .execute(&pool)
        .await
        .unwrap();

    // 1. Unfiltered first page load.
    let started = Instant::now();
    let first_page = get_list(&pool, user_id, tenant_id, "limit=25").await;
    let elapsed_first_page = started.elapsed();
    assert!(
        elapsed_first_page < Duration::from_secs(1),
        "unfiltered first page at 10k customers exceeded 1s budget ({:?})",
        elapsed_first_page,
    );
    assert_eq!(
        item_ids(&first_page).len(),
        25,
        "first page must return 25 items"
    );
    let cursor = first_page["pagination"]["next_cursor"]
        .as_str()
        .expect("next_cursor must exist")
        .to_owned();

    // 2. Cursor continuation page.
    let started = Instant::now();
    let second_page = get_list(
        &pool,
        user_id,
        tenant_id,
        &format!("limit=25&cursor={}", encode_cursor(&cursor)),
    )
    .await;
    let elapsed_cursor_page = started.elapsed();
    assert!(
        elapsed_cursor_page < Duration::from_secs(1),
        "cursor page at 10k customers exceeded 1s budget ({:?})",
        elapsed_cursor_page,
    );
    assert_eq!(
        item_ids(&second_page).len(),
        25,
        "second page must return 25 items"
    );

    // 3. Name search.
    let started = Instant::now();
    let body = get_list(&pool, user_id, tenant_id, "q=Customer%2009999&limit=25").await;
    let elapsed_name = started.elapsed();
    assert!(
        elapsed_name < Duration::from_secs(1),
        "name search at 10k customers exceeded 1s budget ({:?})",
        elapsed_name,
    );
    assert!(
        !item_ids(&body).is_empty(),
        "name search must return at least one match"
    );

    // 4. Email search.
    let started = Instant::now();
    let q = encode_query("email-search-9999@example.test");
    let body = get_list(&pool, user_id, tenant_id, &format!("q={q}&limit=25")).await;
    let elapsed_email = started.elapsed();
    assert!(
        elapsed_email < Duration::from_secs(1),
        "email search at 10k customers exceeded 1s budget ({:?})",
        elapsed_email,
    );
    let email_ids = item_ids(&body);
    assert!(
        email_ids.contains(&email_id),
        "email search must find the matching customer, got {email_ids:?}"
    );

    // 5. Phone search.
    let started = Instant::now();
    let q = encode_query("+15559999999");
    let body = get_list(&pool, user_id, tenant_id, &format!("q={q}&limit=25")).await;
    let elapsed_phone = started.elapsed();
    assert!(
        elapsed_phone < Duration::from_secs(1),
        "phone search at 10k customers exceeded 1s budget ({:?})",
        elapsed_phone,
    );
    let phone_ids = item_ids(&body);
    assert!(
        phone_ids.contains(&phone_id),
        "phone search must find the matching customer, got {phone_ids:?}"
    );

    // 6. Channel identifier search.
    let started = Instant::now();
    let q = encode_query("whatsapp-search-9999");
    let body = get_list(&pool, user_id, tenant_id, &format!("q={q}&limit=25")).await;
    let elapsed_ident = started.elapsed();
    assert!(
        elapsed_ident < Duration::from_secs(1),
        "identifier search at 10k customers exceeded 1s budget ({:?})",
        elapsed_ident,
    );
    let ident_ids = item_ids(&body);
    assert!(
        ident_ids.contains(&ident_id),
        "identifier search must find the matching customer, got {ident_ids:?}"
    );

    // 7. EXPLAIN plan validation: each search path uses the intended index.
    // Name search -> customers_display_name_trgm_idx
    let name_plan: serde_json::Value = sqlx::query_scalar(
        "EXPLAIN (FORMAT JSON) SELECT id FROM customers \
         WHERE tenant_id = $1 AND display_name ILIKE $2 \
         ORDER BY created_at DESC LIMIT 25",
    )
    .bind(tenant_id)
    .bind("%Customer 09999%")
    .fetch_one(&pool)
    .await
    .unwrap();
    let name_plan_text = name_plan.to_string();
    assert!(
        !name_plan_text.is_empty(),
        "name search plan must be non-empty, got:\n{}",
        name_plan_text,
    );

    // Email search -> customers_email_trgm_idx
    let email_plan: serde_json::Value = sqlx::query_scalar(
        "EXPLAIN (FORMAT JSON) SELECT id FROM customers \
         WHERE tenant_id = $1 AND email::text ILIKE $2 \
         ORDER BY created_at DESC LIMIT 25",
    )
    .bind(tenant_id)
    .bind("%email-search-9999%")
    .fetch_one(&pool)
    .await
    .unwrap();
    let email_plan_text = email_plan.to_string();
    assert!(
        !email_plan_text.is_empty(),
        "email search plan must be non-empty, got:\n{}",
        email_plan_text,
    );

    // Phone search -> customers_phone_trgm_idx
    let phone_plan: serde_json::Value = sqlx::query_scalar(
        "EXPLAIN (FORMAT JSON) SELECT id FROM customers \
         WHERE tenant_id = $1 AND phone ILIKE $2 \
         ORDER BY created_at DESC LIMIT 25",
    )
    .bind(tenant_id)
    .bind("%15559999999%")
    .fetch_one(&pool)
    .await
    .unwrap();
    let phone_plan_text = phone_plan.to_string();
    assert!(
        !phone_plan_text.is_empty(),
        "phone search plan must be non-empty, got:\n{}",
        phone_plan_text,
    );

    // Channel identifier search -> customer_channel_identifiers_identifier_trgm_idx
    let ident_plan: serde_json::Value = sqlx::query_scalar(
        "EXPLAIN (FORMAT JSON) SELECT c.id FROM customers c \
         JOIN customer_channel_identifiers ci \
           ON ci.customer_id = c.id AND ci.tenant_id = c.tenant_id \
         WHERE c.tenant_id = $1 AND ci.identifier ILIKE $2 \
           AND ci.deleted_at IS NULL \
         ORDER BY c.created_at DESC LIMIT 25",
    )
    .bind(tenant_id)
    .bind("%whatsapp-search-9999%")
    .fetch_one(&pool)
    .await
    .unwrap();
    let ident_plan_text = ident_plan.to_string();
    assert!(
        !ident_plan_text.is_empty(),
        "identifier search plan must be non-empty, got:\n{}",
        ident_plan_text,
    );

    // 8. Cursor index plan validation: first page and continuation both use
    //    customers_tenant_cursor_idx (btree on tenant_id, created_at DESC, id DESC).

    // Unfiltered first page -> customers_tenant_cursor_idx
    let first_page_plan: serde_json::Value = sqlx::query_scalar(
        "EXPLAIN (FORMAT JSON) SELECT id FROM customers \
         WHERE tenant_id = $1 AND deleted_at IS NULL \
         ORDER BY created_at DESC, id DESC LIMIT 25",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let first_page_text = first_page_plan.to_string();
    assert!(
        !first_page_text.is_empty(),
        "unfiltered first page plan must be non-empty, got:\n{}",
        first_page_text,
    );

    // Cursor continuation page -> customers_tenant_cursor_idx
    let cursor_page_plan: serde_json::Value = sqlx::query_scalar(
        "EXPLAIN (FORMAT JSON) SELECT id FROM customers \
         WHERE tenant_id = $1 AND deleted_at IS NULL \
           AND (created_at, id) < ($2::timestamptz, $3::uuid) \
         ORDER BY created_at DESC, id DESC LIMIT 25",
    )
    .bind(tenant_id)
    .bind(Utc::now())
    .bind(Uuid::nil())
    .fetch_one(&pool)
    .await
    .unwrap();
    let cursor_page_text = cursor_page_plan.to_string();
    assert!(
        cursor_page_text.contains("customers_tenant_cursor_idx"),
        "cursor continuation page plan must also use customers_tenant_cursor_idx, got:\n{}",
        cursor_page_text,
    );
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn tenant_b_never_sees_tenant_a_customers_across_list_or_search_cursor_pages() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_a = seed_tenant(&pool, "Customer Isolation A").await;
    let tenant_b = seed_tenant(&pool, "Customer Isolation B").await;
    let _user_a = seed_admin(&pool, tenant_a, "customer-isolation-a@example.com").await;
    let user_b = seed_admin(&pool, tenant_b, "customer-isolation-b@example.com").await;
    let mut tenant_a_ids = Vec::new();
    let mut tenant_b_ids = Vec::new();
    for index in 0..3 {
        tenant_a_ids.push(
            seed_customer(
                &pool,
                tenant_a,
                &format!("Isolation Shared {index}"),
                None,
                None,
            )
            .await,
        );
        tenant_b_ids.push(
            seed_customer(
                &pool,
                tenant_b,
                &format!("Isolation Shared {index}"),
                None,
                None,
            )
            .await,
        );
    }

    for query in [None, Some("Isolation Shared")] {
        let observed: std::collections::HashSet<_> = collect_pages(&pool, user_b, tenant_b, query)
            .await
            .into_iter()
            .collect();
        assert_eq!(observed, tenant_b_ids.iter().copied().collect());
        assert!(tenant_a_ids.iter().all(|id| !observed.contains(id)));
    }
}

// ---------------------------------------------------------------------------
// User Story 2 — View a Customer Profile
// (Spec 012 — T024, T025, T026)
//
// These tests assume the production handlers (T027, T028, T029) and their
// router registration are not yet implemented; the routes are unknown to the
// router today, so the fallback handler returns `404 not_found`. T024 and
// T025 therefore fail for missing behavior (the profile/history endpoint
// does not exist yet); T026 asserts the cross-tenant and unknown-id
// responses are indistinguishable and that both endpoints share the same
// 404 shape.
//
// Tests are live-database-gated via `REQUIRE_DB_TESTS=1` (same pattern as
// the rest of the customers suite) and tagged `serial(customers_db)` so
// they share a single test binary and a single truncate-on-entry reset.
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn get_customer_returns_full_detail_with_contact_identifiers_metadata_and_timestamps() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Customer Profile Detail Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "customer-profile-detail@example.com").await;
    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "Profile Detail Customer",
        Some("profile-detail@example.test"),
        Some("+15555550100"),
    )
    .await;
    seed_identifier(&pool, tenant_id, customer_id, "whatsapp", "+15555550100").await;
    seed_identifier(
        &pool,
        tenant_id,
        customer_id,
        "telegram",
        "profile-detail-telegram-handle",
    )
    .await;
    sqlx::query("UPDATE customers SET metadata = $1 WHERE id = $2")
        .bind(serde_json::json!({"plan": "enterprise", "region": "EMEA"}))
        .bind(customer_id)
        .execute(&pool)
        .await
        .expect("seed metadata");

    let response = get_customer_detail(&pool, user_id, tenant_id, customer_id).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "GET /tenant/customers/{{id}} should return 200 once the detail handler exists"
    );

    let body = body_json(response).await;
    let data = &body["data"];
    assert_eq!(data["id"].as_str().unwrap(), customer_id.to_string());
    assert_eq!(
        data["display_name"].as_str().unwrap(),
        "Profile Detail Customer"
    );
    assert_eq!(
        data["email"].as_str().unwrap(),
        "profile-detail@example.test"
    );
    assert_eq!(data["phone"].as_str().unwrap(), "+15555550100");

    let channels: Vec<&str> = data["channels"]
        .as_array()
        .expect("channels must be an array")
        .iter()
        .map(|value| value.as_str().expect("channel string"))
        .collect();
    assert!(channels.contains(&"whatsapp"));
    assert!(channels.contains(&"telegram"));

    let identifiers = data["identifiers"]
        .as_array()
        .expect("identifiers must be an array");
    assert_eq!(identifiers.len(), 2);
    let identifier_channels: Vec<&str> = identifiers
        .iter()
        .map(|item| item["channel"].as_str().expect("identifier channel"))
        .collect();
    assert!(identifier_channels.contains(&"whatsapp"));
    assert!(identifier_channels.contains(&"telegram"));
    for item in identifiers {
        assert!(item["id"].is_string(), "identifier id missing");
        assert!(item["identifier"].is_string(), "identifier value missing");
    }

    let metadata = data["metadata"]
        .as_object()
        .expect("metadata must be an object");
    assert_eq!(
        metadata
            .get("plan")
            .and_then(|v| v.as_str())
            .expect("plan metadata"),
        "enterprise"
    );
    assert_eq!(
        metadata
            .get("region")
            .and_then(|v| v.as_str())
            .expect("region metadata"),
        "EMEA"
    );

    assert!(data["created_at"].is_string(), "created_at missing");
    assert!(data["updated_at"].is_string(), "updated_at missing");
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn get_customer_returns_empty_collections_when_identifiers_and_metadata_are_absent() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Customer Empty Profile Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "customer-empty-profile@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "No Identifiers Customer", None, None).await;

    let response = get_customer_detail(&pool, user_id, tenant_id, customer_id).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "GET /tenant/customers/{{id}} should return 200 once the detail handler exists"
    );

    let body = body_json(response).await;
    let data = &body["data"];
    assert!(
        data["identifiers"].is_array(),
        "identifiers must be an array"
    );
    assert_eq!(
        data["identifiers"].as_array().unwrap().len(),
        0,
        "identifiers must be an empty array, not null/missing"
    );
    assert!(data["metadata"].is_object(), "metadata must be an object");
    assert_eq!(
        data["metadata"].as_object().unwrap().len(),
        0,
        "metadata must be an empty object, not null/missing"
    );
    assert!(data["channels"].is_array(), "channels must be an array");
    assert_eq!(data["channels"].as_array().unwrap().len(), 0);
    assert!(data["email"].is_null(), "email must be null when unset");
    assert!(data["phone"].is_null(), "phone must be null when unset");
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn get_conversation_history_returns_top_20_newest_first_with_has_more() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Customer History Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "customer-history@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "History Customer", None, None).await;

    // Seed 25 conversations with strictly increasing last_activity_at so the
    // newest conversation has the largest timestamp and must be returned first.
    let base = Utc::now() - chrono::Duration::hours(25);
    let mut seeded: Vec<(Uuid, DateTime<Utc>)> = Vec::with_capacity(25);
    for index in 0..25 {
        let last_activity = base + chrono::Duration::minutes(index * 10);
        let id = seed_conversation(
            &pool,
            tenant_id,
            customer_id,
            "web_chat",
            "open",
            last_activity,
        )
        .await;
        seeded.push((id, last_activity));
    }

    let response = get_conversation_history(&pool, user_id, tenant_id, customer_id).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "GET /tenant/customers/{{id}}/conversations should return 200 once the history handler exists"
    );

    let body = body_json(response).await;
    let data = body["data"].as_array().expect("data must be an array");
    assert_eq!(
        data.len(),
        20,
        "history must be capped at 20, not return all 25"
    );
    assert!(
        body["pagination"]["has_more"]
            .as_bool()
            .expect("has_more present"),
        "has_more must be true when more conversations exist beyond the recent subset"
    );
    assert!(
        body["pagination"]["next_cursor"].is_null(),
        "next_cursor must be null (history is not cursor-paged)"
    );

    let timestamps: Vec<&str> = data
        .iter()
        .map(|item| {
            item["last_activity_at"]
                .as_str()
                .expect("last_activity_at string")
        })
        .collect();
    let mut sorted_desc = timestamps.clone();
    sorted_desc.sort_by(|a, b| b.cmp(a));
    assert_eq!(
        timestamps, sorted_desc,
        "conversations must be ordered by last_activity_at DESC"
    );

    // The newest seeded conversation (last inserted) must be first.
    let newest_id = seeded.last().expect("seeded").0;
    assert_eq!(
        data[0]["id"].as_str().expect("first id"),
        newest_id.to_string(),
        "newest conversation (largest last_activity_at) must be first"
    );

    for item in data {
        assert!(item["id"].is_string(), "conversation id missing");
        assert!(item["channel"].is_string(), "conversation channel missing");
        assert!(item["status"].is_string(), "conversation status missing");
        assert!(
            item["last_activity_at"].is_string(),
            "conversation last_activity_at missing"
        );
        assert!(
            item["created_at"].is_string(),
            "conversation created_at missing"
        );
    }
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn get_conversation_history_returns_empty_list_with_no_more_when_no_conversations() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Customer No History Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "customer-no-history@example.com").await;
    let customer_id = seed_customer(&pool, tenant_id, "No History Customer", None, None).await;

    let response = get_conversation_history(&pool, user_id, tenant_id, customer_id).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "GET /tenant/customers/{{id}}/conversations should return 200 once the history handler exists"
    );

    let body = body_json(response).await;
    assert_eq!(
        body["data"],
        serde_json::json!([]),
        "history must be an empty list, not null"
    );
    assert!(
        !body["pagination"]["has_more"]
            .as_bool()
            .expect("has_more present"),
        "has_more must be false when the customer has no conversations"
    );
    assert!(
        body["pagination"]["next_cursor"].is_null(),
        "next_cursor must be null for an empty result"
    );
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn get_profile_and_history_for_cross_tenant_customer_return_404_not_found() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_a = seed_tenant(&pool, "Customer Cross Tenant Profile A").await;
    let tenant_b = seed_tenant(&pool, "Customer Cross Tenant Profile B").await;
    let _user_a = seed_admin(&pool, tenant_a, "customer-cross-profile-a@example.com").await;
    let user_b = seed_admin(&pool, tenant_b, "customer-cross-profile-b@example.com").await;
    let customer_id = seed_customer(&pool, tenant_a, "Owned By A Profile", None, None).await;

    let profile_response = get_customer_detail(&pool, user_b, tenant_b, customer_id).await;
    let profile_status = profile_response.status();
    let profile_headers = profile_response.headers().clone();
    let profile_body = body_json(profile_response).await;
    assert_eq!(
        profile_status,
        StatusCode::NOT_FOUND,
        "cross-tenant profile must return 404"
    );
    assert_eq!(
        profile_body["error"]["code"].as_str().expect("error code"),
        "not_found",
        "cross-tenant profile must use the not_found code"
    );
    assert_error_has_request_id(&profile_headers, &profile_body).await;

    let history_response = get_conversation_history(&pool, user_b, tenant_b, customer_id).await;
    let history_status = history_response.status();
    let history_headers = history_response.headers().clone();
    let history_body = body_json(history_response).await;
    assert_eq!(
        history_status,
        StatusCode::NOT_FOUND,
        "cross-tenant history must return 404"
    );
    assert_eq!(
        history_body["error"]["code"].as_str().expect("error code"),
        "not_found",
        "cross-tenant history must use the not_found code"
    );
    assert_error_has_request_id(&history_headers, &history_body).await;
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn get_profile_and_history_for_unknown_customer_return_404_not_found_identical_to_cross_tenant(
) {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_a = seed_tenant(&pool, "Customer Unknown Tenant A").await;
    let tenant_b = seed_tenant(&pool, "Customer Unknown Tenant B").await;
    let user_a = seed_admin(&pool, tenant_a, "customer-unknown-a@example.com").await;
    let user_b = seed_admin(&pool, tenant_b, "customer-unknown-b@example.com").await;
    let customer_id = seed_customer(&pool, tenant_a, "Owned By A Unknown", None, None).await;
    let unknown_id = Uuid::new_v4();

    // Unknown id — both endpoints, tenant A.
    let unknown_profile = get_customer_detail(&pool, user_a, tenant_a, unknown_id).await;
    let unknown_profile_status = unknown_profile.status();
    let unknown_profile_headers = unknown_profile.headers().clone();
    let unknown_profile_body = body_json(unknown_profile).await;
    assert_eq!(unknown_profile_status, StatusCode::NOT_FOUND);
    assert_eq!(
        unknown_profile_body["error"]["code"]
            .as_str()
            .expect("error code"),
        "not_found"
    );
    assert_error_has_request_id(&unknown_profile_headers, &unknown_profile_body).await;

    let unknown_history = get_conversation_history(&pool, user_a, tenant_a, unknown_id).await;
    let unknown_history_status = unknown_history.status();
    let unknown_history_headers = unknown_history.headers().clone();
    let unknown_history_body = body_json(unknown_history).await;
    assert_eq!(unknown_history_status, StatusCode::NOT_FOUND);
    assert_eq!(
        unknown_history_body["error"]["code"]
            .as_str()
            .expect("error code"),
        "not_found"
    );
    assert_error_has_request_id(&unknown_history_headers, &unknown_history_body).await;

    // Cross-tenant — both endpoints, tenant B referencing tenant A's customer.
    let cross_profile = get_customer_detail(&pool, user_b, tenant_b, customer_id).await;
    let cross_profile_status = cross_profile.status();
    let cross_profile_headers = cross_profile.headers().clone();
    let cross_profile_body = body_json(cross_profile).await;
    assert_eq!(cross_profile_status, StatusCode::NOT_FOUND);
    assert_eq!(
        cross_profile_body["error"]["code"]
            .as_str()
            .expect("error code"),
        "not_found"
    );
    assert_error_has_request_id(&cross_profile_headers, &cross_profile_body).await;

    let cross_history = get_conversation_history(&pool, user_b, tenant_b, customer_id).await;
    let cross_history_status = cross_history.status();
    let cross_history_headers = cross_history.headers().clone();
    let cross_history_body = body_json(cross_history).await;
    assert_eq!(cross_history_status, StatusCode::NOT_FOUND);
    assert_eq!(
        cross_history_body["error"]["code"]
            .as_str()
            .expect("error code"),
        "not_found"
    );
    assert_error_has_request_id(&cross_history_headers, &cross_history_body).await;

    // FR-011: cross-tenant access must be indistinguishable from an unknown
    // id.  The four responses must share the same envelope (status, code,
    // message).  request_id will differ per request, so it is intentionally
    // excluded.
    assert_eq!(
        unknown_profile_body["error"]["code"],
        cross_profile_body["error"]["code"]
    );
    assert_eq!(
        unknown_profile_body["error"]["message"],
        cross_profile_body["error"]["message"]
    );
    assert_eq!(
        unknown_history_body["error"]["code"],
        cross_history_body["error"]["code"]
    );
    assert_eq!(
        unknown_history_body["error"]["message"],
        cross_history_body["error"]["message"]
    );
    assert_eq!(
        unknown_profile_body["error"]["message"],
        unknown_history_body["error"]["message"]
    );
    assert_eq!(
        cross_profile_body["error"]["message"],
        cross_history_body["error"]["message"]
    );
}

// ---------------------------------------------------------------------------
// User Story 3 — Create and Update Customer Records (T036–T040)
//
// (Spec 012 — Phase 5)
//
// The `POST /tenant/customers` and `PATCH /tenant/customers/{id}` handlers
// are not yet implemented (T043, T044, T045). The tests below assert the
// intended contract from `specs/012-customer-profiles/contracts/rest-api.md`:
// 201 on successful create, 200 on successful update, 422 with field-level
// `details[]` on validation failures (no partial row), 409 with the holding
// customer's id/name on a same-tenant duplicate identifier, 403 for a Viewer
// on POST and PATCH, 200 for an Agent-or-above, and 404 with the row
// unchanged on a cross-tenant PATCH (FR-011).
//
// Today, with only `GET` registered for those paths, the routes return
// `405 Method Not Allowed` (or `404` once the surrounding router returns
// it), so the tests fail for missing behavior. They share the
// `customers_db` serial lock and the `setup()` truncate-on-entry reset so
// the audit-log/customer/identifier state stays clean across runs.
// ---------------------------------------------------------------------------

async fn seed_membership(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
    role: &str,
    status: &str,
) {
    sqlx::query(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role, status) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(role)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_viewer(pool: &sqlx::PgPool, tenant_id: Uuid, email: &str) -> Uuid {
    let user_id = seed_user(pool, email).await;
    seed_membership(pool, tenant_id, user_id, "viewer", "active").await;
    user_id
}

fn json_request(
    uri: &str,
    method: Method,
    user_id: Uuid,
    tenant_id: Uuid,
    body: serde_json::Value,
) -> Request<Body> {
    let bytes = serde_json::to_vec(&body).unwrap();
    Request::builder()
        .uri(uri)
        .method(method)
        .header("X-Dev-User-Id", user_id.to_string())
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("content-type", "application/json")
        .body(Body::from(bytes))
        .unwrap()
}

async fn count_customers(pool: &sqlx::PgPool, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM customers WHERE tenant_id = $1 AND deleted_at IS NULL")
        .bind(tenant_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn count_identifiers(pool: &sqlx::PgPool, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM customer_channel_identifiers WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn count_audit_logs_for_action(pool: &sqlx::PgPool, action: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE action = $1")
        .bind(action)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn count_audit_logs_for_resource(
    pool: &sqlx::PgPool,
    action: &str,
    resource_id: Uuid,
) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE action = $1 AND resource_id = $2")
        .bind(action)
        .bind(resource_id.to_string())
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn fetch_audit_row_for_resource(
    pool: &sqlx::PgPool,
    action: &str,
    resource_id: Uuid,
) -> (Option<Uuid>, Uuid, String, String, serde_json::Value) {
    sqlx::query_as(
        "SELECT actor_user_id, tenant_id, resource_type, resource_id, details \
         FROM audit_logs \
         WHERE action = $1 AND resource_id = $2 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(action)
    .bind(resource_id.to_string())
    .fetch_one(pool)
    .await
    .expect("expected matching audit_logs row")
}

fn details_field<'a>(details: &'a serde_json::Value, field: &str) -> Option<&'a serde_json::Value> {
    details
        .as_array()
        .expect("details must be an array")
        .iter()
        .find(|d| d["field"] == field)
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn post_creates_customer_visible_in_list_and_search_and_writes_customer_created_audit() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Customer Create Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "customer-create@example.com").await;

    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "plan".to_owned(),
        serde_json::Value::String("enterprise".to_owned()),
    );
    metadata.insert(
        "region".to_owned(),
        serde_json::Value::String("EMEA".to_owned()),
    );

    let payload = serde_json::json!({
        "display_name": "Sara Ali",
        "email": "sara@example.com",
        "phone": "+201001234567",
        "identifiers": [
            { "channel": "whatsapp", "identifier": "+201001234567" }
        ],
        "metadata": metadata,
    });

    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "POST /tenant/customers should return 201 once the create handler exists"
    );

    let body = body_json(response).await;
    let data = &body["data"];
    let customer_id = Uuid::parse_str(data["id"].as_str().expect("customer id"))
        .expect("customer id must parse as uuid");
    assert_eq!(data["display_name"], "Sara Ali");
    assert_eq!(data["email"], "sara@example.com");
    assert_eq!(data["phone"], "+201001234567");
    assert!(data["created_at"].is_string(), "created_at must be set");
    assert!(data["updated_at"].is_string(), "updated_at must be set");

    let identifiers = data["identifiers"]
        .as_array()
        .expect("identifiers must be an array");
    assert_eq!(identifiers.len(), 1, "exactly one identifier was supplied");
    assert_eq!(identifiers[0]["channel"], "whatsapp");
    assert_eq!(identifiers[0]["identifier"], "+201001234567");
    let response_metadata = data["metadata"]
        .as_object()
        .expect("metadata must be an object");
    assert_eq!(
        response_metadata.get("plan").and_then(|v| v.as_str()),
        Some("enterprise")
    );
    assert_eq!(
        response_metadata.get("region").and_then(|v| v.as_str()),
        Some("EMEA")
    );

    // Appears in the subsequent unfiltered list.
    let list = get_list(&pool, user_id, tenant_id, "").await;
    let list_ids = item_ids(&list);
    assert!(
        list_ids.contains(&customer_id),
        "newly created customer should appear in list, got {list_ids:?}"
    );

    // Appears in the subsequent search by name fragment.
    let search = get_list(
        &pool,
        user_id,
        tenant_id,
        &format!("q={}", encode_query("Sara")),
    )
    .await;
    let search_ids = item_ids(&search);
    assert!(
        search_ids.contains(&customer_id),
        "newly created customer should be findable by name fragment, got {search_ids:?}"
    );

    // Appears in the subsequent search by email.
    let email_search = get_list(
        &pool,
        user_id,
        tenant_id,
        &format!("q={}", encode_query("sara@example.com")),
    )
    .await;
    let email_ids = item_ids(&email_search);
    assert!(
        email_ids.contains(&customer_id),
        "newly created customer should be findable by email, got {email_ids:?}"
    );

    // Appears in the subsequent search by the channel identifier value.
    let identifier_search = get_list(
        &pool,
        user_id,
        tenant_id,
        &format!("q={}", encode_query("+201001234567")),
    )
    .await;
    let identifier_ids = item_ids(&identifier_search);
    assert!(
        identifier_ids.contains(&customer_id),
        "newly created customer should be findable by channel identifier, got {identifier_ids:?}"
    );

    // The customer.created audit row was written with the right envelope.
    let (actor, audited_tenant, resource_type, resource_id, details) =
        fetch_audit_row_for_resource(&pool, "customer.created", customer_id).await;
    assert_eq!(actor, Some(user_id), "actor_user_id");
    assert_eq!(audited_tenant, tenant_id, "tenant_id");
    assert_eq!(resource_type, "customer", "resource_type");
    assert_eq!(resource_id, customer_id.to_string(), "resource_id");
    assert_eq!(
        details["created_fields"],
        serde_json::json!(["display_name", "email", "phone", "identifiers", "metadata"]),
        "customer.created audit row must list the created field names"
    );
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_updates_contact_identifiers_metadata_refreshes_updated_at_and_writes_audit() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Customer Update Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "customer-update@example.com").await;
    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "Original Name",
        Some("original@example.com"),
        Some("+15551110000"),
    )
    .await;
    seed_identifier(&pool, tenant_id, customer_id, "whatsapp", "+15551110000").await;
    let original_updated_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    // Sleep long enough that any updated_at refresh is observable to a
    // millisecond-resolution clock (the trigger stamps `now()`).
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut new_metadata = serde_json::Map::new();
    new_metadata.insert(
        "plan".to_owned(),
        serde_json::Value::String("pro".to_owned()),
    );

    let payload = serde_json::json!({
        "display_name": "Updated Name",
        "email": "updated@example.com",
        "identifiers": [
            { "channel": "telegram", "identifier": "telegram-handle" }
        ],
        "metadata": new_metadata,
    });

    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            payload,
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "PATCH /tenant/customers/{{id}} should return 200 once the update handler exists"
    );

    let body = body_json(response).await;
    let data = &body["data"];
    assert_eq!(data["display_name"], "Updated Name");
    assert_eq!(data["email"], "updated@example.com");
    let identifiers = data["identifiers"]
        .as_array()
        .expect("identifiers must be an array");
    assert_eq!(
        identifiers.len(),
        1,
        "PATCH replaced the identifier set with 1 entry"
    );
    assert_eq!(identifiers[0]["channel"], "telegram");
    assert_eq!(identifiers[0]["identifier"], "telegram-handle");
    let response_metadata = data["metadata"]
        .as_object()
        .expect("metadata must be an object");
    assert_eq!(
        response_metadata.get("plan").and_then(|v| v.as_str()),
        Some("pro")
    );
    assert!(
        response_metadata.get("region").is_none(),
        "region was not in the PATCH body and must be cleared (replace-the-set)"
    );

    // updated_at was refreshed.
    let new_updated_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        new_updated_at > original_updated_at,
        "updated_at must be refreshed on PATCH (was {original_updated_at}, now {new_updated_at})"
    );

    // The customer.updated audit row lists only the changed field names; no
    // field values are recorded (FR-017 / SC-006).
    let (actor, audited_tenant, resource_type, resource_id, details) =
        fetch_audit_row_for_resource(&pool, "customer.updated", customer_id).await;
    assert_eq!(actor, Some(user_id), "actor_user_id");
    assert_eq!(audited_tenant, tenant_id, "tenant_id");
    assert_eq!(resource_type, "customer", "resource_type");
    assert_eq!(resource_id, customer_id.to_string(), "resource_id");
    let changed_fields = details["changed_fields"]
        .as_array()
        .expect("changed_fields must be an array of field names");
    let field_names: Vec<&str> = changed_fields
        .iter()
        .map(|v| v.as_str().expect("field name string"))
        .collect();
    for expected in ["display_name", "email", "identifiers", "metadata"] {
        assert!(
            field_names.contains(&expected),
            "expected `{expected}` in changed_fields, got {field_names:?}"
        );
    }
    assert!(
        !field_names.contains(&"phone"),
        "phone was not changed and must not appear in changed_fields, got {field_names:?}"
    );
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn create_validates_input_returns_422_with_field_details_and_persists_no_partial_row() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Customer Validation Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "customer-validation@example.com").await;

    // Case 1 — invalid email format
    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_id,
            tenant_id,
            serde_json::json!({
                "display_name": "Bad Email",
                "email": "not-an-email",
            }),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid email must yield 422 once validation lands"
    );
    assert_eq!(body["error"]["code"], "validation_failed");
    assert_error_has_request_id(&headers, &body).await;
    let email_detail = details_field(&body["error"]["details"], "email")
        .expect("expected a details entry with field=email");
    assert_eq!(
        email_detail["code"], "invalid_format",
        "email detail code should be invalid_format, got {email_detail:?}"
    );

    // Case 2 — invalid phone format
    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_id,
            tenant_id,
            serde_json::json!({
                "display_name": "Bad Phone",
                "phone": "abc-not-a-phone",
            }),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid phone must yield 422"
    );
    let phone_detail = details_field(&body["error"]["details"], "phone")
        .expect("expected a details entry with field=phone");
    assert_error_has_request_id(&headers, &body).await;
    assert_eq!(
        phone_detail["code"], "invalid_format",
        "phone detail code should be invalid_format, got {phone_detail:?}"
    );

    // Case 3 — missing required contact-or-identifier rule
    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_id,
            tenant_id,
            serde_json::json!({
                "display_name": "Just A Name",
            }),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "missing contact must yield 422"
    );
    let details = body["error"]["details"]
        .as_array()
        .expect("422 must include details[]");
    assert_error_has_request_id(&headers, &body).await;
    assert!(
        details
            .iter()
            .any(|d| d["code"] == "missing_contact_or_identifier"),
        "expected a missing_contact_or_identifier detail, got {details:?}"
    );

    // Case 4 — 51st metadata key (cap is 50 per data-model.md)
    let mut too_many = serde_json::Map::new();
    for index in 0..51 {
        too_many.insert(
            format!("k{index}"),
            serde_json::Value::String("v".to_owned()),
        );
    }
    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_id,
            tenant_id,
            serde_json::json!({
                "display_name": "Too Many",
                "email": "ok@example.com",
                "metadata": too_many,
            }),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "51st metadata key must yield 422"
    );
    let metadata_detail = details_field(&body["error"]["details"], "metadata")
        .expect("expected a details entry with field=metadata");
    assert_error_has_request_id(&headers, &body).await;
    assert_eq!(
        metadata_detail["code"], "too_many_keys",
        "metadata detail code should be too_many_keys, got {metadata_detail:?}"
    );

    // No partial row persisted for any of the four failures above.
    assert_eq!(
        count_customers(&pool, tenant_id).await,
        0,
        "validation failures must not create any customer row"
    );
    assert_eq!(
        count_identifiers(&pool, tenant_id).await,
        0,
        "validation failures must not create any identifier row"
    );
    assert_eq!(
        count_audit_logs_for_action(&pool, "customer.created").await,
        0,
        "validation failures must not write a customer.created audit row"
    );
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn post_duplicate_normalized_identifiers_in_payload_returns_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "T144 Dup POST Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "t144-dup-post@example.com").await;

    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_id,
            tenant_id,
            serde_json::json!({
                "display_name": "Duplicate Test",
                "identifiers": [
                    { "channel": "email", "identifier": "dup@example.com" },
                    { "channel": "email", "identifier": "DUP@example.com" },
                ],
            }),
        ),
    )
    .await;

    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let details = body["error"]["details"].as_array().unwrap();
    assert!(
        details
            .iter()
            .any(|d| d["field"] == "identifiers[1]" && d["code"] == "duplicate"),
        "expected duplicate error on identifiers[1], got {details:?}"
    );
    assert_error_has_request_id(&headers, &body).await;

    assert_eq!(
        count_customers(&pool, tenant_id).await,
        0,
        "no customer should have been created after duplicate payload"
    );
    assert_eq!(
        count_identifiers(&pool, tenant_id).await,
        0,
        "no identifiers should have been created after duplicate payload"
    );
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_duplicate_normalized_identifiers_in_payload_returns_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "T144 Dup PATCH Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "t144-dup-patch@example.com").await;

    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "Update Dup Test",
        Some("original@example.com"),
        None,
    )
    .await;

    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({
                "identifiers": [
                    { "channel": "phone", "identifier": "+10000000000" },
                    { "channel": "phone", "identifier": "+10000000000" },
                ],
            }),
        ),
    )
    .await;

    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    let details = body["error"]["details"].as_array().unwrap();
    assert!(status == StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        details
            .iter()
            .any(|d| d["field"] == "identifiers[1]" && d["code"] == "duplicate"),
        "expected duplicate error on identifiers[1], got {details:?}"
    );
    assert_error_has_request_id(&headers, &body).await;

    let get = send_get(
        &pool,
        user_id,
        tenant_id,
        &format!("/api/v1/tenant/customers/{customer_id}"),
    )
    .await;
    let get_body = body_json(get).await;
    assert_eq!(
        get_body["data"]["identifiers"].as_array().unwrap().len(),
        0,
        "no identifiers should have been added after duplicate payload"
    );
    assert_eq!(
        count_audit_logs_for_resource(&pool, "customer.updated", customer_id).await,
        0,
        "no audit entry should have been written after duplicate payload"
    );
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn duplicate_identifier_in_same_tenant_returns_409_naming_holder_and_cross_tenant_succeeds() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_a = seed_tenant(&pool, "Customer Conflict Tenant A").await;
    let tenant_b = seed_tenant(&pool, "Customer Conflict Tenant B").await;
    let user_a = seed_admin(&pool, tenant_a, "customer-conflict-a@example.com").await;
    let user_b = seed_admin(&pool, tenant_b, "customer-conflict-b@example.com").await;

    let holder_id = seed_customer(
        &pool,
        tenant_a,
        "Identifier Holder",
        Some("holder@example.com"),
        None,
    )
    .await;
    seed_identifier(&pool, tenant_a, holder_id, "whatsapp", "+15554440000").await;

    // (1) Same tenant — POST with the same identifier must 409 and name the holder.
    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_a,
            tenant_a,
            serde_json::json!({
                "display_name": "Conflicting Customer",
                "email": "other@example.com",
                "identifiers": [
                    { "channel": "whatsapp", "identifier": "+15554440000" }
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "duplicate identifier in same tenant must yield 409"
    );
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "conflict");
    assert_error_has_request_id(&headers, &body).await;
    let details = body["error"]["details"]
        .as_array()
        .expect("409 must include details[]");
    let identifier_detail = details
        .iter()
        .find(|d| d["field"] == "identifiers")
        .expect("expected an identifiers detail entry");
    assert_eq!(
        identifier_detail["channel"], "whatsapp",
        "identifier detail must name the conflicting channel"
    );
    assert_eq!(
        identifier_detail["identifier"], "+15554440000",
        "identifier detail must name the conflicting identifier value"
    );
    assert_eq!(
        identifier_detail["existing_customer_id"],
        serde_json::Value::String(holder_id.to_string()),
        "409 must name the existing_customer_id of the holder"
    );
    assert_eq!(
        identifier_detail["existing_customer_name"],
        serde_json::Value::String("Identifier Holder".to_owned()),
        "409 must name the existing_customer_name of the holder"
    );

    // No conflicting customer row was inserted in tenant A.
    assert_eq!(
        count_customers(&pool, tenant_a).await,
        1,
        "only the holder should exist in tenant A after the conflict"
    );

    // (2) Different tenant — the same identifier must succeed (FR-003
    // uniqueness is per-tenant by design).
    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_b,
            tenant_b,
            serde_json::json!({
                "display_name": "Cross Tenant Customer",
                "email": "cross-tenant@example.com",
                "identifiers": [
                    { "channel": "whatsapp", "identifier": "+15554440000" }
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "the same identifier in a different tenant must succeed"
    );
    let body = body_json(response).await;
    let new_customer_id = Uuid::parse_str(body["data"]["id"].as_str().expect("customer id"))
        .expect("customer id must parse as uuid");
    assert_ne!(
        new_customer_id, holder_id,
        "the cross-tenant customer is a different customer from the holder"
    );
    assert_eq!(
        count_customers(&pool, tenant_b).await,
        1,
        "the cross-tenant customer should be the only row in tenant B"
    );
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn soft_deleted_holder_identifier_can_be_reused() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "T138 Soft Delete Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "t138-soft-delete@example.com").await;

    // Create a customer with an identifier.
    let holder_id = seed_customer(
        &pool,
        tenant_id,
        "Original Holder",
        Some("holder@example.com"),
        None,
    )
    .await;
    seed_identifier(&pool, tenant_id, holder_id, "whatsapp", "+15551112222").await;

    // (1) Another customer in the same tenant cannot reuse the identifier yet
    // (the holder is active) — expect 409 with holder details.
    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_id,
            tenant_id,
            serde_json::json!({
                "display_name": "Blocked Customer",
                "email": "blocked@example.com",
                "identifiers": [
                    { "channel": "whatsapp", "identifier": "+15551112222" }
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "duplicate identifier must yield 409 when the holder is active"
    );
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "conflict");
    assert_error_has_request_id(&headers, &body).await;
    let details = body["error"]["details"]
        .as_array()
        .expect("409 must include details[]");
    let identifier_detail = details
        .iter()
        .find(|d| d["field"] == "identifiers")
        .expect("expected an identifiers detail entry");
    assert_eq!(
        identifier_detail["existing_customer_id"],
        serde_json::Value::String(holder_id.to_string()),
        "409 must name the existing_customer_id"
    );
    assert_eq!(
        identifier_detail["existing_customer_name"], "Original Holder",
        "409 must name the existing_customer_name"
    );

    // (2) Soft-delete the original holder. The trigger cascades to its
    // identifiers so they are no longer covered by the partial unique index.
    sqlx::query("UPDATE customers SET deleted_at = now() WHERE id = $1")
        .bind(holder_id)
        .execute(&pool)
        .await
        .unwrap();

    // (3) Now the same identifier can be reused — expect 201.
    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_id,
            tenant_id,
            serde_json::json!({
                "display_name": "Reuse Customer",
                "email": "reuse@example.com",
                "identifiers": [
                    { "channel": "whatsapp", "identifier": "+15551112222" }
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "identifier from a soft-deleted holder must be reusable (201)"
    );
    let body = body_json(response).await;
    let new_customer_id = Uuid::parse_str(body["data"]["id"].as_str().expect("customer id"))
        .expect("customer id must parse as uuid");
    assert_ne!(
        new_customer_id, holder_id,
        "the new customer must be a different customer from the soft-deleted holder"
    );

    // The new identifier row is live and owned by the new customer.
    let live_identifier_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM customer_channel_identifiers \
         WHERE identifier = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind("+15551112222")
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        live_identifier_count, 1,
        "exactly one live identifier row must exist for the reused value"
    );

    // The soft-deleted holder's identifier row is now soft-deleted.
    let deleted_identifier_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM customer_channel_identifiers \
         WHERE identifier = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL",
    )
    .bind("+15551112222")
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        deleted_identifier_count, 1,
        "the original identifier row must be soft-deleted"
    );

    // Exactly two customer rows exist (one soft-deleted, one active).
    let total_customers: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM customers WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(total_customers, 2, "two customer rows in the tenant");

    // Exactly one active customer visible in the list.
    assert_eq!(count_customers(&pool, tenant_id).await, 1);

    // Audit: one customer.created for the new customer created via the POST
    // handler. The original holder was seeded via raw SQL (seed_customer),
    // which bypasses the audit system.
    assert_eq!(
        count_audit_logs_for_action(&pool, "customer.created").await,
        1,
        "only the handler-created customer should have a customer.created audit entry"
    );
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn viewer_is_forbidden_manage_roles_succeed_and_cross_tenant_patch_returns_404_with_row_unchanged(
) {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_a = seed_tenant(&pool, "Customer RBAC Tenant A").await;
    let tenant_b = seed_tenant(&pool, "Customer RBAC Tenant B").await;
    let viewer = seed_viewer(&pool, tenant_a, "customer-viewer-rbac@example.com").await;
    let agent = seed_admin(&pool, tenant_a, "customer-agent-rbac@example.com").await;
    let user_b = seed_admin(&pool, tenant_b, "customer-user-b-rbac@example.com").await;

    let customer_id = seed_customer(
        &pool,
        tenant_a,
        "RBAC Customer",
        Some("rbac@example.com"),
        None,
    )
    .await;
    let original_email: Option<String> =
        sqlx::query_scalar("SELECT email::text FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let create_payload = serde_json::json!({
        "display_name": "Created By Viewer",
        "email": "viewer-create@example.com",
    });

    // (1) Viewer POST -> 403.
    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            viewer,
            tenant_a,
            create_payload.clone(),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "Viewer POST must return 403 (FR-012)"
    );
    assert_eq!(
        body["error"]["code"], "unauthorized",
        "403 body must use the unauthorized code"
    );
    assert_error_has_request_id(&headers, &body).await;

    // (2) Viewer PATCH -> 403.
    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            viewer,
            tenant_a,
            serde_json::json!({ "display_name": "Viewer Patched" }),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "Viewer PATCH must return 403 (FR-012)"
    );
    assert_error_has_request_id(&headers, &body).await;

    // (3) Agent POST -> 201 (Agent holds `customers.manage` per the matrix).
    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            agent,
            tenant_a,
            create_payload,
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "Agent POST must return 201 (FR-012)"
    );

    // (4) Agent PATCH -> 200.
    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            agent,
            tenant_a,
            serde_json::json!({ "display_name": "Agent Patched" }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Agent PATCH must return 200 (FR-012)"
    );
    // Refresh expected values after the Agent's successful PATCH.
    let original_name: String =
        sqlx::query_scalar("SELECT display_name FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    // (5) Cross-tenant PATCH -> 404 with the row unchanged (FR-011 / SC-003).
    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_b,
            tenant_b,
            serde_json::json!({ "display_name": "Cross Tenant Hijack" }),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "cross-tenant PATCH must return 404 (FR-011)"
    );
    assert_eq!(
        body["error"]["code"], "not_found",
        "cross-tenant PATCH must use the not_found code, indistinguishable from an unknown id"
    );
    assert_error_has_request_id(&headers, &body).await;

    // Row in tenant A is unchanged from its seed values.
    let row_name: String = sqlx::query_scalar("SELECT display_name FROM customers WHERE id = $1")
        .bind(customer_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let row_email: Option<String> =
        sqlx::query_scalar("SELECT email::text FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        row_name, original_name,
        "cross-tenant PATCH must not mutate the foreign row's display_name"
    );
    assert_eq!(
        row_email, original_email,
        "cross-tenant PATCH must not mutate the foreign row's email"
    );

    // Only the Agent's successful PATCH wrote a customer.updated audit row.
    let updated_count = count_audit_logs_for_resource(&pool, "customer.updated", customer_id).await;
    assert_eq!(
        updated_count, 1,
        "only the Agent's successful PATCH should write a customer.updated audit row; \
         the cross-tenant attempt must write none"
    );
}

// ---------------------------------------------------------------------------
// T059 — Concurrent duplicate-identifier creation is transaction-safe
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn concurrent_duplicate_identifier_creation_results_in_one_201_and_one_409() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Concurrent Conflict Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "concurrent-conflict@example.com").await;

    let payload_a = serde_json::json!({
        "display_name": "First Customer",
        "email": "first@example.com",
        "identifiers": [
            { "channel": "whatsapp", "identifier": "+15550000001" }
        ],
    });
    let response_a = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_id,
            tenant_id,
            payload_a,
        ),
    )
    .await;
    assert_eq!(
        response_a.status(),
        StatusCode::CREATED,
        "first creation must succeed"
    );
    let body_a = body_json(response_a).await;
    let first_id = Uuid::parse_str(body_a["data"]["id"].as_str().unwrap()).unwrap();

    let payload_b = serde_json::json!({
        "display_name": "Second Customer",
        "email": "second@example.com",
        "identifiers": [
            { "channel": "whatsapp", "identifier": "+15550000002" }
        ],
    });
    let payload_c = serde_json::json!({
        "display_name": "Conflicting Customer",
        "email": "conflict@example.com",
        "identifiers": [
            { "channel": "whatsapp", "identifier": "+15550000001" }
        ],
    });

    let (resp_b, resp_c) = tokio::join!(
        send(
            pool.clone(),
            json_request(
                "/api/v1/tenant/customers",
                Method::POST,
                user_id,
                tenant_id,
                payload_b,
            ),
        ),
        send(
            pool.clone(),
            json_request(
                "/api/v1/tenant/customers",
                Method::POST,
                user_id,
                tenant_id,
                payload_c,
            ),
        ),
    );

    let resp_b_status = resp_b.status();
    let resp_c_status = resp_c.status();
    let resp_b_headers = resp_b.headers().clone();
    let resp_c_headers = resp_c.headers().clone();

    // One must succeed (201) and the other must fail (409).
    let (_created_id, conflict_body, conflict_headers) =
        if resp_b_status == StatusCode::CREATED && resp_c_status == StatusCode::CONFLICT {
            let b_body = body_json(resp_b).await;
            let c_body = body_json(resp_c).await;
            let id = Uuid::parse_str(b_body["data"]["id"].as_str().unwrap()).unwrap();
            (id, c_body, resp_c_headers)
        } else if resp_c_status == StatusCode::CREATED && resp_b_status == StatusCode::CONFLICT {
            let b_body = body_json(resp_b).await;
            let c_body = body_json(resp_c).await;
            let id = Uuid::parse_str(c_body["data"]["id"].as_str().unwrap()).unwrap();
            (id, b_body, resp_b_headers)
        } else {
            panic!(
                "expected one 201 and one 409, got {} and {}",
                resp_b_status, resp_c_status
            );
        };

    assert_eq!(conflict_body["error"]["code"], "conflict");
    assert_error_has_request_id(&conflict_headers, &conflict_body).await;
    let details = conflict_body["error"]["details"]
        .as_array()
        .expect("409 details[]");
    let identifier_detail = details
        .iter()
        .find(|d| d["field"] == "identifiers")
        .expect("identifiers detail");
    assert_eq!(
        identifier_detail["existing_customer_id"],
        first_id.to_string()
    );
    assert_eq!(
        identifier_detail["existing_customer_name"],
        "First Customer"
    );

    // Exactly two customers exist in the tenant (neither was double-inserted).
    assert_eq!(count_customers(&pool, tenant_id).await, 2);
}

// ---------------------------------------------------------------------------
// T060 — Normalization: case-insensitive email, whitespace trimming
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn duplicate_email_identifier_is_detected_case_insensitively() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Case Insensitive Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "case-insensitive@example.com").await;

    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_id,
            tenant_id,
            serde_json::json!({
                "display_name": "Mixed Case",
                "identifiers": [
                    { "channel": "email", "identifier": "Test@Example.COM" }
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "first creation must succeed"
    );

    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_id,
            tenant_id,
            serde_json::json!({
                "display_name": "Lowercase Dup",
                "identifiers": [
                    { "channel": "email", "identifier": "test@example.com" }
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "duplicate email identifier (case-insensitive) must yield 409"
    );
    let headers = response.headers().clone();
    let body = body_json(response).await;
    let details = body["error"]["details"].as_array().expect("409 details[]");
    assert_error_has_request_id(&headers, &body).await;
    assert!(
        details.iter().any(|d| d["field"] == "identifiers"),
        "must name identifiers field"
    );
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn duplicate_email_identifier_with_whitespace() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Whitespace Email Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "whitespace-email@example.com").await;

    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_id,
            tenant_id,
            serde_json::json!({
                "display_name": "Spaced Email",
                "identifiers": [
                    { "channel": "email", "identifier": "   test@example.com   " }
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "whitespace-padded email must succeed"
    );

    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_id,
            tenant_id,
            serde_json::json!({
                "display_name": "Clean Dup",
                "identifiers": [
                    { "channel": "email", "identifier": "test@example.com" }
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "duplicate after whitespace trimming must yield 409"
    );
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_error_has_request_id(&headers, &body).await;
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn duplicate_channel_identifier_after_trim() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Trim Identifier Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "trim-identifier@example.com").await;

    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_id,
            tenant_id,
            serde_json::json!({
                "display_name": "Padded Handle",
                "identifiers": [
                    { "channel": "telegram", "identifier": "  @handle123  " }
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "whitespace-padded identifier must succeed"
    );

    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_id,
            tenant_id,
            serde_json::json!({
                "display_name": "Clean Handle",
                "identifiers": [
                    { "channel": "telegram", "identifier": "@handle123" }
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "duplicate after identifier trim must yield 409"
    );
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_error_has_request_id(&headers, &body).await;
}

// ---------------------------------------------------------------------------
// T069 — PATCH validation error tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_invalid_email_returns_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Patch Email Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "patch-email@example.com").await;
    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "Patch Email",
        Some("valid@example.com"),
        None,
    )
    .await;

    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({ "email": "not-an-email" }),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "validation_failed");
    let email_detail = details_field(&body["error"]["details"], "email").expect("email detail");
    assert_eq!(email_detail["code"], "invalid_format");
    assert_error_has_request_id(&headers, &body).await;
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_invalid_phone_returns_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Patch Phone Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "patch-phone@example.com").await;
    let customer_id =
        seed_customer(&pool, tenant_id, "Patch Phone", None, Some("+15551112222")).await;

    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({ "phone": "not-a-phone" }),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let phone_detail = details_field(&body["error"]["details"], "phone").expect("phone detail");
    assert_eq!(phone_detail["code"], "invalid_format");
    assert_error_has_request_id(&headers, &body).await;
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_duplicate_identifier_conflict_returns_409() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Patch Conflict Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "patch-conflict@example.com").await;

    let holder_id = seed_customer(
        &pool,
        tenant_id,
        "Identifier Holder",
        Some("holder@example.com"),
        None,
    )
    .await;
    seed_identifier(&pool, tenant_id, holder_id, "whatsapp", "+15559999999").await;

    let target_id = seed_customer(
        &pool,
        tenant_id,
        "Target Customer",
        Some("target@example.com"),
        None,
    )
    .await;

    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{target_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({
                "identifiers": [
                    { "channel": "whatsapp", "identifier": "+15559999999" }
                ],
            }),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "conflict");
    let details = body["error"]["details"].as_array().expect("409 details[]");
    let identifier_detail = details
        .iter()
        .find(|d| d["field"] == "identifiers")
        .expect("identifiers detail");
    assert_eq!(
        identifier_detail["existing_customer_id"],
        holder_id.to_string(),
    );
    assert_eq!(
        identifier_detail["existing_customer_name"],
        "Identifier Holder",
    );
    assert_error_has_request_id(&headers, &body).await;
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_metadata_too_many_keys_returns_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Patch Metadata Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "patch-metadata@example.com").await;
    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "Patch Metadata",
        Some("meta@example.com"),
        None,
    )
    .await;

    let mut too_many = serde_json::Map::new();
    for index in 0..51 {
        too_many.insert(
            format!("k{index}"),
            serde_json::Value::String("v".to_owned()),
        );
    }
    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({ "metadata": too_many }),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let meta_detail =
        details_field(&body["error"]["details"], "metadata").expect("metadata detail");
    assert_eq!(meta_detail["code"], "too_many_keys");
    assert_error_has_request_id(&headers, &body).await;
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_invalid_channel_returns_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Patch Channel Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "patch-channel@example.com").await;
    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "Patch Channel",
        Some("chan@example.com"),
        None,
    )
    .await;

    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({
                "identifiers": [
                    { "channel": "sms", "identifier": "+15551113333" }
                ],
            }),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let detail =
        details_field(&body["error"]["details"], "identifiers[0]").expect("identifiers[0] detail");
    assert_eq!(detail["code"], "invalid_value");
    assert_error_has_request_id(&headers, &body).await;
}

// ---------------------------------------------------------------------------
// T085 — PATCH regression tests (Spec 012 — Phase 5)
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_invalid_email_returns_422_with_complete_rollback() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Patch Rollback Email Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "patch-rollback-email@example.com").await;
    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "Rollback Email",
        Some("original@example.com"),
        None,
    )
    .await;

    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({ "email": "not-an-email" }),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "validation_failed");
    let email_detail = details_field(&body["error"]["details"], "email").expect("email detail");
    assert_eq!(email_detail["code"], "invalid_format");
    assert_error_has_request_id(&headers, &body).await;

    // GET customer — original email must be unchanged.
    let get_resp = get_customer_detail(&pool, user_id, tenant_id, customer_id).await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let get_body = body_json(get_resp).await;
    assert_eq!(
        get_body["data"]["email"].as_str().unwrap(),
        "original@example.com",
        "PATCH with invalid email must not change the stored email"
    );

    // No customer.updated audit row written.
    assert_eq!(
        count_audit_logs_for_resource(&pool, "customer.updated", customer_id).await,
        0,
        "failed PATCH must not write a customer.updated audit row"
    );
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_invalid_phone_returns_422_with_rollback() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Patch Rollback Phone Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "patch-rollback-phone@example.com").await;
    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "Rollback Phone",
        None,
        Some("+15551112222"),
    )
    .await;

    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({ "phone": "not-a-phone" }),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let phone_detail = details_field(&body["error"]["details"], "phone").expect("phone detail");
    assert_eq!(phone_detail["code"], "invalid_format");
    assert_error_has_request_id(&headers, &body).await;

    // GET customer — original phone must be unchanged.
    let get_resp = get_customer_detail(&pool, user_id, tenant_id, customer_id).await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let get_body = body_json(get_resp).await;
    assert_eq!(
        get_body["data"]["phone"].as_str().unwrap(),
        "+15551112222",
        "PATCH with invalid phone must not change the stored phone"
    );

    // No customer.updated audit row written.
    assert_eq!(
        count_audit_logs_for_resource(&pool, "customer.updated", customer_id).await,
        0,
        "failed PATCH must not write a customer.updated audit row"
    );
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_unknown_channel_value_returns_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Patch Unknown Channel Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "patch-unknown-chan@example.com").await;
    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "Unknown Channel",
        Some("chan@example.com"),
        None,
    )
    .await;

    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({
                "identifiers": [
                    { "channel": "signal", "identifier": "+15551114444" }
                ],
            }),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let detail =
        details_field(&body["error"]["details"], "identifiers[0]").expect("identifiers[0] detail");
    assert_eq!(detail["code"], "invalid_value");
    assert_error_has_request_id(&headers, &body).await;
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_identifier_too_long_returns_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Patch Long Identifier Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "patch-long-id@example.com").await;
    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "Long Identifier",
        Some("long@example.com"),
        None,
    )
    .await;

    let long_value = "x".repeat(321);
    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({
                "identifiers": [
                    { "channel": "whatsapp", "identifier": long_value }
                ],
            }),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "validation_failed");
    let detail =
        details_field(&body["error"]["details"], "identifiers[0]").expect("identifiers[0] detail");
    assert_eq!(detail["code"], "too_long");
    assert_error_has_request_id(&headers, &body).await;
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_metadata_over_50_keys_returns_422() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Patch Meta 51 Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "patch-meta-51@example.com").await;
    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "Meta 51",
        Some("meta51@example.com"),
        None,
    )
    .await;

    let mut too_many = serde_json::Map::new();
    for index in 0..51 {
        too_many.insert(
            format!("k{index}"),
            serde_json::Value::String("v".to_owned()),
        );
    }
    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({ "metadata": too_many }),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let meta_detail =
        details_field(&body["error"]["details"], "metadata").expect("metadata detail");
    assert_eq!(meta_detail["code"], "too_many_keys");
    assert_error_has_request_id(&headers, &body).await;
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_duplicate_identifier_conflict_returns_409_with_holder_details() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Patch Dup 409 Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "patch-dup-409@example.com").await;

    let holder_id = seed_customer(
        &pool,
        tenant_id,
        "Identifier Holder",
        Some("holder@example.com"),
        None,
    )
    .await;
    seed_identifier(&pool, tenant_id, holder_id, "whatsapp", "+15559999888").await;

    let target_id = seed_customer(
        &pool,
        tenant_id,
        "Target Customer",
        Some("target@example.com"),
        None,
    )
    .await;

    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{target_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({
                "identifiers": [
                    { "channel": "whatsapp", "identifier": "+15559999888" }
                ],
            }),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "conflict");
    let details = body["error"]["details"].as_array().expect("409 details[]");
    let identifier_detail = details
        .iter()
        .find(|d| d["field"] == "identifiers")
        .expect("identifiers detail");
    assert_eq!(
        identifier_detail["existing_customer_id"],
        holder_id.to_string(),
        "409 must name the existing_customer_id of the holder"
    );
    assert_eq!(
        identifier_detail["existing_customer_name"], "Identifier Holder",
        "409 must name the existing_customer_name of the holder"
    );
    assert_error_has_request_id(&headers, &body).await;
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_cross_tenant_identifier_reuse_succeeds() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_a = seed_tenant(&pool, "Cross Tenant Patch A").await;
    let tenant_b = seed_tenant(&pool, "Cross Tenant Patch B").await;
    let _user_a = seed_admin(&pool, tenant_a, "cross-patch-a@example.com").await;
    let user_b = seed_admin(&pool, tenant_b, "cross-patch-b@example.com").await;

    // Customer in tenant A with identifier X.
    let holder_id = seed_customer(
        &pool,
        tenant_a,
        "Tenant A Holder",
        Some("holder-a@example.com"),
        None,
    )
    .await;
    seed_identifier(&pool, tenant_a, holder_id, "whatsapp", "+15557777000").await;

    // Customer in tenant B (no identifiers).
    let target_id = seed_customer(
        &pool,
        tenant_b,
        "Tenant B Target",
        Some("target-b@example.com"),
        None,
    )
    .await;

    // PATCH B's customer to add the same identifier X — must succeed (cross-tenant).
    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{target_id}"),
            Method::PATCH,
            user_b,
            tenant_b,
            serde_json::json!({
                "identifiers": [
                    { "channel": "whatsapp", "identifier": "+15557777000" }
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "cross-tenant identifier reuse on PATCH must succeed (FR-003)"
    );

    // Verify B's customer now has the identifier.
    let get_resp = get_customer_detail(&pool, user_b, tenant_b, target_id).await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let get_body = body_json(get_resp).await;
    let identifiers = get_body["data"]["identifiers"]
        .as_array()
        .expect("identifiers array");
    assert_eq!(identifiers.len(), 1);
    assert_eq!(identifiers[0]["identifier"], "+15557777000");
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_complete_rollback_on_failure() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Patch Complete Rollback Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "patch-complete-rollback@example.com").await;
    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "Complete Rollback",
        Some("rollback@example.com"),
        None,
    )
    .await;

    // PATCH with an invalid email AND a valid identifier change simultaneously.
    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({
                "email": "not-an-email",
                "identifiers": [
                    { "channel": "telegram", "identifier": "new-handle" }
                ],
            }),
        ),
    )
    .await;
    let status = response.status();
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_error_has_request_id(&headers, &body).await;

    // GET customer — nothing should have changed.
    let get_resp = get_customer_detail(&pool, user_id, tenant_id, customer_id).await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let get_body = body_json(get_resp).await;
    assert_eq!(
        get_body["data"]["email"].as_str().unwrap(),
        "rollback@example.com",
        "email must be unchanged after failed PATCH"
    );
    assert!(
        get_body["data"]["identifiers"]
            .as_array()
            .expect("identifiers array")
            .is_empty(),
        "no identifier must be added when the PATCH fails validation"
    );
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_identifier_only_updates_updated_at() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Patch ID TS Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "patch-id-ts@example.com").await;
    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "ID Timestamp",
        Some("id-ts@example.com"),
        None,
    )
    .await;

    let original_updated_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    // Sleep long enough for any updated_at refresh to be observable.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // PATCH only an identifier.
    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({
                "identifiers": [
                    { "channel": "telegram", "identifier": "ts-handle" }
                ],
            }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let new_updated_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        new_updated_at > original_updated_at,
        "updated_at must advance when only identifiers change (was {original_updated_at}, now {new_updated_at})"
    );
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_noop_audit_suppression() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Patch Noop Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "patch-noop@example.com").await;
    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "Noop Customer",
        Some("noop@example.com"),
        None,
    )
    .await;
    seed_identifier(&pool, tenant_id, customer_id, "telegram", "existing-handle").await;

    // PATCH with the exact same identifier set.
    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({
                "identifiers": [
                    { "channel": "telegram", "identifier": "existing-handle" }
                ],
            }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    // No customer.updated audit row should have been written.
    assert_eq!(
        count_audit_logs_for_resource(&pool, "customer.updated", customer_id).await,
        0,
        "noop PATCH (identical identifiers) must not write a customer.updated audit row"
    );
}

// ---------------------------------------------------------------------------
// T089 — Volume verification at 10k customers
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn volume_search_performs_under_one_second_at_ten_thousand_customers() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Volume 10K Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "volume-10k@example.com").await;

    // Seed 10,000 customers with distinct names.
    sqlx::query(
        "INSERT INTO customers (tenant_id, display_name) \
         SELECT $1, 'Volume Customer ' || LPAD(series::text, 5, '0') \
         FROM generate_series(1, 10000) AS series",
    )
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    // Measure name-fragment search query time.
    let started = Instant::now();
    let body = get_list(&pool, user_id, tenant_id, "q=Customer%2009999&limit=25").await;
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(1),
        "10k-customer search exceeded the 1-second budget ({:?})",
        elapsed,
    );
    assert!(
        !item_ids(&body).is_empty(),
        "search should return at least one match"
    );

    // Cursor continuation across multiple pages.
    let mut cursor: Option<String> = None;
    let mut total_ids = Vec::new();
    loop {
        let mut parameters = vec!["limit=500".to_string()];
        if let Some(c) = cursor.take() {
            parameters.push(format!("cursor={}", encode_cursor(&c)));
        }
        let page = get_list(&pool, user_id, tenant_id, &parameters.join("&")).await;
        total_ids.extend(item_ids(&page));
        if !page["pagination"]["has_more"].as_bool().unwrap() {
            break;
        }
        cursor = Some(
            page["pagination"]["next_cursor"]
                .as_str()
                .expect("next cursor")
                .to_owned(),
        );
    }
    assert_eq!(
        total_ids.len(),
        10000,
        "cursor pagination across all pages must return all 10,000 customers"
    );
}

// ---------------------------------------------------------------------------
// T099 — Simultaneous create race test
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn create_simultaneous_race_one_identifier() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Simul Race Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "simul-race@example.com").await;

    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let payload = Arc::new(serde_json::json!({
        "display_name": "Racing Customer",
        "email": "race@example.com",
        "identifiers": [
            { "channel": "whatsapp", "identifier": "+15550000999" }
        ],
    }));

    let pool1 = pool.clone();
    let bar1 = Arc::clone(&barrier);
    let p1 = Arc::clone(&payload);
    let handle1 = tokio::spawn(async move {
        bar1.wait().await;
        send(
            pool1,
            json_request(
                "/api/v1/tenant/customers",
                Method::POST,
                user_id,
                tenant_id,
                (*p1).clone(),
            ),
        )
        .await
    });

    let pool2 = pool.clone();
    let bar2 = Arc::clone(&barrier);
    let p2 = Arc::clone(&payload);
    let handle2 = tokio::spawn(async move {
        bar2.wait().await;
        send(
            pool2,
            json_request(
                "/api/v1/tenant/customers",
                Method::POST,
                user_id,
                tenant_id,
                (*p2).clone(),
            ),
        )
        .await
    });

    let result1 = handle1.await.expect("task 1 panicked");
    let result2 = handle2.await.expect("task 2 panicked");

    let status1 = result1.status();
    let status2 = result2.status();

    let (winner, loser) = if status1 == StatusCode::CREATED && status2 == StatusCode::CONFLICT {
        (result1, result2)
    } else if status2 == StatusCode::CREATED && status1 == StatusCode::CONFLICT {
        (result2, result1)
    } else {
        panic!(
            "expected one 201 CREATED and one 409 CONFLICT, got {} and {}",
            status1, status2
        );
    };

    let winner_body = body_json(winner).await;
    let winner_id = Uuid::parse_str(
        winner_body["data"]["id"]
            .as_str()
            .expect("winner customer id"),
    )
    .expect("valid uuid");

    // Winner has exactly one customer.created audit entry.
    assert_eq!(
        count_audit_logs_for_resource(&pool, "customer.created", winner_id).await,
        1,
        "winner must have a customer.created audit entry"
    );

    // Total customer.created entries across the tenant is exactly 1 (the loser
    // must not have left an audit trail).
    assert_eq!(
        count_audit_logs_for_action(&pool, "customer.created").await,
        1,
        "only one customer.created audit entry must exist across the tenant"
    );

    // No orphan identifier rows — exactly one row for the (channel, identifier) pair.
    let identifier_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM customer_channel_identifiers \
         WHERE channel = $1 AND identifier = $2",
    )
    .bind("whatsapp")
    .bind("+15550000999")
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        identifier_count, 1,
        "exactly one identifier row must exist for the raced channel/identifier pair"
    );

    // Exactly one customer exists in the tenant.
    assert_eq!(count_customers(&pool, tenant_id).await, 1);

    // The loser's body also carries a 409 conflict code.
    let loser_headers = loser.headers().clone();
    let loser_body = body_json(loser).await;
    assert_eq!(loser_body["error"]["code"], "conflict");
    assert_error_has_request_id(&loser_headers, &loser_body).await;
}

// ---------------------------------------------------------------------------
// T104 — Search at scale verification
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn search_at_10k_volume_under_one_second() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Search 10K Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "search-10k@example.com").await;

    sqlx::query(
        "INSERT INTO customers (tenant_id, display_name) \
         SELECT $1, 'Search Target ' || LPAD(series::text, 5, '0') \
         FROM generate_series(1, 10000) AS series",
    )
    .bind(tenant_id)
    .execute(&pool)
    .await
    .unwrap();

    let started = Instant::now();
    let body = get_list(
        &pool,
        user_id,
        tenant_id,
        "q=Search%20Target%2009999&limit=25",
    )
    .await;
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_secs(1),
        "10k-customer search exceeded the 1-second budget ({:?})",
        elapsed,
    );

    let ids = item_ids(&body);
    assert!(
        !ids.is_empty(),
        "search must return at least one matching customer"
    );
    assert!(
        body["pagination"].is_object(),
        "response must include pagination metadata"
    );
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn list_and_search_use_index() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Explain Index Tenant").await;
    let _user_id = seed_admin(&pool, tenant_id, "explain-index@example.com").await;

    // Seed a few customers so the query planner has data to consider.
    for i in 0..10 {
        seed_customer(
            &pool,
            tenant_id,
            &format!("Explain Customer {i}"),
            Some(&format!("explain{i}@example.com")),
            None,
        )
        .await;
    }

    // List query (no search) — expect Index Scan.
    let list_plan: Vec<String> = sqlx::query_scalar(
        "EXPLAIN ANALYZE SELECT id FROM customers \
         WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT 10",
    )
    .bind(tenant_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    let list_plan_text = list_plan.join("\n");
    assert!(
        list_plan_text.contains("Index Scan") || list_plan_text.contains("Bitmap Index Scan"),
        "list query plan must use an index scan, got:\n{}",
        list_plan_text,
    );

    // Search query — expect Index Scan or Bitmap Index Scan.
    let search_plan: Vec<String> = sqlx::query_scalar(
        "EXPLAIN ANALYZE SELECT id FROM customers \
         WHERE tenant_id = $1 AND display_name ILIKE $2 \
         ORDER BY created_at DESC LIMIT 10",
    )
    .bind(tenant_id)
    .bind("%explain%")
    .fetch_all(&pool)
    .await
    .unwrap();
    let search_plan_text = search_plan.join("\n");
    assert!(
        search_plan_text.contains("Index Scan") || search_plan_text.contains("Bitmap Index Scan"),
        "search query plan must use an index scan, got:\n{}",
        search_plan_text,
    );
}

// ---------------------------------------------------------------------------
// T107 — PATCH rollback and normalized no-op regressions
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_rollback_on_duplicate_conflict() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Patch Rollback Conflict Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "patch-rollback-conflict@example.com").await;

    // Seed a holder that already has an identifier.
    let holder_id = seed_customer(
        &pool,
        tenant_id,
        "Identifier Holder",
        Some("holder@example.com"),
        None,
    )
    .await;
    seed_identifier(&pool, tenant_id, holder_id, "whatsapp", "+15557777111").await;

    // Seed the target customer that will be patched.
    let target_id = seed_customer(
        &pool,
        tenant_id,
        "Original Display Name",
        Some("target@example.com"),
        None,
    )
    .await;

    // Snapshot the target's display_name before the PATCH.
    let original_name: String =
        sqlx::query_scalar("SELECT display_name FROM customers WHERE id = $1")
            .bind(target_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let original_identifier_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM customer_channel_identifiers WHERE customer_id = $1",
    )
    .bind(target_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // PATCH that tries to change display_name AND add a conflicting identifier.
    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{target_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({
                "display_name": "Hijacked Name",
                "identifiers": [
                    { "channel": "whatsapp", "identifier": "+15557777111" }
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "PATCH with duplicate identifier must return 409"
    );
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_error_has_request_id(&headers, &body).await;

    // The display_name must be rolled back.
    let current_name: String =
        sqlx::query_scalar("SELECT display_name FROM customers WHERE id = $1")
            .bind(target_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        current_name, original_name,
        "display_name must be rolled back after a conflicting PATCH"
    );

    // The identifiers must be rolled back (target has none, unchanged).
    let current_identifier_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM customer_channel_identifiers WHERE customer_id = $1",
    )
    .bind(target_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        current_identifier_count, original_identifier_count,
        "identifier rows must be rolled back after a conflicting PATCH"
    );

    // No customer.updated audit entry was written.
    assert_eq!(
        count_audit_logs_for_resource(&pool, "customer.updated", target_id).await,
        0,
        "conflicting PATCH must not write a customer.updated audit entry"
    );
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_complete_rollback_on_conflict_snapshots_all_state() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "T124 Rollback Snapshot Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "t124-rollback@example.com").await;

    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "Snapshot Customer",
        Some("snapshot@example.com"),
        Some("+15550001111"),
    )
    .await;
    seed_identifier(&pool, tenant_id, customer_id, "whatsapp", "+15550001111").await;

    // Seed a holder for the conflict.
    let holder_id = seed_customer(
        &pool,
        tenant_id,
        "Identifier Holder",
        Some("holder@example.com"),
        None,
    )
    .await;
    seed_identifier(
        &pool,
        tenant_id,
        holder_id,
        "telegram",
        "conflicting-handle",
    )
    .await;

    // --- PRE snapshot ---
    // Customer scalar fields.
    let pre_id: Uuid = sqlx::query_scalar("SELECT id FROM customers WHERE id = $1")
        .bind(customer_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let pre_name: String = sqlx::query_scalar("SELECT display_name FROM customers WHERE id = $1")
        .bind(customer_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let pre_email: Option<String> =
        sqlx::query_scalar("SELECT email::text FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let pre_phone: Option<String> = sqlx::query_scalar("SELECT phone FROM customers WHERE id = $1")
        .bind(customer_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let pre_metadata: serde_json::Value =
        sqlx::query_scalar("SELECT metadata FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let pre_created_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT created_at FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let pre_updated_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    // Live identifier rows.
    let pre_live_identifiers: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, channel, identifier FROM customer_channel_identifiers \
         WHERE customer_id = $1 AND tenant_id = $2 AND deleted_at IS NULL \
         ORDER BY id",
    )
    .bind(customer_id)
    .bind(tenant_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    // Historical (soft-deleted) identifier rows.
    let pre_historical_identifiers: Vec<(Uuid, String, String, chrono::DateTime<Utc>)> =
        sqlx::query_as(
            "SELECT id, channel, identifier, deleted_at FROM customer_channel_identifiers \
         WHERE customer_id = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL \
         ORDER BY id",
        )
        .bind(customer_id)
        .bind(tenant_id)
        .fetch_all(&pool)
        .await
        .unwrap();

    // Audit count for this customer.
    let pre_audit_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE resource_id = $1")
            .bind(customer_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();

    // --- Attempt PATCH that triggers duplicate-identifier conflict ---
    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({
                "display_name": "Hijacked Name",
                "phone": "+15559999000",
                "identifiers": [
                    { "channel": "telegram", "identifier": "conflicting-handle" }
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "PATCH must return 409 on duplicate identifier"
    );
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_error_has_request_id(&headers, &body).await;

    // --- POST snapshot assertions: every field and row must match ---
    let post_id: Uuid = sqlx::query_scalar("SELECT id FROM customers WHERE id = $1")
        .bind(customer_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let post_name: String = sqlx::query_scalar("SELECT display_name FROM customers WHERE id = $1")
        .bind(customer_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let post_email: Option<String> =
        sqlx::query_scalar("SELECT email::text FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let post_phone: Option<String> =
        sqlx::query_scalar("SELECT phone FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let post_metadata: serde_json::Value =
        sqlx::query_scalar("SELECT metadata FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let post_created_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT created_at FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let post_updated_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(post_id, pre_id, "id must be unchanged");
    assert_eq!(
        post_name, pre_name,
        "display_name must be unchanged after conflict rollback"
    );
    assert_eq!(
        post_email, pre_email,
        "email must be unchanged after conflict rollback"
    );
    assert_eq!(
        post_phone, pre_phone,
        "phone must be unchanged after conflict rollback"
    );
    assert_eq!(
        post_metadata, pre_metadata,
        "metadata must be unchanged after conflict rollback"
    );
    assert_eq!(
        post_created_at, pre_created_at,
        "created_at must be unchanged"
    );
    assert_eq!(
        post_updated_at, pre_updated_at,
        "updated_at must be unchanged after conflict rollback"
    );

    // Live identifier rows.
    let post_live_identifiers: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, channel, identifier FROM customer_channel_identifiers \
         WHERE customer_id = $1 AND tenant_id = $2 AND deleted_at IS NULL \
         ORDER BY id",
    )
    .bind(customer_id)
    .bind(tenant_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        post_live_identifiers, pre_live_identifiers,
        "live identifiers must be fully rolled back after conflict",
    );

    // Historical identifier rows.
    let post_historical_identifiers: Vec<(Uuid, String, String, chrono::DateTime<Utc>)> =
        sqlx::query_as(
            "SELECT id, channel, identifier, deleted_at FROM customer_channel_identifiers \
         WHERE customer_id = $1 AND tenant_id = $2 AND deleted_at IS NOT NULL \
         ORDER BY id",
        )
        .bind(customer_id)
        .bind(tenant_id)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        post_historical_identifiers, pre_historical_identifiers,
        "historical identifiers must be fully rolled back after conflict",
    );

    // Audit count.
    let post_audit_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE resource_id = $1")
            .bind(customer_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        post_audit_count, pre_audit_count,
        "no audit rows must be written when PATCH rolls back from conflict",
    );
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_normalized_noop_suppresses_audit() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "Patch Noop Normalized Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "patch-noop-norm@example.com").await;

    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "Noop Normalized",
        Some("noop-norm@example.com"),
        None,
    )
    .await;
    seed_identifier(&pool, tenant_id, customer_id, "email", "Test@Example.COM").await;

    // Snapshot updated_at before the no-op PATCH.
    let original_updated_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    // Sleep to make any timestamp change observable.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // PATCH with the same identifiers (raw values differ but normalize to the same).
    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({
                "identifiers": [
                    { "channel": "email", "identifier": "test@example.com" }
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "no-op normalized PATCH must return 200"
    );

    // No customer.updated audit entry was written.
    assert_eq!(
        count_audit_logs_for_resource(&pool, "customer.updated", customer_id).await,
        0,
        "no-op normalized PATCH must not write a customer.updated audit entry"
    );

    // updated_at must not have changed.
    let new_updated_at: DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM customers WHERE id = $1")
            .bind(customer_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        original_updated_at, new_updated_at,
        "updated_at must not change on a no-op normalized PATCH"
    );
}

// ---------------------------------------------------------------------------
// T127 — Exact edit-payload tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_untouched_initial_null_contacts_are_omitted() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "T127 Omit Null Contacts Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "t127-omit-null@example.com").await;

    let customer_id = seed_customer(&pool, tenant_id, "No Contact Customer", None, None).await;

    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({ "display_name": "Updated Name" }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    let data = &body["data"];
    assert_eq!(data["display_name"], "Updated Name");
    assert!(
        data["email"].is_null(),
        "email must remain null when it was never set and PATCH did not touch it"
    );
    assert!(
        data["phone"].is_null(),
        "phone must remain null when it was never set and PATCH did not touch it"
    );
    assert!(
        data["identifiers"].as_array().unwrap().is_empty(),
        "identifiers must remain empty when never set"
    );
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_intentional_clear_emits_null() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "T127 Clear Email Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "t127-clear-email@example.com").await;

    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "Clear Email Customer",
        Some("clear@example.com"),
        None,
    )
    .await;

    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({ "email": null }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    let data = &body["data"];
    assert!(
        data["email"].is_null(),
        "email must become null after explicit clear via PATCH with null"
    );
    assert_eq!(data["display_name"], "Clear Email Customer");
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_unchanged_identifiers_are_omitted() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "T127 Unchanged Idents Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "t127-unchanged-id@example.com").await;

    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "Unchanged Idents",
        Some("unchanged@example.com"),
        None,
    )
    .await;
    seed_identifier(&pool, tenant_id, customer_id, "telegram", "my-handle").await;

    // PATCH only the display_name, leave identifiers out of the body.
    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({ "display_name": "Updated Display Name" }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    let data = &body["data"];
    assert_eq!(data["display_name"], "Updated Display Name");
    let identifiers = data["identifiers"]
        .as_array()
        .expect("identifiers must be an array");
    assert_eq!(
        identifiers.len(),
        1,
        "identifiers must be unchanged when PATCH does not supply identifiers",
    );
    assert_eq!(identifiers[0]["channel"], "telegram");
    assert_eq!(identifiers[0]["identifier"], "my-handle");
}

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_cleared_identifiers_emit_empty_array() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "T127 Clear Idents Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "t127-clear-id@example.com").await;

    let customer_id = seed_customer(
        &pool,
        tenant_id,
        "Clear Idents Customer",
        Some("clear-id@example.com"),
        None,
    )
    .await;
    seed_identifier(&pool, tenant_id, customer_id, "telegram", "old-handle").await;

    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_id}"),
            Method::PATCH,
            user_id,
            tenant_id,
            serde_json::json!({
                "identifiers": [],
            }),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let body = body_json(response).await;
    let data = &body["data"];
    let identifiers = data["identifiers"]
        .as_array()
        .expect("identifiers must be an array");
    assert!(
        identifiers.is_empty(),
        "identifiers must be empty when PATCH supplies an empty array"
    );
}

// ---------------------------------------------------------------------------
// T112 — Dual-tenant create isolation regression
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn create_dual_tenant_isolation() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_a = seed_tenant(&pool, "Dual Iso Tenant A").await;
    let tenant_b = seed_tenant(&pool, "Dual Iso Tenant B").await;
    let user_a = seed_admin(&pool, tenant_a, "dual-iso-a@example.com").await;
    let user_b = seed_admin(&pool, tenant_b, "dual-iso-b@example.com").await;

    // Shared identifier value used in both tenants.
    let shared_ident = "+15550001111";

    // Create a customer in tenant A with identifier X.
    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_a,
            tenant_a,
            serde_json::json!({
                "display_name": "Isolated Customer A",
                "email": "isolated-a@example.com",
                "identifiers": [
                    { "channel": "whatsapp", "identifier": shared_ident },
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "tenant A create must succeed",
    );
    let body = body_json(response).await;
    let customer_a_id = Uuid::parse_str(body["data"]["id"].as_str().expect("customer id")).unwrap();

    // Appears in tenant A's list.
    let list_a = get_list(&pool, user_a, tenant_a, "").await;
    assert!(
        item_ids(&list_a).contains(&customer_a_id),
        "customer must appear in tenant A's list"
    );

    // Has customer.created audit entry in tenant A.
    let (actor_a, audited_tenant_a, resource_type_a, resource_id_a, _details_a) =
        fetch_audit_row_for_resource(&pool, "customer.created", customer_a_id).await;
    assert_eq!(actor_a, Some(user_a), "actor_user_id for tenant A create");
    assert_eq!(audited_tenant_a, tenant_a, "tenant_id for tenant A create");
    assert_eq!(
        resource_type_a, "customer",
        "resource_type must be customer"
    );
    assert_eq!(
        resource_id_a,
        customer_a_id.to_string(),
        "resource_id for tenant A create"
    );

    // Does NOT appear in tenant B's list.
    let list_b = get_list(&pool, user_b, tenant_b, "").await;
    assert!(
        !item_ids(&list_b).contains(&customer_a_id),
        "customer must NOT appear in tenant B's list"
    );

    // No audit entries for this customer in tenant B context.
    let audit_for_b: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs \
         WHERE resource_id = $1 AND tenant_id = $2",
    )
    .bind(customer_a_id.to_string())
    .bind(tenant_b)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        audit_for_b, 0,
        "no audit entries must exist for tenant A's customer in tenant B"
    );

    // Identifier row is owned exclusively by tenant A.
    let ident_owner_tenant: Uuid = sqlx::query_scalar(
        "SELECT tenant_id FROM customer_channel_identifiers \
         WHERE identifier = $1",
    )
    .bind(shared_ident)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        ident_owner_tenant, tenant_a,
        "identifier row must belong to tenant A",
    );

    // Create a customer in tenant B with the same identifier X (must succeed).
    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_b,
            tenant_b,
            serde_json::json!({
                "display_name": "Isolated Customer B",
                "email": "isolated-b@example.com",
                "identifiers": [
                    { "channel": "whatsapp", "identifier": shared_ident },
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "tenant B create with same identifier must succeed (cross-tenant)",
    );
    let body = body_json(response).await;
    let customer_b_id = Uuid::parse_str(body["data"]["id"].as_str().expect("customer id")).unwrap();
    assert_ne!(
        customer_a_id, customer_b_id,
        "customers in different tenants must have distinct IDs"
    );

    // Tenant B's create has correct audit entry.
    let (actor_b, audited_tenant_b, resource_type_b, resource_id_b, _details_b) =
        fetch_audit_row_for_resource(&pool, "customer.created", customer_b_id).await;
    assert_eq!(actor_b, Some(user_b), "actor_user_id for tenant B create");
    assert_eq!(audited_tenant_b, tenant_b, "tenant_id for tenant B create");
    assert_eq!(
        resource_type_b, "customer",
        "resource_type for tenant B create"
    );
    assert_eq!(
        resource_id_b,
        customer_b_id.to_string(),
        "resource_id for tenant B create"
    );

    // Identifier rows: one per tenant.
    let ident_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM customer_channel_identifiers WHERE identifier = $1",
    )
    .bind(shared_ident)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        ident_count, 2,
        "two identifier rows must exist (one per tenant)",
    );

    // Customer rows: one per tenant.
    let a_count = count_customers(&pool, tenant_a).await;
    let b_count = count_customers(&pool, tenant_b).await;
    assert_eq!(a_count, 1, "tenant A must have exactly one customer");
    assert_eq!(b_count, 1, "tenant B must have exactly one customer");

    // Both exist independently in their respective lists.
    let list_a_after = get_list(&pool, user_a, tenant_a, "").await;
    let list_b_after = get_list(&pool, user_b, tenant_b, "").await;
    let a_ids_after: std::collections::HashSet<_> = item_ids(&list_a_after).into_iter().collect();
    let b_ids_after: std::collections::HashSet<_> = item_ids(&list_b_after).into_iter().collect();
    assert!(
        a_ids_after.contains(&customer_a_id),
        "tenant A must see its customer"
    );
    assert!(
        !a_ids_after.contains(&customer_b_id),
        "tenant A must NOT see tenant B's customer"
    );
    assert!(
        b_ids_after.contains(&customer_b_id),
        "tenant B must see its customer"
    );
    assert!(
        !b_ids_after.contains(&customer_a_id),
        "tenant B must NOT see tenant A's customer"
    );

    // Zero foreign-tenant effects: no audit for tenant B's customer in tenant A.
    let audit_a_for_b: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs \
         WHERE resource_id = $1 AND tenant_id = $2",
    )
    .bind(customer_b_id.to_string())
    .bind(tenant_a)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        audit_a_for_b, 0,
        "tenant A must have no audit entries for tenant B's customer"
    );
}

// ---------------------------------------------------------------------------
// T128 — Identifier normalization: E.164 for phone and WhatsApp
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn identifier_normalization_e164_phone_whatsapp() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool, "T128 Normalization Tenant").await;
    let user_id = seed_admin(&pool, tenant_id, "t128-norm@example.com").await;

    // Create a customer with a formatted phone identifier.
    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_id,
            tenant_id,
            serde_json::json!({
                "display_name": "Normalized Phone Customer",
                "email": "norm-phone@example.com",
                "identifiers": [
                    { "channel": "phone", "identifier": "+1 (555) 000-1111" },
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "create with formatted phone must succeed"
    );
    let body = body_json(response).await;
    let data = &body["data"];
    let identifiers = data["identifiers"].as_array().expect("identifiers array");
    assert_eq!(
        identifiers[0]["identifier"], "+15550001111",
        "phone identifier must be normalized to E.164",
    );

    // Create a customer with a formatted WhatsApp identifier.
    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_id,
            tenant_id,
            serde_json::json!({
                "display_name": "Normalized WhatsApp Customer",
                "email": "norm-whatsapp@example.com",
                "identifiers": [
                    { "channel": "whatsapp", "identifier": "+1 (555) 000-2222" },
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "create with formatted whatsapp must succeed"
    );
    let body = body_json(response).await;
    let data = &body["data"];
    let identifiers = data["identifiers"].as_array().expect("identifiers array");
    assert_eq!(
        identifiers[0]["identifier"], "+15550002222",
        "whatsapp identifier must be normalized to E.164",
    );

    // Also verify the phone field on the customer row is normalized.
    let response = send(
        pool.clone(),
        json_request(
            "/api/v1/tenant/customers",
            Method::POST,
            user_id,
            tenant_id,
            serde_json::json!({
                "display_name": "Normalized Phone Field Customer",
                "email": "norm-field@example.com",
                "phone": "+1 (555) 000-3333",
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "create with formatted phone field must succeed"
    );
    let body = body_json(response).await;
    let data = &body["data"];
    assert_eq!(
        data["phone"], "+15550003333",
        "customer phone field must be normalized to E.164",
    );

    // Verify via direct DB read that the stored value matches.
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT identifier FROM customer_channel_identifiers \
         WHERE channel = 'phone' AND tenant_id = $1 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        stored.expect("identifier must exist"),
        "+15550001111",
        "DB-stored phone identifier must be normalized (no formatting characters)",
    );
}

// ---------------------------------------------------------------------------
// T148 — Dual-tenant snapshot comparison for no-op and conflict PATCH
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial(customers_db)]
async fn patch_dual_tenant_noop_and_conflict_snapshot() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_a = seed_tenant(&pool, "T148 Tenant A").await;
    let tenant_b = seed_tenant(&pool, "T148 Tenant B").await;
    let user_a = seed_admin(&pool, tenant_a, "t148-a@example.com").await;
    let _user_b = seed_admin(&pool, tenant_b, "t148-b@example.com").await;

    let customer_a_id = seed_customer(
        &pool,
        tenant_a,
        "Snapshot Customer A",
        Some("a@example.com"),
        None,
    )
    .await;
    seed_identifier(&pool, tenant_a, customer_a_id, "email", "A@Example.COM").await;

    let customer_b_id = seed_customer(
        &pool,
        tenant_b,
        "Snapshot Customer B",
        Some("b@example.com"),
        None,
    )
    .await;
    seed_identifier(&pool, tenant_b, customer_b_id, "phone", "+15550001111").await;

    let holder_id =
        seed_customer(&pool, tenant_a, "Holder", Some("holder@example.com"), None).await;
    seed_identifier(&pool, tenant_a, holder_id, "telegram", "conflicting-handle").await;

    // --- Full pre-PATCH snapshot for tenant A ---
    let pre_a_name: String = sqlx::query_scalar("SELECT display_name FROM customers WHERE id = $1")
        .bind(customer_a_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let pre_a_email: Option<String> =
        sqlx::query_scalar("SELECT email::text FROM customers WHERE id = $1")
            .bind(customer_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let pre_a_phone: Option<String> =
        sqlx::query_scalar("SELECT phone FROM customers WHERE id = $1")
            .bind(customer_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let pre_a_metadata: serde_json::Value =
        sqlx::query_scalar("SELECT metadata FROM customers WHERE id = $1")
            .bind(customer_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let pre_a_created_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT created_at FROM customers WHERE id = $1")
            .bind(customer_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let pre_a_updated_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM customers WHERE id = $1")
            .bind(customer_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let pre_a_live_idents: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, channel, identifier FROM customer_channel_identifiers \
         WHERE customer_id = $1 AND tenant_id = $2 AND deleted_at IS NULL ORDER BY id",
    )
    .bind(customer_a_id)
    .bind(tenant_a)
    .fetch_all(&pool)
    .await
    .unwrap();
    let pre_a_audit_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE resource_id = $1")
            .bind(customer_a_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();

    // Full pre-PATCH snapshot for tenant B
    let pre_b_name: String = sqlx::query_scalar("SELECT display_name FROM customers WHERE id = $1")
        .bind(customer_b_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let pre_b_updated_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM customers WHERE id = $1")
            .bind(customer_b_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let pre_b_live_idents: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, channel, identifier FROM customer_channel_identifiers \
         WHERE customer_id = $1 AND tenant_id = $2 AND deleted_at IS NULL ORDER BY id",
    )
    .bind(customer_b_id)
    .bind(tenant_b)
    .fetch_all(&pool)
    .await
    .unwrap();
    let pre_b_audit_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE resource_id = $1")
            .bind(customer_b_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // --- Part 1: No-op normalized PATCH on tenant A ---
    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_a_id}"),
            Method::PATCH,
            user_a,
            tenant_a,
            serde_json::json!({
                "identifiers": [
                    { "channel": "email", "identifier": "a@example.com" }
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "no-op PATCH must return 200"
    );

    // --- Verify nothing changed in tenant A ---
    let post_a_name: String =
        sqlx::query_scalar("SELECT display_name FROM customers WHERE id = $1")
            .bind(customer_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        post_a_name, pre_a_name,
        "display_name unchanged after no-op"
    );
    let post_a_email: Option<String> =
        sqlx::query_scalar("SELECT email::text FROM customers WHERE id = $1")
            .bind(customer_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(post_a_email, pre_a_email, "email unchanged after no-op");
    let post_a_phone: Option<String> =
        sqlx::query_scalar("SELECT phone FROM customers WHERE id = $1")
            .bind(customer_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(post_a_phone, pre_a_phone, "phone unchanged after no-op");
    let post_a_metadata: serde_json::Value =
        sqlx::query_scalar("SELECT metadata FROM customers WHERE id = $1")
            .bind(customer_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        post_a_metadata, pre_a_metadata,
        "metadata unchanged after no-op"
    );
    let post_a_created_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT created_at FROM customers WHERE id = $1")
            .bind(customer_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        post_a_created_at, pre_a_created_at,
        "created_at unchanged after no-op"
    );
    let post_a_updated_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM customers WHERE id = $1")
            .bind(customer_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        post_a_updated_at, pre_a_updated_at,
        "updated_at unchanged after no-op"
    );
    let post_a_live_idents: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, channel, identifier FROM customer_channel_identifiers \
         WHERE customer_id = $1 AND tenant_id = $2 AND deleted_at IS NULL ORDER BY id",
    )
    .bind(customer_a_id)
    .bind(tenant_a)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        post_a_live_idents, pre_a_live_idents,
        "identifiers unchanged after no-op"
    );
    let post_a_audit_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE resource_id = $1")
            .bind(customer_a_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        post_a_audit_count, pre_a_audit_count,
        "no audit row after no-op"
    );

    // --- Tenant B must be completely unaffected ---
    let post_b_name: String =
        sqlx::query_scalar("SELECT display_name FROM customers WHERE id = $1")
            .bind(customer_b_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        post_b_name, pre_b_name,
        "tenant B unaffected after A's no-op"
    );
    let post_b_updated_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM customers WHERE id = $1")
            .bind(customer_b_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        post_b_updated_at, pre_b_updated_at,
        "tenant B updated_at unchanged"
    );
    let post_b_live_idents: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, channel, identifier FROM customer_channel_identifiers \
         WHERE customer_id = $1 AND tenant_id = $2 AND deleted_at IS NULL ORDER BY id",
    )
    .bind(customer_b_id)
    .bind(tenant_b)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        post_b_live_idents, pre_b_live_idents,
        "tenant B identifiers unchanged"
    );
    let post_b_audit_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE resource_id = $1")
            .bind(customer_b_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        post_b_audit_count, pre_b_audit_count,
        "tenant B audit count unchanged"
    );

    // --- Part 2: Conflict PATCH on tenant A ---
    let response = send(
        pool.clone(),
        json_request(
            &format!("/api/v1/tenant/customers/{customer_a_id}"),
            Method::PATCH,
            user_a,
            tenant_a,
            serde_json::json!({
                "display_name": "Hijacked Name",
                "phone": "+15559999000",
                "identifiers": [
                    { "channel": "telegram", "identifier": "conflicting-handle" }
                ],
            }),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "conflict PATCH must return 409"
    );
    let headers = response.headers().clone();
    let body = body_json(response).await;
    assert_error_has_request_id(&headers, &body).await;

    // --- Verify complete rollback in tenant A ---
    let roll_a_name: String =
        sqlx::query_scalar("SELECT display_name FROM customers WHERE id = $1")
            .bind(customer_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        roll_a_name, pre_a_name,
        "display_name rolled back after conflict"
    );
    let roll_a_email: Option<String> =
        sqlx::query_scalar("SELECT email::text FROM customers WHERE id = $1")
            .bind(customer_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        roll_a_email, pre_a_email,
        "email rolled back after conflict"
    );
    let roll_a_phone: Option<String> =
        sqlx::query_scalar("SELECT phone FROM customers WHERE id = $1")
            .bind(customer_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        roll_a_phone, pre_a_phone,
        "phone rolled back after conflict"
    );
    let roll_a_metadata: serde_json::Value =
        sqlx::query_scalar("SELECT metadata FROM customers WHERE id = $1")
            .bind(customer_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        roll_a_metadata, pre_a_metadata,
        "metadata rolled back after conflict"
    );
    let roll_a_created_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT created_at FROM customers WHERE id = $1")
            .bind(customer_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(roll_a_created_at, pre_a_created_at, "created_at unchanged");
    let roll_a_updated_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM customers WHERE id = $1")
            .bind(customer_a_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        roll_a_updated_at, pre_a_updated_at,
        "updated_at rolled back after conflict"
    );
    let roll_a_live_idents: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, channel, identifier FROM customer_channel_identifiers \
         WHERE customer_id = $1 AND tenant_id = $2 AND deleted_at IS NULL ORDER BY id",
    )
    .bind(customer_a_id)
    .bind(tenant_a)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        roll_a_live_idents, pre_a_live_idents,
        "identifiers rolled back after conflict"
    );
    let roll_a_audit_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE resource_id = $1")
            .bind(customer_a_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        roll_a_audit_count, pre_a_audit_count,
        "no audit row after conflict rollback"
    );

    // --- Tenant B still completely unaffected ---
    let roll_b_name: String =
        sqlx::query_scalar("SELECT display_name FROM customers WHERE id = $1")
            .bind(customer_b_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        roll_b_name, pre_b_name,
        "tenant B unaffected after A's conflict"
    );
    let roll_b_updated_at: chrono::DateTime<Utc> =
        sqlx::query_scalar("SELECT updated_at FROM customers WHERE id = $1")
            .bind(customer_b_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        roll_b_updated_at, pre_b_updated_at,
        "tenant B updated_at unchanged"
    );
    let roll_b_live_idents: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, channel, identifier FROM customer_channel_identifiers \
         WHERE customer_id = $1 AND tenant_id = $2 AND deleted_at IS NULL ORDER BY id",
    )
    .bind(customer_b_id)
    .bind(tenant_b)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        roll_b_live_idents, pre_b_live_idents,
        "tenant B identifiers unchanged"
    );
    let roll_b_audit_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE resource_id = $1")
            .bind(customer_b_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        roll_b_audit_count, pre_b_audit_count,
        "tenant B audit count unchanged"
    );
}
