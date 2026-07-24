use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use integrations::provisioner::{ConnectionProvisioner, ProvisionContext};
use telegram::api::{ApiError, MockTelegramApi, TelegramApi};
use telegram::model::BotIdentity;
use telegram::provisioner::TelegramProvisioner;

// ---------------------------------------------------------------------------
// Call-recording wrapper around MockTelegramApi
// ---------------------------------------------------------------------------

struct RecordingApi {
    inner: MockTelegramApi,
    send_message_calls: Mutex<Vec<(String, i64, String)>>,
    set_webhook_calls: Mutex<Vec<(String, String, String)>>,
    delete_webhook_calls: Mutex<Vec<String>>,
}

#[async_trait]
impl TelegramApi for RecordingApi {
    async fn get_me(&self, token: &str) -> Result<BotIdentity, ApiError> {
        self.inner.get_me(token).await
    }

    async fn set_webhook(
        &self,
        token: &str,
        url: &str,
        secret: &str,
    ) -> Result<(), ApiError> {
        self.set_webhook_calls
            .lock()
            .unwrap()
            .push((token.to_string(), url.to_string(), secret.to_string()));
        self.inner.set_webhook(token, url, secret).await
    }

    async fn delete_webhook(&self, token: &str) -> Result<(), ApiError> {
        self.delete_webhook_calls
            .lock()
            .unwrap()
            .push(token.to_string());
        self.inner.delete_webhook(token).await
    }

    async fn send_message(
        &self,
        token: &str,
        chat_id: i64,
        text: &str,
    ) -> Result<i64, telegram::api::SendError> {
        self.send_message_calls
            .lock()
            .unwrap()
            .push((token.to_string(), chat_id, text.to_string()));
        self.inner.send_message(token, chat_id, text).await
    }

    async fn send_photo(
        &self,
        token: &str,
        chat_id: i64,
        bytes: Vec<u8>,
        filename: &str,
        caption: Option<&str>,
    ) -> Result<i64, telegram::api::SendError> {
        self.inner
            .send_photo(token, chat_id, bytes, filename, caption)
            .await
    }

    async fn send_document(
        &self,
        token: &str,
        chat_id: i64,
        bytes: Vec<u8>,
        filename: &str,
        caption: Option<&str>,
    ) -> Result<i64, telegram::api::SendError> {
        self.inner
            .send_document(token, chat_id, bytes, filename, caption)
            .await
    }

    async fn get_file(&self, token: &str, file_id: &str) -> Result<String, ApiError> {
        self.inner.get_file(token, file_id).await
    }

    async fn download_file(
        &self,
        token: &str,
        file_path: &str,
    ) -> Result<Vec<u8>, ApiError> {
        self.inner.download_file(token, file_path).await
    }
}

fn make_context(secrets: HashMap<String, String>) -> ProvisionContext {
    ProvisionContext {
        tenant_id: Uuid::new_v4(),
        slug: "telegram".into(),
        decrypted_secrets: secrets,
        webhook_token: "wht-abc123".into(),
        public_base_url: "https://app.example.com".into(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn on_connect_rejects_invalid_token() {
    let mock = MockTelegramApi::new();
    mock.push_get_me(Err(ApiError::UnexpectedResponse("bad token".into())));
    let recording = Arc::new(RecordingApi {
        inner: mock,
        send_message_calls: Mutex::new(Vec::new()),
        set_webhook_calls: Mutex::new(Vec::new()),
        delete_webhook_calls: Mutex::new(Vec::new()),
    });
    let provisioner = TelegramProvisioner::new(recording);

    let mut secrets = HashMap::new();
    secrets.insert("bot_token".into(), "test-token".into());
    let ctx = make_context(secrets);

    let result = provisioner.on_connect(ctx).await;
    let err = result.unwrap_err();
    assert_eq!(err.code, "telegram_token_invalid");
}

#[tokio::test]
async fn on_connect_returns_bot_identity_and_generates_secret() {
    let mock = MockTelegramApi::new();
    mock.push_get_me(Ok(BotIdentity {
        id: 42,
        username: "supportbot".into(),
    }));
    let recording = Arc::new(RecordingApi {
        inner: mock,
        send_message_calls: Mutex::new(Vec::new()),
        set_webhook_calls: Mutex::new(Vec::new()),
        delete_webhook_calls: Mutex::new(Vec::new()),
    });
    let provisioner = TelegramProvisioner::new(recording);

    let mut secrets = HashMap::new();
    secrets.insert("bot_token".into(), "test-token".into());
    let ctx = make_context(secrets);

    let outcome = provisioner.on_connect(ctx).await.unwrap();
    assert_eq!(outcome.config.get("botId"), Some(&json!(42)));
    assert_eq!(
        outcome.config.get("botUsername"),
        Some(&json!("supportbot"))
    );
    assert_eq!(outcome.secrets.len(), 1);
    assert_eq!(outcome.secrets[0].0, "webhook_secret");
    assert!(outcome.secrets[0].1.len() >= 32);
    assert!(!outcome.secrets[0].1.is_empty());
}

#[tokio::test]
async fn after_connect_calls_set_webhook_with_expected_url_and_secret() {
    let mock = MockTelegramApi::new();
    mock.push_set_webhook(Ok(()));
    let recording = Arc::new(RecordingApi {
        inner: mock,
        send_message_calls: Mutex::new(Vec::new()),
        set_webhook_calls: Mutex::new(Vec::new()),
        delete_webhook_calls: Mutex::new(Vec::new()),
    });
    let provisioner = TelegramProvisioner::new(recording.clone());

    let mut secrets = HashMap::new();
    secrets.insert("bot_token".into(), "test-token".into());
    secrets.insert("webhook_secret".into(), "test-secret".into());
    let ctx = make_context(secrets);

    let result = provisioner.after_connect(ctx).await;
    assert!(result.is_ok());

    let calls = recording.set_webhook_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].1,
        "https://app.example.com/integrations/telegram/webhook/wht-abc123"
    );
    assert_eq!(calls[0].2, "test-secret");
}

#[tokio::test]
async fn on_disconnect_calls_delete_webhook() {
    let mock = MockTelegramApi::new();
    mock.push_delete_webhook(Ok(()));
    let recording = Arc::new(RecordingApi {
        inner: mock,
        send_message_calls: Mutex::new(Vec::new()),
        set_webhook_calls: Mutex::new(Vec::new()),
        delete_webhook_calls: Mutex::new(Vec::new()),
    });
    let provisioner = TelegramProvisioner::new(recording.clone());

    let mut secrets = HashMap::new();
    secrets.insert("bot_token".into(), "test-token".into());
    let ctx = make_context(secrets);

    let result = provisioner.on_disconnect(ctx).await;
    assert!(result.is_ok());

    let calls = recording.delete_webhook_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
}
