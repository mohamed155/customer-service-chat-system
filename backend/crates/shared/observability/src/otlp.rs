use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{Sampler, SdkTracer, SdkTracerProvider};
use opentelemetry_sdk::Resource;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::Registry;

/// Build an OTLP telemetry layer for the given endpoint and sample ratio.
/// Intended to be layered on a `Registry` subscriber.
///
/// FR-020: the SDK exports every span so that the collector's tail-based
/// sampler — not head sampling — decides which non-errored traces to retain.
/// `_sample_ratio` is retained in the signature only for backward
/// compatibility; callers that relied on it to reduce SDK export volume
/// should remove the argument once the deployment pipeline is updated.
pub fn build_layer(endpoint: &str, _sample_ratio: f64) -> OpenTelemetryLayer<Registry, SdkTracer> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .expect("failed to build OTLP span exporter");

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_sampler(Sampler::AlwaysOn)
        .with_resource(
            Resource::builder_empty()
                .with_service_name("customer-service-platform")
                .build(),
        )
        .build();

    let tracer = provider.tracer("customer-service-platform");
    OpenTelemetryLayer::new(tracer)
}
