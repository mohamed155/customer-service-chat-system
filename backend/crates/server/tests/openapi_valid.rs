//! OpenAPI document validity + secrets-in-responses tests (US2).
//!
//! These tests do not require a live database — they build the documented
//! `OpenApi` from the production router (`server::router::documented_openapi`)
//! in-process and inspect the resulting value.
//!
//! After T034, the `OpenApi` is sourced from the router (which co-registers
//! paths via `utoipa_axum::routes!`), not from the static `ApiDoc` seed —
//! the structural guarantee that every documented path corresponds to a
//! registered route is now built into how the doc is produced.
//!
//! See `specs/016-backend-swagger-docs/spec.md` (FR-013, SC-003, FR-011,
//! quickstart Scenario 7).

use serde_json::Value;
use server::router::documented_openapi;

/// Render the documented `OpenApi` once for the whole module.
fn doc_value() -> Value {
    let doc = documented_openapi(false);
    serde_json::to_value(doc).expect("OpenApi serializes to JSON")
}

#[test]
fn api_doc_is_a_valid_openapi_31_document() {
    let doc = doc_value();

    // Top-level required OpenAPI 3.1 fields.
    assert_eq!(
        doc["openapi"].as_str().unwrap(),
        "3.1.0",
        "document must declare OpenAPI 3.1 (FR-013)"
    );
    assert!(doc["info"].is_object(), "info block required");
    assert_eq!(
        doc["info"]["title"].as_str().unwrap(),
        "AI Customer Service Platform API"
    );
    assert!(
        doc["info"]["version"].is_string(),
        "info.version must be a string"
    );
    assert!(doc["paths"].is_object(), "paths block required");
    assert!(doc["components"].is_object(), "components block required");

    // The session_cookie scheme and /api/v1 server MUST be present (FR-009).
    let scheme = &doc["components"]["securitySchemes"]["session_cookie"];
    assert_eq!(scheme["type"], "apiKey", "session_cookie must be apiKey");
    assert_eq!(scheme["in"], "cookie", "session_cookie must be in cookie");
    assert_eq!(
        scheme["name"], "app_session",
        "session_cookie must be named app_session"
    );

    let servers = doc["servers"].as_array().expect("servers array");
    assert!(
        servers.iter().any(|s| s["url"].as_str() == Some("/api/v1")),
        "servers must include /api/v1"
    );
}

#[test]
fn every_documented_path_has_a_valid_operation() {
    let doc = doc_value();
    let paths = doc["paths"].as_object().unwrap();
    assert!(!paths.is_empty(), "document must contain at least one path");

    let valid_methods = ["get", "post", "put", "patch", "delete", "head", "options"];
    for (path, ops) in paths {
        let ops = ops
            .as_object()
            .unwrap_or_else(|| panic!("paths['{path}'] must be an object"));
        assert!(
            !ops.is_empty(),
            "paths['{path}'] must declare at least one method"
        );
        for (method, operation) in ops {
            assert!(
                valid_methods.contains(&method.as_str()),
                "paths['{path}'].{method} — invalid HTTP method"
            );
            assert!(
                operation.is_object(),
                "paths['{path}'].{method} must be an object"
            );
            // Every operation MUST declare a tag (FR-009 grouping).
            let tags = operation["tags"]
                .as_array()
                .unwrap_or_else(|| panic!("paths['{path}'].{method} must declare tags"));
            assert!(
                !tags.is_empty(),
                "paths['{path}'].{method} must declare at least one tag"
            );
            // Every operation MUST declare responses.
            let responses = operation["responses"]
                .as_object()
                .unwrap_or_else(|| panic!("paths['{path}'].{method} must declare responses"));
            assert!(
                responses.contains_key("200")
                    || responses.contains_key("201")
                    || responses.contains_key("202")
                    || responses.contains_key("204"),
                "paths['{path}'].{method} must declare a success response"
            );
        }
    }
}

#[test]
fn api_doc_declares_all_required_tag_groups() {
    let doc = doc_value();
    let tags: Vec<&str> = doc["tags"]
        .as_array()
        .expect("tags array")
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for required in [
        "auth",
        "invitations",
        "identity",
        "platform-tenants",
        "platform-ai",
        "tenant",
        "customers",
        "conversations",
        "escalations",
        "members",
        "tenant-ai",
        "ops",
    ] {
        assert!(
            tags.contains(&required),
            "tag '{required}' missing from root tags (have: {tags:?})"
        );
    }
}

/// FR-011: every `password` / `api_key` field MUST be marked
/// `writeOnly: true` (input only), and no **response** schema may expose
/// either field. The schema list is flat, so we identify response schemas
/// by inspecting each operation's documented responses.
#[test]
fn no_secrets_in_responses() {
    let doc = doc_value();
    let schemas = doc["components"]["schemas"]
        .as_object()
        .expect("components.schemas");

    // 1. Every `password` and `api_key` field, wherever it appears, MUST
    // be `writeOnly: true`. This catches accidental `Serialize` without
    // the write-only annotation.
    for (name, schema) in schemas {
        let props = match schema.get("properties") {
            Some(p) if p.is_object() => p.as_object().unwrap(),
            _ => continue,
        };
        for field_name in ["password", "api_key"] {
            if let Some(field) = props.get(field_name) {
                assert_eq!(
                    field["writeOnly"], true,
                    "schema '{name}.{field_name}' must be writeOnly (FR-011): {field}"
                );
            }
        }
    }

    // 2. No **response** schema (one referenced by an operation's
    // response body) may carry a `password` or `api_key` field.
    let mut response_schemas = std::collections::BTreeSet::new();
    for ops in doc["paths"].as_object().unwrap().values() {
        for op in ops.as_object().unwrap().values() {
            for resp in op
                .get("responses")
                .and_then(|r| r.as_object())
                .into_iter()
                .flatten()
            {
                let body = resp
                    .1
                    .get("content")
                    .and_then(|c| c.get("application/json"))
                    .and_then(|m| m.get("schema"))
                    .and_then(|s| s.get("$ref"))
                    .and_then(|r| r.as_str());
                if let Some(s) = body {
                    if let Some(name) = s.strip_prefix("#/components/schemas/") {
                        response_schemas.insert(name.to_owned());
                    }
                }
            }
        }
    }
    let mut violations = Vec::new();
    for name in &response_schemas {
        let Some(props) = schemas
            .get(name)
            .and_then(|s| s.get("properties"))
            .and_then(|p| p.as_object())
        else {
            continue;
        };
        for field_name in ["password", "api_key"] {
            if props.contains_key(field_name) {
                violations.push(format!("response schema '{name}' exposes '{field_name}'"));
            }
        }
    }
    assert!(violations.is_empty(), "FR-011 violations: {violations:?}");

    // 3. The two canonical credential inputs must exist as components
    // and be marked writeOnly.
    for (schema_name, field) in [
        ("LoginRequest", "password"),
        ("CredentialPayload", "api_key"),
    ] {
        let field_schema = &schemas[schema_name]["properties"][field];
        assert_eq!(
            field_schema["writeOnly"], true,
            "{schema_name}.{field} must be writeOnly (FR-011)"
        );
    }
}

/// Every documented path operation should reference an `ErrorEnvelope` body
/// for at least one error status code (FR-008) — except purely public, GET-only
/// endpoints that have no documented error modes.
#[test]
fn error_responses_reference_error_envelope() {
    let doc = doc_value();
    let schemas = doc["components"]["schemas"]
        .as_object()
        .expect("components.schemas");
    assert!(
        schemas.contains_key("ErrorEnvelope"),
        "ErrorEnvelope must be registered as a component (FR-008)"
    );

    let mut missing = Vec::new();
    for (path, ops) in doc["paths"].as_object().unwrap() {
        for (method, operation) in ops.as_object().unwrap() {
            let responses = match operation.get("responses") {
                Some(r) => r.as_object().unwrap(),
                None => continue,
            };
            let has_error = responses.iter().any(|(status, resp)| {
                let code = status.as_str();
                let is_error = code.starts_with('4') || code.starts_with('5');
                if !is_error {
                    return false;
                }
                let body = resp.get("content");
                let schema_ref = body
                    .and_then(|c| c.get("application/json"))
                    .and_then(|m| m.get("schema"))
                    .and_then(|s| s.get("$ref"))
                    .and_then(|r| r.as_str());
                schema_ref == Some("#/components/schemas/ErrorEnvelope")
            });
            if !has_error {
                // It's OK for purely-informational public endpoints
                // (e.g. /health, /ready, /metrics) and a small set of
                // ops endpoints to have no error envelope. But every
                // /api/v1/* business endpoint MUST have one.
                if path.starts_with("/api/v1") {
                    missing.push(format!("{method} {path}"));
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "the following /api/v1/* operations are missing an ErrorEnvelope error response (FR-008): {missing:?}"
    );
}
