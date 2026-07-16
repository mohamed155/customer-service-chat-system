#[test]
fn api_doc_serializes() {
    let doc = server::openapi::ApiDoc::openapi();
    let json = serde_json::to_string(&doc).expect("serialize");
    assert!(json.contains("session_cookie"));
    assert!(json.contains("/api/v1"));
    assert!(json.contains("openapi"));
}
