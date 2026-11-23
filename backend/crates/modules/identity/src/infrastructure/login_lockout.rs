use crate::application::login_lockout::{
    FailedLogin, LockoutEvent, LoginAccount, LoginAttempt, LoginLockoutStore, LoginReservation,
    NotificationClaim, NotificationError,
};
use crate::audit;
use crate::domain::login_lockout as domain;
use chrono::{DateTime, Utc};
use config::LoginLockoutLimits;
use serde_json::json;
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Commit the reservation before Argon2. Abandoned requests still count conservatively.
async fn begin_login(
    pool: &PgPool,
    email: &str,
    limits: LoginLockoutLimits,
) -> sqlx::Result<LoginAttempt> {
    let account: Option<LoginAccount> = sqlx::query_as(
        "SELECT id, email::text AS email, display_name, platform_role, password_hash,
                password_reset_at
         FROM users
         WHERE lower(email) = lower($1) AND deleted_at IS NULL",
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;

    let reservation = match account.as_ref() {
        Some(account) if account.password_hash.is_some() => {
            reserve_locked_account(pool, account.id, limits).await?
        }
        _ => {
            // Unknown and passwordless accounts share the no-op query on the pool before their dummy Argon2 verification.
            sqlx::query("UPDATE users SET failed_login_count = failed_login_count WHERE id = $1")
                .bind(Uuid::nil())
                .execute(pool)
                .await?;
            None
        }
    };

    Ok(LoginAttempt::new(account, reservation))
}

async fn reserve_locked_account(
    pool: &PgPool,
    user_id: Uuid,
    limits: LoginLockoutLimits,
) -> sqlx::Result<Option<LoginReservation>> {
    sqlx::query_as(
        "WITH prior AS MATERIALIZED (
             SELECT failed_login_count, last_failed_login_at, locked_until,
                    clock_timestamp() AS attempt_at
             FROM users
             WHERE id = $1 AND deleted_at IS NULL AND password_hash IS NOT NULL
             FOR UPDATE
         ), state AS (
             SELECT prior.*,
                    (last_failed_login_at IS NULL OR
                     last_failed_login_at <= attempt_at - ($2::integer * interval '1 second'))
                        AS reset_streak,
                    (locked_until IS NOT NULL AND locked_until <= attempt_at) AS expired_lock
             FROM prior
         ), desired AS (
             SELECT state.*,
                    CASE WHEN reset_streak THEN 1
                         ELSE LEAST(failed_login_count::bigint + 1, 2147483647)::integer
                    END AS next_count
             FROM state
         ), changed AS (
             UPDATE users AS account
             SET failed_login_count = desired.next_count,
                 last_failed_login_at = desired.attempt_at,
                 locked_until = CASE
                     WHEN desired.next_count >= $3 THEN
                         desired.attempt_at + (
                             LEAST(
                                 $5::numeric,
                                 $4::numeric * power(
                                     2::numeric,
                                     LEAST(desired.next_count - $3, $6)
                                 )
                             )::double precision * interval '1 second'
                         )
                     ELSE NULL
                 END
             FROM desired
             WHERE account.id = $1
               AND (desired.locked_until IS NULL OR desired.locked_until <= desired.attempt_at)
             RETURNING account.failed_login_count, account.locked_until
         )
         SELECT changed.failed_login_count, changed.locked_until, desired.expired_lock
         FROM changed CROSS JOIN desired",
    )
    .bind(user_id)
    .bind(limits.reset_after_secs)
    .bind(limits.threshold)
    .bind(limits.base_secs)
    .bind(limits.max_secs)
    .bind(domain::max_doublings(limits.base_secs, limits.max_secs))
    .fetch_optional(pool)
    .await
}

#[async_trait::async_trait]
impl LoginLockoutStore for PgPool {
    async fn begin_login(
        &self,
        email: &str,
        limits: LoginLockoutLimits,
    ) -> sqlx::Result<LoginAttempt> {
        begin_login(self, email, limits).await
    }

    async fn finish_failed(&self, failure: FailedLogin) -> sqlx::Result<()> {
        let mut transaction = self.begin().await?;
        let FailedLogin {
            user_id,
            email,
            reason,
            reservation,
            lockout_event,
            request_trace_id,
        } = failure;

        if let (Some(user_id), Some(reservation)) = (user_id, reservation.as_ref()) {
            if reservation.expired_lock {
                audit::record_in(
                    &mut *transaction,
                    "auth.account_unlocked",
                    Some(user_id),
                    Some(&user_id.to_string()),
                    &json!({ "reason": "expired" }),
                )
                .await?;
            }
        }

        audit::record_in(
            &mut *transaction,
            "auth.login_failed",
            None,
            Some(&email),
            &json!({ "email": email, "reason": reason }),
        )
        .await?;

        if let (Some(user_id), Some(event)) = (user_id, lockout_event) {
            record_account_locked(&mut transaction, user_id, &event).await?;
            if event.notify_owner {
                enqueue_notification(&mut transaction, user_id, &event, &request_trace_id).await?;
            }
        }

        transaction.commit().await
    }

    async fn finish_success(
        &self,
        account: LoginAccount,
        reservation: LoginReservation,
    ) -> sqlx::Result<bool> {
        let mut transaction = self.begin().await?;
        if reservation.expired_lock {
            audit::record_in(
                &mut *transaction,
                "auth.account_unlocked",
                Some(account.id),
                Some(&account.id.to_string()),
                &json!({ "reason": "expired" }),
            )
            .await?;
        }
        let cleared = sqlx::query_scalar::<_, Uuid>(
            "UPDATE users
             SET failed_login_count = 0, last_failed_login_at = NULL, locked_until = NULL
             WHERE id = $1 AND deleted_at IS NULL AND password_hash = $2
               AND password_reset_at IS NOT DISTINCT FROM $3
             RETURNING id",
        )
        .bind(account.id)
        .bind(&account.password_hash)
        .bind(account.password_reset_at)
        .fetch_optional(&mut *transaction)
        .await?
        .is_some();
        transaction.commit().await?;
        Ok(cleared)
    }
    async fn claim_notification(
        &self,
        smtp_configured: bool,
    ) -> Result<NotificationClaim, NotificationError> {
        claim_notification(self, smtp_configured).await
    }

    async fn finish_notification(&self, id: Uuid, attempts: i32) -> sqlx::Result<()> {
        finish(self, id, attempts).await
    }
}

async fn record_account_locked(
    transaction: &mut Transaction<'static, Postgres>,
    user_id: Uuid,
    event: &LockoutEvent,
) -> sqlx::Result<()> {
    audit::record_in(
        &mut **transaction,
        "auth.account_locked",
        None,
        Some(&user_id.to_string()),
        &json!({
            "failed_attempts": event.failed_attempts,
            "locked_until": event.locked_until,
            "lock_seconds": event.lock_seconds,
        }),
    )
    .await
}

async fn enqueue_notification(
    transaction: &mut Transaction<'static, Postgres>,
    user_id: Uuid,
    event: &LockoutEvent,
    request_trace_id: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO login_lockout_notifications
             (user_id, locked_until, failed_attempts, request_trace_id)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(user_id)
    .bind(event.locked_until)
    .bind(event.failed_attempts)
    .bind(request_trace_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct Notification {
    id: Uuid,
    user_id: Uuid,
    locked_until: DateTime<Utc>,
    failed_attempts: i32,
    attempts: i32,
    request_trace_id: String,
}

async fn claim(conn: &mut PgConnection) -> sqlx::Result<Option<Notification>> {
    sqlx::query_as(
        "SELECT id, user_id, locked_until, failed_attempts, attempts, request_trace_id
         FROM login_lockout_notifications
         WHERE available_at <= clock_timestamp()
         ORDER BY available_at, id LIMIT 1 FOR UPDATE SKIP LOCKED",
    )
    .fetch_optional(conn)
    .await
}

async fn finish<'c>(db: impl sqlx::PgExecutor<'c>, id: Uuid, attempts: i32) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM login_lockout_notifications WHERE id = $1 AND attempts = $2")
        .bind(id)
        .bind(attempts)
        .execute(db)
        .await?;
    Ok(())
}

async fn claim_notification(
    pool: &PgPool,
    smtp_configured: bool,
) -> Result<NotificationClaim, NotificationError> {
    const MAX_ATTEMPTS: i32 = 3;
    let mut tx = pool.begin().await?;
    let Some(notification) = claim(&mut tx).await? else {
        return Ok(NotificationClaim::Idle);
    };
    if notification.attempts >= MAX_ATTEMPTS || !smtp_configured {
        if !smtp_configured {
            tracing::warn!(request_id = %notification.request_trace_id, "login lockout notification SMTP is unconfigured");
        }
        finish(&mut *tx, notification.id, notification.attempts).await?;
        tx.commit().await?;
        return Ok(NotificationClaim::Discarded);
    }

    let recipient: Option<String> =
        sqlx::query_scalar("SELECT email::text FROM users WHERE id = $1 AND deleted_at IS NULL")
            .bind(notification.user_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(recipient) = recipient else {
        finish(&mut *tx, notification.id, notification.attempts).await?;
        tx.commit().await?;
        return Ok(NotificationClaim::Discarded);
    };

    let attempts = notification.attempts + 1;
    sqlx::query(
        "UPDATE login_lockout_notifications
         SET attempts = $2, available_at = clock_timestamp() + interval '1 minute'
         WHERE id = $1",
    )
    .bind(notification.id)
    .bind(attempts)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(NotificationClaim::Send {
        id: notification.id,
        request_trace_id: notification.request_trace_id,
        attempts,
        recipient,
        locked_until: notification.locked_until,
        failed_attempts: notification.failed_attempts,
    })
}

pub async fn queue_health(pool: &PgPool) -> sqlx::Result<(i64, f64)> {
    sqlx::query_as(
        "SELECT count(*),
                GREATEST(0, EXTRACT(EPOCH FROM clock_timestamp() - min(created_at)))::double precision
         FROM login_lockout_notifications",
    )
    .fetch_one(pool)
    .await
}
