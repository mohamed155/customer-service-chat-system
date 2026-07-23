use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine;
use hmac::{Hmac, Mac};
use kernel::{ApiError, InMemoryRateLimitStore};
use rand::Rng;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::crypto::{self, MasterKey};
use crate::model;
use crate::queries;

const SIGNING_SECRET_FIELD: &str = "signing_secret";

/// Per-connection intake rate limit: 60 deliveries per 60 seconds.
const RATE_LIMIT_REQUESTS: u32 = 60;
const RATE_LIMIT_WINDOW_SECS: u64 = 60;

/// Once-per-minute budget for unauthenticated rejection events. The
/// underlying `InMemoryRateLimitStore` window aligns with the rejection
/// throttling policy in the contract.
const REJECTION_EVENT_BUDGET: u32 = 1;
const REJECTION_EVENT_WINDOW_SECS: u64 = 60;

pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn hash_token(token: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hasher.finalize().to_vec()
}

/// Constant-time HMAC-SHA256 verification of an incoming webhook signature.
///
/// `header` is the raw `X-Webhook-Signature` value, expected to be of the
/// form `sha256=<hex>`. The function strips the prefix, hex-decodes the
/// remainder, and uses `Hmac::<Sha256>::verify_slice` to compare the
/// recomputed MAC against the supplied one in constant time — never a
/// hand-rolled byte comparison.
pub fn verify_signature(secret: &str, raw_body: &[u8], header: &str) -> bool {
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

/// Build the byte-identical 404 response shared by every unauthenticated
/// rejection branch (unknown token, inactive connection, invalid signature).
/// FR-009 requires that these responses do not leak whether the address
/// ever existed.
fn not_found_response() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "not found" })),
    )
        .into_response()
}

/// Resolve the integration master key from app config. Mirrors the helper
/// in `routes::load_master_key`; duplicated here so the webhook crate does
/// not need to call back into `routes`.
fn load_master_key(config: &config::AppConfig) -> Result<MasterKey, Response> {
    let raw = config
        .integration_secrets_key
        .as_deref()
        .ok_or_else(|| {
            ApiError::internal_error("Integration secrets key is not configured").into_response()
        })?;
    MasterKey::from_base64(raw).map_err(|_| {
        ApiError::internal_error("Integration secrets key is invalid").into_response()
    })
}

/// Persist a non-throttled rejection event (used for `invalid_signature` and
/// `malformed_payload`, which are reachable only after the connection is
/// known and the per-connection rate limit has not been exceeded).
async fn log_rejection_event(
    pool: &PgPool,
    tenant_id: uuid::Uuid,
    connection_id: uuid::Uuid,
    reason: &str,
) {
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::warn!(%e, "log_rejection_event: begin failed");
            return;
        }
    };
    if let Err(e) = queries::insert_event(
        &mut tx,
        tenant_id,
        connection_id,
        model::EventType::DeliveryRejected.as_str(),
        "failure",
        Some(reason),
        None,
    )
    .await
    {
        tracing::warn!(%e, "log_rejection_event: insert failed");
        let _ = tx.rollback().await;
        return;
    }
    if let Err(e) = tx.commit().await {
        tracing::warn!(%e, "log_rejection_event: commit failed");
    }
}

#[utoipa::path(
    post,
    path = "/hooks/v1/{token}",
    tag = "integrations",
    operation_id = "receive_webhook",
    summary = "Public inbound-webhook intake (HMAC-verified, rate-limited)",
    description = "Receives a webhook delivery for the connection identified by `token`. \
                   The body must be JSON and accompanied by an `X-Webhook-Signature: sha256=<hex>` \
                   header carrying the HMAC-SHA256 of the raw body keyed by the connection's \
                   `signing_secret`. Per-connection rate limit is 60 requests / 60 seconds; body \
                   limit is 256 KB. No authentication is required — the token in the URL is the \
                   credential. Unknown, inactive, and unverifiable deliveries all return the same \
                   generic 404 (no existence leak).",
    params(
        ("token" = String, Path, description = "Per-connection intake token"),
    ),
    request_body(
        content = Vec::<u8>,
        description = "Raw JSON body, HMAC-SHA256-verified before parsing",
        content_type = "application/json",
    ),
    responses(
        (status = 202, description = "Delivery accepted.", body = serde_json::Value),
        (status = 404, description = "Unknown, inactive, or unverifiable delivery."),
        (status = 413, description = "Payload exceeds 256 KB limit."),
        (status = 422, description = "Body is not valid JSON."),
        (status = 429, description = "Per-connection rate limit exceeded."),
    ),
    security(())
)]
pub async fn receive_webhook(
    Path(token): Path<String>,
    State(pool): State<PgPool>,
    Extension(store): Extension<Arc<InMemoryRateLimitStore>>,
    Extension(config): Extension<Arc<config::AppConfig>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let signature_header = headers
        .get("x-webhook-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let token_hash = hash_token(&token);
    let conn = match queries::find_connection_by_token_hash(&pool, &token_hash).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            // Unknown token — same byte-identical 404 as every other
            // unauthenticated rejection branch, no event row written.
            return not_found_response();
        }
        Err(e) => {
            tracing::error!(%e, "receive_webhook: find_connection_by_token_hash failed");
            return ApiError::internal_error("Internal error").into_response();
        }
    };

    if !conn.is_active {
        // Inactive connection — 404 + throttled rejection event.
        let reason = model::RejectionReason::InactiveConnection.as_str();
        if store.check(
            &format!("integration_evt:{}:{}", conn.connection_id, reason),
            REJECTION_EVENT_BUDGET,
            std::time::Duration::from_secs(REJECTION_EVENT_WINDOW_SECS),
        ) {
            log_rejection_event(&pool, conn.tenant_id, conn.connection_id, reason).await;
        }
        return not_found_response();
    }

    if !store.check(
        &format!("integration_conn:{}", conn.connection_id),
        RATE_LIMIT_REQUESTS,
        std::time::Duration::from_secs(RATE_LIMIT_WINDOW_SECS),
    ) {
        // Rate limited — 429 + throttled rejection event.
        let reason = model::RejectionReason::RateLimited.as_str();
        if store.check(
            &format!("integration_evt:{}:{}", conn.connection_id, reason),
            REJECTION_EVENT_BUDGET,
            std::time::Duration::from_secs(REJECTION_EVENT_WINDOW_SECS),
        ) {
            log_rejection_event(&pool, conn.tenant_id, conn.connection_id, reason).await;
        }
        return ApiError::rate_limited("Too many requests").into_response();
    }

    let master = match load_master_key(&config) {
        Ok(m) => m,
        Err(resp) => return resp,
    };

    let (ct, nonce) =
        match queries::find_secret_ciphertext(&pool, conn.connection_id, SIGNING_SECRET_FIELD)
            .await
        {
            Ok(Some(pair)) => pair,
            Ok(None) => {
                log_rejection_event(
                    &pool,
                    conn.tenant_id,
                    conn.connection_id,
                    model::RejectionReason::InvalidSignature.as_str(),
                )
                .await;
                return not_found_response();
            }
            Err(e) => {
                tracing::error!(%e, "receive_webhook: find_secret_ciphertext failed");
                return ApiError::internal_error("Internal error").into_response();
            }
        };

    let aad = crypto::aad(conn.tenant_id, &conn.catalog_slug, SIGNING_SECRET_FIELD);
    let secret = match crypto::open(&master, &aad, &ct, &nonce) {
        Ok(s) => s,
        Err(_) => {
            log_rejection_event(
                &pool,
                conn.tenant_id,
                conn.connection_id,
                model::RejectionReason::InvalidSignature.as_str(),
            )
            .await;
            return not_found_response();
        }
    };

    if !verify_signature(&secret, &body, signature_header) {
        log_rejection_event(
            &pool,
            conn.tenant_id,
            conn.connection_id,
            model::RejectionReason::InvalidSignature.as_str(),
        )
        .await;
        return not_found_response();
    }

    let payload: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            log_rejection_event(
                &pool,
                conn.tenant_id,
                conn.connection_id,
                model::RejectionReason::MalformedPayload.as_str(),
            )
            .await;
            return ApiError::unprocessable_entity("Body is not valid JSON").into_response();
        }
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!(%e, "receive_webhook: begin tx failed");
            return ApiError::internal_error("Failed to record delivery").into_response();
        }
    };
    if let Err(e) = queries::insert_delivery(&mut tx, conn.tenant_id, conn.connection_id, &payload)
        .await
    {
        tracing::error!(%e, "receive_webhook: insert_delivery failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to record delivery").into_response();
    }
    if let Err(e) = queries::insert_event(
        &mut tx,
        conn.tenant_id,
        conn.connection_id,
        model::EventType::DeliveryAccepted.as_str(),
        "success",
        None,
        None,
    )
    .await
    {
        tracing::error!(%e, "receive_webhook: insert_event failed");
        let _ = tx.rollback().await;
        return ApiError::internal_error("Failed to record delivery").into_response();
    }
    if let Err(e) = tx.commit().await {
        tracing::error!(%e, "receive_webhook: commit failed");
        return ApiError::internal_error("Failed to record delivery").into_response();
    }

    (
        StatusCode::ACCEPTED,
        Json(json!({ "status": "accepted" })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hmac_hex(secret: &str, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let bytes = mac.finalize().into_bytes();
        hex::encode(bytes)
    }

    #[test]
    fn hash_token_is_deterministic() {
        let token = "test-token-123";
        let h1 = hash_token(token);
        let h2 = hash_token(token);
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_token_never_returns_the_raw_token() {
        let token = generate_token();
        let hash = hash_token(&token);
        assert_ne!(hash, token.as_bytes());
    }

    #[test]
    fn generated_token_is_url_safe_base64_no_padding() {
        let token = generate_token();
        assert!(token.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '-' || c == '_'
        }));
        assert!(!token.contains('='));
    }

    #[test]
    fn two_generated_tokens_differ() {
        assert_ne!(generate_token(), generate_token());
    }

    #[test]
    fn verify_signature_accepts_valid_signature() {
        let secret = "whsec_test_secret";
        let body = br#"{"hello":"world"}"#;
        let sig = hmac_hex(secret, body);
        assert!(verify_signature(secret, body, &format!("sha256={sig}")));
    }

    #[test]
    fn verify_signature_rejects_wrong_secret() {
        let body = br#"{"hello":"world"}"#;
        let sig = hmac_hex("right", body);
        assert!(!verify_signature("wrong", body, &format!("sha256={sig}")));
    }

    #[test]
    fn verify_signature_rejects_missing_prefix() {
        let body = br#"{"hello":"world"}"#;
        let sig = hmac_hex("secret", body);
        assert!(!verify_signature("secret", body, &sig));
    }

    #[test]
    fn verify_signature_rejects_malformed_hex() {
        let body = br#"{"hello":"world"}"#;
        assert!(!verify_signature("secret", body, "sha256=zzzznotvalidhex"));
    }

    #[test]
    fn verify_signature_rejects_tampered_body() {
        let secret = "whsec_test_secret";
        let original = br#"{"hello":"world"}"#;
        let sig = hmac_hex(secret, original);
        let tampered = br#"{"hello":"WORLD"}"#;
        assert!(!verify_signature(secret, tampered, &format!("sha256={sig}")));
    }
}
