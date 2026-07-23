use async_trait::async_trait;
use bytes::Bytes;
use reqwest::header;
use serde::Deserialize;

#[derive(Debug)]
pub struct MediaInfo {
    pub url: String,
    pub mime_type: String,
}

#[derive(Debug)]
pub enum SendError {
    WindowExpired,
    Unauthorized,
    Transient(String),
    Other(String),
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WindowExpired => write!(f, "messaging window expired"),
            Self::Unauthorized => write!(f, "unauthorized"),
            Self::Transient(msg) => write!(f, "transient error: {msg}"),
            Self::Other(msg) => write!(f, "error: {msg}"),
        }
    }
}

impl std::error::Error for SendError {}

#[async_trait]
pub trait WhatsAppApi: Send + Sync {
    async fn send_text(
        &self,
        access_token: &str,
        phone_number_id: &str,
        to_e164: &str,
        body: &str,
    ) -> Result<String, SendError>;

    async fn media_url(
        &self,
        access_token: &str,
        media_id: &str,
    ) -> Result<MediaInfo, SendError>;

    async fn download(
        &self,
        access_token: &str,
        url: &str,
    ) -> Result<Bytes, SendError>;
}

pub struct GraphWhatsAppApi {
    http: reqwest::Client,
    base_url: String,
}

impl GraphWhatsAppApi {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: "https://graph.facebook.com/v23.0".to_string(),
        }
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into(),
        }
    }
}

impl Default for GraphWhatsAppApi {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Deserialize)]
struct SendTextResponse {
    messages: Vec<SendTextMessage>,
}

#[derive(Deserialize)]
struct SendTextMessage {
    id: String,
}

#[derive(Deserialize)]
struct MediaUrlResponse {
    url: String,
    mime_type: String,
}

#[async_trait]
impl WhatsAppApi for GraphWhatsAppApi {
    async fn send_text(
        &self,
        access_token: &str,
        phone_number_id: &str,
        to_e164: &str,
        body: &str,
    ) -> Result<String, SendError> {
        let url = format!("{}/{}/messages", self.base_url, phone_number_id);
        let payload = serde_json::json!({
            "messaging_product": "whatsapp",
            "recipient_type": "individual",
            "to": to_e164,
            "type": "text",
            "text": {
                "preview_url": false,
                "body": body,
            },
        });

        let response = self
            .http
            .post(&url)
            .header(header::AUTHORIZATION, format!("Bearer {access_token}"))
            .json(&payload)
            .send()
            .await
            .map_err(|e| SendError::Transient(format!("request failed: {e}")))?;

        let status = response.status();

        if status == 401 || status == 403 {
            return Err(SendError::Unauthorized);
        }

        if status.is_server_error() {
            return Err(SendError::Transient(format!("server returned {status}")));
        }

        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();

            if body_text.contains("131047") {
                return Err(SendError::WindowExpired);
            }

            return Err(SendError::Other(format!("{status}: {body_text}")));
        }

        let send_resp: SendTextResponse = response
            .json()
            .await
            .map_err(|e| SendError::Other(format!("failed to parse response: {e}")))?;

        send_resp
            .messages
            .into_iter()
            .next()
            .map(|m| m.id)
            .ok_or_else(|| SendError::Other("no message id in response".into()))
    }

    async fn media_url(
        &self,
        access_token: &str,
        media_id: &str,
    ) -> Result<MediaInfo, SendError> {
        let url = format!("{}/{media_id}", self.base_url);

        let response = self
            .http
            .get(&url)
            .header(header::AUTHORIZATION, format!("Bearer {access_token}"))
            .send()
            .await
            .map_err(|e| SendError::Transient(format!("request failed: {e}")))?;

        let status = response.status();

        if status == 401 || status == 403 {
            return Err(SendError::Unauthorized);
        }

        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(SendError::Other(format!("{status}: {body_text}")));
        }

        let media: MediaUrlResponse = response
            .json()
            .await
            .map_err(|e| SendError::Other(format!("failed to parse media response: {e}")))?;

        Ok(MediaInfo {
            url: media.url,
            mime_type: media.mime_type,
        })
    }

    async fn download(
        &self,
        access_token: &str,
        url: &str,
    ) -> Result<Bytes, SendError> {
        let response = self
            .http
            .get(url)
            .header(header::AUTHORIZATION, format!("Bearer {access_token}"))
            .send()
            .await
            .map_err(|e| SendError::Transient(format!("download failed: {e}")))?;

        let status = response.status();

        if status == 401 || status == 403 {
            return Err(SendError::Unauthorized);
        }

        if !status.is_success() {
            return Err(SendError::Other(format!("download returned {status}")));
        }

        response
            .bytes()
            .await
            .map_err(|e| SendError::Transient(format!("failed to read body: {e}")))
    }
}

pub struct MockWhatsAppApi {
    send_text_responses: std::sync::Mutex<Vec<Result<String, SendError>>>,
    media_url_responses: std::sync::Mutex<Vec<Result<MediaInfo, SendError>>>,
    download_responses: std::sync::Mutex<Vec<Result<Bytes, SendError>>>,
}

impl MockWhatsAppApi {
    pub fn new() -> Self {
        Self {
            send_text_responses: std::sync::Mutex::new(Vec::new()),
            media_url_responses: std::sync::Mutex::new(Vec::new()),
            download_responses: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn push_send_text(&self, result: Result<String, SendError>) {
        self.send_text_responses.lock().unwrap().push(result);
    }

    pub fn push_media_url(&self, result: Result<MediaInfo, SendError>) {
        self.media_url_responses.lock().unwrap().push(result);
    }

    pub fn push_download(&self, result: Result<Bytes, SendError>) {
        self.download_responses.lock().unwrap().push(result);
    }
}

#[async_trait]
impl WhatsAppApi for MockWhatsAppApi {
    async fn send_text(
        &self,
        _access_token: &str,
        _phone_number_id: &str,
        _to_e164: &str,
        _body: &str,
    ) -> Result<String, SendError> {
        self.send_text_responses
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| {
                Err(SendError::Other(
                    "no mock response configured for send_text".into(),
                ))
            })
    }

    async fn media_url(
        &self,
        _access_token: &str,
        _media_id: &str,
    ) -> Result<MediaInfo, SendError> {
        self.media_url_responses
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| {
                Err(SendError::Other(
                    "no mock response configured for media_url".into(),
                ))
            })
    }

    async fn download(
        &self,
        _access_token: &str,
        _url: &str,
    ) -> Result<Bytes, SendError> {
        self.download_responses
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| {
                Err(SendError::Other(
                    "no mock response configured for download".into(),
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_send_error_window_expired() {
        let err = SendError::WindowExpired;
        assert_eq!(format!("{err}"), "messaging window expired");
    }

    #[test]
    fn display_send_error_unauthorized() {
        let err = SendError::Unauthorized;
        assert_eq!(format!("{err}"), "unauthorized");
    }

    #[test]
    fn display_send_error_transient() {
        let err = SendError::Transient("timeout".into());
        assert_eq!(format!("{err}"), "transient error: timeout");
    }

    #[test]
    fn display_send_error_other() {
        let err = SendError::Other("something broke".into());
        assert_eq!(format!("{err}"), "error: something broke");
    }

    #[test]
    fn mock_send_text_returns_configured_value() {
        let mock = MockWhatsAppApi::new();
        mock.push_send_text(Ok("wamid.abc123".into()));
        let result = mock.send_text("tok", "pid", "+15551234567", "hello");
        let id = futures::executor::block_on(result).unwrap();
        assert_eq!(id, "wamid.abc123");
    }

    #[test]
    fn mock_send_text_returns_error_when_empty() {
        let mock = MockWhatsAppApi::new();
        let result = mock.send_text("tok", "pid", "+15551234567", "hello");
        let err = futures::executor::block_on(result).unwrap_err();
        assert!(matches!(err, SendError::Other(_)));
    }

    #[test]
    fn mock_media_url_returns_configured_value() {
        let mock = MockWhatsAppApi::new();
        mock.push_media_url(Ok(MediaInfo {
            url: "https://cdn.example.com/media.jpg".into(),
            mime_type: "image/jpeg".into(),
        }));
        let result = mock.media_url("tok", "media123");
        let info = futures::executor::block_on(result).unwrap();
        assert_eq!(info.url, "https://cdn.example.com/media.jpg");
        assert_eq!(info.mime_type, "image/jpeg");
    }

    #[test]
    fn mock_download_returns_configured_value() {
        let mock = MockWhatsAppApi::new();
        mock.push_download(Ok(Bytes::from("binary data")));
        let result = mock.download("tok", "https://cdn.example.com/media.jpg");
        let data = futures::executor::block_on(result).unwrap();
        assert_eq!(data, Bytes::from("binary data"));
    }

    #[test]
    fn error_impl_send_sync() {
        fn assert_send<T: Send + Sync>() {}
        assert_send::<SendError>();
    }

    #[test]
    fn trait_impl_send_sync() {
        fn assert_send<T: Send + Sync>() {}
        assert_send::<&dyn WhatsAppApi>();
    }
}
