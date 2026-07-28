use axum::http::header;
use axum::http::{HeaderName, HeaderValue};
use config::AppConfig;
use tower_http::set_header::SetResponseHeaderLayer;

pub fn scope_a_layers(config: &AppConfig) -> Vec<SetResponseHeaderLayer<HeaderValue>> {
    let mut layers = Vec::new();

    layers.push(SetResponseHeaderLayer::overriding(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    ));

    layers.push(SetResponseHeaderLayer::overriding(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    ));

    if config.hsts_max_age > 0 {
        let value = format!("max-age={}; includeSubDomains", config.hsts_max_age);
        layers.push(SetResponseHeaderLayer::overriding(
            header::STRICT_TRANSPORT_SECURITY,
            value.parse().expect("invalid HSTS header value"),
        ));
    }

    layers
}

pub fn scope_b_layers(config: &AppConfig) -> Vec<SetResponseHeaderLayer<HeaderValue>> {
    let mut layers = Vec::new();

    layers.push(SetResponseHeaderLayer::overriding(
        header::X_FRAME_OPTIONS,
        HeaderValue::from_static("DENY"),
    ));

    let header_name = if config.csp_report_only {
        HeaderName::from_static("content-security-policy-report-only")
    } else {
        header::CONTENT_SECURITY_POLICY
    };
    layers.push(SetResponseHeaderLayer::overriding(
        header_name,
        config.csp_policy.parse().expect("invalid CSP header value"),
    ));

    layers
}
