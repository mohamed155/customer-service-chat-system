use email::{EmailDeliveryStatus, EmailMessage, EmailSender};
use kernel::ErrorDetail;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

use crate::domain::password_reset as domain;
use crate::password;

pub(crate) const MAX_ATTEMPTS: i32 = 3;
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(20);
const POLL_INTERVAL: Duration = Duration::from_secs(1);

pub use crate::domain::password_reset::normalize_email;

pub trait PasswordResetStore: Sync {
    fn eligible_user(
        &self,
        hash: &str,
    ) -> impl Future<Output = sqlx::Result<Option<uuid::Uuid>>> + Send;

    fn redeem(
        &self,
        user: uuid::Uuid,
        token_hash: &str,
        password_hash: &str,
    ) -> impl Future<Output = Result<(), ResetError>> + Send;

    fn claim_delivery(
        &self,
        smtp_configured: bool,
        dashboard_url: &Url,
    ) -> impl Future<Output = Result<DeliveryClaim, DeliveryError>> + Send;

    fn finish_delivery(
        &self,
        id: uuid::Uuid,
        attempts: i32,
    ) -> impl Future<Output = sqlx::Result<()>> + Send;
}

pub enum DeliveryClaim {
    Idle,
    Discarded,
    Send {
        id: uuid::Uuid,
        trace_id: String,
        attempts: i32,
        message: EmailMessage,
    },
}

pub fn email_rate_limit_key(email: &str) -> String {
    domain::sha256(email)
}

#[derive(Debug)]
pub enum ResetError {
    InvalidToken,
    InvalidPassword(ErrorDetail),
    Database(sqlx::Error),
    Hash(argon2::password_hash::Error),
    HashTask(tokio::task::JoinError),
}

impl From<sqlx::Error> for ResetError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

pub async fn reset(
    store: &impl PasswordResetStore,
    raw_token: &str,
    password: String,
) -> Result<(), ResetError> {
    let token_hash = domain::token_hash(raw_token).ok_or(ResetError::InvalidToken)?;
    // An unlocked preflight keeps misses cheap and keeps Argon2 off the transaction's lock path.
    let user = store
        .eligible_user(&token_hash)
        .await?
        .ok_or(ResetError::InvalidToken)?;
    password::validate_new_password(&password).map_err(ResetError::InvalidPassword)?;
    let password_hash = tokio::task::spawn_blocking(move || password::hash_password(&password))
        .await
        .map_err(ResetError::HashTask)?
        .map_err(ResetError::Hash)?;

    store.redeem(user, &token_hash, &password_hash).await
}

#[derive(Debug)]
pub enum DeliveryError {
    Database(sqlx::Error),
    Randomness,
    DashboardUrl(url::ParseError),
}

impl From<sqlx::Error> for DeliveryError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

/// Process one outbox request. A retry creates a new token because only the hash survives
/// outside this call. SMTP is bounded by the lease and does not hold database locks.
pub async fn process_delivery_once(
    store: &impl PasswordResetStore,
    sender: &dyn EmailSender,
    dashboard_base_url: &str,
) -> Result<bool, DeliveryError> {
    let dashboard_url = Url::parse(dashboard_base_url).map_err(DeliveryError::DashboardUrl)?;
    let (id, trace_id, attempts, message) = match store
        .claim_delivery(sender.is_configured(), &dashboard_url)
        .await?
    {
        DeliveryClaim::Idle => return Ok(false),
        DeliveryClaim::Discarded => return Ok(true),
        DeliveryClaim::Send {
            id,
            trace_id,
            attempts,
            message,
        } => (id, trace_id, attempts, message),
    };
    let result = tokio::time::timeout(DELIVERY_TIMEOUT, sender.send(message)).await;
    if matches!(result, Ok(EmailDeliveryStatus::Sent)) {
        store.finish_delivery(id, attempts).await?;
    } else {
        // Provider error details may contain message contents or recipient addresses.
        tracing::warn!(request_id = %trace_id, attempts, "password reset email delivery failed");
        if attempts >= MAX_ATTEMPTS {
            store.finish_delivery(id, attempts).await?;
        }
    }
    Ok(true)
}

pub async fn run_delivery_worker(
    store: impl PasswordResetStore,
    sender: Arc<dyn EmailSender>,
    dashboard_url: String,
) {
    loop {
        match process_delivery_once(&store, sender.as_ref(), &dashboard_url).await {
            Ok(true) => continue,
            Ok(false) => {}
            Err(error) => {
                tracing::error!(?error, "password reset delivery worker iteration failed")
            }
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
