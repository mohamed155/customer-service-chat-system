use axum::{
    body::{to_bytes, Body},
    extract::FromRequestParts,
    http::{header::HeaderName, request::Parts, HeaderValue, Request, Response, StatusCode},
    middleware::Next,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyKey(pub String);

impl<S> FromRequestParts<S> for IdempotencyKey
where
    S: Send + Sync,
{
    type Rejection = StatusCode;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .headers
            .get("Idempotency-Key")
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty())
            .map(|value| Self(value.to_owned()))
            .ok_or(StatusCode::BAD_REQUEST)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedResponse {
    pub status: StatusCode,
    pub body: Vec<u8>,
}

pub trait IdempotencyStore: Send + Sync {
    fn get(&self, key: &str) -> Option<CachedResponse>;
    fn put(&self, key: String, response: CachedResponse);
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryIdempotencyStore(Arc<Mutex<HashMap<String, CachedResponse>>>);

impl IdempotencyStore for InMemoryIdempotencyStore {
    fn get(&self, key: &str) -> Option<CachedResponse> {
        self.0
            .lock()
            .expect("store lock poisoned")
            .get(key)
            .cloned()
    }
    fn put(&self, key: String, response: CachedResponse) {
        self.0
            .lock()
            .expect("store lock poisoned")
            .insert(key, response);
    }
}

impl CachedResponse {
    pub fn replay_headers() -> [(HeaderName, HeaderValue); 1] {
        [(
            HeaderName::from_static("idempotency-replayed"),
            HeaderValue::from_static("true"),
        )]
    }

    pub fn into_replayed_response(self) -> Response<Body> {
        let mut response = Response::new(Body::from(self.body));
        *response.status_mut() = self.status;
        for (name, value) in Self::replay_headers() {
            response.headers_mut().insert(name, value);
        }
        response
    }
}

pub async fn idempotency_middleware(
    store: InMemoryIdempotencyStore,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let key = request
        .headers()
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let Some(key) = key else {
        return next.run(request).await;
    };
    if let Some(cached) = store.get(&key) {
        return cached.into_replayed_response();
    }
    let response = next.run(request).await;
    let (parts, body) = response.into_parts();
    let body = to_bytes(body, usize::MAX).await.unwrap_or_default();
    store.put(
        key,
        CachedResponse {
            status: parts.status,
            body: body.to_vec(),
        },
    );
    Response::from_parts(parts, Body::from(body))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_returns_cached_response_and_header() {
        let store = InMemoryIdempotencyStore::default();
        let response = CachedResponse {
            status: StatusCode::CREATED,
            body: b"created".to_vec(),
        };
        store.put("key-1".into(), response.clone());
        assert_eq!(store.get("key-1"), Some(response));
        let replay = store.get("key-1").unwrap().into_replayed_response();
        assert_eq!(replay.status(), StatusCode::CREATED);
        assert_eq!(
            replay.headers().get("Idempotency-Replayed").unwrap(),
            "true"
        );
    }
}
