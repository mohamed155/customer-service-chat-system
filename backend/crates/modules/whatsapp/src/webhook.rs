use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Extension, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use hmac::{Hmac, Mac};
use kernel::{ApiError, InMemoryRateLimitStore};
use serde::Deserialize;
use serde_json::json;
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

use integrations::crypto::MasterKey;
use integrations::queries;

/// Per-connection intake rate limit: 60 deliveries per 60 seconds (same as integrations).
const RATE_LIMIT_REQUESTS: u32 = 60;
const RATE_LIMIT_WINDOW_SECS: u64 = 60;
const REJECTION_EVENT_BUDGET: u32 = 1;
const REJECTION_EVENT_WINDOW_SECS: u64 = 60;

/// Byte-identical 404 response for every rejection branch
/// (unknown token, inactive connection, wrong verify_token, wrong mode).
fn not_found_response() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "not found" })),
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
pub struct VerifyQuery {
    #[serde(rename = "hub.mode")]
    pub mode: Option<String>,
    #[serde(rename = "hub.verify_token")]
    pub verify_token: Option<String>,
    #[serde(rename = "hub.challenge")]
    pub challenge: Option<String>,
}

pub async fn verify_subscription(
    Path(token): Path<String>,
    State(pool): State<PgPool>,
    Extension(master_key): Extension<Arc<MasterKey>>,
    query: Query<VerifyQuery>,
) -> Response {
    let span = tracing::info_span!(
        "whatsapp_verify",
        token_hash = %integrations::webhook::hash_token(&token)[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let _guard = span.enter();

    let token_hash = integrations::webhook::hash_token(&token);
    let conn = match queries::connection_by_token_and_slug(&pool, &token_hash, "whatsapp").await {
        Ok(Some(c)) => c,
        Ok(None) => return not_found_response(),
        Err(e) => {
            tracing::error!(%e, "verify_subscription: db error");
            return ApiError::internal_error("Internal error").into_response();
        }
    };

    if !conn.is_active {
        return not_found_response();
    }

    let q = query.0;
    let mode = q.mode.as_deref().unwrap_or("");
    let challenge = q.challenge.as_deref().unwrap_or("");
    let verify_token_input = q.verify_token.as_deref().unwrap_or("");

    let stored_verify_token = match queries::decrypted_secret(&pool, &master_key, conn.id, "verify_token").await {
        Ok(Some(t)) => t,
        Ok(None) => {
            tracing::error!("verify_subscription: verify_token secret not found for connection {}", conn.id);
            return not_found_response();
        }
        Err(e) => {
            tracing::error!(%e, "verify_subscription: decryption failed");
            return not_found_response();
        }
    };

    if mode == "subscribe" && verify_token_input == stored_verify_token {
        (StatusCode::OK, challenge.to_string()).into_response()
    } else {
        not_found_response()
    }
}

fn verify_meta_signature(secret: &str, raw_body: &[u8], header: &str) -> bool {
    let hex_sig = match header.strip_prefix("sha256=") {
        Some(s) => s,
        None => return false,
    };
    let provided = match hex::decode(hex_sig) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let mut mac = match Hmac::<Sha256>::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(raw_body);
    mac.verify_slice(&provided).is_ok()
}

pub async fn receive_message(
    Path(token): Path<String>,
    State(pool): State<PgPool>,
    Extension(master_key): Extension<Arc<MasterKey>>,
    Extension(store): Extension<Arc<InMemoryRateLimitStore>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let token_hash = integrations::webhook::hash_token(&token);
    let conn = match queries::connection_by_token_and_slug(&pool, &token_hash, "whatsapp").await {
        Ok(Some(c)) => c,
        Ok(None) => return not_found_response(),
        Err(e) => {
            tracing::error!(%e, "receive_message: db lookup failed");
            return ApiError::internal_error("Internal error").into_response();
        }
    };

    if !conn.is_active {
        return not_found_response();
    }

    if !store.check(
        &format!("whatsapp_conn:{}", conn.id),
        RATE_LIMIT_REQUESTS,
        std::time::Duration::from_secs(RATE_LIMIT_WINDOW_SECS),
    ) {
        return (StatusCode::TOO_MANY_REQUESTS, Json(json!({"error":"rate_limited"}))).into_response();
    }

    let sig_header = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let app_secret = match queries::decrypted_secret(&pool, &master_key, conn.id, "app_secret").await {
        Ok(Some(s)) => s,
        Ok(None) => {
            tracing::error!("receive_message: app_secret not found");
            return not_found_response();
        }
        Err(e) => {
            tracing::error!(%e, "receive_message: decryption failed");
            return not_found_response();
        }
    };

    if !verify_meta_signature(&app_secret, &body, sig_header) {
        if store.check(
            &format!("whatsapp_evt:{}:invalid_signature", conn.id),
            REJECTION_EVENT_BUDGET,
            std::time::Duration::from_secs(REJECTION_EVENT_WINDOW_SECS),
        ) {
            tracing::warn!("whatsapp webhook invalid signature for connection {}", conn.id);
        }
        return not_found_response();
    }

    let envelope: crate::model::WebhookEnvelope = match serde_json::from_slice(&body) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(%e, "receive_message: malformed JSON");
            return (StatusCode::BAD_REQUEST, Json(json!({"error":"malformed_payload"}))).into_response();
        }
    };

    for entry in &envelope.entry {
        for change in &entry.changes {
            let value = &change.value;

            for msg in &value.messages {
                let contact = value.contacts.iter().find(|c| c.wa_id == msg.from);
                if let Err(e) = crate::inbound::process_message(&pool, conn.tenant_id, msg, contact).await {
                    tracing::error!(%e, wamid = %msg.id, "receive_message: process_message failed");
                }
            }

            for status in &value.statuses {
                if let Err(e) = handle_status_update(&pool, conn.tenant_id, status).await {
                    tracing::error!(%e, "receive_message: handle_status_update failed");
                }
            }
        }
    }

    (StatusCode::OK, Json(json!({"received":true}))).into_response()
}

async fn handle_status_update(
    pool: &PgPool,
    tenant_id: Uuid,
    status: &crate::model::StatusUpdate,
) -> sqlx::Result<()> {
    let failure_reason = status.errors.first().map(|e| e.title.as_str());
    match crate::queries::update_status_by_wamid_in_tx(
        &mut pool.begin().await?,
        tenant_id,
        &status.id,
        &status.status,
        failure_reason,
    )
    .await
    {
        Ok(Some((_conversation_id, _message_id))) => {}
        Ok(None) => {
            tracing::debug!(wamid = %status.id, "status update: no change or unknown wamid");
        }
        Err(e) => {
            tracing::error!(%e, wamid = %status.id, "handle_status_update failed");
        }
    }
    Ok(())
}
