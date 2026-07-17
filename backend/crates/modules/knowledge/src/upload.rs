use crate::validate::{sanitize_filename, ItemStatus, ValidationIssue, MAX_DOCUMENT_BYTES};
use axum::extract::Multipart;
use std::future::Future;
use std::path::Path;
use storage::{ObjectStorage, StorageError};
use uuid::Uuid;

#[derive(Debug)]
pub struct ParsedUpload {
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
    pub title: Option<String>,
    pub status: ItemStatus,
    pub category_id: Option<Uuid>,
    pub tags: Vec<String>,
}

pub async fn parse(multipart: Multipart) -> Result<ParsedUpload, ValidationIssue> {
    let mut filename: Option<String> = None;
    let mut content_type: Option<String> = None;
    let mut bytes: Option<Vec<u8>> = None;
    let mut title: Option<String> = None;
    let mut raw_status: Option<String> = None;
    let mut raw_category_id: Option<String> = None;
    let mut raw_tags: Option<String> = None;

    let mut mp = multipart;
    while let Some(mut field) = mp.next_field().await.map_err(|e| ValidationIssue {
        field: "file".into(),
        code: "parse_error".into(),
        message: format!("failed to read multipart field: {e}"),
    })? {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                let fname = field.file_name().unwrap_or("").to_string();
                let ct = field
                    .content_type()
                    .unwrap_or("application/octet-stream")
                    .to_string();

                let mut buf = Vec::new();
                while let Some(chunk) = field.chunk().await.map_err(|e| ValidationIssue {
                    field: "file".into(),
                    code: "read_error".into(),
                    message: format!("failed to read file chunk: {e}"),
                })? {
                    if buf.len() as u64 + chunk.len() as u64 > MAX_DOCUMENT_BYTES {
                        return Err(ValidationIssue {
                            field: "file".into(),
                            code: "too_large".into(),
                            message: format!(
                                "file must not exceed {} MB",
                                MAX_DOCUMENT_BYTES / (1024 * 1024)
                            ),
                        });
                    }
                    buf.extend_from_slice(&chunk);
                }

                filename = Some(fname);
                content_type = Some(ct);
                bytes = Some(buf);
            }
            "title" => {
                let data = field.bytes().await.map_err(|e| ValidationIssue {
                    field: "title".into(),
                    code: "read_error".into(),
                    message: format!("failed to read title: {e}"),
                })?;
                let s = String::from_utf8_lossy(&data).to_string();
                if !s.trim().is_empty() {
                    title = Some(s);
                }
            }
            "status" => {
                let data = field.bytes().await.map_err(|e| ValidationIssue {
                    field: "status".into(),
                    code: "read_error".into(),
                    message: format!("failed to read status: {e}"),
                })?;
                let s = String::from_utf8_lossy(&data).to_string();
                if !s.trim().is_empty() {
                    raw_status = Some(s.trim().to_lowercase());
                }
            }
            "categoryId" => {
                let data = field.bytes().await.map_err(|e| ValidationIssue {
                    field: "categoryId".into(),
                    code: "read_error".into(),
                    message: format!("failed to read categoryId: {e}"),
                })?;
                let s = String::from_utf8_lossy(&data).to_string();
                if !s.trim().is_empty() {
                    raw_category_id = Some(s.trim().to_string());
                }
            }
            "tags" => {
                let data = field.bytes().await.map_err(|e| ValidationIssue {
                    field: "tags".into(),
                    code: "read_error".into(),
                    message: format!("failed to read tags: {e}"),
                })?;
                let s = String::from_utf8_lossy(&data).to_string();
                if !s.trim().is_empty() {
                    raw_tags = Some(s.trim().to_string());
                }
            }
            _ => {}
        }
    }

    let filename = filename.ok_or_else(|| ValidationIssue {
        field: "file".into(),
        code: "required".into(),
        message: "file field is required".into(),
    })?;

    let content_type = content_type.ok_or_else(|| ValidationIssue {
        field: "file".into(),
        code: "required".into(),
        message: "file content type is required".into(),
    })?;

    let file_bytes = bytes.ok_or_else(|| ValidationIssue {
        field: "file".into(),
        code: "required".into(),
        message: "file data is required".into(),
    })?;

    validate_file(&filename, &content_type, file_bytes.len() as u64)?;

    let title = title.or_else(|| {
        Path::new(&filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    });

    let status = match raw_status.as_deref() {
        None | Some("draft") => ItemStatus::Draft,
        Some("published") => ItemStatus::Published,
        Some(other) => {
            return Err(ValidationIssue {
                field: "status".into(),
                code: "invalid_status".into(),
                message: format!("invalid status '{other}': must be 'draft' or 'published'"),
            });
        }
    };

    let category_id = raw_category_id
        .filter(|s| !s.is_empty())
        .map(|s| {
            Uuid::parse_str(&s).map_err(|_| ValidationIssue {
                field: "categoryId".into(),
                code: "invalid_uuid".into(),
                message: format!("invalid categoryId: '{s}'"),
            })
        })
        .transpose()?;

    let tags = raw_tags
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Ok(ParsedUpload {
        filename: sanitize_filename(&filename),
        content_type,
        bytes: file_bytes,
        title,
        status,
        category_id,
        tags,
    })
}

const ALLOWED_TYPES: &[(&str, &[&str])] = &[
    (".pdf", &["application/pdf"]),
    (
        ".docx",
        &["application/vnd.openxmlformats-officedocument.wordprocessingml.document"],
    ),
    (".txt", &["text/plain"]),
    (".md", &["text/markdown", "text/x-markdown", "text/plain"]),
];

pub fn validate_file(
    filename: &str,
    declared_content_type: &str,
    size: u64,
) -> Result<(), ValidationIssue> {
    if size > MAX_DOCUMENT_BYTES {
        return Err(ValidationIssue {
            field: "file".into(),
            code: "too_large".into(),
            message: format!(
                "file must not exceed {} MB; got {:.1} MB",
                MAX_DOCUMENT_BYTES / (1024 * 1024),
                size as f64 / (1024.0 * 1024.0),
            ),
        });
    }

    let ext = filename
        .rsplit('.')
        .next()
        .map(|e| format!(".{}", e.to_lowercase()))
        .unwrap_or_default();

    let allowed = ALLOWED_TYPES.iter().find(|(e, _)| *e == ext);

    match allowed {
        Some((ext, mimes)) => {
            if !mimes.contains(&declared_content_type) {
                let allowed_mimes = mimes.join(", ");
                return Err(ValidationIssue {
                    field: "file".into(),
                    code: "invalid_content_type".into(),
                    message: format!(
                        "content type '{declared_content_type}' does not match extension \
                         '{ext}'; allowed types for '{ext}' files: {allowed_mimes}",
                    ),
                });
            }
        }
        None => {
            let allowed_exts: Vec<&str> = ALLOWED_TYPES.iter().map(|(e, _)| *e).collect();
            return Err(ValidationIssue {
                field: "file".into(),
                code: "invalid_extension".into(),
                message: format!(
                    "unsupported file extension '{ext}'; allowed extensions: {}",
                    allowed_exts.join(", "),
                ),
            });
        }
    }

    Ok(())
}

#[derive(Debug)]
pub enum UploadFailure<E> {
    Storage(StorageError),
    Persist(E),
}

pub async fn put_then_persist<F, Fut, T, E>(
    storage: &dyn ObjectStorage,
    key: &str,
    content_type: &str,
    bytes: Vec<u8>,
    persist: F,
) -> Result<T, UploadFailure<E>>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    storage
        .put(key, content_type, bytes)
        .await
        .map_err(UploadFailure::Storage)?;

    let result = persist().await;

    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            let _ = storage.delete(key).await;
            Err(UploadFailure::Persist(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::ItemStatus;
    use axum::body::Body;
    use axum::extract::FromRequest;
    use axum::http::Request;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use storage::InMemoryStorage;

    #[allow(clippy::type_complexity)]
    fn multipart_body(fields: &[(&str, Option<&str>, Option<&str>, &[u8])]) -> Vec<u8> {
        let boundary = b"----testboundary";
        let mut body = Vec::new();
        for (name, filename, content_type, data) in fields {
            body.extend_from_slice(b"--");
            body.extend_from_slice(boundary);
            body.extend_from_slice(b"\r\n");
            body.extend_from_slice(b"Content-Disposition: form-data; name=\"");
            body.extend_from_slice(name.as_bytes());
            body.extend_from_slice(b"\"");
            if let Some(fname) = filename {
                body.extend_from_slice(b"; filename=\"");
                body.extend_from_slice(fname.as_bytes());
                body.extend_from_slice(b"\"");
            }
            body.extend_from_slice(b"\r\n");
            if let Some(ct) = content_type {
                body.extend_from_slice(b"Content-Type: ");
                body.extend_from_slice(ct.as_bytes());
                body.extend_from_slice(b"\r\n");
            }
            body.extend_from_slice(b"\r\n");
            body.extend_from_slice(data);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary);
        body.extend_from_slice(b"--\r\n");
        body
    }

    #[allow(clippy::type_complexity)]
    async fn make_multipart(fields: &[(&str, Option<&str>, Option<&str>, &[u8])]) -> Multipart {
        let body = multipart_body(fields);
        let req = Request::builder()
            .header(
                "content-type",
                "multipart/form-data; boundary=----testboundary",
            )
            .body(Body::from(bytes::Bytes::from(body)))
            .unwrap();
        Multipart::from_request(req, &()).await.unwrap()
    }

    // --- validate_file: accept/reject matrix ---

    #[test]
    fn accept_pdf() {
        validate_file("doc.pdf", "application/pdf", 1000).unwrap();
    }

    #[test]
    fn accept_docx() {
        validate_file(
            "doc.docx",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            1000,
        )
        .unwrap();
    }

    #[test]
    fn accept_txt() {
        validate_file("notes.txt", "text/plain", 1000).unwrap();
    }

    #[test]
    fn accept_md_markdown() {
        validate_file("readme.md", "text/markdown", 1000).unwrap();
    }

    #[test]
    fn accept_md_x_markdown() {
        validate_file("readme.md", "text/x-markdown", 1000).unwrap();
    }

    #[test]
    fn accept_md_plain() {
        validate_file("readme.md", "text/plain", 1000).unwrap();
    }

    #[test]
    fn reject_extension_mime_mismatch_exe_declared_pdf() {
        let err = validate_file("evil.exe", "application/pdf", 1000).unwrap_err();
        assert_eq!(err.code, "invalid_extension");
    }

    #[test]
    fn reject_extension_mime_mismatch_pdf_declared_exe() {
        let err = validate_file("evil.pdf", "application/x-msdownload", 1000).unwrap_err();
        assert_eq!(err.code, "invalid_content_type");
    }

    #[test]
    fn accept_at_max_size() {
        validate_file("doc.pdf", "application/pdf", MAX_DOCUMENT_BYTES).unwrap();
    }

    #[test]
    fn reject_one_byte_over_max() {
        let err = validate_file("doc.pdf", "application/pdf", MAX_DOCUMENT_BYTES + 1).unwrap_err();
        assert_eq!(err.code, "too_large");
    }

    #[test]
    fn reject_unknown_extension() {
        let err = validate_file("foo.png", "image/png", 1000).unwrap_err();
        assert_eq!(err.code, "invalid_extension");
    }

    // --- parse: title defaulting ---

    #[tokio::test]
    async fn title_defaults_to_filename_stem() {
        let mp =
            make_multipart(&[("file", Some("report.pdf"), Some("application/pdf"), b"data")]).await;
        let result = parse(mp).await.unwrap();
        assert_eq!(result.title, Some("report".to_string()));
    }

    #[tokio::test]
    async fn title_uses_provided_value() {
        let mp = make_multipart(&[
            ("file", Some("report.pdf"), Some("application/pdf"), b"data"),
            ("title", None, None, b"Custom Title"),
        ])
        .await;
        let result = parse(mp).await.unwrap();
        assert_eq!(result.title, Some("Custom Title".to_string()));
    }

    // --- parse: status parsing ---

    #[tokio::test]
    async fn status_defaults_to_draft() {
        let mp =
            make_multipart(&[("file", Some("doc.pdf"), Some("application/pdf"), b"data")]).await;
        let result = parse(mp).await.unwrap();
        assert_eq!(result.status, ItemStatus::Draft);
    }

    #[tokio::test]
    async fn status_accepts_draft() {
        let mp = make_multipart(&[
            ("file", Some("doc.pdf"), Some("application/pdf"), b"data"),
            ("status", None, None, b"draft"),
        ])
        .await;
        let result = parse(mp).await.unwrap();
        assert_eq!(result.status, ItemStatus::Draft);
    }

    #[tokio::test]
    async fn status_accepts_published() {
        let mp = make_multipart(&[
            ("file", Some("doc.pdf"), Some("application/pdf"), b"data"),
            ("status", None, None, b"published"),
        ])
        .await;
        let result = parse(mp).await.unwrap();
        assert_eq!(result.status, ItemStatus::Published);
    }

    #[tokio::test]
    async fn status_rejects_archived() {
        let mp = make_multipart(&[
            ("file", Some("doc.pdf"), Some("application/pdf"), b"data"),
            ("status", None, None, b"archived"),
        ])
        .await;
        let err = parse(mp).await.unwrap_err();
        assert_eq!(err.code, "invalid_status");
    }

    // --- put_then_persist ---

    #[tokio::test]
    async fn persist_success_keeps_object() {
        let storage = InMemoryStorage::default();
        let key = "test.txt";

        let result: Result<&str, UploadFailure<&str>> =
            put_then_persist(&storage, key, "text/plain", b"hello".to_vec(), || async {
                Ok("done")
            })
            .await;

        assert_eq!(result.unwrap(), "done");
        let (data, _) = storage.get(key).await.unwrap();
        assert_eq!(data, b"hello");
    }

    #[tokio::test]
    async fn persist_failure_deletes_object() {
        let storage = InMemoryStorage::default();
        let key = "test.txt";

        let result: Result<&str, UploadFailure<&str>> =
            put_then_persist(&storage, key, "text/plain", b"hello".to_vec(), || async {
                Err("db_error")
            })
            .await;

        assert!(matches!(result, Err(UploadFailure::Persist("db_error"))));
        let err = storage.get(key).await.unwrap_err();
        assert!(matches!(err, StorageError::NotFound));
    }

    #[tokio::test]
    async fn put_failure_skips_persist() {
        struct FailingStorage;

        #[async_trait::async_trait]
        impl ObjectStorage for FailingStorage {
            async fn put(
                &self,
                _key: &str,
                _ct: &str,
                _bytes: Vec<u8>,
            ) -> Result<(), StorageError> {
                Err(StorageError::Other("put failed".into()))
            }

            async fn get(&self, _key: &str) -> Result<(Vec<u8>, String), StorageError> {
                unreachable!()
            }

            async fn delete(&self, _key: &str) -> Result<(), StorageError> {
                unreachable!()
            }
        }

        let storage = FailingStorage;
        let persist_called = Arc::new(AtomicBool::new(false));
        let pc = persist_called.clone();

        let result: Result<&str, UploadFailure<&str>> =
            put_then_persist(&storage, "key", "text/plain", b"data".to_vec(), move || {
                pc.store(true, Ordering::SeqCst);
                async { Ok("should not be called") }
            })
            .await;

        assert!(matches!(result, Err(UploadFailure::Storage(_))));
        assert!(!persist_called.load(Ordering::SeqCst));
    }
}
