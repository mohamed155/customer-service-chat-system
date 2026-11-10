//! The sweeps and the platform routes as audited cross-tenant functions (MOH-46 part 3, MOH-55).
//!
//! Like `xtenant.rs`, each test seeds in an owner transaction, puts forced Row-Level Security on the
//! tables the function touches, drops to `app_runtime` and calls the function. A direct read of
//! those tables returns nothing; the function still does its job and writes its audit row.
//! Nothing is committed.
use sqlx::{PgConnection, Postgres, Transaction};
use uuid::Uuid;

type Tx = Transaction<'static, Postgres>;

fn can_run() -> bool {
    if std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
        || std::env::var("MIGRATION_DATABASE_URL").is_ok_and(|v| !v.is_empty())
    {
        return true;
    }
    eprintln!("skipping xtenant platform test: MIGRATION_DATABASE_URL not set");
    false
}

/// Audit rows for `function` and `resource_id`, read as `app_platform`.
async fn audit_rows(conn: &mut PgConnection, function: &str, resource_id: &str) -> i64 {
    db::testing::become_platform(conn).await;
    let n = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs \
         WHERE action = $1 AND resource_type = 'cross_tenant_access' AND resource_id = $2",
    )
    .bind(format!("xtenant.{function}"))
    .bind(resource_id)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    db::testing::become_runtime(conn).await;
    n
}

async fn count_as_platform(conn: &mut PgConnection, sql: &str, bind: Uuid) -> i64 {
    db::testing::become_platform(conn).await;
    let n = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.to_string()))
        .bind(bind)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    db::testing::become_runtime(conn).await;
    n
}

async fn seed_tenant(tx: &mut Tx) -> Uuid {
    let unique = Uuid::new_v4().simple().to_string();
    sqlx::query_scalar("INSERT INTO tenants (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(format!("Tenant {unique}"))
        .bind(format!("t-{unique}"))
        .fetch_one(&mut **tx)
        .await
        .unwrap()
}

async fn seed_user(tx: &mut Tx) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO users (email, display_name) VALUES ($1, 'Xtenant Test') RETURNING id",
    )
    .bind(format!("{}@example.test", Uuid::new_v4().simple()))
    .fetch_one(&mut **tx)
    .await
    .unwrap()
}

async fn seed_membership(tx: &mut Tx, tenant: Uuid, user: Uuid) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO tenant_memberships (tenant_id, user_id, role) VALUES ($1, $2, 'agent') RETURNING id",
    )
    .bind(tenant)
    .bind(user)
    .fetch_one(&mut **tx)
    .await
    .unwrap()
}

// ---------------------------------------------------------------------------------------------
// Sweeps
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn widget_session_sweep_deletes_expired_sessions_and_audits_the_count() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let tenant = seed_tenant(&mut tx).await;
    let instance: Uuid = sqlx::query_scalar(
        "INSERT INTO widget_instances (tenant_id, public_id, name) VALUES ($1, $2, 'W') RETURNING id",
    )
    .bind(tenant)
    .bind(format!("wgt_{}", Uuid::new_v4().simple()))
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    for interval in ["-2 hours", "-1 hour", "1 hour"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO widget_sessions (tenant_id, widget_instance_id, token_hash, expires_at) \
             VALUES ($1, $2, $3, now() + interval '{interval}')"
        )))
        .bind(tenant)
        .bind(instance)
        .bind(Uuid::new_v4().as_bytes().to_vec())
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    db::testing::enforce_rls(&mut tx, &["widget_sessions", "audit_logs"]).await;
    db::testing::become_runtime(&mut tx).await;

    let before = audit_rows(&mut tx, "widget_session_sweep", "widget_session_expiry").await;
    let deleted: i64 = sqlx::query_scalar("SELECT xtenant.widget_session_sweep()")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(
        deleted >= 2,
        "at least this test's two expired sessions go, got {deleted}"
    );
    let remaining = count_as_platform(
        &mut tx,
        "SELECT count(*) FROM widget_sessions WHERE widget_instance_id = $1",
        instance,
    )
    .await;
    assert_eq!(remaining, 1, "the live session stays");
    assert_eq!(
        audit_rows(&mut tx, "widget_session_sweep", "widget_session_expiry").await,
        before + 1,
        "one audit row for the run"
    );
    let again: i64 = sqlx::query_scalar("SELECT xtenant.widget_session_sweep()")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(again, 0);
    assert_eq!(
        audit_rows(&mut tx, "widget_session_sweep", "widget_session_expiry").await,
        before + 1,
        "a run that deletes nothing writes no audit row"
    );
}

#[tokio::test]
async fn retention_sweeps_delete_only_rows_older_than_ninety_days() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let tenant = seed_tenant(&mut tx).await;
    let user = seed_user(&mut tx).await;
    let membership = seed_membership(&mut tx, tenant, user).await;
    let catalog: Uuid =
        sqlx::query_scalar("SELECT id FROM integration_catalog WHERE slug = 'generic-webhook'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    let connection: Uuid = sqlx::query_scalar(
        "INSERT INTO integration_connections \
            (tenant_id, catalog_id, webhook_token_hash, webhook_token_ciphertext, webhook_token_nonce) \
         VALUES ($1, $2, $3, '\\x00', '\\x00') RETURNING id",
    )
    .bind(tenant)
    .bind(catalog)
    .bind(Uuid::new_v4().as_bytes().to_vec())
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    for age in ["100 days", "1 day"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO integration_events (tenant_id, connection_id, event_type, outcome, created_at) \
             VALUES ($1, $2, 'connected', 'success', now() - interval '{age}')"
        )))
        .bind(tenant)
        .bind(connection)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO integration_webhook_deliveries (tenant_id, connection_id, payload, received_at) \
             VALUES ($1, $2, '{{}}'::jsonb, now() - interval '{age}')"
        )))
        .bind(tenant)
        .bind(connection)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO notifications \
                (tenant_id, recipient_membership_id, kind, title, subject_type, subject_id, dedupe_key, created_at) \
             VALUES ($1, $2, 'escalation.new', 't', 'conversation', $3, $4, now() - interval '{age}')"
        )))
        .bind(tenant)
        .bind(membership)
        .bind(Uuid::new_v4())
        .bind(Uuid::new_v4().to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    db::testing::enforce_rls(
        &mut tx,
        &[
            "integration_events",
            "integration_webhook_deliveries",
            "notifications",
            "audit_logs",
        ],
    )
    .await;
    db::testing::become_runtime(&mut tx).await;

    let integration: i64 = sqlx::query_scalar("SELECT xtenant.integration_retention_sweep()")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(
        integration >= 2,
        "the old event and delivery go, got {integration}"
    );
    let notifications: i64 = sqlx::query_scalar("SELECT xtenant.notification_retention_sweep()")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(
        notifications >= 1,
        "the old notification goes, got {notifications}"
    );

    for (sql, expected) in [
        (
            "SELECT count(*) FROM integration_events WHERE connection_id = $1",
            1,
        ),
        (
            "SELECT count(*) FROM integration_webhook_deliveries WHERE connection_id = $1",
            1,
        ),
    ] {
        assert_eq!(
            count_as_platform(&mut tx, sql, connection).await,
            expected,
            "{sql}"
        );
    }
    assert_eq!(
        count_as_platform(
            &mut tx,
            "SELECT count(*) FROM notifications WHERE recipient_membership_id = $1",
            membership
        )
        .await,
        1,
        "the recent notification stays"
    );
}

#[tokio::test]
async fn tool_request_expiry_sweep_expires_overdue_requests_and_queues_the_decision() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    // Rows for tables under forced Row-Level Security are seeded as the bypass role.
    db::testing::become_platform(&mut tx).await;
    let tenant = seed_tenant(&mut tx).await;
    let customer: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, 'C') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let conversation: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) VALUES ($1, $2, 'widget', 'open') RETURNING id",
    )
    .bind(tenant)
    .bind(customer)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let message: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) VALUES ($1, $2, 'customer', 'hi') RETURNING id",
    )
    .bind(tenant)
    .bind(conversation)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let generation: Uuid = sqlx::query_scalar(
        "INSERT INTO ai_generations (tenant_id, conversation_id, trigger_message_id, outcome, latency_ms) \
         VALUES ($1, $2, $3, 'awaiting_tool_approval', 1) RETURNING id",
    )
    .bind(tenant)
    .bind(conversation)
    .bind(message)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let mut ids = Vec::new();
    for expiry in ["-1 minute", "1 hour"] {
        let id: Uuid = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "INSERT INTO tool_requests \
                (tenant_id, conversation_id, generation_id, tool_name, tool_source, arguments, status, \
                 approval_required, chain_index, expires_at) \
             VALUES ($1, $2, $3, 'lookup', 'builtin', '{{}}'::jsonb, 'awaiting_approval', true, 0, \
                     now() + interval '{expiry}') RETURNING id"
        )))
        .bind(tenant)
        .bind(conversation)
        .bind(generation)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        ids.push(id);
    }
    db::testing::become_owner(&mut tx).await;
    db::testing::enforce_rls(&mut tx, &["tool_requests", "outbox_events", "audit_logs"]).await;
    db::testing::become_runtime(&mut tx).await;

    let expired: i64 = sqlx::query_scalar("SELECT xtenant.tool_request_expiry_sweep()")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(expired >= 1, "the overdue request expires, got {expired}");

    db::testing::become_platform(&mut tx).await;
    let statuses: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, status FROM tool_requests WHERE conversation_id = $1 ORDER BY created_at, id",
    )
    .bind(conversation)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    let status_of = |id: Uuid| statuses.iter().find(|(i, _)| *i == id).unwrap().1.clone();
    assert_eq!(status_of(ids[0]), "expired");
    assert_eq!(
        status_of(ids[1]),
        "awaiting_approval",
        "a request still within its deadline is left alone"
    );
    let (event_type, payload): (String, serde_json::Value) = sqlx::query_as(
        "SELECT event_type, payload FROM outbox_events WHERE aggregate_id = $1 AND event_type = 'ai.tool_decision'",
    )
    .bind(conversation.to_string())
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(event_type, "ai.tool_decision");
    assert_eq!(payload["toolRequestId"], ids[0].to_string());
    assert_eq!(payload["outcome"], "expired");
    assert_eq!(payload["conversationId"], conversation.to_string());
    assert_eq!(payload["tenantId"], tenant.to_string());
}

#[tokio::test]
async fn available_agents_lists_available_agents_across_tenants() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let tenant = seed_tenant(&mut tx).await;
    let (u1, u2) = (seed_user(&mut tx).await, seed_user(&mut tx).await);
    let (m1, m2) = (
        seed_membership(&mut tx, tenant, u1).await,
        seed_membership(&mut tx, tenant, u2).await,
    );
    for (m, state) in [(m1, "available"), (m2, "away")] {
        sqlx::query(
            "INSERT INTO agent_availability (tenant_id, membership_id, state) VALUES ($1, $2, $3)",
        )
        .bind(tenant)
        .bind(m)
        .bind(state)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    db::testing::enforce_rls(&mut tx, &["agent_availability", "audit_logs"]).await;
    db::testing::become_runtime(&mut tx).await;

    let direct: i64 = sqlx::query_scalar("SELECT count(*) FROM agent_availability")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        direct, 0,
        "app_runtime must not read availability directly without a tenant"
    );
    let rows: Vec<(Uuid, Uuid)> =
        sqlx::query_as("SELECT tenant_id, membership_id FROM xtenant.available_agents()")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert!(
        rows.contains(&(tenant, m1)),
        "the available agent is listed"
    );
    assert!(!rows.iter().any(|(_, m)| *m == m2), "an away agent is not");
}

// ---------------------------------------------------------------------------------------------
// Platform dashboards
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn platform_dashboard_functions_aggregate_across_tenants() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    // Rows for tables under forced Row-Level Security are seeded as the bypass role.
    db::testing::become_platform(&mut tx).await;
    let tenant = seed_tenant(&mut tx).await;
    let customer: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, 'C') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) VALUES ($1, $2, 'widget', 'open')",
    )
    .bind(tenant)
    .bind(customer)
    .execute(&mut *tx)
    .await
    .unwrap();
    let provider = "gemini".to_string();
    for (status, category) in [("success", None), ("failure", Some("timeout"))] {
        sqlx::query(
            "INSERT INTO ai_usage_records (tenant_id, provider, model, status, streamed, latency_ms, error_category, input_tokens, output_tokens) \
             VALUES ($1, $2, 'm', $3, false, 120, $4, 10, 5)",
        )
        .bind(tenant)
        .bind(&provider)
        .bind(status)
        .bind(category)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at, attempts) \
         VALUES ($1, 'x', 'x', NULL, $2, '{}'::jsonb, now() - interval '1 hour', 6)",
    )
    .bind(Uuid::new_v4())
    .bind(format!("xtenant.test.{}", Uuid::new_v4().simple()))
    .execute(&mut *tx)
    .await
    .unwrap();
    db::testing::become_owner(&mut tx).await;
    db::testing::enforce_rls(
        &mut tx,
        &[
            "conversations",
            "ai_usage_records",
            "outbox_events",
            "audit_logs",
        ],
    )
    .await;
    db::testing::become_runtime(&mut tx).await;

    let direct: i64 = sqlx::query_scalar("SELECT count(*) FROM conversations")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        direct, 0,
        "app_runtime must not read conversations directly without a tenant"
    );

    let active: i64 = sqlx::query_scalar("SELECT xtenant.platform_active_conversation_count()")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(active >= 1);

    let totals: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT total, successes, failures, input_tokens, output_tokens \
         FROM xtenant.platform_ai_usage_totals(now() - interval '1 day', now() + interval '1 day')",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(totals.0 >= 2 && totals.1 >= 1 && totals.2 >= 1 && totals.3 >= 20 && totals.4 >= 10);

    let metrics: Vec<(String, i64, i64, i64, Option<i64>, i64)> = sqlx::query_as(
        "SELECT provider, total, successes, failures, p50_latency_ms, timeout \
         FROM xtenant.platform_provider_metrics(now() - interval '1 day', now() + interval '1 day')",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    let mine = metrics
        .iter()
        .find(|m| m.0 == provider)
        .expect("this test's provider");
    assert!(
        mine.1 >= 2 && mine.2 >= 1 && mine.3 >= 1 && mine.4.is_some() && mine.5 >= 1,
        "the provider's row includes this test's two calls: {mine:?}"
    );

    let failures: Vec<(String, String, Option<String>, i32)> = sqlx::query_as(
        "SELECT provider, model, error_category, latency_ms \
         FROM xtenant.platform_recent_provider_failures(now() - interval '1 day', now() + interval '1 day')",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert!(failures
        .iter()
        .any(|f| f.0 == provider && f.2.as_deref() == Some("timeout") && f.3 == 120));

    let health: (i64, Option<chrono::DateTime<chrono::Utc>>, i64) =
        sqlx::query_as("SELECT pending, oldest, stuck FROM xtenant.platform_queue_health()")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(health.0 >= 1 && health.1.is_some() && health.2 >= 1);

    // Throttled to a row an hour, however often the dashboard polls.
    for _ in 0..3 {
        sqlx::query("SELECT * FROM xtenant.platform_queue_health()")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    }
    let rows = audit_rows(&mut tx, "platform_queue_health", "platform_dashboard").await;
    assert!(rows >= 1);
    db::testing::become_platform(&mut tx).await;
    let in_this_tx: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_logs WHERE action = 'xtenant.platform_queue_health' AND created_at >= now() - interval '1 hour'",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        in_this_tx, 1,
        "polling four times in an hour writes one audit row"
    );
}

// ---------------------------------------------------------------------------------------------
// Platform audit listing
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn platform_audit_entries_lists_every_tenants_entries_and_records_who_asked() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let (t1, t2) = (seed_tenant(&mut tx).await, seed_tenant(&mut tx).await);
    let admin = seed_user(&mut tx).await;
    for (tenant, res) in [(t1, "r1"), (t2, "r2")] {
        sqlx::query(
            "INSERT INTO audit_logs (actor_user_id, action, resource_type, resource_id, tenant_id) \
             VALUES ($1, 'xtenant.test.entry', 'test', $2, $3)",
        )
        .bind(admin)
        .bind(res)
        .bind(tenant)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    db::testing::enforce_rls(&mut tx, &["audit_logs"]).await;
    db::testing::become_runtime(&mut tx).await;

    let direct: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_logs")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        direct, 0,
        "app_runtime must not read audit_logs directly without a tenant"
    );

    let rows: Vec<(String, Option<Uuid>, Option<String>)> = sqlx::query_as(
        "SELECT resource_id, tenant_id, display_name \
         FROM xtenant.platform_audit_entries($1, NULL, NULL, NULL, ARRAY['xtenant.test.%'], NULL, NULL, NULL, 10)",
    )
    .bind(admin)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    let mut seen: Vec<(String, Option<Uuid>)> = rows.iter().map(|r| (r.0.clone(), r.1)).collect();
    seen.sort();
    assert_eq!(
        seen,
        vec![("r1".to_string(), Some(t1)), ("r2".to_string(), Some(t2))]
    );
    assert!(
        rows.iter().all(|r| r.2.as_deref() == Some("Xtenant Test")),
        "the actor is joined"
    );

    let one: Vec<(String,)> = sqlx::query_as(
        "SELECT resource_id FROM xtenant.platform_audit_entries($1, $2, NULL, NULL, NULL, NULL, NULL, NULL, 10) \
         WHERE action = 'xtenant.test.entry'",
    )
    .bind(admin)
    .bind(t1)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(one, vec![("r1".to_string(),)], "the tenant filter applies");
    assert_eq!(
        audit_rows(&mut tx, "platform_audit_entries", &admin.to_string()).await,
        1,
        "who asked is recorded, once an hour"
    );
}

// ---------------------------------------------------------------------------------------------
// Billing subscriptions
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn platform_subscription_set_assigns_plans_and_audits_for_the_tenant() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let tenant = seed_tenant(&mut tx).await;
    let admin = seed_user(&mut tx).await;
    let active: Uuid = sqlx::query_scalar(
        "INSERT INTO billing_plans (name, monthly_price_cents) VALUES ($1, 100) RETURNING id",
    )
    .bind(format!("Active {}", Uuid::new_v4().simple()))
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let retired: Uuid = sqlx::query_scalar(
        "INSERT INTO billing_plans (name, monthly_price_cents, status) VALUES ($1, 50, 'retired') RETURNING id",
    )
    .bind(format!("Retired {}", Uuid::new_v4().simple()))
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    db::testing::enforce_rls(&mut tx, &["tenant_subscriptions", "audit_logs"]).await;
    db::testing::become_runtime(&mut tx).await;

    let direct: i64 = sqlx::query_scalar("SELECT count(*) FROM tenant_subscriptions")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        direct, 0,
        "app_runtime must not read subscriptions directly without a tenant"
    );

    let set = |plan: Uuid, status: &'static str| {
        sqlx::query_as::<_, (String, Option<Uuid>, Option<Uuid>, Option<String>)>(
            "SELECT outcome, subscription_id, plan_id, status FROM xtenant.platform_subscription_set($1, $2, $3, $4)",
        )
        .bind(admin)
        .bind(tenant)
        .bind(plan)
        .bind(status)
    };

    let missing = set(Uuid::new_v4(), "active")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(missing.0, "plan_not_found");
    let refused = set(retired, "active").fetch_one(&mut *tx).await.unwrap();
    assert_eq!(
        refused.0, "retired_plan",
        "a retired plan cannot be newly assigned"
    );

    let first = set(active, "active").fetch_one(&mut *tx).await.unwrap();
    assert_eq!(
        (first.0.as_str(), first.2, first.3.as_deref()),
        ("ok", Some(active), Some("active"))
    );
    let subscription = first.1.unwrap();
    let second = set(active, "past_due").fetch_one(&mut *tx).await.unwrap();
    assert_eq!(
        (second.0.as_str(), second.1, second.3.as_deref()),
        ("ok", Some(subscription), Some("past_due"))
    );

    db::testing::become_platform(&mut tx).await;
    let (action, tenant_id, details): (String, Option<Uuid>, serde_json::Value) = sqlx::query_as(
        "SELECT action, tenant_id, details FROM audit_logs \
         WHERE resource_id = $1 AND action = 'billing_subscription.changed' AND details->>'status' = 'past_due'",
    )
    .bind(subscription.to_string())
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        (action.as_str(), tenant_id),
        ("billing_subscription.changed", Some(tenant))
    );
    assert_eq!(details["status"], "past_due");
    assert_eq!(details["previous_status"], "active");
    assert_eq!(details["previous_plan_id"], active.to_string());
    db::testing::become_runtime(&mut tx).await;

    // A tenant already on a plan that was later retired may keep it (change its status only).
    db::testing::become_platform(&mut tx).await;
    sqlx::query("UPDATE billing_plans SET status = 'retired' WHERE id = $1")
        .bind(active)
        .execute(&mut *tx)
        .await
        .unwrap();
    db::testing::become_runtime(&mut tx).await;
    let kept = set(active, "canceled").fetch_one(&mut *tx).await.unwrap();
    assert_eq!(kept.0, "ok");

    let listed: Vec<(Uuid, bool)> =
        sqlx::query_as("SELECT tenant_id, explicit FROM xtenant.platform_subscriptions_list()")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert!(
        listed.contains(&(tenant, true)),
        "the list shows the explicit subscription"
    );
}

// ---------------------------------------------------------------------------------------------
// Platform-wide AI configuration and credentials
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn platform_ai_functions_manage_the_platform_rows_and_audit_every_change() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let admin = seed_user(&mut tx).await;
    let provider = "anthropic".to_string();
    // Park any existing platform default and credential so this test sees only its own rows
    // (rolled back with the transaction).
    sqlx::query("UPDATE ai_configurations SET deleted_at = now() WHERE tenant_id IS NULL AND deleted_at IS NULL")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE ai_credentials SET deleted_at = now() WHERE tenant_id IS NULL AND deleted_at IS NULL",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    db::testing::enforce_rls(
        &mut tx,
        &["ai_configurations", "ai_credentials", "audit_logs"],
    )
    .await;
    db::testing::become_runtime(&mut tx).await;

    let direct: i64 = sqlx::query_scalar("SELECT count(*) FROM ai_credentials")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        direct, 0,
        "app_runtime must not read credentials directly without a tenant"
    );

    // Configuration: first call inserts, second updates the same row.
    let details = serde_json::json!({"provider": "p", "model": "m"});
    let set_config = |model: &'static str| {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT xtenant.platform_ai_config_set($1, 'openai', $2, 512, 0.5, '[]'::jsonb, $3)",
        )
        .bind(admin)
        .bind(model)
        .bind(details.clone())
    };
    let id1 = set_config("m1").fetch_one(&mut *tx).await.unwrap();
    let id2 = set_config("m2").fetch_one(&mut *tx).await.unwrap();
    assert_eq!(id1, id2, "there is one platform default");
    let config: Vec<(Uuid, String, Option<Uuid>)> =
        sqlx::query_as("SELECT id, model, tenant_id FROM xtenant.platform_ai_config()")
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert_eq!(config, vec![(id1, "m2".to_string(), None)]);

    // Credentials: set, rotate, read the hint and the sealed key, delete.
    let set_key = |hint: &'static str, cipher: u8| {
        sqlx::query_as::<_, (Uuid, bool)>(
            "SELECT id, rotated FROM xtenant.platform_ai_credential_set($1, $2, $3, $4, $5)",
        )
        .bind(admin)
        .bind(provider.clone())
        .bind(vec![cipher; 4])
        .bind(vec![cipher; 12])
        .bind(hint)
    };
    let (cid, rotated) = set_key("...abcd", 1).fetch_one(&mut *tx).await.unwrap();
    assert!(!rotated);
    let (cid2, rotated2) = set_key("...wxyz", 2).fetch_one(&mut *tx).await.unwrap();
    assert_eq!(
        (cid2, rotated2),
        (cid, true),
        "the second call rotates the same row"
    );
    let hint: Option<(String,)> =
        sqlx::query_as("SELECT key_hint FROM xtenant.platform_ai_credential_hint($1)")
            .bind(&provider)
            .fetch_optional(&mut *tx)
            .await
            .unwrap();
    assert_eq!(hint.map(|h| h.0), Some("...wxyz".to_string()));
    let secret: Option<(Vec<u8>, Vec<u8>)> =
        sqlx::query_as("SELECT ciphertext, nonce FROM xtenant.platform_ai_credential_secret($1)")
            .bind(&provider)
            .fetch_optional(&mut *tx)
            .await
            .unwrap();
    assert_eq!(secret, Some((vec![2; 4], vec![2; 12])));
    let deleted: Option<Uuid> =
        sqlx::query_scalar("SELECT xtenant.platform_ai_credential_delete($1, $2)")
            .bind(admin)
            .bind(&provider)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(deleted, Some(cid));
    let gone: Option<Uuid> =
        sqlx::query_scalar("SELECT xtenant.platform_ai_credential_delete($1, $2)")
            .bind(admin)
            .bind(&provider)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(gone, None, "deleting what is not there reports nothing");

    // Each write left its domain audit row, with no tenant.
    db::testing::become_platform(&mut tx).await;
    let actions: Vec<(String, Option<Uuid>)> = sqlx::query_as(
        "SELECT action, tenant_id FROM audit_logs WHERE actor_user_id = $1 \
           AND action IN ('ai_config.updated', 'ai_credential.set', 'ai_credential.deleted') \
         ORDER BY created_at, id",
    )
    .bind(admin)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    // Rows written in one transaction share a timestamp, so compare the set, not the order.
    let mut names: Vec<&str> = actions.iter().map(|a| a.0.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "ai_config.updated",
            "ai_config.updated",
            "ai_credential.deleted",
            "ai_credential.set",
            "ai_credential.set",
        ]
    );
    assert!(actions.iter().all(|a| a.1.is_none()));
}
