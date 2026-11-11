//! The committed OpenAPI document must match what the code generates (MOH-43, ADR-012 gap A5).
//!
//! `docs/api/openapi.json` is the API contract. The frontend's TypeScript types are generated from
//! it, and CI compares it with the base branch for breaking changes (`oasdiff`). This test keeps it
//! honest: it renders the document from the production router, in-process and without a database,
//! and fails if the committed copy differs.
//!
//! After an intended API change, regenerate the file and commit it:
//!
//! ```text
//! UPDATE_OPENAPI=1 cargo test -p server --test openapi_snapshot
//! (cd frontend && pnpm gen:api)
//! ```

use std::path::PathBuf;

use server::router::documented_openapi;

fn committed_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../docs/api/openapi.json")
}

/// The production document (no test-only routes), pretty-printed with a trailing newline.
fn rendered() -> String {
    let doc = serde_json::to_value(documented_openapi(false)).expect("OpenApi serializes to JSON");
    let mut text = serde_json::to_string_pretty(&doc).expect("JSON value pretty-prints");
    text.push('\n');
    text
}

#[test]
fn rendering_is_deterministic() {
    assert_eq!(
        rendered(),
        rendered(),
        "two renderings of the same router differ, so a committed copy could never match"
    );
}

#[test]
fn the_committed_openapi_document_is_current() {
    let path = committed_path();
    let current = rendered();

    if std::env::var("UPDATE_OPENAPI").as_deref() == Ok("1") {
        std::fs::write(&path, &current).expect("write docs/api/openapi.json");
        return;
    }

    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read {}: {e}\nGenerate it with: UPDATE_OPENAPI=1 cargo test -p server --test openapi_snapshot",
            path.display()
        )
    });
    if committed != current {
        let first_difference = committed
            .lines()
            .zip(current.lines())
            .position(|(a, b)| a != b)
            .map(|i| i + 1)
            .unwrap_or_else(|| committed.lines().count().min(current.lines().count()) + 1);
        panic!(
            "docs/api/openapi.json is out of date (first difference near line {first_difference}).\n\
             The API changed; regenerate the contract and commit it:\n  \
             UPDATE_OPENAPI=1 cargo test -p server --test openapi_snapshot\n  \
             (cd frontend && pnpm gen:api)"
        );
    }
}
