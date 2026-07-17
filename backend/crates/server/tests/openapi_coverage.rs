//! OpenAPI route-coverage enforcement (US3, FR-015, SC-001, T034).
//!
//! Two assertions, both loading the documented `OpenApi` from the
//! production router (via `server::router::documented_openapi`):
//!
//! 1. **Coverage gate (T034)** — every operation in the contract inventory
//!    (`specs/016-backend-swagger-docs/contracts/openapi-coverage.md`) is
//!    present in the router-derived document, and nothing extra is
//!    documented. The contract inventory is the proxy for "routes the app
//!    actually serves": if a route is registered with `.route()` instead
//!    of `routes!()` in `router.rs`, its `#[utoipa::path]` annotation is
//!    not picked up and the path will be missing from the doc, causing
//!    this test to fail and name the offender. Combined with T035's
//!    migration to `routes!()` everywhere, this gives the structural
//!    "no undocumented route" guarantee (FR-015) without runtime
//!    router-introspection (which axum 0.8 does not expose).
//!
//! 2. **Test-route leak guard (T027)** — no path under `/test/...` or
//!    `/test-...` appears in the published document (FR-004). Test routes
//!    are closures, not function paths, so they cannot use the `routes!()`
//!    co-registration macro and stay on the plain `.route()` passthrough —
//!    they register in the live `Router` only and never in the documented
//!    `OpenApi`.

use server::router::documented_openapi;
use std::collections::BTreeSet;

type Method = String;
type Path = String;

fn documented_set() -> BTreeSet<(Method, Path)> {
    let doc = documented_openapi(false);
    let json = serde_json::to_value(doc).unwrap();
    let mut set = BTreeSet::new();
    for (path, ops) in json["paths"].as_object().unwrap() {
        for method in ops.as_object().unwrap().keys() {
            set.insert((method.to_ascii_uppercase(), path.clone()));
        }
    }
    set
}

/// Authoritative inventory of every non-test endpoint the OpenAPI
/// document MUST cover. Mirrors
/// `specs/016-backend-swagger-docs/contracts/openapi-coverage.md`.
///
/// When the contract grows, this list grows; the test fails if the doc
/// drifts in either direction (missing or extra).
const EXPECTED: &[(&str, &str)] = &[
    // Public (auth, invitations)
    ("POST", "/auth/login"),
    ("POST", "/auth/logout"),
    ("GET", "/invitations/{token}"),
    ("POST", "/invitations/{token}/accept"),
    // Authenticated (identity)
    ("GET", "/me"),
    // Platform tenants
    ("GET", "/platform/tenants"),
    ("POST", "/platform/tenants"),
    ("GET", "/platform/tenants/{id}"),
    ("PATCH", "/platform/tenants/{id}"),
    ("POST", "/platform/tenants/{id}/switch"),
    // Platform AI
    ("GET", "/platform/ai/config"),
    ("PUT", "/platform/ai/config"),
    ("PUT", "/platform/ai/credentials/{provider}"),
    ("DELETE", "/platform/ai/credentials/{provider}"),
    ("POST", "/platform/ai/config/test"),
    // Tenant profile
    ("GET", "/tenant"),
    // Customers
    ("GET", "/tenant/customers"),
    ("POST", "/tenant/customers"),
    ("GET", "/tenant/customers/{id}"),
    ("PATCH", "/tenant/customers/{id}"),
    ("GET", "/tenant/customers/{id}/conversations"),
    // Conversations
    ("GET", "/tenant/conversations"),
    ("POST", "/tenant/conversations"),
    ("GET", "/tenant/conversations/{id}"),
    ("PATCH", "/tenant/conversations/{id}"),
    ("GET", "/tenant/conversations/{id}/messages"),
    ("POST", "/tenant/conversations/{id}/messages"),
    // Realtime + escalations
    ("GET", "/tenant/events"),
    ("POST", "/tenant/conversations/{id}/escalate"),
    ("GET", "/tenant/escalations/queue"),
    ("POST", "/tenant/escalations/{id}/claim"),
    // Availability + skills
    ("GET", "/tenant/availability/me"),
    ("PUT", "/tenant/availability/me"),
    ("GET", "/tenant/skills"),
    ("POST", "/tenant/skills"),
    ("PATCH", "/tenant/skills/{id}"),
    ("DELETE", "/tenant/skills/{id}"),
    // Members + invitations (tenant)
    ("GET", "/tenant/members"),
    ("PATCH", "/tenant/members/{id}"),
    ("PUT", "/tenant/members/{membershipId}/skills"),
    ("GET", "/tenant/members/invitations"),
    ("POST", "/tenant/members/invitations"),
    ("GET", "/tenant/members/invitations/{id}/delivery"),
    ("DELETE", "/tenant/members/invitations/{id}"),
    // Tenant AI
    ("GET", "/tenant/ai/config"),
    ("PUT", "/tenant/ai/config"),
    ("DELETE", "/tenant/ai/config"),
    ("PUT", "/tenant/ai/credentials/{provider}"),
    ("DELETE", "/tenant/ai/credentials/{provider}"),
    ("POST", "/tenant/ai/config/test"),
    ("GET", "/tenant/ai/usage"),
    ("GET", "/tenant/ai/usage/summary"),
    ("GET", "/tenant/ai/usage/{id}"),
    // Tenant AI Agent
    ("GET", "/tenant/ai/agent"),
    ("PUT", "/tenant/ai/agent"),
    ("GET", "/tenant/ai/agent/avatar"),
    ("PUT", "/tenant/ai/agent/avatar"),
    ("GET", "/tenant/ai/agent/options"),
    ("POST", "/tenant/conversations/{id}/ai-handling"),
    // Tenant AI Agent Prompt (prompt management)
    ("GET", "/tenant/ai/agent/prompt"),
    ("PUT", "/tenant/ai/agent/prompt"),
    ("GET", "/tenant/ai/agent/prompt/versions"),
    ("GET", "/tenant/ai/agent/prompt/versions/{number}"),
    ("POST", "/tenant/ai/agent/prompt/versions/{number}/restore"),
    // Tenant Knowledge Base (US1)
    ("GET", "/tenant/knowledge/items"),
    ("POST", "/tenant/knowledge/items"),
    ("GET", "/tenant/knowledge/items/{id}"),
    ("PATCH", "/tenant/knowledge/items/{id}"),
    ("POST", "/tenant/knowledge/items/{id}/status"),
    ("POST", "/tenant/knowledge/items/{id}/reindex"),
    ("GET", "/tenant/knowledge/items/{id}/file"),
    ("POST", "/tenant/knowledge/documents"),
    // Tenant Knowledge Base (US4 categories)
    ("GET", "/tenant/knowledge/categories"),
    ("POST", "/tenant/knowledge/categories"),
    ("PATCH", "/tenant/knowledge/categories/{category_id}"),
    ("DELETE", "/tenant/knowledge/categories/{category_id}"),
    // Operational
    ("GET", "/health"),
    ("GET", "/ready"),
    ("GET", "/metrics"),
];

#[test]
fn documented_paths_equal_expected_inventory() {
    let expected: BTreeSet<(String, String)> = EXPECTED
        .iter()
        .map(|(m, p)| (m.to_string(), p.to_string()))
        .collect();
    let actual = documented_set();

    let missing: Vec<_> = expected.difference(&actual).collect();
    let extra: Vec<_> = actual.difference(&expected).collect();

    assert!(
        missing.is_empty(),
        "expected operations not documented (FR-015): {missing:?}"
    );
    assert!(
        extra.is_empty(),
        "documented operations not in the contract inventory: {extra:?}"
    );
}

#[test]
fn no_test_only_routes_appear_in_documented_set() {
    let actual = documented_set();

    let mut leaked = Vec::new();
    for (method, path) in &actual {
        if path.starts_with("/test") || path.contains("/test-") {
            leaked.push((method.clone(), path.clone()));
        }
    }
    assert!(
        leaked.is_empty(),
        "FR-004 violation: test-only routes leaked into the published docs: {leaked:?}"
    );
}

/// Sanity check that the documented surface is non-empty: the router
/// must register at least one production route through `routes!()` and
/// the operational surface (`/health`, `/ready`, `/metrics`) must be
/// represented. A blank document means the seed `ApiDoc` is no longer
/// contributing the operational paths or `routes!()` is not picking up
/// any handler annotations — both regressions worth catching fast.
#[test]
fn documented_set_is_non_trivially_populated() {
    let actual = documented_set();
    assert!(
        actual.len() >= 50,
        "expected at least 50 documented operations (T034 sanity), found {}: {:?}",
        actual.len(),
        actual
    );
    // Operational endpoints must be present (FR-003).
    for (method, path) in [("GET", "/health"), ("GET", "/ready"), ("GET", "/metrics")] {
        assert!(
            actual.contains(&(method.to_owned(), path.to_owned())),
            "operational endpoint {method} {path} missing from documented set (FR-003): {actual:?}"
        );
    }
}
