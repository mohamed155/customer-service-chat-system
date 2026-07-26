use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

#[derive(Debug)]
pub enum StorageError {
    NotFound,
    Other(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::NotFound => f.write_str("object not found"),
            StorageError::Other(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for StorageError {}

#[async_trait::async_trait]
pub trait ObjectStorage: Send + Sync + 'static {
    async fn put(&self, key: &str, content_type: &str, bytes: Vec<u8>) -> Result<(), StorageError>;
    async fn get(&self, key: &str) -> Result<(Vec<u8>, String), StorageError>;
    async fn delete(&self, key: &str) -> Result<(), StorageError>;
    async fn probe(&self) -> Result<(), StorageError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageKind {
    S3,
    InMemory,
}

#[derive(Debug, Clone, Copy)]
pub struct StorageBackend {
    pub kind: StorageKind,
    pub s3_configured: bool,
}

pub struct S3Storage {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl S3Storage {
    pub async fn new(cfg: &config::S3Config) -> Result<Self, StorageError> {
        use aws_sdk_s3::config::{Credentials, Region};

        let creds = Credentials::new(
            &cfg.access_key_id,
            &cfg.secret_access_key,
            None,
            None,
            "static",
        );

        let shared_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .endpoint_url(&cfg.endpoint)
            .region(Region::new(cfg.region.clone()))
            .credentials_provider(creds)
            .load()
            .await;

        let s3_config = aws_sdk_s3::config::Builder::from(&shared_config)
            .force_path_style(cfg.force_path_style)
            .build();

        let client = aws_sdk_s3::Client::from_conf(s3_config);

        Ok(Self {
            client,
            bucket: cfg.bucket.clone(),
        })
    }
}

#[async_trait::async_trait]
impl ObjectStorage for S3Storage {
    async fn put(&self, key: &str, content_type: &str, bytes: Vec<u8>) -> Result<(), StorageError> {
        use aws_sdk_s3::primitives::ByteStream;

        let body = ByteStream::from(bytes);

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .body(body)
            .send()
            .await
            .map_err(|e| StorageError::Other(e.to_string()))?;

        Ok(())
    }

    async fn get(&self, key: &str) -> Result<(Vec<u8>, String), StorageError> {
        let output = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                let err_str = e.to_string();
                if err_str.contains("NoSuchKey") {
                    StorageError::NotFound
                } else {
                    StorageError::Other(err_str)
                }
            })?;

        let content_type = output
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_owned();

        let bytes = output
            .body
            .collect()
            .await
            .map_err(|e| StorageError::Other(format!("failed to read response body: {e}")))?;

        Ok((bytes.to_vec(), content_type))
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::Other(e.to_string()))?;

        Ok(())
    }

    async fn probe(&self) -> Result<(), StorageError> {
        self.client
            .head_bucket()
            .bucket(&self.bucket)
            .send()
            .await
            .map_err(|e| StorageError::Other(e.to_string()))?;
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryStorage(Mutex<HashMap<String, (Vec<u8>, String)>>);

#[async_trait::async_trait]
impl ObjectStorage for InMemoryStorage {
    async fn put(&self, key: &str, content_type: &str, bytes: Vec<u8>) -> Result<(), StorageError> {
        let mut map = self
            .0
            .lock()
            .map_err(|e| StorageError::Other(format!("lock poisoned: {e}")))?;
        map.insert(key.to_owned(), (bytes, content_type.to_owned()));
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<(Vec<u8>, String), StorageError> {
        let map = self
            .0
            .lock()
            .map_err(|e| StorageError::Other(format!("lock poisoned: {e}")))?;
        map.get(key)
            .map(|(bytes, ct)| (bytes.clone(), ct.clone()))
            .ok_or(StorageError::NotFound)
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let mut map = self
            .0
            .lock()
            .map_err(|e| StorageError::Other(format!("lock poisoned: {e}")))?;
        map.remove(key);
        Ok(())
    }

    async fn probe(&self) -> Result<(), StorageError> {
        Ok(())
    }
}

pub async fn select_storage(
    s3: Option<&config::S3Config>,
) -> (std::sync::Arc<dyn ObjectStorage>, StorageBackend) {
    if let Some(s3_config) = s3 {
        match S3Storage::new(s3_config).await {
            Ok(s) => (
                std::sync::Arc::new(s),
                StorageBackend {
                    kind: StorageKind::S3,
                    s3_configured: true,
                },
            ),
            Err(e) => {
                tracing::warn!(error = %e, "failed to create S3 storage, falling back to in-memory");
                (
                    std::sync::Arc::new(InMemoryStorage::default()),
                    StorageBackend {
                        kind: StorageKind::InMemory,
                        s3_configured: true,
                    },
                )
            }
        }
    } else {
        (
            std::sync::Arc::new(InMemoryStorage::default()),
            StorageBackend {
                kind: StorageKind::InMemory,
                s3_configured: false,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn in_memory_put_get_roundtrip() {
        let storage = InMemoryStorage::default();
        let key = "test.txt";
        let content_type = "text/plain";
        let data = b"hello world".to_vec();

        storage.put(key, content_type, data.clone()).await.unwrap();

        let (got_bytes, got_ct) = storage.get(key).await.unwrap();
        assert_eq!(got_bytes, data);
        assert_eq!(got_ct, content_type);
    }

    #[tokio::test]
    async fn in_memory_get_unknown_key_returns_not_found() {
        let storage = InMemoryStorage::default();
        let err = storage.get("nonexistent").await.unwrap_err();
        assert!(matches!(err, StorageError::NotFound));
    }

    #[tokio::test]
    async fn in_memory_delete_then_get_returns_not_found() {
        let storage = InMemoryStorage::default();
        storage
            .put("key", "text/plain", b"data".to_vec())
            .await
            .unwrap();
        storage.delete("key").await.unwrap();
        let err = storage.get("key").await.unwrap_err();
        assert!(matches!(err, StorageError::NotFound));
    }

    #[tokio::test]
    async fn in_memory_delete_unknown_key_is_idempotent() {
        let storage = InMemoryStorage::default();
        storage.delete("does-not-exist").await.unwrap();
    }

    #[tokio::test]
    async fn in_memory_probe_ok() {
        let storage = InMemoryStorage::default();
        assert!(storage.probe().await.is_ok());
    }

    #[tokio::test]
    async fn select_storage_no_s3_returns_in_memory() {
        let (_store, backend) = select_storage(None).await;
        assert_eq!(backend.kind, StorageKind::InMemory);
        assert!(!backend.s3_configured);
    }
}
