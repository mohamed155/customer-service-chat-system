use crate::application::password_reset::{
    DeliveryClaim, DeliveryError, PasswordResetStore, ResetError, MAX_ATTEMPTS,
};
use crate::domain::password_reset as domain;
use crate::{audit, sessions};
use chrono::{DateTime, Utc};
use email::EmailMessage;
use sqlx::{PgConnection, PgPool};
use std::net::IpAddr;
use url::Url;
use uuid::Uuid;

pub async fn enqueue(
    pool: &PgPool,
    email: &str,
    ip: Option<IpAddr>,
    request_trace_id: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO password_reset_requests (email, ip, request_trace_id)
         VALUES ($1, $2::text::inet, $3)",
    )
    .bind(email)
    .bind(ip.map(|ip| ip.to_string()))
    .bind(request_trace_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct Request {
    id: Uuid,
    email: String,
    created_at: DateTime<Utc>,
    attempts: i32,
    stale: bool,
    request_trace_id: String,
}

async fn claim(conn: &mut PgConnection) -> sqlx::Result<Option<Request>> {
    sqlx::query_as(
        "SELECT id, email, created_at, attempts, request_trace_id,
                created_at <= clock_timestamp() - interval '30 minutes' AS stale
         FROM password_reset_requests
         WHERE available_at <= clock_timestamp()
         ORDER BY available_at, id LIMIT 1 FOR UPDATE SKIP LOCKED",
    )
    .fetch_optional(conn)
    .await
}

async fn delivery_account(
    conn: &mut PgConnection,
    request: &Request,
) -> sqlx::Result<Option<(Uuid, String)>> {
    // Passwordless users have no enabled password credential. A request from before a
    // completed reset cannot mint a new token for the old credential.
    sqlx::query_as(
        "SELECT id, email::text FROM users
         WHERE email = $1::citext AND deleted_at IS NULL AND password_hash IS NOT NULL
           AND (password_reset_at IS NULL OR password_reset_at < $2)
         FOR UPDATE",
    )
    .bind(&request.email)
    .bind(request.created_at)
    .fetch_optional(conn)
    .await
}

async fn superseded(conn: &mut PgConnection, user: Uuid, request: &Request) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM password_reset_tokens
             WHERE user_id = $1 AND (requested_at, request_id) > ($2, $3)
         ) OR EXISTS (
             SELECT 1 FROM password_reset_requests
             WHERE email::citext = $4::text::citext AND (created_at, id) > ($2, $3)
         )",
    )
    .bind(user)
    .bind(request.created_at)
    .bind(request.id)
    .bind(&request.email)
    .fetch_one(conn)
    .await
}

async fn issue(
    conn: &mut PgConnection,
    user: Uuid,
    request: &Request,
    hash: &str,
) -> sqlx::Result<()> {
    invalidate_tokens(conn, user).await?;
    sqlx::query(
        "INSERT INTO password_reset_tokens
            (user_id, request_id, requested_at, token_hash, created_at, expires_at, ip)
         SELECT $1, id, created_at, $2, issued_at, issued_at + interval '30 minutes', ip
         FROM password_reset_requests CROSS JOIN (SELECT clock_timestamp() AS issued_at) issued
         WHERE id = $3",
    )
    .bind(user)
    .bind(hash)
    .bind(request.id)
    .execute(&mut *conn)
    .await?;
    // A lease makes the request recoverable if this worker stops during SMTP delivery.
    sqlx::query(
        "UPDATE password_reset_requests
         SET attempts = attempts + 1, available_at = clock_timestamp() + interval '1 minute'
         WHERE id = $1",
    )
    .bind(request.id)
    .execute(conn)
    .await?;
    Ok(())
}

async fn finish<'c>(
    db: impl sqlx::PgExecutor<'c>,
    request_id: Uuid,
    attempts: i32,
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM password_reset_requests WHERE id = $1 AND attempts = $2")
        .bind(request_id)
        .bind(attempts)
        .execute(db)
        .await?;
    Ok(())
}

async fn eligible_user(pool: &PgPool, hash: &str) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "SELECT t.user_id FROM password_reset_tokens t JOIN users u ON u.id = t.user_id
         WHERE t.token_hash = $1 AND t.used_at IS NULL AND t.expires_at > clock_timestamp()
           AND u.deleted_at IS NULL AND u.password_hash IS NOT NULL
           AND NOT EXISTS (
               SELECT 1 FROM password_reset_requests r
               WHERE r.email::citext = u.email AND (r.created_at, r.id) > (t.requested_at, t.request_id)
           )",
    )
    .bind(hash)
    .fetch_optional(pool)
    .await
}

async fn lock_eligible_account(conn: &mut PgConnection, user: Uuid) -> sqlx::Result<bool> {
    let found: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM users WHERE id = $1 AND deleted_at IS NULL AND password_hash IS NOT NULL
         FOR UPDATE",
    )
    .bind(user)
    .fetch_optional(conn)
    .await?;
    Ok(found.is_some())
}

async fn lock_valid_token(conn: &mut PgConnection, user: Uuid, hash: &str) -> sqlx::Result<bool> {
    let found: Option<Uuid> = sqlx::query_scalar(
        "SELECT t.id FROM password_reset_tokens t JOIN users u ON u.id = t.user_id
         WHERE t.token_hash = $1 AND t.user_id = $2 AND t.used_at IS NULL
           AND t.expires_at > clock_timestamp()
           AND NOT EXISTS (
               SELECT 1 FROM password_reset_requests r
               WHERE r.email::citext = u.email AND (r.created_at, r.id) > (t.requested_at, t.request_id)
           )
         FOR UPDATE OF t",
    )
    .bind(hash)
    .bind(user)
    .fetch_optional(conn)
    .await?;
    Ok(found.is_some())
}

async fn change_password(conn: &mut PgConnection, user: Uuid, hash: &str) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE users SET password_hash = $1, password_reset_at = clock_timestamp() WHERE id = $2",
    )
    .bind(hash)
    .bind(user)
    .execute(&mut *conn)
    .await?;
    invalidate_tokens(conn, user).await
}

async fn invalidate_tokens(conn: &mut PgConnection, user: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE password_reset_tokens SET used_at = clock_timestamp()
         WHERE user_id = $1 AND used_at IS NULL",
    )
    .bind(user)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn queue_health(pool: &PgPool) -> sqlx::Result<(i64, f64)> {
    sqlx::query_as(
        "SELECT count(*),
                GREATEST(0, EXTRACT(EPOCH FROM now() - min(created_at)))::double precision
         FROM password_reset_requests",
    )
    .fetch_one(pool)
    .await
}

pub async fn sweep_tokens(pool: &PgPool) -> sqlx::Result<u64> {
    // Keep the issuance order of superseded tokens until their original expiry so a retry
    // for an older request cannot supersede a newer link.
    let removed = sqlx::query("DELETE FROM password_reset_tokens WHERE expires_at <= now()")
        .execute(pool)
        .await?;
    Ok(removed.rows_affected())
}

impl PasswordResetStore for PgPool {
    async fn eligible_user(&self, hash: &str) -> sqlx::Result<Option<Uuid>> {
        eligible_user(self, hash).await
    }

    async fn redeem(
        &self,
        user: Uuid,
        token_hash: &str,
        password_hash: &str,
    ) -> Result<(), ResetError> {
        let mut tx = self.begin().await?;
        // Issuance and redemption both lock the user first. This serializes changes across
        // multiple reset requests and establishes a consistent order for token/session locks.
        if !lock_eligible_account(&mut tx, user).await?
            || !lock_valid_token(&mut tx, user, token_hash).await?
        {
            return Err(ResetError::InvalidToken);
        }
        change_password(&mut tx, user, password_hash).await?;
        sessions::revoke_user_sessions_in(&mut tx, user, sessions::reason::PASSWORD_RESET, None)
            .await?;
        audit::record_in(
            &mut *tx,
            "auth.password_reset",
            Some(user),
            Some(&user.to_string()),
            &serde_json::json!({}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn claim_delivery(
        &self,
        smtp_configured: bool,
        dashboard_url: &Url,
    ) -> Result<DeliveryClaim, DeliveryError> {
        let mut tx = self.begin().await?;
        let Some(request) = claim(&mut tx).await? else {
            return Ok(DeliveryClaim::Idle);
        };

        let message = prepare_delivery(&mut tx, &request, smtp_configured, dashboard_url).await?;
        if message.is_none() {
            finish(&mut *tx, request.id, request.attempts).await?;
        }
        tx.commit().await?;

        Ok(match message {
            Some(message) => DeliveryClaim::Send {
                id: request.id,
                trace_id: request.request_trace_id,
                attempts: request.attempts + 1,
                message,
            },
            None => DeliveryClaim::Discarded,
        })
    }

    async fn finish_delivery(&self, id: Uuid, attempts: i32) -> sqlx::Result<()> {
        finish(self, id, attempts).await
    }
}

async fn prepare_delivery(
    conn: &mut PgConnection,
    request: &Request,
    smtp_configured: bool,
    dashboard_url: &Url,
) -> Result<Option<EmailMessage>, DeliveryError> {
    if request.attempts >= MAX_ATTEMPTS {
        return Ok(None);
    }
    let Some((user, to)) = delivery_account(conn, request).await? else {
        return Ok(None);
    };
    if superseded(conn, user, request).await? {
        return Ok(None);
    }
    if request.stale || !smtp_configured {
        // Once an accepted request supersedes a link, dropping the queue row must not
        // make the older link valid again, even when this request cannot be delivered.
        invalidate_tokens(conn, user).await?;
        if !smtp_configured {
            tracing::warn!(request_id = %request.request_trace_id, "password reset SMTP is unconfigured");
        }
        return Ok(None);
    }
    let token = domain::new_token().map_err(|_| DeliveryError::Randomness)?;
    let link = dashboard_url
        .join(&format!("reset-password/{token}"))
        .map_err(DeliveryError::DashboardUrl)?;
    issue(conn, user, request, &domain::sha256(&token)).await?;
    Ok(Some(EmailMessage {
        to,
        subject: "Reset your password".into(),
        body_text: format!(
            "Open this link within 30 minutes to reset your password:\n{link}\n\n\
             If you did not request this change, you can ignore this email."
        ),
        body_html: None,
    }))
}
