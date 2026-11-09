use std::time::Duration;

use uuid::Uuid;

pub fn require_db_tests() -> bool {
    std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
}

pub async fn get_pool() -> Option<sqlx::PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("skipping billing tests: DATABASE_URL not set");
            return None;
        }
    };
    let pool = db::lazy_pool(&url, 2, Duration::from_secs(5));
    if sqlx::query("SELECT 1").execute(&pool).await.is_err() {
        if require_db_tests() {
            panic!("REQUIRE_DB_TESTS=1 but DATABASE_URL is unreachable");
        }
        eprintln!("skipping billing tests: DATABASE_URL is unreachable");
        return None;
    }
    Some(pool)
}

pub async fn setup(pool: &sqlx::PgPool) {
    db::testing::migrate().await;
    sqlx::query(
        "TRUNCATE TABLE \
         ai_usage_records, tenant_subscriptions, conversations, \
         customers, tenants \
         RESTART IDENTITY CASCADE",
    )
    .execute(&db::testing::owner_pool())
    .await
    .expect("truncate test tables");
}

pub async fn seed_tenant(pool: &sqlx::PgPool) -> Uuid {
    let slug = format!("test-tenant-{}", Uuid::new_v4().simple());
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind("Test Tenant")
        .bind(&slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

pub async fn create_plan(
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
    .bind(format!("{name} {}", Uuid::new_v4().simple()))
    .bind(token_allowance)
    .bind(conversation_allowance)
    .fetch_one(pool)
    .await
    .unwrap()
}

pub async fn subscribe(pool: &sqlx::PgPool, tenant_id: Uuid, plan_id: Uuid, status: &str) {
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
