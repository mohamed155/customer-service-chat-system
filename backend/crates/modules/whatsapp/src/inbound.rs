use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::identity;
use crate::model::{Contact, IncomingMessage, MediaContent};

fn extract_body(msg: &IncomingMessage) -> String {
    match msg.message_type.as_str() {
        "text" => msg.text.as_ref().map(|t| t.body.clone()).unwrap_or_default(),
        "image" => msg
            .image
            .as_ref()
            .and_then(|m| m.caption.clone())
            .unwrap_or_else(|| "[Image]".to_string()),
        "audio" => "[Audio]".to_string(),
        "video" => msg
            .video
            .as_ref()
            .and_then(|m| m.caption.clone())
            .unwrap_or_else(|| "[Video]".to_string()),
        "document" => msg
            .document
            .as_ref()
            .and_then(|m| m.caption.clone())
            .unwrap_or_else(|| "[Document]".to_string()),
        "location" => "[Location]".to_string(),
        "contacts" => "[Contact]".to_string(),
        "sticker" => "[Sticker]".to_string(),
        "button" => "[Button]".to_string(),
        "interactive" => "[Interactive]".to_string(),
        "order" => "[Order]".to_string(),
        "system" => "[System]".to_string(),
        _ => "[Unsupported message]".to_string(),
    }
}

fn extract_media(msg: &IncomingMessage) -> Option<(String, MediaContent)> {
    let candidates = [
        ("image", &msg.image),
        ("audio", &msg.audio),
        ("video", &msg.video),
        ("document", &msg.document),
    ];
    for (kind, content) in candidates {
        if let Some(media) = content {
            if !media.id.is_empty() {
                return Some((kind.to_string(), media.clone()));
            }
        }
    }
    None
}

pub async fn process_message(
    pool: &PgPool,
    tenant_id: Uuid,
    msg: &IncomingMessage,
    contact: Option<&Contact>,
) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;

    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM whatsapp_message_meta WHERE tenant_id = $1 AND wamid = $2)",
    )
    .bind(tenant_id)
    .bind(&msg.id)
    .fetch_one(&mut *tx)
    .await?;

    if exists {
        tx.rollback().await?;
        return Ok(());
    }

    let profile_name = contact
        .and_then(|c| c.profile.as_ref())
        .and_then(|p| p.name.as_deref());
    let customer_id = identity::resolve_customer_in_tx(&mut tx, tenant_id, &msg.from, profile_name).await?;

    let conversation_id: Uuid =
        match conversations::queries::find_open_conversation_in_tx(&mut tx, tenant_id, customer_id, "whatsapp")
            .await?
        {
            Some(cid) => cid,
            None => sqlx::query_scalar(
                "INSERT INTO conversations (tenant_id, customer_id, channel, status, last_activity_at) \
                 VALUES ($1, $2, 'whatsapp', 'open', now()) RETURNING id",
            )
            .bind(tenant_id)
            .bind(customer_id)
            .fetch_one(&mut *tx)
            .await?,
        };

    let body = extract_body(msg);
    let message_id: Uuid = sqlx::query_scalar(
        "INSERT INTO messages (tenant_id, conversation_id, kind, body) \
         VALUES ($1, $2, 'customer', $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(&body)
    .fetch_one(&mut *tx)
    .await?;

    if let Some((kind, media)) = extract_media(msg) {
        crate::queries::insert_attachment_in_tx(
            &mut tx,
            tenant_id,
            message_id,
            &kind,
            &media.id,
            media.filename.as_deref(),
        )
        .await?;
    }

    let provider_ts = msg.timestamp.parse::<chrono::DateTime<chrono::Utc>>().ok();
    sqlx::query(
        "INSERT INTO whatsapp_message_meta (tenant_id, message_id, conversation_id, direction, wamid, provider_timestamp) \
         VALUES ($1, $2, $3, 'inbound', $4, $5)",
    )
    .bind(tenant_id)
    .bind(message_id)
    .bind(conversation_id)
    .bind(&msg.id)
    .bind(provider_ts)
    .execute(&mut *tx)
    .await?;

    sqlx::query("UPDATE conversations SET last_activity_at = now() WHERE id = $1")
        .bind(conversation_id)
        .execute(&mut *tx)
        .await?;

    conversations::outbox::emit_customer_message_in_tx(&mut tx, tenant_id, conversation_id, message_id, "whatsapp")
        .await?;

    tx.commit().await?;
    Ok(())
}
