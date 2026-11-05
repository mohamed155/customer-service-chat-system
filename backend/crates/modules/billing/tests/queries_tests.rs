use chrono::{DateTime, Datelike, TimeZone, Utc};
use serial_test::serial;
use uuid::Uuid;

mod common;
use common::*;

// ── effective_plan tests ─────────────────────────────────────────

#[tokio::test]
#[serial]
async fn tenant_without_subscription_falls_back_to_free_plan() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;

    let (plan, status) = billing::queries::effective_plan(&pool, tenant_id)
        .await
        .unwrap();

    assert_eq!(plan.name, "Free");
    assert_eq!(plan.monthly_price_cents, 0);
    assert!(plan.is_default);
    assert_eq!(plan.status, "active");
    assert_eq!(status, "active");
}

#[tokio::test]
#[serial]
async fn tenant_with_explicit_subscription_resolves_correctly() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;

    // Plan names are globally unique and rows persist across runs.
    let plan_name = format!("Pro Plan {}", Uuid::new_v4());
    let plan_id: Uuid = sqlx::query_scalar(
        "INSERT INTO billing_plans \
         (name, monthly_price_cents, token_allowance, conversation_allowance, is_default, status) \
         VALUES ($1, $2, $3, $4, false, 'active') RETURNING id",
    )
    .bind(&plan_name)
    .bind(2999)
    .bind(1_000_000i64)
    .bind(1000i32)
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO tenant_subscriptions (tenant_id, plan_id, status) \
         VALUES ($1, $2, 'active')",
    )
    .bind(tenant_id)
    .bind(plan_id)
    .execute(&pool)
    .await
    .unwrap();

    let (plan, status) = billing::queries::effective_plan(&pool, tenant_id)
        .await
        .unwrap();

    assert_eq!(plan.name, plan_name);
    assert_eq!(plan.monthly_price_cents, 2999);
    assert_eq!(plan.token_allowance, Some(1_000_000));
    assert_eq!(plan.conversation_allowance, Some(1000));
    assert!(!plan.is_default);
    assert_eq!(plan.status, "active");
    assert_eq!(status, "active");
}

// ── token_usage tests ────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn token_usage_sums_input_and_output_with_null_handling() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;

    let now = Utc::now();
    let start = Utc
        .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .unwrap();
    let end = if now.month() == 12 {
        Utc.with_ymd_and_hms(now.year() + 1, 1, 1, 0, 0, 0).unwrap()
    } else {
        Utc.with_ymd_and_hms(now.year(), now.month() + 1, 1, 0, 0, 0)
            .unwrap()
    };

    for (input, output) in &[
        (Some(10), Some(20)),
        (None, Some(15)),
        (Some(25), None),
        (None, None),
    ] {
        sqlx::query(
            "INSERT INTO ai_usage_records \
             (tenant_id, provider, model, input_tokens, output_tokens, status, streamed, latency_ms, created_at) \
             VALUES ($1, 'openai', 'gpt-4', $2, $3, 'success', false, 100, $4)",
        )
        .bind(tenant_id)
        .bind(*input)
        .bind(*output)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();
    }

    let total = billing::queries::token_usage(&pool, tenant_id, start, end)
        .await
        .unwrap();
    assert_eq!(total, 70);
}

#[tokio::test]
#[serial]
async fn token_usage_excludes_records_outside_period() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;

    let start = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();

    let cases: Vec<(DateTime<Utc>, i32, i32, i64)> = vec![
        (
            Utc.with_ymd_and_hms(2026, 6, 30, 23, 59, 59).unwrap(),
            10,
            0,
            0,
        ), // before
        (
            Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap(),
            10,
            20,
            30,
        ), // at start — included
        (
            Utc.with_ymd_and_hms(2026, 7, 15, 12, 0, 0).unwrap(),
            10,
            10,
            20,
        ), // mid — included
        (
            Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap(),
            10,
            10,
            0,
        ), // at end — excluded
        (
            Utc.with_ymd_and_hms(2026, 8, 2, 0, 0, 0).unwrap(),
            10,
            10,
            0,
        ), // after — excluded
    ];

    for (ts, input, output, _expected_contribution) in &cases {
        sqlx::query(
            "INSERT INTO ai_usage_records \
             (tenant_id, provider, model, input_tokens, output_tokens, status, streamed, latency_ms, created_at) \
             VALUES ($1, 'openai', 'gpt-4', $2, $3, 'success', false, 100, $4)",
        )
        .bind(tenant_id)
        .bind(*input)
        .bind(*output)
        .bind(ts)
        .execute(&pool)
        .await
        .unwrap();
    }

    let total = billing::queries::token_usage(&pool, tenant_id, start, end)
        .await
        .unwrap();
    assert_eq!(total, 50);
}

// ── conversation_usage tests ─────────────────────────────────────

#[tokio::test]
#[serial]
async fn conversation_usage_excludes_soft_deleted() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;

    let customer_id: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("Conv Test Customer")
    .fetch_one(&pool)
    .await
    .unwrap();

    let start = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();

    sqlx::query(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, created_at) \
         VALUES ($1, $2, 'web_chat', 'open', $3)",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(Utc.with_ymd_and_hms(2026, 7, 5, 0, 0, 0).unwrap())
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, created_at, deleted_at) \
         VALUES ($1, $2, 'web_chat', 'closed', $3, $4)",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(Utc.with_ymd_and_hms(2026, 7, 10, 0, 0, 0).unwrap())
    .bind(Utc.with_ymd_and_hms(2026, 7, 12, 0, 0, 0).unwrap())
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status, created_at) \
         VALUES ($1, $2, 'whatsapp', 'open', $3)",
    )
    .bind(tenant_id)
    .bind(customer_id)
    .bind(Utc.with_ymd_and_hms(2026, 7, 15, 0, 0, 0).unwrap())
    .execute(&pool)
    .await
    .unwrap();

    let count = billing::queries::conversation_usage(&pool, tenant_id, start, end)
        .await
        .unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
#[serial]
async fn conversation_usage_empty_when_no_conversations() {
    let Some(pool) = get_pool().await else { return };
    setup(&pool).await;
    let tenant_id = seed_tenant(&pool).await;

    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();

    let count = billing::queries::conversation_usage(&pool, tenant_id, start, end)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
