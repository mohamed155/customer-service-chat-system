//! No-op (logging) email sender — fallback when SMTP is not configured.

use async_trait::async_trait;
use tracing::info;

use crate::{EmailDeliveryStatus, EmailMessage, EmailSender};

/// Logs email messages at `info` level. Always reports `is_configured() = false`.
pub struct LogEmailSender;

#[async_trait]
impl EmailSender for LogEmailSender {
    fn is_configured(&self) -> bool {
        false
    }

    async fn send(&self, msg: EmailMessage) -> EmailDeliveryStatus {
        info!(
            to = %msg.to,
            subject = %msg.subject,
            "email delivery suppressed (SMTP not configured): would have sent email"
        );
        EmailDeliveryStatus::Unconfigured
    }
}
