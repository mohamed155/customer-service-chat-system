//! Stops new code from putting error text into a 5xx response (MOH-32, ADR-012 gap A1).
//!
//! `ApiError::internal_error` takes a `&'static str`, so `format!` doesn't compile. This test is
//! the second guard: it fails if someone loosens that signature, or passes anything but a string
//! literal as the step description of `internal_error` / `internal_error_from`.

use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn internal_error_call_sites_pass_only_string_literals() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    rust_files(&crates, &mut files);
    assert!(files.len() > 100, "scan found only {} files", files.len());

    // `internal_error(` or `internal_error_from(` followed by something that is not a string
    // literal, on this line or the next. The definitions in the kernel crate
    // (`fn internal_error(context: ...`) are exempt.
    let call = Regex::new(r#"internal_error(?:_from)?\(\s*([^\s")][^)]{0,40})"#).unwrap();
    let mut offenders = Vec::new();
    for file in files {
        if file.ends_with("internal_error_hygiene.rs") {
            continue;
        }
        let text = fs::read_to_string(&file).unwrap();
        for found in call.captures_iter(&text) {
            let whole = found.get(0).unwrap();
            if text[..whole.start()].ends_with("fn ") {
                continue;
            }
            let line = text[..whole.start()].matches('\n').count() + 1;
            offenders.push(format!(
                "{}:{line}: internal_error argument is not a string literal: {}",
                file.display(),
                found[1].trim()
            ));
        }
    }
    assert!(
        offenders.is_empty(),
        "5xx responses must not carry dynamic text. Use a string literal and pass the error to \
         ApiError::internal_error_from:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn internal_error_never_takes_format_macro() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    rust_files(&crates, &mut files);
    let format_call =
        Regex::new(r#"(?s)internal_error(?:_from)?\(\s*(?:&?format!|&?\w+\.to_string)"#).unwrap();
    let mut offenders = Vec::new();
    for file in files {
        if file.ends_with("internal_error_hygiene.rs") {
            continue;
        }
        let text = fs::read_to_string(&file).unwrap();
        if format_call.is_match(&text) {
            offenders.push(file.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "format! inside internal_error: {offenders:?}"
    );
}
