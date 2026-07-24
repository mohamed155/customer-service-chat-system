use std::time::Duration;

use chrono::{Datelike, TimeZone, Utc};
use serial_test::serial;
use uuid::Uuid;

fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("skipping billing enforcement tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping billing enforcement tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

async fn setup(pool: &sqlx::PgPool) {
    db::run_migrations(pool)
        .await
        .expect("run_migrations should succeed");
    sqlx::query(
        "TRUNCATE TABLE \
         ai_usage_records, tenant_subscriptions, conversations, \
         customers, tenants \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("truncate test tables");
}

async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    let slug = format!("billing-enf-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Billing Enforcement Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn create_plan(
    pool: &sqlx::PgPool,
    name: &str,
    token_allowance: Option<i64>,
    conversation_allowance: Option<i32>,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO billing_plans \
         (name, monthly_price_cents, token_allowance, conversation_allowance, is_default, status) \
         VALUES ($1, 0, $2, $3, false, 'active') RETURNING id",
    )
    .bind(name)
    .bind(token_allowance)
    .bind(conversation_allowance)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn subscribe(pool: &sqlx::PgPool, tenant_id: Uuid, plan_id: Uuid, status: &str) {
    sqlx::query(
        "INSERT INTO tenant_subscriptions (tenant_id, plan_id, status) VALUES ($1, $2, $3)",
    )
    .bind(tenant_id)
    .bind(plan_id)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_tokens(pool: &sqlx::PgPool, tenant_id: Uuid, input: i32, output: i32, at: chrono::DateTime<Utc>) {
    sqlx::query(
        "INSERT INTO ai_usage_records \
         (tenant_id, provider, model, input_tokens, output_tokens, status, streamed, latency_ms, created_at) \
         VALUES ($1, 'openai', 'gpt-4', $2, $3, 'success', false, 100, $4)",
    )
    .bind(tenant_id)
    .bind(input)
    .bind(output)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_customer(pool: &sqlx::PgPool, tenant_id: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Enf Test Customer")
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_conversation(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    customer_id: Uuid,
    at: chrono::DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO conversations \
         (tenant_id, customer_id, channel, status, created_at, last_activity_at, updated_at) \
         VALUES ($1, $2, 'web_chat', 'open', $3, $3, $3)",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn unlimited_plan_with_heavy_usage_allows() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let plan_id = create_plan(&pool, "Unlimited", None, None).await;
    subscribe(&pool, tenant_id, plan_id, "active").await;

    let now = Utc::now();
    insert_tokens(&pool, tenant_id, 500_000, 499_999, now).await;
    let customer_id = seed_customer(&pool, tenant_id).await;
    for _ in 0..500 {
        insert_conversation(&pool, tenant_id, customer_id, now).await;
    }

    let decision = billing::enforcement::check(&pool, tenant_id)
        .await
        .unwrap();
    assert!(decision.allowed);
    assert_eq!(decision.reason, None);
}

#[tokio::test]
#[serial]
async fn usage_just_under_token_limit_allows() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let plan_id = create_plan(&pool, "Token Limit 1000", Some(1000), Some(1000)).await;
    subscribe(&pool, tenant_id, plan_id, "active").await;

    let now = Utc::now();
    insert_tokens(&pool, tenant_id, 500, 499, now).await;

    let decision = billing::enforcement::check(&pool, tenant_id)
        .await
        .unwrap();
    assert!(decision.allowed);
    assert_eq!(decision.reason, None);
}

#[tokio::test]
#[serial]
async fn usage_exactly_at_token_limit_pauses_token_limit() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let plan_id = create_plan(&pool, "Token Limit 1000", Some(1000), Some(1000)).await;
    subscribe(&pool, tenant_id, plan_id, "active").await;

    let now = Utc::now();
    insert_tokens(&pool, tenant_id, 1000, 0, now).await;

    let decision = billing::enforcement::check(&pool, tenant_id)
        .await
        .unwrap();
    assert!(!decision.allowed);
    assert_eq!(
        decision.reason,
        Some(billing::enforcement::PauseReason::TokenLimit)
    );
}

#[tokio::test]
#[serial]
async fn usage_over_limit_pauses() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let plan_id = create_plan(&pool, "Token Limit 1000", Some(1000), Some(1000)).await;
    subscribe(&pool, tenant_id, plan_id, "active").await;

    let now = Utc::now();
    insert_tokens(&pool, tenant_id, 800, 700, now).await;

    let decision = billing::enforcement::check(&pool, tenant_id)
        .await
        .unwrap();
    assert!(!decision.allowed);
    assert_eq!(
        decision.reason,
        Some(billing::enforcement::PauseReason::TokenLimit)
    );
}

#[tokio::test]
#[serial]
async fn conversations_at_limit_pauses() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let plan_id = create_plan(&pool, "Conv Limit 3", Some(10_000), Some(3)).await;
    subscribe(&pool, tenant_id, plan_id, "active").await;

    let now = Utc::now();
    let customer_id = seed_customer(&pool, tenant_id).await;
    for _ in 0..3 {
        insert_conversation(&pool, tenant_id, customer_id, now).await;
    }

    let decision = billing::enforcement::check(&pool, tenant_id)
        .await
        .unwrap();
    assert!(!decision.allowed);
    assert_eq!(
        decision.reason,
        Some(billing::enforcement::PauseReason::ConversationLimit)
    );
}

#[tokio::test]
#[serial]
async fn past_due_subscription_pauses_regardless_of_usage() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let plan_id = create_plan(&pool, "Any Plan", Some(1_000_000), Some(10_000)).await;
    subscribe(&pool, tenant_id, plan_id, "past_due").await;

    let decision = billing::enforcement::check(&pool, tenant_id)
        .await
        .unwrap();
    assert!(!decision.allowed);
    assert_eq!(
        decision.reason,
        Some(billing::enforcement::PauseReason::SubscriptionStatus)
    );
}

#[tokio::test]
#[serial]
async fn canceled_subscription_pauses_regardless_of_usage() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let plan_id = create_plan(&pool, "Any Plan", Some(1_000_000), Some(10_000)).await;
    subscribe(&pool, tenant_id, plan_id, "canceled").await;

    let decision = billing::enforcement::check(&pool, tenant_id)
        .await
        .unwrap();
    assert!(!decision.allowed);
    assert_eq!(
        decision.reason,
        Some(billing::enforcement::PauseReason::SubscriptionStatus)
    );
}

#[tokio::test]
#[serial]
async fn no_subscription_falls_back_to_free_plan() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    // No subscription — effective_plan falls back to Free plan (100000 tokens, 100 conversations)

    let now = Utc::now();
    insert_tokens(&pool, tenant_id, 50_000, 0, now).await;
    let customer_id = seed_customer(&pool, tenant_id).await;
    for _ in 0..50 {
        insert_conversation(&pool, tenant_id, customer_id, now).await;
    }

    let decision = billing::enforcement::check(&pool, tenant_id)
        .await
        .unwrap();
    assert!(decision.allowed);
    assert_eq!(decision.reason, None);
}

#[tokio::test]
#[serial]
async fn previous_month_usage_does_not_count() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;
    let plan_id = create_plan(&pool, "Low Token Limit", Some(100), Some(1000)).await;
    subscribe(&pool, tenant_id, plan_id, "active").await;

    let now = Utc::now();
    let prev_month = if now.month() == 1 {
        Utc.with_ymd_and_hms(now.year() - 1, 12, 15, 0, 0, 0).unwrap()
    } else {
        Utc.with_ymd_and_hms(now.year(), now.month() - 1, 15, 0, 0, 0).unwrap()
    };
    // Insert heavy usage in previous month — should not count
    insert_tokens(&pool, tenant_id, 500, 0, prev_month).await;

    let decision = billing::enforcement::check(&pool, tenant_id)
        .await
        .unwrap();
    assert!(decision.allowed);
    assert_eq!(decision.reason, None);
}

#[tokio::test]
#[serial]
async fn raising_plan_allowance_above_usage_flips_decision() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;

    let plan_a = create_plan(&pool, "Low Allowance A", Some(100), Some(1000)).await;
    subscribe(&pool, tenant_id, plan_a, "active").await;

    let now = Utc::now();
    insert_tokens(&pool, tenant_id, 150, 0, now).await;

    let decision = billing::enforcement::check(&pool, tenant_id)
        .await
        .unwrap();
    assert!(!decision.allowed);
    assert_eq!(
        decision.reason,
        Some(billing::enforcement::PauseReason::TokenLimit)
    );

    let plan_b = create_plan(&pool, "Higher Allowance B", Some(200), Some(1000)).await;
    sqlx::query("UPDATE tenant_subscriptions SET plan_id = $1 WHERE tenant_id = $2")
        .bind(plan_b)
        .bind(tenant_id)
        .execute(&pool)
        .await
        .unwrap();

    let decision = billing::enforcement::check(&pool, tenant_id)
        .await
        .unwrap();
    assert!(decision.allowed);
    assert_eq!(decision.reason, None);
}
