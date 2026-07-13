//! Email delivery — port and implementations.
//!
//! # Purpose
//! Provide an abstraction over email sending so that tenant-team invitations
//! (and future password-reset/notification features) can send emails without
//! depending on a specific transport. SMTP is the built-in implementation;
//! a no-op logging fallback is used when SMTP is not configured.
//!
//! # Port
//! - `EmailMessage` — structured email data
//! - `EmailError` — error type
//! - `EmailDeliveryStatus` — delivery outcome
//! - `EmailSender` trait — `send` returns `EmailDeliveryStatus`
//!
//! # Implementations
//! - `SmtpEmailSender` — lettre SMTP with tokio + rustls
//! - `LogEmailSender` — info-level logging, always unconfigured

pub mod noop;
pub mod smtp;

use async_trait::async_trait;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct EmailMessage {
    pub to: String,
    pub subject: String,
    pub body_text: String,
    pub body_html: Option<String>,
}

#[derive(Debug)]
pub enum EmailError {
    Configuration(String),
    Send(String),
}

impl std::fmt::Display for EmailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Configuration(msg) => write!(f, "email configuration error: {msg}"),
            Self::Send(msg) => write!(f, "email send error: {msg}"),
        }
    }
}

impl std::error::Error for EmailError {}

/// Delivery outcome for an email message.
///
/// Serialized as a plain string in API responses — `Failed(String)` becomes
/// `"failed"` so that the frontend type `InvitationDeliveryStatus` remains
/// a simple union of four string literals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmailDeliveryStatus {
    Unconfigured,
    Queued,
    Sent,
    Failed(String),
}

impl Serialize for EmailDeliveryStatus {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl EmailDeliveryStatus {
    /// The short status label used for JSON serialization and DB storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unconfigured => "unconfigured",
            Self::Queued => "queued",
            Self::Sent => "sent",
            Self::Failed(_) => "failed",
        }
    }

    /// The error message carried by `Failed(String)`, if any.
    pub fn error_message(&self) -> Option<&str> {
        match self {
            Self::Failed(msg) => Some(msg.as_str()),
            _ => None,
        }
    }
}

#[async_trait]
pub trait EmailSender: Send + Sync {
    /// Whether the sender is configured for actual delivery.
    /// When `false`, the caller may still call `send` (it will log), but
    /// should report `email_sent: false` to the API consumer.
    fn is_configured(&self) -> bool;

    /// Deliver an email message, returning the delivery outcome.
    async fn send(&self, msg: EmailMessage) -> EmailDeliveryStatus;
}
