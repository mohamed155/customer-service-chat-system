//! Tenant privacy request persistence and claim fencing.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RequestStatus {
    pub request_id: Uuid,
    pub status: String,
}

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Export,
    Erase,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Export => "export",
            Self::Erase => "erase",
        }
    }

    fn request_action(self) -> &'static str {
        match self {
            Self::Export => "customer.export_requested",
            Self::Erase => "customer.erase_requested",
        }
    }
}

#[derive(Debug)]
pub enum RequestError {
    NotFound,
    Erasing,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for RequestError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

mod archive;
pub mod inventory;
pub mod routes;
mod worker;

pub use worker::{run, Notifications, Worker};

pub async fn request(
    pool: &PgPool,
    tenant: Uuid,
    customer: Uuid,
    actor: Uuid,
    kind: Kind,
) -> Result<RequestStatus, RequestError> {
    let mut work = db::UnitOfWork::begin_for_tenant(pool, tenant).await?;
    let erasing = customers::customer_privacy::lock_customer(work.conn(), tenant, customer)
        .await?
        .ok_or(RequestError::NotFound)?;
    if matches!(kind, Kind::Erase) {
        let existing: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT id, status FROM privacy_requests
             WHERE tenant_id = $1 AND customer_id = $2 AND kind = 'erase'",
        )
        .bind(tenant)
        .bind(customer)
        .fetch_optional(work.conn())
        .await?;
        if let Some((request_id, status)) = existing {
            work.commit().await?;
            return Ok(RequestStatus { request_id, status });
        }
        customers::customer_privacy::mark_pending(work.conn(), tenant, customer).await?;
        customers::customer_privacy::erase_customer(work.conn(), tenant, customer).await?;
    } else if erasing {
        return Err(RequestError::Erasing);
    }

    let request_id = kernel::ids::new_id();
    let export_key =
        matches!(kind, Kind::Export).then(|| format!("privacy-exports/{tenant}/{request_id}.zip"));
    // Persist the deterministic locator and cleanup deadline before any worker can upload.
    sqlx::query(
        "INSERT INTO privacy_requests (id, tenant_id, customer_id, requested_by, kind, object_key, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6,
                 CASE WHEN $5 = 'export' THEN now() + interval '24 hours' END)",
    )
    .bind(request_id)
    .bind(tenant)
    .bind(customer)
    .bind(actor)
    .bind(kind.as_str())
    .bind(export_key)
    .execute(work.conn())
    .await?;
    audit_log::record_in_tx(
        work.conn(),
        kind.request_action(),
        Some(actor),
        Some(tenant),
        "customer",
        Some(&customer.to_string()),
        &serde_json::json!({"request_id": request_id}),
    )
    .await?;
    work.commit().await?;
    Ok(RequestStatus {
        request_id,
        status: "queued".into(),
    })
}
