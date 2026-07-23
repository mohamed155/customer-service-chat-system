use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

use crate::api::{SendError, WhatsAppApi};
use crate::queries;
use crate::window;

pub async fn run_whatsapp_sender_worker(
    pool: PgPool,
    api: Arc<dyn WhatsAppApi>,
    master_key: Arc<integrations::crypto::MasterKey>,
) -> ! {
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    loop {
        interval.tick().await;

        if let Err(e) = process_one(&pool, &*api, &master_key).await {
            tracing::error!(%e, "sender worker: process_one failed");
        }
    }
}

struct OutboundEvent {
    id: Uuid,
    tenant_id: Uuid,
    conversation_id: Uuid,
    message_id: Uuid,
}

async fn claim_outbound_event(pool: &PgPool) -> sqlx::Result<Option<OutboundEvent>> {
    let row: Option<(Uuid, Uuid, Uuid, Uuid)> = sqlx::query_as(
        "WITH claimed AS ( \
            SELECT id, tenant_id, \
                   (payload->>'conversationId')::uuid AS conversation_id, \
                   (payload->>'messageId')::uuid AS message_id \
            FROM outbox_events \
            WHERE event_type = 'whatsapp.outbound_message' \
            ORDER BY created_at ASC \
            LIMIT 1 \
            FOR UPDATE SKIP LOCKED \
        ) \
        DELETE FROM outbox_events WHERE id IN (SELECT id FROM claimed) \
        RETURNING id, tenant_id, conversation_id, message_id",
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(id, tenant_id, conversation_id, message_id)| OutboundEvent {
        id,
        tenant_id,
        conversation_id,
        message_id,
    }))
}

pub async fn process_one(
    pool: &PgPool,
    api: &dyn WhatsAppApi,
    master_key: &integrations::crypto::MasterKey,
) -> Result<(), Box<dyn std::error::Error>> {
    let event = match claim_outbound_event(pool).await? {
        Some(e) => e,
        None => return Ok(()),
    };

    // 1. Get message body
    let body = queries::message_body_for_outbound(pool, event.tenant_id, event.message_id)
        .await?
        .ok_or_else(|| format!("message {} not found", event.message_id))?;

    // 2. Get customer's whatsapp identifier
    let customer_id: Uuid = sqlx::query_scalar(
        "SELECT customer_id FROM conversations WHERE tenant_id = $1 AND id = $2",
    )
    .bind(event.tenant_id)
    .bind(event.conversation_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| format!("conversation {} not found", event.conversation_id))?;

    let wa_identifier = sqlx::query_scalar::<_, String>(
        "SELECT identifier FROM customer_channel_identifiers \
         WHERE tenant_id = $1 AND customer_id = $2 AND channel = 'whatsapp' AND deleted_at IS NULL",
    )
    .bind(event.tenant_id)
    .bind(customer_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| format!("no whatsapp identifier for customer {}", customer_id))?;

    // 3. Insert outbound meta (pending) in a transaction
    let mut tx = pool.begin().await?;

    let meta_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM whatsapp_message_meta WHERE tenant_id = $1 AND message_id = $2)",
    )
    .bind(event.tenant_id)
    .bind(event.message_id)
    .fetch_one(&mut *tx)
    .await?;

    if !meta_exists {
        queries::insert_outbound_meta_in_tx(&mut tx, event.tenant_id, event.message_id, event.conversation_id)
            .await?;
    }
    tx.commit().await?;

    // 4. Re-check window
    let last_customer = conversations::queries::last_customer_message_at(
        pool, event.tenant_id, event.conversation_id,
    )
    .await?;

    if !window::window_open(last_customer, chrono::Utc::now()) {
        let mut tx = pool.begin().await?;
        queries::set_outbound_failed_in_tx(
            &mut tx, event.tenant_id, event.message_id,
            "The WhatsApp messaging window has expired for this conversation.",
        )
        .await?;
        tx.commit().await?;
        return Ok(());
    }

    // 5. Get connection config and access_token
    let connection_id = match integrations::queries::active_connection_for_slug(
        pool, event.tenant_id, "whatsapp",
    )
    .await?
    {
        Some(id) => id,
        None => {
            let mut tx = pool.begin().await?;
            queries::set_outbound_failed_in_tx(
                &mut tx, event.tenant_id, event.message_id,
                "WhatsApp channel is disconnected.",
            )
            .await?;
            tx.commit().await?;
            return Ok(());
        }
    };

    let config = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT config FROM integration_connections WHERE id = $1",
    )
    .bind(connection_id)
    .fetch_optional(pool)
    .await?;

    let config = match config {
        Some(c) => c,
        None => {
            let mut tx = pool.begin().await?;
            queries::set_outbound_failed_in_tx(
                &mut tx, event.tenant_id, event.message_id,
                "WhatsApp channel configuration not found.",
            )
            .await?;
            tx.commit().await?;
            return Ok(());
        }
    };

    let phone_number_id = match config.get("phone_number_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => {
            let mut tx = pool.begin().await?;
            queries::set_outbound_failed_in_tx(
                &mut tx, event.tenant_id, event.message_id,
                "WhatsApp phone number ID not configured.",
            )
            .await?;
            tx.commit().await?;
            return Ok(());
        }
    };

    let access_token = match integrations::queries::decrypted_secret(
        pool, master_key, connection_id, "access_token",
    )
    .await?
    {
        Some(t) => t,
        None => {
            let mut tx = pool.begin().await?;
            queries::set_outbound_failed_in_tx(
                &mut tx, event.tenant_id, event.message_id,
                "WhatsApp access token not found.",
            )
            .await?;
            tx.commit().await?;
            return Ok(());
        }
    };

    // 6. Send via API
    match api.send_text(&access_token, &phone_number_id, &wa_identifier, &body).await {
        Ok(wamid) => {
            let mut tx = pool.begin().await?;
            queries::set_outbound_sent_in_tx(&mut tx, event.tenant_id, event.message_id, &wamid)
                .await?;
            tx.commit().await?;
        }
        Err(SendError::WindowExpired) => {
            let mut tx = pool.begin().await?;
            queries::set_outbound_failed_in_tx(
                &mut tx, event.tenant_id, event.message_id,
                "The WhatsApp messaging window has expired for this conversation.",
            )
            .await?;
            tx.commit().await?;
        }
        Err(SendError::Unauthorized) => {
            let mut tx = pool.begin().await?;
            queries::set_outbound_failed_in_tx(
                &mut tx, event.tenant_id, event.message_id,
                "WhatsApp credentials rejected \u{2014} reconnect the integration.",
            )
            .await?;
            tx.commit().await?;
        }
        Err(SendError::Transient(msg)) => {
            tracing::warn!(%msg, "sender worker: transient error, will retry on next poll");
            let mut tx = pool.begin().await?;
            queries::set_outbound_failed_in_tx(
                &mut tx, event.tenant_id, event.message_id,
                &format!("Transient error: {msg}"),
            )
            .await?;
            tx.commit().await?;
        }
        Err(SendError::Other(msg)) => {
            let mut tx = pool.begin().await?;
            queries::set_outbound_failed_in_tx(
                &mut tx, event.tenant_id, event.message_id,
                &format!("Send failed: {msg}"),
            )
            .await?;
            tx.commit().await?;
        }
    }

    Ok(())
}
