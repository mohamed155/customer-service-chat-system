//! What a page of messages is enriched with, read in units of work of their own.

use std::collections::HashMap;

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use super::messages;
use crate::application::model::{AttachmentInfo, CitationView, DeliveryInfo};
use crate::application::ports::MessageEnrichment;
use crate::application::StoreResult;

/// [`MessageEnrichment`] on the connection pool: each read opens a Unit of Work for the tenant.
pub struct PgEnrichment<'p> {
    pool: &'p PgPool,
}

impl<'p> PgEnrichment<'p> {
    pub fn new(pool: &'p PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MessageEnrichment for PgEnrichment<'_> {
    async fn citations(
        &self,
        tenant_id: Uuid,
        message_ids: &[Uuid],
    ) -> StoreResult<HashMap<Uuid, Vec<CitationView>>> {
        Ok(messages::load_citations_for_messages(self.pool, tenant_id, message_ids).await?)
    }

    async fn attachments(
        &self,
        tenant_id: Uuid,
        message_ids: &[Uuid],
    ) -> StoreResult<HashMap<Uuid, Vec<AttachmentInfo>>> {
        Ok(messages::load_attachments_for_messages(self.pool, tenant_id, message_ids).await?)
    }

    async fn deliveries(
        &self,
        tenant_id: Uuid,
        message_ids: &[Uuid],
    ) -> StoreResult<HashMap<Uuid, DeliveryInfo>> {
        Ok(messages::load_whatsapp_meta_for_messages(self.pool, tenant_id, message_ids).await?)
    }
}
