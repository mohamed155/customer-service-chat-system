use std::{collections::HashSet, time::Duration};

use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::{postgres::PgRow, FromRow, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    application::platform_access::{AccessSession, AccessTerms, SelectedTenant},
    domain::{access_denial, AccessDenial},
    PlatformAccessGrant,
};

impl FromRow<'_, PgRow> for AccessSession {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            user_id: row.try_get("user_id")?,
            tenant_id: row.try_get("tenant_id")?,
            reason: row.try_get("reason")?,
            started_at: row.try_get("started_at")?,
            expires_at: row.try_get("expires_at")?,
            end_reason: row.try_get("end_reason")?,
        })
    }
}

impl FromRow<'_, PgRow> for SelectedTenant {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            slug: row.try_get("slug")?,
            status: row.try_get("status")?,
            plan: row.try_get("plan")?,
        })
    }
}

pub(crate) enum StartError {
    TenantUnavailable,
    StartInProgress,
    Storage(sqlx::Error),
}

impl From<sqlx::Error> for StartError {
    fn from(error: sqlx::Error) -> Self {
        Self::Storage(error)
    }
}

pub(crate) async fn start(
    pool: &PgPool,
    user_id: Uuid,
    tenant_id: Uuid,
    auth_session_id: Uuid,
    terms: AccessTerms<'_>,
    ip: Option<&str>,
) -> Result<(SelectedTenant, AccessSession), StartError> {
    let mut tx = pool.begin().await?;
    let tenant = sqlx::query_as::<_, SelectedTenant>(
        "SELECT id, name, slug, status, plan FROM tenants
         WHERE id = $1 AND deleted_at IS NULL FOR SHARE",
    )
    .bind(tenant_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(tenant) = tenant else {
        return Err(StartError::TenantUnavailable);
    };

    // Serialize overlapping starts, even before a user's first session has a row to lock.
    // The partial unique index independently prevents two open rows.
    let locked: bool = sqlx::query_scalar(
        "SELECT pg_try_advisory_xact_lock(hashtextextended('platform_access:' || $1::uuid::text, 0))",
    )
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;
    if !locked {
        return Err(StartError::StartInProgress);
    }

    let open = open_ids(&mut tx, user_id, None).await?;
    close(&mut tx, &open, EndReason::Replaced).await?;

    let session = sqlx::query_as::<_, AccessSession>(
        "INSERT INTO platform_access_sessions
            (user_id, tenant_id, auth_session_id, reason, started_at, expires_at, ip)
         VALUES ($1, $2, $3, $4, statement_timestamp(),
                 statement_timestamp() + make_interval(mins => $5), $6::text::inet)
         RETURNING id, user_id, tenant_id, reason, started_at, expires_at, end_reason",
    )
    .bind(user_id)
    .bind(tenant_id)
    .bind(auth_session_id)
    .bind(terms.reason)
    .bind(terms.duration_minutes)
    .bind(ip)
    .fetch_one(&mut *tx)
    .await?;
    record_lifecycle(&mut tx, &session).await?;
    tx.commit().await?;
    Ok((tenant, session))
}

/// Uses the most recent row for this tenant and sign-in. An expired row still reports expiry
/// after a sweep closes it. A later explicit end or a replacement takes precedence.
pub(crate) async fn access_status(
    pool: &PgPool,
    user_id: Uuid,
    tenant_id: Uuid,
    auth_session_id: Uuid,
) -> sqlx::Result<Result<PlatformAccessGrant, AccessDenial>> {
    let row = sqlx::query_as::<_, (Uuid, Option<String>, DateTime<Utc>, DateTime<Utc>, bool)>(
        "SELECT s.id, s.end_reason, s.expires_at, statement_timestamp(),
                CASE WHEN s.end_reason = 'expired' THEN EXISTS (
                    SELECT 1 FROM platform_access_sessions other
                    WHERE other.user_id = $1 AND other.ended_at IS NULL
                ) ELSE false END
         FROM platform_access_sessions s
         WHERE s.user_id = $1 AND s.tenant_id = $2 AND s.auth_session_id = $3
         ORDER BY s.started_at DESC, s.id DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(tenant_id)
    .bind(auth_session_id)
    .fetch_optional(pool)
    .await?;
    let Some((id, end_reason, expires_at, observed_at, another_is_open)) = row else {
        return Ok(Err(AccessDenial::Required));
    };
    if let Some(denial) = access_denial(
        end_reason.as_deref(),
        expires_at,
        observed_at,
        another_is_open,
    ) {
        if denial == AccessDenial::Expired && end_reason.is_none() {
            expire(pool, id).await?;
        }
        return Ok(Err(denial));
    }
    Ok(Ok(PlatformAccessGrant {
        id,
        expires_at,
        auth_session_id,
    }))
}

impl PlatformAccessGrant {
    /// Revalidates the exact access and sign-in that authorized a long-lived response.
    /// A database failure closes the response, just like a revoked session.
    pub async fn is_current(self, pool: &PgPool) -> bool {
        let active = async {
            let open: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM platform_access_sessions
                 WHERE id = $1 AND ended_at IS NULL AND expires_at > statement_timestamp())",
            )
            .bind(self.id)
            .fetch_one(pool)
            .await?;
            if !open {
                return Ok(false);
            }
            let sign_ins =
                identity::sessions::active_session_ids(pool, &[self.auth_session_id]).await?;
            Ok::<_, sqlx::Error>(!sign_ins.is_empty())
        };
        match active.await {
            Ok(current) => current,
            Err(error) => {
                tracing::warn!(%error, "Failed to revalidate platform tenant access");
                false
            }
        }
    }

    /// Wakes an idle stream when its platform access ends. Each outgoing event separately
    /// revalidates the grant before sending, so the poll interval permits no event delivery.
    pub async fn wait_for_end(self, pool: PgPool) {
        let remaining = (self.expires_at - Utc::now()).to_std().unwrap_or_default();
        let checks = async {
            let mut interval = tokio::time::interval(Duration::from_secs(2));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if !self.is_current(&pool).await {
                    break;
                }
            }
        };
        let _ = tokio::time::timeout(remaining, checks).await;
    }
}

pub(crate) async fn current(
    pool: &PgPool,
    user_id: Uuid,
    auth_session_id: Uuid,
) -> sqlx::Result<Option<(String, AccessSession)>> {
    let row = sqlx::query(
        "SELECT s.id, s.user_id, s.tenant_id, s.reason, s.started_at, s.expires_at, s.end_reason,
                s.expires_at <= statement_timestamp() AS elapsed, t.name AS tenant_name
         FROM platform_access_sessions s JOIN tenants t ON t.id = s.tenant_id
         WHERE s.user_id = $1 AND s.auth_session_id = $2 AND s.ended_at IS NULL
           AND t.deleted_at IS NULL",
    )
    .bind(user_id)
    .bind(auth_session_id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let session = AccessSession::from_row(&row)?;
    if row.try_get("elapsed")? {
        expire(pool, session.id).await?;
        return Ok(None);
    }
    Ok(Some((row.try_get("tenant_name")?, session)))
}

pub(crate) async fn end(pool: &PgPool, user_id: Uuid, auth_session_id: Uuid) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    let open = open_ids(&mut tx, user_id, Some(auth_session_id)).await?;
    close(&mut tx, &open, EndReason::Ended).await?;
    tx.commit().await
}

pub async fn sweep(pool: &PgPool) -> sqlx::Result<usize> {
    let open = sqlx::query_as::<_, (Uuid, Uuid, bool)>(
        "SELECT id, auth_session_id, expires_at <= statement_timestamp()
         FROM platform_access_sessions WHERE ended_at IS NULL",
    )
    .fetch_all(pool)
    .await?;
    let auth_sessions: Vec<_> = open
        .iter()
        .filter_map(|&(_, sid, elapsed)| (!elapsed).then_some(sid))
        .collect();
    let active: HashSet<_> = identity::sessions::active_session_ids(pool, &auth_sessions)
        .await?
        .into_iter()
        .collect();
    let ended: Vec<_> = open
        .into_iter()
        .filter_map(|(id, sid, elapsed)| (elapsed || !active.contains(&sid)).then_some(id))
        .collect();
    if ended.is_empty() {
        return Ok(0);
    }

    let mut tx = pool.begin().await?;
    let count = close(&mut tx, &ended, EndReason::SignedOut).await?;
    tx.commit().await?;
    Ok(count)
}

#[derive(Clone, Copy)]
enum EndReason {
    Ended,
    Expired,
    Replaced,
    SignedOut,
}

impl EndReason {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Ended => "ended",
            Self::Expired => "expired",
            Self::Replaced => "replaced",
            Self::SignedOut => "signed_out",
        }
    }
}

async fn expire(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    close(&mut tx, &[id], EndReason::Expired).await?;
    tx.commit().await
}

async fn open_ids(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    auth_session_id: Option<Uuid>,
) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar(
        "SELECT id FROM platform_access_sessions WHERE user_id = $1 AND ended_at IS NULL
         AND ($2::uuid IS NULL OR auth_session_id = $2)",
    )
    .bind(user_id)
    .bind(auth_session_id)
    .fetch_all(&mut **tx)
    .await
}

async fn close(
    tx: &mut Transaction<'_, Postgres>,
    ids: &[Uuid],
    requested_reason: EndReason,
) -> sqlx::Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }

    // Explicit end, replacement, a request, and the sweeper can race. Only the transaction
    // that transitions a still-open row records the end, and expiry wins once time has elapsed.
    let closed = sqlx::query_as::<_, AccessSession>(
        "UPDATE platform_access_sessions
         SET ended_at = statement_timestamp(),
             end_reason = CASE WHEN expires_at <= statement_timestamp() THEN 'expired' ELSE $2 END
         WHERE id = ANY($1) AND ended_at IS NULL
           AND ($2 <> 'expired' OR expires_at <= statement_timestamp())
         RETURNING id, user_id, tenant_id, reason, started_at, expires_at, end_reason",
    )
    .bind(ids)
    .bind(requested_reason.as_str())
    .fetch_all(&mut **tx)
    .await?;
    for session in &closed {
        record_lifecycle(tx, session).await?;
    }
    Ok(closed.len())
}

async fn record_lifecycle(
    tx: &mut Transaction<'_, Postgres>,
    session: &AccessSession,
) -> sqlx::Result<()> {
    let action = match session.end_reason.as_deref() {
        None => "platform.tenant_access_started",
        Some("expired") => "platform.tenant_access_expired",
        Some(_) => "platform.tenant_access_ended",
    };
    let mut details = json!({
        "session_id": session.id,
        "reason": session.reason,
        "started_at": session.started_at,
        "expires_at": session.expires_at,
    });
    if let Some(end_reason) = &session.end_reason {
        details["end_reason"] = json!(end_reason);
    }
    db::set_tenant_in_tx(tx, session.tenant_id).await?;
    audit_log::record_in_tx(
        tx,
        action,
        Some(session.user_id),
        Some(session.tenant_id),
        "platform_access_session",
        Some(&session.id.to_string()),
        &details,
    )
    .await
}
