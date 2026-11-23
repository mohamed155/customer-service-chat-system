use chrono::{DateTime, Utc};
use config::LoginLockoutLimits;
use email::{EmailDeliveryStatus, EmailMessage, EmailSender};
use std::{sync::Arc, time::Duration};
use url::Url;
use uuid::Uuid;

const MAX_ATTEMPTS: i32 = 3;
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(20);
const POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LoginAccount {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub platform_role: Option<String>,
    pub password_hash: Option<String>,
    pub password_reset_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LoginReservation {
    pub failed_login_count: i32,
    pub locked_until: Option<DateTime<Utc>>,
    pub expired_lock: bool,
}

#[derive(Debug, Clone)]
pub struct LockoutEvent {
    pub failed_attempts: i32,
    pub locked_until: DateTime<Utc>,
    pub lock_seconds: i32,
    pub notify_owner: bool,
}

#[derive(Debug, Clone)]
pub struct FailedLogin {
    pub user_id: Option<Uuid>,
    pub email: String,
    pub reason: &'static str,
    pub reservation: Option<LoginReservation>,
    pub lockout_event: Option<LockoutEvent>,
    pub request_trace_id: String,
}

pub struct LoginAttempt {
    account: Option<LoginAccount>,
    reservation: Option<LoginReservation>,
}

impl LoginAttempt {
    pub(crate) fn new(
        account: Option<LoginAccount>,
        reservation: Option<LoginReservation>,
    ) -> Self {
        Self {
            account,
            reservation,
        }
    }

    pub fn account(&self) -> Option<&LoginAccount> {
        self.account.as_ref()
    }

    /// A missing reservation means either the account has no password or its lock is active.
    pub fn can_authenticate(&self) -> bool {
        self.reservation.is_some()
    }

    /// Persist the generic failure and any lock transition after verification. The committed reservation counts even if the request is abandoned.
    pub async fn failed(
        self,
        store: &impl LoginLockoutStore,
        email: &str,
        request_trace_id: &str,
        limits: LoginLockoutLimits,
    ) -> sqlx::Result<()> {
        let password_account = self
            .account
            .as_ref()
            .is_some_and(|account| account.password_hash.is_some());
        let reason = if password_account && self.reservation.is_none() {
            "account_locked"
        } else {
            "invalid_credentials"
        };
        let lockout_event = self.reservation.as_ref().and_then(|reservation| {
            if reservation.failed_login_count < limits.threshold {
                return None;
            }
            reservation.locked_until.map(|locked_until| LockoutEvent {
                failed_attempts: reservation.failed_login_count,
                locked_until,
                lock_seconds: crate::domain::login_lockout::lock_duration_secs(
                    reservation.failed_login_count,
                    limits.threshold,
                    limits.base_secs,
                    limits.max_secs,
                ),
                notify_owner: reservation.failed_login_count == limits.threshold,
            })
        });
        store
            .finish_failed(FailedLogin {
                user_id: self.account.as_ref().map(|account| account.id),
                email: email.to_owned(),
                reason,
                reservation: self.reservation,
                lockout_event,
                request_trace_id: request_trace_id.to_owned(),
            })
            .await
    }

    /// Complete a correct password step only if the verified credential is still current.
    pub async fn succeeded(self, store: &impl LoginLockoutStore) -> sqlx::Result<bool> {
        let Self {
            account,
            reservation,
        } = self;
        let (Some(account), Some(reservation)) = (account, reservation) else {
            return Ok(false);
        };
        if account.password_hash.is_none() {
            return Ok(false);
        }
        store.finish_success(account, reservation).await
    }
}

#[async_trait::async_trait]
pub trait LoginLockoutStore: Sync {
    async fn begin_login(
        &self,
        email: &str,
        limits: LoginLockoutLimits,
    ) -> sqlx::Result<LoginAttempt>;

    async fn finish_failed(&self, failure: FailedLogin) -> sqlx::Result<()>;

    async fn finish_success(
        &self,
        account: LoginAccount,
        reservation: LoginReservation,
    ) -> sqlx::Result<bool>;

    async fn claim_notification(
        &self,
        smtp_configured: bool,
    ) -> Result<NotificationClaim, NotificationError>;

    async fn finish_notification(&self, id: Uuid, attempts: i32) -> sqlx::Result<()>;
}

pub async fn begin_login(
    store: &impl LoginLockoutStore,
    email: &str,
    limits: LoginLockoutLimits,
) -> sqlx::Result<LoginAttempt> {
    store.begin_login(email, limits).await
}

pub async fn finalize_failed_password(
    store: &impl LoginLockoutStore,
    attempt: LoginAttempt,
    email: &str,
    request_trace_id: &str,
    limits: LoginLockoutLimits,
) {
    if let Err(error) = attempt.failed(store, email, request_trace_id, limits).await {
        tracing::error!(%error, "failed to finalize failed password attempt");
    }
}

pub async fn complete_password_success(
    store: &impl LoginLockoutStore,
    attempt: LoginAttempt,
) -> sqlx::Result<bool> {
    attempt.succeeded(store).await
}

pub enum NotificationClaim {
    Idle,
    Discarded,
    Send {
        id: Uuid,
        request_trace_id: String,
        attempts: i32,
        recipient: String,
        locked_until: DateTime<Utc>,
        failed_attempts: i32,
    },
}

#[derive(Debug)]
pub enum NotificationError {
    Database(sqlx::Error),
    DashboardUrl(url::ParseError),
}

impl From<sqlx::Error> for NotificationError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

/// Claim one queued message, send it with a timeout, and finish or leave it leased for retry.
pub async fn process_notification_once(
    store: &impl LoginLockoutStore,
    sender: &dyn EmailSender,
    dashboard_base_url: &str,
) -> Result<bool, NotificationError> {
    let dashboard_url = Url::parse(dashboard_base_url).map_err(NotificationError::DashboardUrl)?;
    let (id, request_trace_id, attempts, recipient, locked_until, failed_attempts) =
        match store.claim_notification(sender.is_configured()).await? {
            NotificationClaim::Idle => return Ok(false),
            NotificationClaim::Discarded => return Ok(true),
            NotificationClaim::Send {
                id,
                request_trace_id,
                attempts,
                recipient,
                locked_until,
                failed_attempts,
            } => (
                id,
                request_trace_id,
                attempts,
                recipient,
                locked_until,
                failed_attempts,
            ),
        };

    let forgot_password = dashboard_url
        .join("auth/forgot-password")
        .map_err(NotificationError::DashboardUrl)?;
    let locked_until_text = locked_until.format("%Y-%m-%d %H:%M:%S");
    let message = EmailMessage {
        to: recipient,
        subject: "Your account was temporarily locked".into(),
        body_text: format!(
            "We recorded {failed_attempts} consecutive incorrect passwords. Sign-in is temporarily locked until about {locked_until_text} UTC.\n\n\
             If this was you, wait until then. If it was not you, reset your password: {forgot_password}"
        ),
        body_html: None,
    };

    let result = tokio::time::timeout(DELIVERY_TIMEOUT, sender.send(message)).await;
    let sent = matches!(result, Ok(EmailDeliveryStatus::Sent));
    if sent || attempts >= MAX_ATTEMPTS {
        if !sent {
            tracing::warn!(request_id = %request_trace_id, attempts, "login lockout notification delivery exhausted retries");
        }
        store.finish_notification(id, attempts).await?;
    } else {
        tracing::warn!(request_id = %request_trace_id, attempts, "login lockout notification delivery failed");
    }
    Ok(true)
}

pub async fn run_notification_worker(
    store: impl LoginLockoutStore,
    sender: Arc<dyn EmailSender>,
    dashboard_url: String,
) {
    loop {
        match process_notification_once(&store, sender.as_ref(), &dashboard_url).await {
            Ok(true) => continue,
            Ok(false) => {}
            Err(error) => {
                tracing::error!(?error, "login lockout notification worker iteration failed")
            }
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
