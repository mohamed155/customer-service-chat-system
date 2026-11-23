//! Audit-log — the one way to write an audit record (ADR-017, MOH-196)
//!
//! # Purpose
//! Sensitive actions write an audit record (Constitution III), and the record must commit with the
//! action: if the audit write fails, the action must not happen. That makes the write synchronous
//! and in the caller's transaction, so it cannot be an event the `audit` module consumes later, and
//! every module that records an action needs it. It lives in a shared crate, as the outbox does,
//! so that any module can use it without depending on `audit` (an event consumer that no module
//! may call, ADR-017 rule 4) and without a dependency cycle: `audit` depends on `tenancy`, which
//! writes audit records itself.
//!
//! The `audit` module still owns the table: it reads `audit_logs` for the audit API, and its
//! migrations define it. This crate only writes it, and is the only place outside `audit` whose
//! SQL names it.
//!
//! # Public Interfaces
//! - [`record_in_tx`] — writes the record in the caller's transaction and returns the error, so the
//!   caller rolls the action back with it. Use it for anything a person did.
//! - [`record`] — writes the record in a transaction of its own and only logs a failure. Use it
//!   for a record about something that has already happened and changes nothing that must stay
//!   consistent with it (a rate-limit refusal, a tool that ran).
//! - [`with_platform_access_session`] — retains the session that authorized a tenant request while
//!   its handler executes.
//!
//! # Rows
//! - A row for a tenant is inserted directly: Row-Level Security refuses it unless the transaction
//!   is that tenant's (`db::UnitOfWork::begin_for_tenant`), so one tenant cannot write another's.
//! - A row without a tenant is a platform-level event. Row-Level Security refuses a direct insert
//!   without a tenant, so it goes through `xtenant.record_platform_event`, which accepts only the
//!   platform actions it lists. (Authentication events use `xtenant.record_auth_event`, in
//!   `identity`.)
//! - The nil UUID as the actor means "the system" (the routing engine, a presence timeout). The
//!   `actor_user_id` column references `users`, so it is stored as NULL.
//!
//! # Dependencies
//! - `db` (the tenant Unit of Work), `sqlx`, `tokio` (request scope), `tracing`. Never a module.

use sqlx::{PgConnection, PgPool};
use std::future::Future;
use tracing::error;
use uuid::Uuid;

struct PlatformAccessLink {
    actor_id: Uuid,
    tenant_id: Uuid,
    session_id: Uuid,
}

tokio::task_local! {
    static PLATFORM_ACCESS_LINK: PlatformAccessLink;
}

/// Runs a tenant request under its validated access grant, so in-flight writes keep their original
/// attribution if the platform user ends or replaces access while the request is executing.
pub async fn with_platform_access_session<F: Future>(
    actor_id: Uuid,
    tenant_id: Uuid,
    session_id: Uuid,
    request: F,
) -> F::Output {
    PLATFORM_ACCESS_LINK
        .scope(
            PlatformAccessLink {
                actor_id,
                tenant_id,
                session_id,
            },
            request,
        )
        .await
}

/// Writes an audit record in the caller's transaction.
///
/// `conn` is the caller's open transaction (a `Transaction`, or a `UnitOfWork`, derefs to it). The
/// row commits or rolls back with it. The error is returned, not logged: a caller that records a
/// sensitive action must roll the action back when the record cannot be written.
pub async fn record_in_tx(
    conn: &mut PgConnection,
    action: &str,
    actor_user_id: Option<Uuid>,
    tenant_id: Option<Uuid>,
    resource_type: &str,
    resource_id: Option<&str>,
    details: &serde_json::Value,
) -> sqlx::Result<()> {
    let actor_user_id = actor_user_id.filter(|id| !id.is_nil());
    match tenant_id {
        Some(tenant_id) => {
            insert_tenant_row(
                conn,
                action,
                actor_user_id,
                tenant_id,
                resource_type,
                resource_id,
                details,
            )
            .await
        }
        None => {
            insert_platform_event(
                conn,
                action,
                actor_user_id,
                resource_type,
                resource_id,
                details,
            )
            .await
        }
    }
}

/// Writes an audit record in a transaction of its own and logs a failure instead of returning it.
///
/// For a record about something that has already happened and changes nothing that must stay
/// consistent with it. Anything a person did belongs in [`record_in_tx`].
pub async fn record(
    pool: &PgPool,
    action: &str,
    actor_user_id: Option<Uuid>,
    tenant_id: Option<Uuid>,
    resource_type: &str,
    resource_id: Option<&str>,
    details: &serde_json::Value,
) {
    // A row for a tenant is written inside that tenant's Unit of Work (app.tenant_id set); a row
    // without one is a platform-level event and needs no transaction of its own.
    let result: sqlx::Result<()> = async {
        match tenant_id {
            Some(tenant) => {
                let mut uow = db::UnitOfWork::begin_for_tenant(pool, tenant).await?;
                record_in_tx(
                    uow.conn(),
                    action,
                    actor_user_id,
                    tenant_id,
                    resource_type,
                    resource_id,
                    details,
                )
                .await?;
                uow.commit().await
            }
            None => {
                let mut conn = pool.acquire().await?;
                record_in_tx(
                    &mut conn,
                    action,
                    actor_user_id,
                    None,
                    resource_type,
                    resource_id,
                    details,
                )
                .await
            }
        }
    }
    .await;

    if let Err(e) = result {
        error!(
            action = %action,
            resource_type = %resource_type,
            error = %e,
            "failed to record audit log entry"
        );
    }
}

async fn insert_tenant_row(
    conn: &mut PgConnection,
    action: &str,
    actor_user_id: Option<Uuid>,
    tenant_id: Uuid,
    resource_type: &str,
    resource_id: Option<&str>,
    details: &serde_json::Value,
) -> sqlx::Result<()> {
    let access_session_id = PLATFORM_ACCESS_LINK
        .try_with(|link| {
            (actor_user_id == Some(link.actor_id) && tenant_id == link.tenant_id)
                .then_some(link.session_id)
        })
        .ok()
        .flatten();
    sqlx::query(
        "INSERT INTO audit_logs (
             actor_user_id, action, resource_type, resource_id, tenant_id, details,
             platform_access_session_id
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(actor_user_id)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(tenant_id)
    .bind(details)
    .bind(access_session_id)
    .execute(conn)
    .await
    .map(|_| ())
}

/// Writes an audit row that belongs to no tenant, through the audited function that accepts only
/// the platform-level actions (`xtenant.record_platform_event`).
async fn insert_platform_event(
    conn: &mut PgConnection,
    action: &str,
    actor_user_id: Option<Uuid>,
    resource_type: &str,
    resource_id: Option<&str>,
    details: &serde_json::Value,
) -> sqlx::Result<()> {
    sqlx::query("SELECT xtenant.record_platform_event($1, $2, $3, $4, $5)")
        .bind(action)
        .bind(actor_user_id)
        .bind(resource_type)
        .bind(resource_id)
        .bind(details)
        .execute(conn)
        .await
        .map(|_| ())
}
