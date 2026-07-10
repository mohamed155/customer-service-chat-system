use axum::{extract::Request, middleware::Next, response::Response};
use std::time::Instant;
use tracing::{Instrument, Span};

pub async fn trace_middleware(request: Request, next: Next) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_owned();
    let request_id = request
        .headers()
        .get(&super::request_id::REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_owned();

    let start = Instant::now();
    let span = request_span(&request_id, &method, &path);

    let response = next.run(request).instrument(span).await;

    let latency_ms = start.elapsed().as_millis() as u64;
    let status = response.status().as_u16();

    tracing::info!(
        request_id = %request_id,
        method = %method,
        path = %path,
        status = status,
        latency_ms = latency_ms,
        "request completed"
    );

    response
}

fn request_span(request_id: &str, method: &str, path: &str) -> Span {
    tracing::info_span!(
        "request",
        request_id = %request_id,
        method = %method,
        path = %path,
        principal.id = tracing::field::Empty,
        principal.kind = tracing::field::Empty,
        tenant.id = tracing::field::Empty,
        authz.denied_permission = tracing::field::Empty,
    )
}

#[cfg(test)]
mod tests {
    use super::request_span;
    use std::sync::{Arc, Mutex};
    use tracing::{field::Visit, span, Subscriber};
    use tracing_subscriber::{layer::Context, prelude::*, registry::LookupSpan, Layer};

    struct CaptureDeniedPermission {
        value: Arc<Mutex<Option<String>>>,
    }

    impl<S> Layer<S> for CaptureDeniedPermission
    where
        S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    {
        fn on_record(&self, _span: &span::Id, values: &span::Record<'_>, _ctx: Context<'_, S>) {
            values.record(&mut DeniedPermissionVisitor {
                value: Arc::clone(&self.value),
            });
        }
    }

    struct DeniedPermissionVisitor {
        value: Arc<Mutex<Option<String>>>,
    }

    impl Visit for DeniedPermissionVisitor {
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            if field.name() == "authz.denied_permission" {
                *self.value.lock().unwrap() = Some(value.to_owned());
            }
        }

        fn record_debug(&mut self, _field: &tracing::field::Field, _value: &dyn std::fmt::Debug) {}
    }

    #[test]
    fn request_span_accepts_denied_permission_recording() {
        let recorded = Arc::new(Mutex::new(None));
        let subscriber = tracing_subscriber::registry().with(CaptureDeniedPermission {
            value: Arc::clone(&recorded),
        });

        tracing::subscriber::with_default(subscriber, || {
            let span = request_span("req_test", "GET", "/api/v1/tenant");
            span.record("authz.denied_permission", "customers.manage");
        });

        assert_eq!(
            recorded.lock().unwrap().as_deref(),
            Some("customers.manage")
        );
    }
}
