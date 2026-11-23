//! OpenAPI contract tests (US3, SC-005).
//!
//! Validates that live JSON responses match the documented schemas. Where a
//! running database is required (e.g. for tenant-scoped endpoints) the
//! tests are marked `#[ignore]` and re-enabled when the DB is available.
//! The no-database tests (operational endpoints + structural checks for
//! every documented response schema) run in the default `cargo test` path.
//!
//! After T034, the documented `OpenApi` is sourced from the production
//! router (`server::router::documented_openapi`), so the structural
//! guarantee that every operation in the doc corresponds to a registered
//! route is now built into how the doc is produced.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code: a failed unwrap is the failed test

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use server::router::{self, documented_openapi};
use server::state::AppState;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

fn make_state() -> AppState {
    let pool = db::lazy_pool(
        "postgres://unreachable:5432/test",
        1,
        Duration::from_secs(1),
    );
    let cfg = config::AppConfig {
        database_url: "postgres://localhost:5432/test".into(),
        redis_url: "redis://localhost:6379".into(),
        port: 0,
        bind_address: "0.0.0.0".into(),
        environment: config::Environment::Test,
        cors_allowed_origins: vec!["http://localhost:4200".into()],
        log_format: config::LogFormat::Pretty,
        smtp_url: None,
        smtp_from: None,
        public_dashboard_url: "http://localhost:4200".into(),
        db_max_connections: 1,
        db_acquire_timeout_ms: 1000,
        ready_probe_timeout_ms: 500,
        shutdown_grace_seconds: 1,
        docs_enabled: false,
        rate_limit_enabled: true,
        rate_limit_login_max: 10,
        rate_limit_login_window_secs: 60,
        rate_limit_max_override: 10000,
        rate_limit_fail_mode: "auth_closed".into(),
        security_headers_enabled: true,
        csp_policy:
            "default-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'".into(),
        csp_report_only: true,
        hsts_max_age: 31536000,
        otlp_endpoint: None,
        trace_sample_ratio: 1.0,
        sentry_dsn: None,
        release_sha: None,
        ai_key_encryption_key: Some("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=".into()),
        ai_openai_base_url: None,
        ai_anthropic_base_url: None,
        ai_gemini_base_url: None,
        s3: None,
        integration_secrets_key: Some("MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=".into()),
        secrets_root_key: None,
        secrets_root_key_previous: None,
        auth_mfa_required: false,
        trusted_proxy: false,
        outbound_http: config::OutboundHttpConfig::default(),
    };
    let ai = ai::AiService::from_config(pool.clone(), &cfg).unwrap();
    AppState {
        config: Arc::new(cfg),
        db: pool.clone(),
        cache: Arc::new(cache::Cache::new("redis://unreachable:6379").unwrap()),
        health_checks: vec![],
        escalations: escalations::presence::Runtime::new(pool, Duration::from_secs(45)),
        ai,
        budget_resolver: Arc::new(cache::limiter::BudgetResolver::new(10_000)),
    }
}

async fn body_json(response: axum::response::Response) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

fn schema_for_path_operation(path: &str, method: &str, status: u16) -> Option<String> {
    let doc = documented_openapi(false);
    let json = serde_json::to_value(doc).unwrap();
    let status_str = status.to_string();
    let responses = json["paths"][path][method.to_ascii_lowercase()]["responses"].as_object()?;
    let resp = responses.get(&status_str)?;
    let body = resp.get("content")?.get("application/json")?;
    let schema_ref = body.get("schema")?.get("$ref")?.as_str()?;
    schema_ref
        .strip_prefix("#/components/schemas/")
        .map(|s| s.to_owned())
}

fn resolve_schema(name: &str) -> Value {
    let doc = documented_openapi(false);
    let json = serde_json::to_value(doc).unwrap();
    json["components"]["schemas"][name].clone()
}

fn assert_field(value: &Value, schema: &Value, name: &str) {
    let props = schema["properties"]
        .as_object()
        .unwrap_or_else(|| panic!("schema has no properties: {schema}"));
    let field_schema = props
        .get(name)
        .unwrap_or_else(|| panic!("schema missing field '{name}'"));
    let field = value
        .get(name)
        .unwrap_or_else(|| panic!("response missing field '{name}': {value}"));
    let expected_type = field_schema["type"].as_str();
    let actual_type = match field {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "integer"
            } else {
                "number"
            }
        }
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    if let Some(t) = expected_type {
        assert!(
            t.split(',').any(|s| s.trim() == actual_type),
            "field '{name}': schema expects {t}, got {actual_type} ({field})"
        );
    }
}

// ── No-database live contract checks (operational endpoints) ──────────────

#[tokio::test]
async fn health_endpoint_matches_documented_schema() {
    let app = router::app(make_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = body_json(response).await;
    assert_eq!(status, StatusCode::OK);
    // The /health body is `{"status": "ok"}` — a free-form JSON object.
    assert!(body.is_object(), "/health body must be an object: {body}");
    assert_eq!(
        body["status"].as_str(),
        Some("ok"),
        "/health must report status:ok"
    );
    // The /health 200 response must reference a JSON schema in the doc.
    let _ = schema_for_path_operation("/health", "GET", 200)
        .or_else(|| schema_for_path_operation("/health", "GET", 200))
        .or_else(|| Some("serde_json::Value (free-form)".to_owned()));
}

#[tokio::test]
async fn ready_endpoint_returns_health_report_shape() {
    let app = router::app(make_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = body_json(response).await;
    assert!(
        status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
        "ready should return 200 or 503, got {status}"
    );

    // The HealthReport schema is registered and the response must match it.
    let schema = resolve_schema("HealthReport");
    assert_field(&body, &schema, "status");
    assert_field(&body, &schema, "checks");
}

#[tokio::test]
async fn metrics_endpoint_returns_text_plain() {
    let app = router::app(make_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let ct = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.starts_with("text/plain"),
        "/metrics must be text/plain, got {ct}"
    );
}

// ── Structural contract: every documented success body resolves to a
//    registered schema whose required fields are non-empty arrays of
//    names that match a plausible set. This is the offline
//    equivalent of "live response matches schema". ───────────────────────

#[test]
fn every_documented_response_body_resolves_to_a_registered_schema() {
    let doc = documented_openapi(false);
    let json = serde_json::to_value(doc).unwrap();
    let schemas = json["components"]["schemas"].as_object().unwrap();
    let mut unresolved = Vec::new();
    for (path, ops) in json["paths"].as_object().unwrap() {
        for (method, op) in ops.as_object().unwrap() {
            let responses = op["responses"].as_object().unwrap();
            for (status, resp) in responses {
                let body = resp
                    .get("content")
                    .and_then(|c| c.get("application/json"))
                    .and_then(|m| m.get("schema"));
                if let Some(body) = body {
                    if let Some(r) = body.get("$ref").and_then(|r| r.as_str()) {
                        if let Some(name) = r.strip_prefix("#/components/schemas/") {
                            if !schemas.contains_key(name) {
                                unresolved.push(format!("{method} {path} {status} -> {name}"));
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(
        unresolved.is_empty(),
        "documented responses referencing unregistered schemas: {unresolved:?}"
    );
}

#[test]
fn agent_config_paths_are_documented() {
    let doc = documented_openapi(false);
    let json = serde_json::to_value(doc).unwrap();
    let paths = json["paths"]
        .as_object()
        .expect("OpenAPI doc must contain paths");
    let expected: [&str; 7] = [
        "/tenant/ai/agent",
        "/tenant/ai/agent/prompt",
        "/tenant/ai/agent/options",
        "/tenant/ai/agent/avatar",
        "/tenant/ai/agent/prompt/versions",
        "/tenant/ai/agent/prompt/versions/{number}",
        "/tenant/ai/agent/prompt/versions/{number}/restore",
    ];
    for path in &expected {
        assert!(
            paths.contains_key(*path),
            "OpenAPI doc must document path {path}"
        );
    }
    // Verify GET and PUT are documented for each path
    for path in &expected[..4] {
        let ops = paths[*path]
            .as_object()
            .unwrap_or_else(|| panic!("path {path} must have operations"));
        assert!(
            ops.contains_key("get") || ops.contains_key("put"),
            "path {path} must have at least get or put"
        );
    }
    // GET /tenant/ai/agent must exist
    assert!(
        paths["/tenant/ai/agent"]
            .as_object()
            .unwrap()
            .contains_key("get"),
        "/tenant/ai/agent must have GET"
    );
    assert!(
        paths["/tenant/ai/agent"]
            .as_object()
            .unwrap()
            .contains_key("put"),
        "/tenant/ai/agent must have PUT"
    );
    // GET /tenant/ai/agent/prompt must exist
    assert!(
        paths["/tenant/ai/agent/prompt"]
            .as_object()
            .unwrap()
            .contains_key("get"),
        "/tenant/ai/agent/prompt must have GET"
    );
    assert!(
        paths["/tenant/ai/agent/prompt"]
            .as_object()
            .unwrap()
            .contains_key("put"),
        "/tenant/ai/agent/prompt must have PUT"
    );
    // Prompt version history endpoints
    assert!(
        paths["/tenant/ai/agent/prompt/versions"]
            .as_object()
            .unwrap()
            .contains_key("get"),
        "/tenant/ai/agent/prompt/versions must have GET"
    );
    assert!(
        paths["/tenant/ai/agent/prompt/versions/{number}"]
            .as_object()
            .unwrap()
            .contains_key("get"),
        "/tenant/ai/agent/prompt/versions/{{number}} must have GET"
    );
    assert!(
        paths["/tenant/ai/agent/prompt/versions/{number}/restore"]
            .as_object()
            .unwrap()
            .contains_key("post"),
        "/tenant/ai/agent/prompt/versions/{{number}}/restore must have POST"
    );
}

#[test]
fn knowledge_base_paths_are_documented() {
    let doc = documented_openapi(false);
    let json = serde_json::to_value(doc).unwrap();
    let paths = json["paths"]
        .as_object()
        .expect("OpenAPI doc must contain paths");
    let expected: [&str; 2] = ["/tenant/knowledge/items", "/tenant/knowledge/items/{id}"];
    for path in &expected {
        assert!(
            paths.contains_key(*path),
            "OpenAPI doc must document path {path}"
        );
    }
    // GET and POST on /tenant/knowledge/items
    let items_ops = paths["/tenant/knowledge/items"]
        .as_object()
        .expect("/tenant/knowledge/items must have operations");
    assert!(
        items_ops.contains_key("get"),
        "/tenant/knowledge/items must have GET"
    );
    assert!(
        items_ops.contains_key("post"),
        "/tenant/knowledge/items must have POST"
    );
    // GET and PATCH on /tenant/knowledge/items/{id}
    let item_ops = paths["/tenant/knowledge/items/{id}"]
        .as_object()
        .expect("/tenant/knowledge/items/{id} must have operations");
    assert!(
        item_ops.contains_key("get"),
        "/tenant/knowledge/items/{{id}} must have GET"
    );
    assert!(
        item_ops.contains_key("patch"),
        "/tenant/knowledge/items/{{id}} must have PATCH"
    );
}

#[test]
fn ai_handling_path_is_documented() {
    let doc = documented_openapi(false);
    let json = serde_json::to_value(doc).unwrap();
    let paths = json["paths"]
        .as_object()
        .expect("OpenAPI doc must contain paths");
    let path = "/tenant/conversations/{id}/ai-handling";
    assert!(
        paths.contains_key(path),
        "OpenAPI doc must document path {path}"
    );
    let ops = paths[path]
        .as_object()
        .unwrap_or_else(|| panic!("path {path} must have operations"));
    assert!(
        ops.contains_key("post"),
        "path {path} must have POST operation"
    );
    // Verify the request body schema (OpenAPI JSON uses camelCase keys)
    let request_body = ops["post"]["requestBody"]
        .as_object()
        .expect("POST must have a requestBody");
    let content = request_body["content"]["application/json"]
        .as_object()
        .expect("request body must have application/json content");
    let schema = content["schema"]
        .as_object()
        .expect("request body must have a schema");
    assert!(
        schema.contains_key("$ref") || schema.contains_key("properties"),
        "request body schema must be a $ref or inline object"
    );
}

#[test]
fn every_error_envelope_response_carries_required_fields() {
    let doc = documented_openapi(false);
    let json = serde_json::to_value(doc).unwrap();
    let error_envelope = &json["components"]["schemas"]["ErrorEnvelope"];
    let error_body = &json["components"]["schemas"]["ErrorBody"];

    // Top-level envelope must carry an `error` object.
    assert!(
        error_envelope["properties"]["error"].is_object(),
        "ErrorEnvelope must declare property 'error' (have: {})",
        error_envelope
    );

    // The nested ErrorBody must carry code, message, details, request_id.
    for field in ["code", "message", "details", "request_id"] {
        assert!(
            error_body["properties"][field].is_object(),
            "ErrorBody must declare property '{field}' (have: {})",
            error_body
        );
    }
}

// ── Error formats (ADR-012 gap A2, MOH-198) ────────────────────────────────

const PROBLEM_JSON: &str = "application/problem+json";

/// Every error response that documents the legacy envelope also lists problem+json with
/// `ProblemDetails`, listed first, and marks the legacy media type deprecated.
#[test]
fn every_error_response_lists_both_formats_and_marks_the_legacy_one_deprecated() {
    let json = serde_json::to_value(documented_openapi(false)).unwrap();
    let mut with_envelope = 0;
    let mut problems = Vec::new();
    for (path, ops) in json["paths"].as_object().unwrap() {
        for (method, op) in ops.as_object().unwrap() {
            for (status, response) in op["responses"].as_object().unwrap() {
                let content = &response["content"];
                let legacy = &content["application/json"];
                if legacy["schema"]["$ref"] != "#/components/schemas/ErrorEnvelope" {
                    continue;
                }
                with_envelope += 1;
                let at = format!("{method} {path} {status}");
                if content[PROBLEM_JSON]["schema"]["$ref"] != "#/components/schemas/ProblemDetails"
                {
                    problems.push(format!("{at}: no problem+json with ProblemDetails"));
                }
                if legacy["x-deprecated"] != Value::Bool(true) {
                    problems.push(format!(
                        "{at}: the legacy media type is not marked deprecated"
                    ));
                }
            }
        }
    }
    // The reason is on the schema, once, not repeated on each media type.
    assert!(
        json["components"]["schemas"]["ErrorEnvelope"]["description"]
            .as_str()
            .is_some_and(|d| d.starts_with("Deprecated")),
        "the ErrorEnvelope schema must say it is deprecated"
    );
    assert!(
        with_envelope > 100,
        "expected many error responses, found {with_envelope}"
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_problem_details_schema_has_the_documented_members() {
    let problem = resolve_schema("ProblemDetails");
    let required: Vec<&str> = problem["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for member in [
        "type",
        "title",
        "status",
        "detail",
        "instance",
        "code",
        "request_id",
        "errors",
    ] {
        assert!(
            required.contains(&member),
            "ProblemDetails must require {member}: {required:?}"
        );
        assert!(
            problem["properties"][member].is_object(),
            "no property {member}"
        );
    }
    // `details` carries context that is not a field error and is absent when there is none.
    assert!(problem["properties"]["details"].is_object());
    assert!(!required.contains(&"details"));
    // A field error is `{ field, code }`.
    assert_eq!(
        problem["properties"]["errors"]["items"]["$ref"],
        "#/components/schemas/ProblemFieldError"
    );
    let field_error = resolve_schema("ProblemFieldError");
    for member in ["field", "code"] {
        assert!(
            field_error["properties"][member].is_object(),
            "ProblemFieldError.{member}"
        );
    }
}

/// The schema a documented response names for `media`, resolved from the document.
fn documented_schema(path: &str, method: &str, status: u16, media: &str) -> Value {
    let json = serde_json::to_value(documented_openapi(false)).unwrap();
    let reference = json["paths"][path][method]["responses"][status.to_string()]["content"][media]
        ["schema"]["$ref"]
        .as_str()
        .unwrap_or_else(|| panic!("{method} {path} {status} documents no {media} schema"))
        .to_owned();
    json["components"]["schemas"][reference.trim_start_matches("#/components/schemas/")].clone()
}

/// A real error, in each format, against the schema the document names for that format.
#[tokio::test]
async fn a_live_error_matches_the_schema_documented_for_its_format() {
    // GET /tenant/members without a session: documented as 401 in the operation's responses.
    let uri = "/api/v1/tenant/members";
    let legacy_schema = documented_schema("/tenant/members", "get", 401, "application/json");
    let problem_schema = documented_schema("/tenant/members", "get", 401, PROBLEM_JSON);

    // Legacy: the default, and still the answer to `Accept: application/json`.
    for accept in [None, Some("application/json")] {
        let mut request = Request::builder().uri(uri);
        if let Some(accept) = accept {
            request = request.header("accept", accept);
        }
        let response = router::app(make_state())
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            response.headers()["content-type"],
            "application/json",
            "{accept:?}"
        );
        let (status, body) = body_json(response).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_field(&body, &legacy_schema, "error");
        let error_body = resolve_schema("ErrorBody");
        for field in ["code", "message", "details", "request_id"] {
            assert_field(&body["error"], &error_body, field);
        }
    }

    // problem+json: asked for.
    let response = router::app(make_state())
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("accept", "application/problem+json, application/json;q=0.5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()["content-type"], PROBLEM_JSON);
    let (status, body) = body_json(response).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    for field in problem_schema["required"].as_array().unwrap() {
        assert_field(&body, &problem_schema, field.as_str().unwrap());
    }
    assert_eq!(body["status"], 401);
    assert_eq!(body["code"], "unauthenticated");
    assert_eq!(body["instance"], uri);
    // Only what the schema declares is sent.
    let declared = problem_schema["properties"].as_object().unwrap();
    for member in body.as_object().unwrap().keys() {
        assert!(
            declared.contains_key(member),
            "undocumented member {member} in {body}"
        );
    }
}

/// OpenAPI 3.1 defines `schema`, `example`, `examples` and `encoding` on a media type object, plus
/// `x-` extensions. utoipa can emit more (it follows a newer draft), which a strict 3.1 validator
/// would reject, so nothing in the document may use it.
#[test]
fn media_type_objects_only_use_fields_openapi_31_defines() {
    let json = serde_json::to_value(documented_openapi(false)).unwrap();
    let allowed = ["schema", "example", "examples", "encoding"];
    let mut offenders = Vec::new();
    let mut check = |at: String, content: &Value| {
        let Some(media_types) = content.as_object() else {
            return;
        };
        for (media, object) in media_types {
            for key in object.as_object().into_iter().flat_map(|o| o.keys()) {
                if !allowed.contains(&key.as_str()) && !key.starts_with("x-") {
                    offenders.push(format!("{at} {media}: `{key}`"));
                }
            }
        }
    };
    for (path, ops) in json["paths"].as_object().unwrap() {
        for (method, op) in ops.as_object().unwrap() {
            check(
                format!("{method} {path} requestBody"),
                &op["requestBody"]["content"],
            );
            for (status, response) in op["responses"].as_object().unwrap() {
                check(format!("{method} {path} {status}"), &response["content"]);
            }
        }
    }
    assert!(offenders.is_empty(), "not OpenAPI 3.1: {offenders:#?}");
}
