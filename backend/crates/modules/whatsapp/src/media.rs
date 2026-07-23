use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

use integrations::crypto::MasterKey;
use storage::ObjectStorage;

use crate::api::WhatsAppApi;

pub async fn run_media_fetch_worker(
    pool: PgPool,
    api: Arc<dyn WhatsAppApi>,
    storage: Arc<dyn ObjectStorage>,
    master_key: Arc<MasterKey>,
) -> ! {
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    loop {
        interval.tick().await;

        if let Err(e) = process_one(&pool, &*api, &*storage, &master_key).await {
            tracing::error!(%e, "media worker: process_one failed");
        }
    }
}

async fn process_one(
    pool: &PgPool,
    api: &dyn WhatsAppApi,
    storage: &dyn ObjectStorage,
    master_key: &MasterKey,
) -> sqlx::Result<()> {
    let attachment = match crate::queries::claim_pending_attachment(pool).await? {
        Some(a) => a,
        None => return Ok(()),
    };

    let access_token = match get_tenant_access_token(pool, master_key, attachment.tenant_id).await? {
        Some(token) => token,
        None => {
            crate::queries::mark_attachment_failed_attempt(pool, attachment.id).await?;
            return Ok(());
        }
    };

    let provider_media_id = match &attachment.provider_media_id {
        Some(id) => id.clone(),
        None => {
            crate::queries::mark_attachment_failed_attempt(pool, attachment.id).await?;
            return Ok(());
        }
    };

    let media_info = match api.media_url(&access_token, &provider_media_id).await {
        Ok(info) => info,
        Err(e) => {
            tracing::warn!(%e, provider_media_id, "media worker: media_url failed");
            crate::queries::mark_attachment_failed_attempt(pool, attachment.id).await?;
            return Ok(());
        }
    };

    let bytes = match api.download(&access_token, &media_info.url).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(%e, provider_media_id, "media worker: download failed");
            crate::queries::mark_attachment_failed_attempt(pool, attachment.id).await?;
            return Ok(());
        }
    };

    let storage_key = format!("whatsapp-media/{}/{}", attachment.tenant_id, attachment.id);

    match storage.put(&storage_key, &media_info.mime_type, bytes.to_vec()).await {
        Ok(()) => {
            let size = bytes.len() as i64;
            crate::queries::mark_attachment_stored(pool, attachment.id, &storage_key, &media_info.mime_type, size).await?;
        }
        Err(e) => {
            tracing::warn!(%e, %storage_key, "media worker: storage put failed");
            crate::queries::mark_attachment_failed_attempt(pool, attachment.id).await?;
        }
    }

    Ok(())
}

async fn get_tenant_access_token(
    pool: &PgPool,
    master_key: &MasterKey,
    tenant_id: Uuid,
) -> sqlx::Result<Option<String>> {
    let connection_id =
        match integrations::queries::active_connection_for_slug(pool, tenant_id, "whatsapp").await? {
            Some(id) => id,
            None => return Ok(None),
        };
    integrations::queries::decrypted_secret(pool, master_key, connection_id, "access_token").await
}
