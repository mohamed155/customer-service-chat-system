use super::*;

#[tokio::test]
async fn a_reclaimed_lease_fences_the_old_worker() -> sqlx::Result<()> {
    let required = std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1");
    let Ok(url) = std::env::var("DATABASE_URL") else {
        assert!(!required, "DATABASE_URL is required");
        return Ok(());
    };
    let pool = db::lazy_pool(&url, 4, Duration::from_secs(5));
    if let Err(error) = sqlx::query("SELECT 1").execute(&pool).await {
        assert!(!required, "database required: {error}");
        return Ok(());
    }
    db::testing::migrate().await;
    let tenant = db::testing::insert_tenant(
        &pool,
        "Lease fencing",
        &format!("lease-{}", Uuid::new_v4().simple()),
    )
    .await;
    let seed = db::testing::seed_pool();
    let user: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email,display_name) VALUES ($1,'Operator') RETURNING id",
    )
    .bind(format!("{}@privacy.test", Uuid::new_v4().simple()))
    .fetch_one(&seed)
    .await?;
    let customer: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id,display_name) VALUES ($1,'Customer') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(&seed)
    .await?;
    let mut seed_work = seed.begin().await?;
    let old = Uuid::new_v4();
    let id: Uuid = sqlx::query_scalar(
            "INSERT INTO privacy_requests (tenant_id,customer_id,requested_by,kind,status,lease_token,lease_until)
             VALUES ($1,$2,$3,'erase','running',$4,now()-interval '1 second') RETURNING id",
        ).bind(tenant).bind(customer).bind(user).bind(old).fetch_one(&mut *seed_work).await?;
    let new = Uuid::new_v4();
    // Model the queue claim's fenced transition without claiming another test's jobs.
    sqlx::query("UPDATE privacy_requests SET lease_token = $2, lease_until = now()+interval '10 minutes' WHERE id = $1")
            .bind(id).bind(new).execute(&mut *seed_work).await?;
    seed_work.commit().await?;
    let mut work = db::UnitOfWork::begin_for_tenant(&pool, tenant).await?;
    assert!(lock_request(work.conn(), tenant, id, old).await?.is_none());
    assert!(lock_request(work.conn(), tenant, id, new).await?.is_some());
    sqlx::query("UPDATE privacy_requests SET status = 'expired' WHERE tenant_id = $1 AND id = $2")
        .bind(tenant)
        .bind(id)
        .execute(work.conn())
        .await?;
    work.commit().await?;
    Ok(())
}
