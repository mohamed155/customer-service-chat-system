use std::sync::Arc;
use std::time::Duration;

use sqlx::{PgConnection, PgPool};
use storage::{ObjectStorage, StorageError};
use uuid::Uuid;

use crate::archive::Archive;

#[async_trait::async_trait]
pub trait Notifications: Send + Sync {
    async fn export_customer(
        &self,
        conn: &mut PgConnection,
        tenant: Uuid,
        conversations: &[Uuid],
        export: &mut dyn db::json_export::JsonExport,
    ) -> sqlx::Result<()>;

    async fn erase_customer(
        &self,
        conn: &mut PgConnection,
        tenant: Uuid,
        conversations: &[Uuid],
    ) -> sqlx::Result<()>;
}

#[derive(Clone)]
pub struct Worker {
    pub pool: PgPool,
    pub storage: Arc<dyn ObjectStorage>,
    pub cache: Arc<cache::Cache>,
    pub notifications: Arc<dyn Notifications>,
}

#[derive(sqlx::FromRow)]
struct Request {
    id: Uuid,
    tenant_id: Uuid,
    customer_id: Uuid,
    requested_by: Uuid,
    kind: String,
    object_key: Option<String>,
    expired: bool,
}

#[derive(Debug, thiserror::Error)]
enum Failure {
    #[error("database error")]
    Database(#[from] sqlx::Error),
    #[error("storage error")]
    Storage(#[from] StorageError),
    #[error("archive error")]
    Archive(#[from] std::io::Error),
    #[error("customer has an active erasure request")]
    Erasing,
    #[error("cache cleanup failed")]
    Cache,
}

impl Failure {
    fn code(&self) -> &'static str {
        match self {
            Self::Database(sqlx::Error::Io(error)) | Self::Archive(error)
                if error.kind() == std::io::ErrorKind::FileTooLarge =>
            {
                "archive_too_large"
            }
            Self::Database(_) => "database_unavailable",
            Self::Storage(_) => "storage_unavailable",
            Self::Archive(_) => "archive_unavailable",
            Self::Erasing => "customer_erasing",
            Self::Cache => "cache_unavailable",
        }
    }
}

pub async fn run(worker: Worker) {
    let mut ticks = tokio::time::interval(Duration::from_secs(1));
    loop {
        ticks.tick().await;
        if let Err(error) = worker.process_one().await {
            tracing::warn!(%error, "privacy queue polling failed");
        }
    }
}

impl Worker {
    pub async fn process_one(&self) -> sqlx::Result<bool> {
        // uuid-v4-ok: opaque lease claim token
        let token = Uuid::new_v4();
        let claimed: Option<(Uuid, Uuid)> =
            sqlx::query_as("SELECT tenant_id, request_id FROM xtenant.privacy_claim($1)")
                .bind(token)
                .fetch_optional(&self.pool)
                .await?;
        let Some((tenant, id)) = claimed else {
            return Ok(false);
        };
        if let Err(error) = self.process(tenant, id, token).await {
            let mut work = db::UnitOfWork::begin_for_tenant(&self.pool, tenant).await?;
            sqlx::query(
                "UPDATE privacy_requests SET status = 'failed', failure_code = $4,
                    available_at = now() + interval '30 seconds', lease_token = NULL,
                    lease_until = NULL, updated_at = now()
                 WHERE tenant_id = $1 AND id = $2 AND lease_token = $3",
            )
            .bind(tenant)
            .bind(id)
            .bind(token)
            .bind(error.code())
            .execute(work.conn())
            .await?;
            work.commit().await?;
        }
        Ok(true)
    }

    async fn process(&self, tenant: Uuid, id: Uuid, token: Uuid) -> Result<(), Failure> {
        let mut work = db::UnitOfWork::begin_for_tenant(&self.pool, tenant).await?;
        let Some(request) = lock_request(work.conn(), tenant, id, token).await? else {
            return Ok(());
        };
        if request.expired {
            if let Some(key) = &request.object_key {
                delete_object(self.storage.as_ref(), key).await?;
            }
            set_result(work.conn(), &request, token, "expired", None).await?;
            work.commit().await?;
            return Ok(());
        }
        if request.kind == "export" {
            self.export(work.conn(), &request, token).await?;
            work.commit().await?;
            return Ok(());
        }
        work.commit().await?;

        for step in [
            "identifiers",
            "content",
            "attachments",
            "exports",
            "finalize",
        ] {
            let mut work = db::UnitOfWork::begin_for_tenant(&self.pool, tenant).await?;
            let Some(request) = lock_request(work.conn(), tenant, id, token).await? else {
                return Ok(());
            };
            let started = sqlx::query(
                "INSERT INTO privacy_request_steps (tenant_id, request_id, step_name, status, attempts)
                 VALUES ($1, $2, $3, 'running', 1)
                 ON CONFLICT (tenant_id, request_id, step_name) DO UPDATE SET
                    status = 'running', attempts = privacy_request_steps.attempts + 1, updated_at = now()
                 WHERE privacy_request_steps.status <> 'completed'",
            )
            .bind(tenant).bind(id).bind(step)
            .execute(work.conn()).await?.rows_affected() != 0;
            if started {
                sqlx::query("SAVEPOINT privacy_step")
                    .execute(work.conn())
                    .await?;
                if let Err(error) = self.erase_step(work.conn(), &request, token, step).await {
                    sqlx::query("ROLLBACK TO SAVEPOINT privacy_step")
                        .execute(work.conn())
                        .await?;
                    sqlx::query(
                        "UPDATE privacy_request_steps SET status = 'failed', updated_at = now()
                         WHERE tenant_id = $1 AND request_id = $2 AND step_name = $3",
                    )
                    .bind(tenant)
                    .bind(id)
                    .bind(step)
                    .execute(work.conn())
                    .await?;
                    work.commit().await?;
                    return Err(error);
                }
                sqlx::query(
                    "UPDATE privacy_request_steps SET status = 'completed', updated_at = now()
                     WHERE tenant_id = $1 AND request_id = $2 AND step_name = $3",
                )
                .bind(tenant)
                .bind(id)
                .bind(step)
                .execute(work.conn())
                .await?;
            }
            work.commit().await?;
        }
        Ok(())
    }

    async fn erase_step(
        &self,
        conn: &mut PgConnection,
        request: &Request,
        token: Uuid,
        step: &str,
    ) -> Result<(), Failure> {
        let tenant = request.tenant_id;
        let customer = request.customer_id;
        match step {
            "identifiers" => {
                customers::customer_privacy::erase_customer(conn, tenant, customer).await?;
                let sessions =
                    widgets::customer_privacy::erase_customer(conn, tenant, customer).await?;
                for session in sessions {
                    self.cache
                        .delete_by_prefix(&format!("rl:widget_message:{session}:"))
                        .await
                        .map_err(|_| Failure::Cache)?;
                }
            }
            "content" => {
                let conversations =
                    conversations::customer_privacy::conversation_ids(conn, tenant, customer)
                        .await?;
                let ids = &conversations;
                self.notifications.erase_customer(conn, tenant, ids).await?;
                let messages =
                    conversations::customer_privacy::message_ids(conn, tenant, ids).await?;
                integrations::customer_privacy::erase_customer(
                    conn, tenant, customer, ids, &messages,
                )
                .await?;
                let escalations =
                    escalations::customer_privacy::subject_ids(conn, tenant, ids).await?;
                let tools = tools::customer_privacy::subject_ids(conn, tenant, ids).await?;
                cancel_events(conn, tenant, customer, ids, &messages, &escalations, &tools).await?;
                conversations::customer_privacy::erase_customer(conn, tenant, ids).await?;
                ai::customer_privacy::erase_customer(conn, tenant, ids).await?;
                escalations::customer_privacy::erase_customer(conn, tenant, ids).await?;
                feedback::customer_privacy::erase_customer(conn, tenant, ids).await?;
                tools::customer_privacy::erase_customer(conn, tenant, ids).await?;
                whatsapp::customer_privacy::erase_customer(conn, tenant, ids).await?;
            }
            "attachments" => {
                let ids = conversations::customer_privacy::conversation_ids(conn, tenant, customer)
                    .await?;
                for item in conversations::customer_privacy::attachments(conn, tenant, &ids).await?
                {
                    let key = attachment_key(tenant, &item)?;
                    delete_object(self.storage.as_ref(), &key).await?;
                    conversations::customer_privacy::delete_attachment(conn, tenant, item.id)
                        .await?;
                }
            }
            "exports" => {
                let objects: Vec<(Uuid, Option<String>)> = sqlx::query_as(
                    "SELECT id, object_key FROM privacy_requests
                     WHERE tenant_id = $1 AND customer_id = $2 AND kind = 'export' FOR UPDATE",
                )
                .bind(tenant)
                .bind(customer)
                .fetch_all(&mut *conn)
                .await?;
                for (id, key) in objects {
                    let key = key.unwrap_or_else(|| format!("privacy-exports/{tenant}/{id}.zip"));
                    delete_object(self.storage.as_ref(), &key).await?;
                }
                sqlx::query(
                    "UPDATE privacy_requests SET status = 'expired', object_key = NULL,
                          lease_token = NULL, lease_until = NULL, updated_at = now()
                     WHERE tenant_id = $1 AND customer_id = $2 AND kind = 'export'",
                )
                .bind(tenant)
                .bind(customer)
                .execute(conn)
                .await?;
            }
            "finalize" => {
                customers::customer_privacy::finalize_erasure(conn, tenant, customer).await?;
                complete(conn, request, token, None).await?;
            }
            _ => {
                return Err(Failure::Database(sqlx::Error::Protocol(
                    "unknown privacy step".into(),
                )))
            }
        }
        Ok(())
    }

    async fn export(
        &self,
        conn: &mut PgConnection,
        request: &Request,
        token: Uuid,
    ) -> Result<(), Failure> {
        let tenant = request.tenant_id;
        let customer = request.customer_id;
        if !customers::customer_privacy::is_active(conn, tenant, customer).await? {
            return Err(Failure::Erasing);
        }
        let ids = conversations::customer_privacy::conversation_ids(conn, tenant, customer).await?;
        let mut archive = Archive::new()?;
        customers::customer_privacy::export_customer(conn, tenant, customer, &mut archive).await?;
        conversations::customer_privacy::export_customer(conn, tenant, &ids, &mut archive).await?;
        ai::customer_privacy::export_customer(conn, tenant, &ids, &mut archive).await?;
        escalations::customer_privacy::export_customer(conn, tenant, &ids, &mut archive).await?;
        feedback::customer_privacy::export_customer(conn, tenant, &ids, &mut archive).await?;
        let messages = conversations::customer_privacy::message_ids(conn, tenant, &ids).await?;
        integrations::customer_privacy::export_customer(
            conn,
            tenant,
            customer,
            &ids,
            &messages,
            &mut archive,
        )
        .await?;
        tools::customer_privacy::export_customer(conn, tenant, &ids, &mut archive).await?;
        whatsapp::customer_privacy::export_customer(conn, tenant, &ids, &mut archive).await?;
        widgets::customer_privacy::export_customer(conn, tenant, customer, &mut archive).await?;
        self.notifications
            .export_customer(conn, tenant, &ids, &mut archive)
            .await?;
        let mut attachments = Vec::new();
        for item in conversations::customer_privacy::attachments(conn, tenant, &ids).await? {
            if item.storage_key.is_some() {
                let key = attachment_key(tenant, &item)?;
                let path = format!("attachments/{}", item.id);
                archive
                    .write_attachment(&path, self.storage.get_reader(&key).await?)
                    .await?;
                attachments
                    .push(serde_json::json!({"path": path, "message_id": item.message_id, "original_filename": item.file_name}));
            }
        }
        archive.write_json(
            "manifest.json",
            &[serde_json::json!({"version": 1, "attachments": attachments})],
        )?;
        archive.finish()?;
        let key = format!("privacy-exports/{tenant}/{}.zip", request.id);
        self.storage
            .put_file(&key, "application/zip", &archive.path)
            .await?;
        if customers::customer_privacy::lock_customer(conn, tenant, customer).await? != Some(false)
        {
            delete_object(self.storage.as_ref(), &key).await?;
            return Err(Failure::Erasing);
        }
        if let Err(error) = complete(conn, request, token, Some(&key)).await {
            delete_object(self.storage.as_ref(), &key).await?;
            return Err(error.into());
        }
        Ok(())
    }
}

async fn lock_request(
    conn: &mut PgConnection,
    tenant: Uuid,
    id: Uuid,
    token: Uuid,
) -> sqlx::Result<Option<Request>> {
    sqlx::query_as(
        "SELECT id, tenant_id, customer_id, requested_by, kind, object_key,
                COALESCE(expires_at <= now(), false) AS expired FROM privacy_requests
         WHERE tenant_id = $1 AND id = $2 AND lease_token = $3 AND status = 'running'
         FOR UPDATE",
    )
    .bind(tenant)
    .bind(id)
    .bind(token)
    .fetch_optional(conn)
    .await
}

async fn set_result(
    conn: &mut PgConnection,
    req: &Request,
    token: Uuid,
    status: &str,
    key: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE privacy_requests SET status = $4, object_key = $5,
              expires_at = CASE WHEN $5 IS NULL THEN expires_at ELSE now() + interval '24 hours' END,
              failure_code = NULL, lease_token = NULL, lease_until = NULL, updated_at = now()
         WHERE tenant_id = $1 AND id = $2 AND lease_token = $3",
    )
    .bind(req.tenant_id).bind(req.id).bind(token).bind(status).bind(key).execute(conn).await?;
    Ok(())
}

async fn complete(
    conn: &mut PgConnection,
    req: &Request,
    token: Uuid,
    key: Option<&str>,
) -> sqlx::Result<()> {
    set_result(conn, req, token, "completed", key).await?;
    audit_log::record_in_tx(
        conn,
        if req.kind == "export" {
            "customer.export_completed"
        } else {
            "customer.erased"
        },
        Some(req.requested_by),
        Some(req.tenant_id),
        "customer",
        Some(&req.customer_id.to_string()),
        &serde_json::json!({"request_id": req.id}),
    )
    .await
}

fn attachment_key(
    tenant: Uuid,
    item: &conversations::customer_privacy::Attachment,
) -> Result<String, StorageError> {
    let prefix = format!("whatsapp-media/{tenant}/");
    match item.storage_key.as_deref() {
        Some(key) if key.starts_with(&prefix) => Ok(key.to_owned()),
        Some(_) => Err(StorageError::Other(
            "attachment is outside its tenant prefix".into(),
        )),
        None => Ok(format!("{prefix}{}", item.id)),
    }
}

async fn delete_object(storage: &dyn ObjectStorage, key: &str) -> Result<(), StorageError> {
    match storage.delete(key).await {
        Ok(()) | Err(StorageError::NotFound) => Ok(()),
        Err(error) => Err(error),
    }
}

async fn cancel_events(
    conn: &mut PgConnection,
    tenant: Uuid,
    customer: Uuid,
    conversations: &[Uuid],
    messages: &[Uuid],
    escalations: &[Uuid],
    tools: &[Uuid],
) -> sqlx::Result<()> {
    fn ids(values: &[Uuid]) -> Vec<String> {
        values.iter().map(Uuid::to_string).collect()
    }
    sqlx::query(
        "UPDATE outbox_events SET payload = '{}'::jsonb, last_error = NULL,
                event_type = 'privacy.erased', processed_at = COALESCE(processed_at, now()),
                dead_lettered_at = COALESCE(dead_lettered_at, now()), claim_token = NULL
         WHERE tenant_id = $1 AND (
                (aggregate_type = 'customer' AND aggregate_id = $2)
                OR (aggregate_type = 'conversation' AND aggregate_id = ANY($3))
                OR payload->>'conversation_id' = ANY($3)
                OR payload->>'conversationId' = ANY($3)
                OR payload->>'message_id' = ANY($4)
                OR payload->>'messageId' = ANY($4)
                OR ((payload->>'subjectType') = 'conversation' AND payload->>'subjectId' = ANY($3))
                OR ((payload->>'subjectType') = 'escalation' AND payload->>'subjectId' = ANY($5))
                OR ((payload->>'subjectType') = 'tool_request' AND payload->>'subjectId' = ANY($6))
                OR payload->>'customerId' = $2 OR payload->>'customer_id' = $2
         )",
    )
    .bind(tenant.to_string())
    .bind(customer.to_string())
    .bind(ids(conversations))
    .bind(ids(messages))
    .bind(ids(escalations))
    .bind(ids(tools))
    .execute(conn)
    .await?;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/support/worker_fencing.rs"]
mod tests;
