//! Circuit breaker and bulkhead around the WhatsApp Cloud API (MOH-40, architecture §15).
//!
//! [`GuardedWhatsAppApi`] wraps any [`WhatsAppApi`]. Two guards, because they are two hosts that
//! fail independently: `whatsapp:graph` for sending and media lookup (the Graph API) and
//! `whatsapp:media` for downloads (Meta's CDN).
//!
//! Only [`SendError::Transient`] (5xx, connection errors, timeouts) counts against the circuit.
//! `Unauthorized` is one tenant's revoked token, `WindowExpired` is the 24-hour rule and `Other` is
//! a 4xx about one message. WhatsApp answered all of them, so one tenant's expired credentials never
//! stop everyone else's messages.
//!
//! A refused call returns [`SendError::Refused`], which the workers treat as "not sent, try again
//! later", not as a failed attempt. [`available`](WhatsAppApi::available) and
//! [`media_available`](WhatsAppApi::media_available) are false while the circuits they need are
//! open, so the sender and media workers stop claiming work (see `sender.rs` and `media.rs`).

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use resilience::{Guard, Hub, Permit, Policy, Rejection};

use crate::api::{MediaInfo, SendError, WhatsAppApi};

/// Guard name for sending and media lookup.
pub const GRAPH_GUARD: &str = "whatsapp:graph";
/// Guard name for media downloads.
pub const MEDIA_GUARD: &str = "whatsapp:media";

/// Settings for the WhatsApp guards: fewer parallel calls than the AI providers, since each worker
/// handles one message at a time.
pub fn default_policy() -> Policy {
    Policy {
        max_concurrent: 8,
        ..Policy::default()
    }
}

pub struct GuardedWhatsAppApi {
    inner: Arc<dyn WhatsAppApi>,
    graph: Arc<Guard>,
    media: Arc<Guard>,
}

impl GuardedWhatsAppApi {
    pub fn new(inner: Arc<dyn WhatsAppApi>, hub: &Hub, policy: Policy) -> Self {
        Self {
            inner,
            graph: hub.guard(GRAPH_GUARD, policy),
            media: hub.guard(MEDIA_GUARD, policy),
        }
    }
}

fn refused(guard: &Guard, why: Rejection) -> SendError {
    SendError::Refused(match why {
        Rejection::CircuitOpen => format!("circuit open for {}; call not sent", guard.name()),
        Rejection::BulkheadFull => format!(
            "too many calls in flight to {} (limit {}); call not sent",
            guard.name(),
            guard.policy().max_concurrent
        ),
    })
}

fn record<T>(permit: &mut Permit, result: &Result<T, SendError>) {
    match result {
        Err(SendError::Transient(_)) => permit.failure(),
        _ => permit.success(),
    }
}

#[async_trait]
impl WhatsAppApi for GuardedWhatsAppApi {
    async fn send_text(
        &self,
        access_token: &str,
        phone_number_id: &str,
        to_e164: &str,
        body: &str,
    ) -> Result<String, SendError> {
        let mut permit = self
            .graph
            .acquire()
            .await
            .map_err(|why| refused(&self.graph, why))?;
        let result = self
            .inner
            .send_text(access_token, phone_number_id, to_e164, body)
            .await;
        record(&mut permit, &result);
        result
    }

    async fn media_url(&self, access_token: &str, media_id: &str) -> Result<MediaInfo, SendError> {
        let mut permit = self
            .graph
            .acquire()
            .await
            .map_err(|why| refused(&self.graph, why))?;
        let result = self.inner.media_url(access_token, media_id).await;
        record(&mut permit, &result);
        result
    }

    async fn download(&self, access_token: &str, url: &str) -> Result<Bytes, SendError> {
        let mut permit = self
            .media
            .acquire()
            .await
            .map_err(|why| refused(&self.media, why))?;
        let result = self.inner.download(access_token, url).await;
        record(&mut permit, &result);
        result
    }

    fn available(&self) -> bool {
        !self.graph.would_reject() && self.inner.available()
    }

    fn media_available(&self) -> bool {
        !self.graph.would_reject() && !self.media.would_reject() && self.inner.media_available()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use resilience::State;

    use super::*;
    use crate::api::MockWhatsAppApi;

    fn policy() -> Policy {
        Policy {
            failure_threshold: 3,
            max_concurrent: 1,
            acquire_timeout: Duration::from_millis(100),
            ..Policy::default()
        }
    }

    fn guarded(mock: &Arc<MockWhatsAppApi>) -> (GuardedWhatsAppApi, Arc<Hub>) {
        let hub = Hub::new();
        let api = GuardedWhatsAppApi::new(mock.clone(), &hub, policy());
        (api, hub)
    }

    async fn send(api: &GuardedWhatsAppApi) -> Result<String, SendError> {
        api.send_text("tok", "123", "+15550000000", "hi").await
    }

    fn graph_state(hub: &Hub) -> State {
        hub.get(GRAPH_GUARD).unwrap().snapshot().state
    }

    #[tokio::test(start_paused = true)]
    async fn opens_after_repeated_transient_errors_and_stops_sending() {
        let mock = Arc::new(MockWhatsAppApi::new());
        for _ in 0..3 {
            mock.push_send_text(Err(SendError::Transient("server returned 503".into())));
        }
        let (api, hub) = guarded(&mock);
        assert!(api.available());

        for _ in 0..3 {
            assert!(matches!(send(&api).await, Err(SendError::Transient(m)) if m.contains("503")));
        }
        assert_eq!(graph_state(&hub), State::Open);
        assert!(!api.available(), "workers stop claiming work");

        // The mock has no scripted result left: reaching it again would be a different error.
        match send(&api).await {
            Err(SendError::Refused(m)) => assert!(m.contains("circuit open"), "{m}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn one_tenants_revoked_token_does_not_open_the_circuit() {
        let mock = Arc::new(MockWhatsAppApi::new());
        for _ in 0..10 {
            mock.push_send_text(Err(SendError::Unauthorized));
            mock.push_send_text(Err(SendError::WindowExpired));
            mock.push_send_text(Err(SendError::Other("400 bad number".into())));
        }
        let (api, hub) = guarded(&mock);
        for _ in 0..30 {
            send(&api).await.unwrap_err();
        }
        assert_eq!(graph_state(&hub), State::Closed);
        assert!(api.available());
    }

    #[tokio::test(start_paused = true)]
    async fn recovers_through_half_open_when_whatsapp_answers_again() {
        let mock = Arc::new(MockWhatsAppApi::new());
        for _ in 0..3 {
            mock.push_send_text(Err(SendError::Transient("503".into())));
        }
        let (api, hub) = guarded(&mock);
        for _ in 0..3 {
            send(&api).await.unwrap_err();
        }
        assert!(!api.available());

        tokio::time::advance(Duration::from_secs(30)).await;
        assert!(api.available(), "a probe is due");
        mock.push_send_text(Ok("wamid.1".into()));
        assert_eq!(send(&api).await.unwrap(), "wamid.1");
        assert_eq!(graph_state(&hub), State::Closed);
    }

    #[tokio::test(start_paused = true)]
    async fn media_downloads_have_their_own_circuit() {
        let mock = Arc::new(MockWhatsAppApi::new());
        for _ in 0..3 {
            mock.push_download(Err(SendError::Transient("cdn 502".into())));
        }
        let (api, hub) = guarded(&mock);
        for _ in 0..3 {
            api.download("tok", "https://cdn.example/x")
                .await
                .unwrap_err();
        }
        assert_eq!(hub.get(MEDIA_GUARD).unwrap().snapshot().state, State::Open);
        assert_eq!(graph_state(&hub), State::Closed, "sending is unaffected");
        assert!(api.available());
    }
    // Access token and phone number ID are tenant data. A token that is not a valid header value
    // never leaves this process, so it must not count against the shared circuit.
    #[tokio::test(start_paused = true)]
    async fn a_tenants_unusable_token_cannot_open_the_shared_circuit() {
        let hub = Hub::new();
        let real = Arc::new(crate::api::GraphWhatsAppApi::with_base_url(
            "http://127.0.0.1:9", // never reached: the request can't be built
        ));
        let api = GuardedWhatsAppApi::new(real, &hub, policy());

        for _ in 0..10 {
            let err = api
                .send_text("bad\ntoken", "123", "+15550000000", "hi")
                .await
                .unwrap_err();
            assert!(matches!(err, SendError::Other(_)), "{err:?}");
        }
        assert_eq!(graph_state(&hub), State::Closed);
        assert!(api.available());
    }
    // Server errors from the media endpoints must count: the adapter reports them as Transient.
    // Real time: paused time would jump forward while the test waits on the network, and age the
    // failure streak out.
    #[tokio::test]
    async fn media_server_errors_open_the_circuits_through_the_real_adapter() {
        use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&mock)
            .await;
        let hub = Hub::new();
        let real = Arc::new(crate::api::GraphWhatsAppApi::with_base_url(mock.uri()));
        let api = GuardedWhatsAppApi::new(real, &hub, policy());

        for _ in 0..3 {
            let err = api.media_url("tok", "media-1").await.unwrap_err();
            assert!(matches!(err, SendError::Transient(_)), "{err:?}");
        }
        assert_eq!(
            graph_state(&hub),
            State::Open,
            "media lookup 503s opened the Graph circuit"
        );

        for _ in 0..3 {
            let err = api
                .download("tok", &format!("{}/file", mock.uri()))
                .await
                .unwrap_err();
            assert!(matches!(err, SendError::Transient(_)), "{err:?}");
        }
        assert_eq!(hub.get(MEDIA_GUARD).unwrap().snapshot().state, State::Open);
    }

    #[tokio::test(start_paused = true)]
    async fn an_open_download_circuit_stops_media_work_but_not_sending() {
        let mock = Arc::new(MockWhatsAppApi::new());
        for _ in 0..3 {
            mock.push_download(Err(SendError::Transient("cdn 502".into())));
        }
        let (api, _hub) = guarded(&mock);
        assert!(api.available() && api.media_available());
        for _ in 0..3 {
            api.download("tok", "https://cdn.example/x")
                .await
                .unwrap_err();
        }
        assert!(api.available(), "text messages still go out");
        assert!(!api.media_available(), "attachments are left unclaimed");
    }
}
