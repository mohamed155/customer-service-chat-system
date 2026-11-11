//! Circuit breaker and bulkhead around a provider adapter (MOH-40, architecture §10.2).
//!
//! [`GuardedChat`] and [`GuardedEmbedding`] wrap an adapter with a [`resilience::Guard`], so a
//! provider that is down or slow is refused at once instead of tying up workers, and no more than
//! `max_concurrent` calls are in flight to one provider.
//!
//! Which errors count as the provider being unhealthy is decided by [`counts_as_failure`]. Only
//! `Unavailable` and `Timeout` do. `Authentication`, `InvalidRequest` and `RateLimited` are about
//! one tenant's key or request, and the provider did answer, so they never open the circuit: one
//! tenant's expired API key must not stop every other tenant's AI turns.
//!
//! A refused call returns [`ProviderError`] with category `Unavailable` and `retriable: false`, so
//! the retry loop does not sleep and retry against an open circuit, and the caller takes its
//! existing path for an unavailable provider (next provider in the fallback chain, otherwise the
//! honest customer message and escalation to a human).

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::Stream;
use resilience::{Guard, Permit, Rejection};

use crate::contract::*;

/// Does this error say the provider itself is unhealthy?
pub fn counts_as_failure(error: &ProviderError) -> bool {
    matches!(
        error.category,
        ErrorCategory::Unavailable | ErrorCategory::Timeout
    )
}

fn refused(guard: &Guard, why: Rejection) -> ProviderError {
    let detail = match why {
        Rejection::CircuitOpen => format!("circuit open for {}; call not sent", guard.name()),
        Rejection::BulkheadFull => format!(
            "too many calls in flight to {} (limit {}); call not sent",
            guard.name(),
            guard.policy().max_concurrent
        ),
    };
    ProviderError {
        category: ErrorCategory::Unavailable,
        retriable: false,
        detail,
    }
}

fn record<T>(permit: &mut Permit, result: &Result<T, ProviderError>) {
    match result {
        Err(error) if counts_as_failure(error) => permit.failure(),
        _ => permit.success(),
    }
}

pub struct GuardedChat {
    inner: Arc<dyn ChatProvider>,
    guard: Arc<Guard>,
}

impl GuardedChat {
    pub fn new(inner: Arc<dyn ChatProvider>, guard: Arc<Guard>) -> Self {
        Self { inner, guard }
    }
}

/// A response stream that keeps its bulkhead slot until it is finished or dropped.
struct HeldStream {
    inner: ChatStream,
    _permit: Permit,
}

impl Stream for HeldStream {
    type Item = Result<StreamEvent, ProviderError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

#[async_trait::async_trait]
impl ChatProvider for GuardedChat {
    async fn complete(
        &self,
        key: &SecretKey,
        req: &ChatRequest,
    ) -> Result<ChatCompletion, ProviderError> {
        let mut permit = self
            .guard
            .acquire()
            .await
            .map_err(|why| refused(&self.guard, why))?;
        let result = self.inner.complete(key, req).await;
        record(&mut permit, &result);
        result
    }

    /// The circuit judges the connection (the call that returns the stream). The bulkhead slot is
    /// held until the stream ends or is dropped.
    async fn stream(
        &self,
        key: &SecretKey,
        req: &ChatRequest,
    ) -> Result<ChatStream, ProviderError> {
        let mut permit = self
            .guard
            .acquire()
            .await
            .map_err(|why| refused(&self.guard, why))?;
        let result = self.inner.stream(key, req).await;
        record(&mut permit, &result);
        result.map(|inner| {
            Box::pin(HeldStream {
                inner,
                _permit: permit,
            }) as ChatStream
        })
    }
}

pub struct GuardedEmbedding {
    inner: Arc<dyn EmbeddingProvider>,
    guard: Arc<Guard>,
}

impl GuardedEmbedding {
    pub fn new(inner: Arc<dyn EmbeddingProvider>, guard: Arc<Guard>) -> Self {
        Self { inner, guard }
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for GuardedEmbedding {
    async fn embed(
        &self,
        key: &SecretKey,
        req: &EmbeddingRequest,
    ) -> Result<EmbeddingResponse, ProviderError> {
        let mut permit = self
            .guard
            .acquire()
            .await
            .map_err(|why| refused(&self.guard, why))?;
        let result = self.inner.embed(key, req).await;
        record(&mut permit, &result);
        result
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use resilience::{Policy, State};

    use super::*;

    /// Answers with a scripted category, or a completion when the script says `None`. Counts calls.
    struct Scripted {
        outcome: std::sync::Mutex<Option<ErrorCategory>>,
        calls: AtomicUsize,
    }

    impl Scripted {
        fn new(outcome: Option<ErrorCategory>) -> Arc<Self> {
            Arc::new(Self {
                outcome: std::sync::Mutex::new(outcome),
                calls: AtomicUsize::new(0),
            })
        }
        fn set(&self, outcome: Option<ErrorCategory>) {
            *self.outcome.lock().unwrap() = outcome;
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    fn error(category: ErrorCategory) -> ProviderError {
        ProviderError {
            category,
            retriable: category.retriable(),
            detail: "scripted".into(),
        }
    }

    #[async_trait::async_trait]
    impl ChatProvider for Scripted {
        async fn complete(
            &self,
            _key: &SecretKey,
            _req: &ChatRequest,
        ) -> Result<ChatCompletion, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match *self.outcome.lock().unwrap() {
                Some(category) => Err(error(category)),
                None => Ok(ChatCompletion {
                    content: "hi".into(),
                    model: "m".into(),
                    usage: TokenUsage::default(),
                    finish: FinishReason::Stop,
                    tool_calls: vec![],
                }),
            }
        }

        async fn stream(
            &self,
            _key: &SecretKey,
            _req: &ChatRequest,
        ) -> Result<ChatStream, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match *self.outcome.lock().unwrap() {
                Some(category) => Err(error(category)),
                None => Ok(Box::pin(futures::stream::empty())),
            }
        }
    }

    #[async_trait::async_trait]
    impl EmbeddingProvider for Scripted {
        async fn embed(
            &self,
            _key: &SecretKey,
            _req: &EmbeddingRequest,
        ) -> Result<EmbeddingResponse, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match *self.outcome.lock().unwrap() {
                Some(category) => Err(error(category)),
                None => Ok(EmbeddingResponse {
                    embeddings: vec![],
                    model: "m".into(),
                    usage: TokenUsage::default(),
                }),
            }
        }
    }

    fn policy() -> Policy {
        Policy {
            failure_threshold: 3,
            failure_window: Duration::from_secs(60),
            open_duration: Duration::from_secs(30),
            max_concurrent: 1,
            acquire_timeout: Duration::from_millis(100),
        }
    }

    fn request() -> ChatRequest {
        ChatRequest {
            system: None,
            messages: vec![],
            model: "m".into(),
            max_output_tokens: None,
            temperature: None,
            request_id: None,
            tools: vec![],
        }
    }

    fn key() -> SecretKey {
        SecretKey::new("k".into())
    }

    #[tokio::test(start_paused = true)]
    async fn opens_after_repeated_unavailable_and_stops_calling_the_provider() {
        let inner = Scripted::new(Some(ErrorCategory::Unavailable));
        let guard = Guard::new("ai:test", policy());
        let provider = GuardedChat::new(inner.clone(), guard.clone());

        for _ in 0..3 {
            let err = provider.complete(&key(), &request()).await.unwrap_err();
            assert_eq!(err.category, ErrorCategory::Unavailable);
            assert!(err.retriable, "the real error is passed through");
        }
        assert_eq!(guard.snapshot().state, State::Open);

        let refused = provider.complete(&key(), &request()).await.unwrap_err();
        assert_eq!(refused.category, ErrorCategory::Unavailable);
        assert!(!refused.retriable, "no retry loop against an open circuit");
        assert!(refused.detail.contains("circuit open"));
        assert_eq!(
            inner.calls(),
            3,
            "the fourth call never reached the provider"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn timeouts_count_but_key_and_request_errors_do_not() {
        let inner = Scripted::new(Some(ErrorCategory::Authentication));
        let guard = Guard::new("ai:test", policy());
        let provider = GuardedChat::new(inner.clone(), guard.clone());

        for category in [
            ErrorCategory::Authentication,
            ErrorCategory::InvalidRequest,
            ErrorCategory::RateLimited,
        ] {
            inner.set(Some(category));
            for _ in 0..5 {
                provider.complete(&key(), &request()).await.unwrap_err();
            }
        }
        let snap = guard.snapshot();
        assert_eq!(
            snap.state,
            State::Closed,
            "one tenant's bad key must not open it"
        );
        assert_eq!(snap.consecutive_failures, 0);
        assert_eq!(inner.calls(), 15);

        inner.set(Some(ErrorCategory::Timeout));
        for _ in 0..3 {
            provider.complete(&key(), &request()).await.unwrap_err();
        }
        assert_eq!(guard.snapshot().state, State::Open);
    }

    #[tokio::test(start_paused = true)]
    async fn recovers_through_half_open() {
        let inner = Scripted::new(Some(ErrorCategory::Unavailable));
        let guard = Guard::new("ai:test", policy());
        let provider = GuardedChat::new(inner.clone(), guard.clone());
        for _ in 0..3 {
            provider.complete(&key(), &request()).await.unwrap_err();
        }
        assert_eq!(guard.snapshot().state, State::Open);

        // Still failing at the first probe: the circuit opens again.
        tokio::time::advance(Duration::from_secs(30)).await;
        provider.complete(&key(), &request()).await.unwrap_err();
        assert_eq!(guard.snapshot().state, State::Open);
        assert_eq!(inner.calls(), 4);

        // The provider is back: the next probe closes it, and calls flow again.
        inner.set(None);
        tokio::time::advance(Duration::from_secs(30)).await;
        provider
            .complete(&key(), &request())
            .await
            .expect("probe succeeds");
        assert_eq!(guard.snapshot().state, State::Closed);
        provider.complete(&key(), &request()).await.expect("closed");
        assert_eq!(inner.calls(), 6);
    }

    #[tokio::test(start_paused = true)]
    async fn bulkhead_limits_calls_in_flight_to_one_provider() {
        let inner = Scripted::new(None);
        let guard = Guard::new("ai:test", policy());
        let provider = GuardedChat::new(inner.clone(), guard.clone());

        // An open stream holds the only slot.
        let stream = provider.stream(&key(), &request()).await.expect("stream");
        let refused = provider.complete(&key(), &request()).await.unwrap_err();
        assert_eq!(refused.category, ErrorCategory::Unavailable);
        assert!(!refused.retriable);
        assert!(refused.detail.contains("too many calls in flight"));
        assert_eq!(inner.calls(), 1);
        assert_eq!(guard.snapshot().state, State::Closed);

        drop(stream);
        provider
            .complete(&key(), &request())
            .await
            .expect("slot freed");
    }

    #[tokio::test(start_paused = true)]
    async fn embeddings_are_guarded_too() {
        let inner = Scripted::new(Some(ErrorCategory::Unavailable));
        let guard = Guard::new("ai:test:embed", policy());
        let provider = GuardedEmbedding::new(inner.clone(), guard.clone());
        let req = EmbeddingRequest {
            model: "m".into(),
            inputs: vec![],
            request_id: None,
        };
        for _ in 0..3 {
            provider.embed(&key(), &req).await.unwrap_err();
        }
        let refused = provider.embed(&key(), &req).await.unwrap_err();
        assert!(refused.detail.contains("circuit open"));
        assert_eq!(inner.calls(), 3);
    }
}
