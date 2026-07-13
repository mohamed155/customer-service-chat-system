//! SMTP email sender implementation using lettre.

use async_trait::async_trait;
use lettre::{
    message::Mailbox, transport::smtp::authentication::Credentials, AsyncSmtpTransport,
    AsyncTransport, Message, Tokio1Executor,
};
use percent_encoding::percent_decode_str;
use url::Url;

use crate::{EmailDeliveryStatus, EmailError, EmailMessage, EmailSender};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SmtpTransportMode {
    StartTls,
    ImplicitTls,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SmtpConfig {
    mode: SmtpTransportMode,
    host: String,
    port: u16,
    credentials: Option<(String, String)>,
}

/// Sends email via SMTP using lettre with tokio + rustls.
pub struct SmtpEmailSender {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpEmailSender {
    /// Construct from an SMTP URL and a sender address.
    ///
    /// The SMTP URL format is `smtp://[user:pass@]host[:port]` or
    /// `smtps://[user:pass@]host[:port]` for implicit TLS.
    pub fn new(smtp_url: &str, smtp_from: &str) -> Result<Self, EmailError> {
        let from: Mailbox = smtp_from
            .parse()
            .map_err(|e| EmailError::Configuration(format!("invalid sender address: {e}")))?;

        let config = parse_smtp_url(smtp_url)?;

        let mut builder = match config.mode {
            SmtpTransportMode::StartTls => {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
                    .map_err(|e| EmailError::Configuration(format!("SMTP relay error: {e}")))?
            }
            SmtpTransportMode::ImplicitTls => {
                AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
                    .map_err(|e| EmailError::Configuration(format!("SMTP relay error: {e}")))?
            }
        };

        builder = builder.port(config.port);

        if let Some((user, password)) = config.credentials {
            builder = builder.credentials(Credentials::new(user, password));
        }

        Ok(Self {
            transport: builder.build(),
            from,
        })
    }
}

fn parse_smtp_url(url: &str) -> Result<SmtpConfig, EmailError> {
    let parsed =
        Url::parse(url).map_err(|e| EmailError::Configuration(format!("invalid SMTP URL: {e}")))?;

    let mode = match parsed.scheme() {
        "smtp" => SmtpTransportMode::StartTls,
        "smtps" => SmtpTransportMode::ImplicitTls,
        other => {
            return Err(EmailError::Configuration(format!(
                "unsupported SMTP URL scheme: {other}"
            )));
        }
    };

    let host = parsed
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| EmailError::Configuration("SMTP URL must include a host".to_string()))?
        .trim_matches(['[', ']'])
        .to_owned();

    let port = parsed.port().unwrap_or(match mode {
        SmtpTransportMode::StartTls => 587,
        SmtpTransportMode::ImplicitTls => 465,
    });

    let credentials = if parsed.username().is_empty() && parsed.password().is_none() {
        None
    } else {
        Some((
            percent_decode_str(parsed.username())
                .decode_utf8_lossy()
                .to_string(),
            percent_decode_str(parsed.password().unwrap_or_default())
                .decode_utf8_lossy()
                .to_string(),
        ))
    };

    Ok(SmtpConfig {
        mode,
        host,
        port,
        credentials,
    })
}

#[async_trait]
impl EmailSender for SmtpEmailSender {
    fn is_configured(&self) -> bool {
        true
    }

    async fn send(&self, msg: EmailMessage) -> EmailDeliveryStatus {
        let to: Mailbox = match msg.to.parse() {
            Ok(to) => to,
            Err(e) => {
                return EmailDeliveryStatus::Failed(format!("invalid recipient address: {e}"))
            }
        };

        let mut builder = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(&msg.subject);

        if msg.body_html.is_some() {
            builder = builder.header(lettre::message::header::ContentType::TEXT_HTML);
        }

        let email = match if let Some(body_html) = &msg.body_html {
            builder.body(body_html.clone())
        } else {
            builder.body(msg.body_text.clone())
        } {
            Ok(email) => email,
            Err(e) => return EmailDeliveryStatus::Failed(format!("failed to build message: {e}")),
        };

        match self.transport.send(email).await {
            Ok(_) => EmailDeliveryStatus::Sent,
            Err(e) => EmailDeliveryStatus::Failed(format!("SMTP send failed: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_smtp_url_uses_starttls_for_smtp_urls() {
        let config = parse_smtp_url("smtp://user:pass@smtp.example.com:587").unwrap();
        assert_eq!(config.mode, SmtpTransportMode::StartTls);
        assert_eq!(config.host, "smtp.example.com");
        assert_eq!(config.port, 587);
        assert_eq!(config.credentials, Some(("user".into(), "pass".into())));
    }

    #[test]
    fn parse_smtps_url_uses_implicit_tls_and_default_port() {
        let config = parse_smtp_url("smtps://smtp.example.com").unwrap();
        assert_eq!(config.mode, SmtpTransportMode::ImplicitTls);
        assert_eq!(config.host, "smtp.example.com");
        assert_eq!(config.port, 465);
        assert!(config.credentials.is_none());
    }

    #[test]
    fn parse_smtp_url_supports_encoded_credentials_and_ipv6() {
        let config =
            parse_smtp_url("smtp://user%40example.com:pa%24%24@[2001:db8::1]:2525").unwrap();
        assert_eq!(config.host, "2001:db8::1");
        assert_eq!(config.port, 2525);
        assert_eq!(
            config.credentials,
            Some(("user@example.com".into(), "pa$$".into()))
        );
    }

    #[test]
    fn parse_smtp_url_rejects_bad_inputs() {
        assert!(parse_smtp_url("http://smtp.example.com").is_err());
        assert!(parse_smtp_url("smtp://").is_err());
        assert!(parse_smtp_url("smtp://smtp.example.com:bad").is_err());
    }
}
