//! The outbox queue as audited cross-tenant functions (MOH-46 part 2, MOH-54).
//!
//! Each test seeds events in an owner transaction, puts forced Row-Level Security on the tables
//! (see `db::testing`), drops to `app_runtime` and runs a worker-style flow through the functions.
//! Nothing is committed. Events use a unique event type per test where the function takes one, so
//! rows committed by other tests cannot be claimed here; the fixed-type functions seed an event
//! older than anything else (created_at in 1970) so it is the one claimed.
//!
//! They need the role split (`MIGRATION_DATABASE_URL`) and skip without it unless
//! `REQUIRE_DB_TESTS=1`.
use sqlx::{PgConnection, Postgres, Transaction};
use uuid::Uuid;

type Tx = Transaction<'static, Postgres>;

fn can_run() -> bool {
    if std::env::var("REQUIRE_DB_TESTS").as_deref() == Ok("1")
        || std::env::var("MIGRATION_DATABASE_URL").is_ok_and(|v| !v.is_empty())
    {
        return true;
    }
    eprintln!("skipping xtenant outbox test: MIGRATION_DATABASE_URL not set");
    false
}

fn unique_type() -> String {
    format!("xtenant.test.{}", Uuid::new_v4().simple())
}

/// Audit rows written for `function` and `resource_id`, read as `app_platform`.
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

/// One outbox row, read as `app_platform` (the table is hidden from everyone else once RLS is on).
async fn outbox_row(
    conn: &mut PgConnection,
    id: Uuid,
) -> Option<(bool, Option<Uuid>, i32, Option<String>, bool)> {
    db::testing::become_platform(conn).await;
    let row = sqlx::query_as(
        "SELECT claimed_at IS NOT NULL, claim_token, attempts, last_error, available_at > now() \
         FROM outbox_events WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .unwrap();
    db::testing::become_runtime(conn).await;
    row
}

async fn seed_event(
    tx: &mut Tx,
    event_type: &str,
    tenant: Option<Uuid>,
    created_at_sql: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, 'test', $2, $3, $4, '{{}}'::jsonb, {created_at_sql})"
    )))
    .bind(id)
    .bind(id.to_string())
    .bind(tenant.map(|t| t.to_string()))
    .bind(event_type)
    .execute(&mut **tx)
    .await
    .unwrap();
    id
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

async fn direct_outbox_count(conn: &mut PgConnection) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM outbox_events")
        .fetch_one(conn)
        .await
        .unwrap()
}

#[tokio::test]
async fn claim_takes_the_oldest_unclaimed_event_of_the_given_types_and_ack_removes_it() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let tenant = seed_tenant(&mut tx).await;
    let (type_a, type_b, other) = (unique_type(), unique_type(), unique_type());
    let oldest = seed_event(
        &mut tx,
        &type_a,
        Some(tenant),
        "now() - interval '3 minutes'",
    )
    .await;
    let middle = seed_event(
        &mut tx,
        &type_b,
        Some(tenant),
        "now() - interval '2 minutes'",
    )
    .await;
    let unrelated = seed_event(
        &mut tx,
        &other,
        Some(tenant),
        "now() - interval '9 minutes'",
    )
    .await;
    db::testing::enforce_rls(&mut tx, &["outbox_events", "audit_logs"]).await;
    db::testing::become_runtime(&mut tx).await;

    assert_eq!(
        direct_outbox_count(&mut tx).await,
        0,
        "app_runtime must not read the outbox directly without a tenant"
    );

    let token = Uuid::new_v4();
    let types = [type_a.as_str(), type_b.as_str()];
    let first: Option<(Uuid, Option<String>, String, String)> = sqlx::query_as(
        "SELECT id, tenant_id, aggregate_id, event_type FROM xtenant.outbox_claim($1, $2)",
    )
    .bind(&types[..])
    .bind(token)
    .fetch_optional(&mut *tx)
    .await
    .unwrap();
    let (id, tenant_id, aggregate_id, event_type) = first.expect("an event");
    assert_eq!((id, event_type.as_str()), (oldest, type_a.as_str()));
    assert_eq!(tenant_id, Some(tenant.to_string()));
    assert_eq!(aggregate_id, oldest.to_string());
    let row = outbox_row(&mut tx, oldest).await.unwrap();
    assert!(
        row.0 && row.1 == Some(token),
        "claimed with the caller's token"
    );
    assert_eq!(audit_rows(&mut tx, "outbox_claim", &type_a).await, 1);

    // The claimed event is not handed out again; the next one is.
    let second: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM xtenant.outbox_claim($1, $2)")
        .bind(&types[..])
        .bind(Uuid::new_v4())
        .fetch_optional(&mut *tx)
        .await
        .unwrap();
    assert_eq!(second.map(|r| r.0), Some(middle));
    let third: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM xtenant.outbox_claim($1, $2)")
        .bind(&types[..])
        .bind(Uuid::new_v4())
        .fetch_optional(&mut *tx)
        .await
        .unwrap();
    assert!(third.is_none(), "nothing left to claim: an idle poll");
    assert!(
        !outbox_row(&mut tx, unrelated).await.unwrap().0,
        "events of other types are not touched"
    );

    // Release puts it back; ack removes it.
    let released: bool = sqlx::query_scalar("SELECT xtenant.outbox_release($1)")
        .bind(oldest)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(released);
    assert!(!outbox_row(&mut tx, oldest).await.unwrap().0);
    assert_eq!(audit_rows(&mut tx, "outbox_release", &type_a).await, 1);
    let acked: bool = sqlx::query_scalar("SELECT xtenant.outbox_ack($1)")
        .bind(oldest)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(acked);
    assert!(outbox_row(&mut tx, oldest).await.is_none());
    assert_eq!(audit_rows(&mut tx, "outbox_ack", &type_a).await, 1);
    let again: bool = sqlx::query_scalar("SELECT xtenant.outbox_ack($1)")
        .bind(oldest)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(
        !again,
        "acknowledging a missing event is a no-op that writes no audit row"
    );
}

#[tokio::test]
async fn an_idle_poll_writes_no_audit_row() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let ty = unique_type();
    db::testing::enforce_rls(&mut tx, &["outbox_events", "audit_logs"]).await;
    db::testing::become_runtime(&mut tx).await;

    for _ in 0..3 {
        let none: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM xtenant.outbox_claim($1, $2)")
            .bind(&[ty.as_str()][..])
            .bind(Uuid::new_v4())
            .fetch_optional(&mut *tx)
            .await
            .unwrap();
        assert!(none.is_none());
    }
    assert_eq!(audit_rows(&mut tx, "outbox_claim", &ty).await, 0);
}

#[tokio::test]
async fn claims_are_audited_at_most_once_a_minute_per_event_type() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let ty = unique_type();
    for _ in 0..4 {
        seed_event(&mut tx, &ty, None, "now()").await;
    }
    db::testing::enforce_rls(&mut tx, &["outbox_events", "audit_logs"]).await;
    db::testing::become_runtime(&mut tx).await;

    for _ in 0..4 {
        let claimed: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM xtenant.outbox_claim($1, $2)")
                .bind(&[ty.as_str()][..])
                .bind(Uuid::new_v4())
                .fetch_optional(&mut *tx)
                .await
                .unwrap();
        assert!(claimed.is_some());
    }
    assert_eq!(
        audit_rows(&mut tx, "outbox_claim", &ty).await,
        1,
        "four claims of one event type within a minute write one audit row"
    );
}

#[tokio::test]
async fn claim_retryable_honours_backoff_counts_attempts_and_reschedule_needs_the_claim() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let ty = unique_type();
    let ready = seed_event(&mut tx, &ty, None, "now() - interval '2 minutes'").await;
    let backed_off = seed_event(&mut tx, &ty, None, "now() - interval '1 minute'").await;
    let dead = seed_event(&mut tx, &ty, None, "now() - interval '3 minutes'").await;
    sqlx::query("UPDATE outbox_events SET available_at = now() + interval '1 hour' WHERE id = $1")
        .bind(backed_off)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE outbox_events SET dead_lettered_at = now() WHERE id = $1")
        .bind(dead)
        .execute(&mut *tx)
        .await
        .unwrap();
    db::testing::enforce_rls(&mut tx, &["outbox_events", "audit_logs"]).await;
    db::testing::become_runtime(&mut tx).await;

    let token = Uuid::new_v4();
    let claimed: Option<(Uuid, i32)> =
        sqlx::query_as("SELECT id, attempts FROM xtenant.outbox_claim_retryable($1, $2)")
            .bind(&ty)
            .bind(token)
            .fetch_optional(&mut *tx)
            .await
            .unwrap();
    assert_eq!(
        claimed,
        Some((ready, 1)),
        "the ready event, with its attempt counted"
    );
    let none: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM xtenant.outbox_claim_retryable($1, $2)")
            .bind(&ty)
            .bind(Uuid::new_v4())
            .fetch_optional(&mut *tx)
            .await
            .unwrap();
    assert!(
        none.is_none(),
        "backed-off and dead-lettered events are skipped"
    );

    // Only the holder of the claim token can reschedule.
    let wrong: bool = sqlx::query_scalar(
        "SELECT xtenant.outbox_reschedule($1, $2, now() + interval '1 hour', 'x')",
    )
    .bind(ready)
    .bind(Uuid::new_v4())
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(!wrong);
    let right: bool = sqlx::query_scalar(
        "SELECT xtenant.outbox_reschedule($1, $2, now() + interval '1 hour', 'retriable: x')",
    )
    .bind(ready)
    .bind(token)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(right);
    let row = outbox_row(&mut tx, ready).await.unwrap();
    assert_eq!(
        (row.0, row.1, row.2, row.3.as_deref(), row.4),
        (false, None, 1, Some("retriable: x"), true),
        "released, attempt kept, error recorded, available later"
    );
}

#[tokio::test]
async fn coalesce_removes_only_older_unclaimed_customer_messages_of_that_conversation() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let conversation = Uuid::new_v4();
    let other_conversation = Uuid::new_v4();
    let mut ids = Vec::new();
    for (conv, age) in [
        (conversation, "3 minutes"),
        (conversation, "2 minutes"),
        (conversation, "1 minute"),
        (other_conversation, "3 minutes"),
    ] {
        let id = Uuid::new_v4();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
             VALUES ($1, 'conversation', $2, NULL, 'conversation.customer_message', \
                     jsonb_build_object('conversation_id', $3::text), now() - interval '{age}')"
        )))
        .bind(id)
        .bind(conv.to_string())
        .bind(conv.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
        ids.push(id);
    }
    db::testing::enforce_rls(&mut tx, &["outbox_events", "audit_logs"]).await;
    db::testing::become_runtime(&mut tx).await;

    // Keep the newest of the conversation: the two older ones go.
    let removed: i64 =
        sqlx::query_scalar("SELECT xtenant.outbox_coalesce_customer_messages($1, $2)")
            .bind(conversation)
            .bind(ids[2])
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(removed, 2);
    assert!(outbox_row(&mut tx, ids[0]).await.is_none());
    assert!(outbox_row(&mut tx, ids[1]).await.is_none());
    assert!(
        outbox_row(&mut tx, ids[2]).await.is_some(),
        "the kept event stays"
    );
    assert!(
        outbox_row(&mut tx, ids[3]).await.is_some(),
        "another conversation is untouched"
    );
}

#[tokio::test]
async fn the_whatsapp_sender_takes_the_oldest_outbound_event_and_removes_it() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let tenant = seed_tenant(&mut tx).await;
    let (conversation, message) = (Uuid::new_v4(), Uuid::new_v4());
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at) \
         VALUES ($1, 'message', $2, $3, 'whatsapp.outbound_message', \
                 jsonb_build_object('conversationId', $4::text, 'messageId', $5::text), 'epoch')",
    )
    .bind(id)
    .bind(message.to_string())
    .bind(tenant.to_string())
    .bind(conversation.to_string())
    .bind(message.to_string())
    .execute(&mut *tx)
    .await
    .unwrap();
    db::testing::enforce_rls(&mut tx, &["outbox_events", "audit_logs"]).await;
    db::testing::become_runtime(&mut tx).await;

    let taken: Option<(Uuid, Uuid, Uuid, Uuid)> = sqlx::query_as(
        "SELECT id, tenant_id, conversation_id, message_id FROM xtenant.outbox_take_whatsapp_outbound()",
    )
    .fetch_optional(&mut *tx)
    .await
    .unwrap();
    assert_eq!(taken, Some((id, tenant, conversation, message)));
    assert!(
        outbox_row(&mut tx, id).await.is_none(),
        "taking removes the event"
    );
}

#[tokio::test]
async fn invitation_delivery_runs_claim_retry_finalize_and_expiry_through_the_functions() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let tenant = seed_tenant(&mut tx).await;
    let inviter: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name) VALUES ($1, 'Inviter') RETURNING id",
    )
    .bind(format!("{}@example.test", Uuid::new_v4().simple()))
    .fetch_one(&mut *tx)
    .await
    .unwrap();

    // Two invitations, both waiting for delivery ('queued').
    let mut invitations = Vec::new();
    for _ in 0..2 {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO tenant_invitations (tenant_id, email, role, token_hash, invited_by, expires_at, email_delivery_status) \
             VALUES ($1, $2, 'agent', $3, $4, now() + interval '1 day', 'queued') RETURNING id",
        )
        .bind(tenant)
        .bind(format!("{}@example.test", Uuid::new_v4().simple()))
        .bind(Uuid::new_v4().simple().to_string())
        .bind(inviter)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        invitations.push(id);
    }
    let mut events = Vec::new();
    for (invitation, attempts, created) in [
        (invitations[0], 3, "'epoch'::timestamptz"),
        (
            invitations[1],
            0,
            "'epoch'::timestamptz + interval '1 second'",
        ),
    ] {
        let id = Uuid::new_v4();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO outbox_events (id, aggregate_type, aggregate_id, tenant_id, event_type, payload, created_at, attempts) \
             VALUES ($1, 'tenant_invitation', $2, $3, 'invitation.email_delivery', \
                     '{{\"to\": \"x@example.test\", \"accept_url\": \"https://example.test\"}}'::jsonb, {created}, $4)"
        )))
        .bind(id)
        .bind(invitation.to_string())
        .bind(tenant.to_string())
        .bind(attempts)
        .execute(&mut *tx)
        .await
        .unwrap();
        events.push(id);
    }
    db::testing::enforce_rls(
        &mut tx,
        &["outbox_events", "tenant_invitations", "audit_logs"],
    )
    .await;
    db::testing::become_runtime(&mut tx).await;

    // 1. The first event used all its attempts: it is dead-lettered and its invitation fails.
    let expired: bool =
        sqlx::query_scalar("SELECT xtenant.invitation_delivery_expire_exhausted(3)")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(expired);
    db::testing::become_platform(&mut tx).await;
    let (status, error): (String, Option<String>) = sqlx::query_as(
        "SELECT email_delivery_status, email_delivery_error FROM tenant_invitations WHERE id = $1",
    )
    .bind(invitations[0])
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        (status.as_str(), error.as_deref()),
        ("failed", Some("Max retries exceeded"))
    );
    let dead: bool =
        sqlx::query_scalar("SELECT dead_lettered_at IS NOT NULL FROM outbox_events WHERE id = $1")
            .bind(events[0])
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(dead);
    db::testing::become_runtime(&mut tx).await;
    let nothing_left: bool =
        sqlx::query_scalar("SELECT xtenant.invitation_delivery_expire_exhausted(3)")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(!nothing_left);

    // 2. Claim the second event: attempt counted, payload returned.
    let token = Uuid::new_v4();
    let claimed: Option<(Uuid, String, String, i32)> = sqlx::query_as(
        "SELECT id, aggregate_id, tenant_id, attempts FROM xtenant.invitation_delivery_claim($1, 3)",
    )
    .bind(token)
    .fetch_optional(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        claimed,
        Some((events[1], invitations[1].to_string(), tenant.to_string(), 1))
    );
    let none: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM xtenant.invitation_delivery_claim($1, 3)")
            .bind(Uuid::new_v4())
            .fetch_optional(&mut *tx)
            .await
            .unwrap();
    assert!(none.is_none(), "a claimed event is not handed out again");

    // 3. Retry needs the claim and schedules it a moment later.
    let wrong: bool = sqlx::query_scalar("SELECT xtenant.invitation_delivery_retry($1, $2, 'x')")
        .bind(events[1])
        .bind(Uuid::new_v4())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(!wrong);
    let retried: bool =
        sqlx::query_scalar("SELECT xtenant.invitation_delivery_retry($1, $2, 'smtp down')")
            .bind(events[1])
            .bind(token)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert!(retried);
    let row = outbox_row(&mut tx, events[1]).await.unwrap();
    assert_eq!(
        (row.0, row.1, row.3.as_deref()),
        (false, None, Some("smtp down"))
    );

    // 4. Claim again (after the one second back-off) and finalize as sent: the invitation follows.
    db::testing::become_platform(&mut tx).await;
    sqlx::query("UPDATE outbox_events SET available_at = now() WHERE id = $1")
        .bind(events[1])
        .execute(&mut *tx)
        .await
        .unwrap();
    db::testing::become_runtime(&mut tx).await;
    let token2 = Uuid::new_v4();
    let again: Option<(Uuid, i32)> =
        sqlx::query_as("SELECT id, attempts FROM xtenant.invitation_delivery_claim($1, 3)")
            .bind(token2)
            .fetch_optional(&mut *tx)
            .await
            .unwrap();
    assert_eq!(again, Some((events[1], 2)));
    let finalized: bool = sqlx::query_scalar(
        "SELECT xtenant.invitation_delivery_finalize($1, $2, NULL, $3, $4, 'sent', NULL)",
    )
    .bind(events[1])
    .bind(token2)
    .bind(invitations[1])
    .bind(tenant)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(finalized);
    db::testing::become_platform(&mut tx).await;
    let (processed, status): (bool, String) = sqlx::query_as(
        "SELECT o.processed_at IS NOT NULL, i.email_delivery_status \
         FROM outbox_events o, tenant_invitations i WHERE o.id = $1 AND i.id = $2",
    )
    .bind(events[1])
    .bind(invitations[1])
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!((processed, status.as_str()), (true, "sent"));
    db::testing::become_runtime(&mut tx).await;
    let stale: bool = sqlx::query_scalar(
        "SELECT xtenant.invitation_delivery_finalize($1, $2, NULL, NULL, NULL, NULL, NULL)",
    )
    .bind(events[1])
    .bind(Uuid::new_v4())
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(!stale, "a finalize without the claim does nothing");
    assert!(
        audit_rows(
            &mut tx,
            "invitation_delivery_finalize",
            "invitation.email_delivery"
        )
        .await
            >= 1
    );
}

#[tokio::test]
async fn pending_attachment_next_returns_the_oldest_pending_attachment() {
    if !can_run() {
        return;
    }
    let mut tx = db::testing::owner_tx().await;
    let tenant = seed_tenant(&mut tx).await;
    let customer: Uuid = sqlx::query_scalar(
        "INSERT INTO customers (tenant_id, display_name) VALUES ($1, 'C') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let conversation: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (tenant_id, customer_id, channel, status) VALUES ($1, $2, 'whatsapp', 'open') RETURNING id",
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
    let pending: Uuid = sqlx::query_scalar(
        "INSERT INTO message_attachments (tenant_id, message_id, kind, created_at) \
         VALUES ($1, $2, 'image', 'epoch') RETURNING id",
    )
    .bind(tenant)
    .bind(message)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO message_attachments (tenant_id, message_id, kind, fetch_attempts, created_at) \
         VALUES ($1, $2, 'image', 5, 'epoch')",
    )
    .bind(tenant)
    .bind(message)
    .execute(&mut *tx)
    .await
    .unwrap();
    db::testing::enforce_rls(&mut tx, &["message_attachments", "audit_logs"]).await;
    db::testing::become_runtime(&mut tx).await;

    let direct: i64 = sqlx::query_scalar("SELECT count(*) FROM message_attachments")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        direct, 0,
        "app_runtime must not read attachments directly without a tenant"
    );
    let next: Option<(Uuid, Uuid, i32)> = sqlx::query_as(
        "SELECT id, tenant_id, fetch_attempts FROM xtenant.pending_attachment_next()",
    )
    .fetch_optional(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        next,
        Some((pending, tenant, 0)),
        "the oldest pending one, not the exhausted one"
    );
    assert_eq!(
        audit_rows(&mut tx, "pending_attachment_next", &pending.to_string()).await,
        1
    );
}
