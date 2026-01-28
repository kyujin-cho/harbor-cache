//! S3-compatible storage backend
//!
//! Uses the `object_store` crate to provide S3-compatible storage
//! for Harbor Cache. Supports AWS S3, MinIO, and other S3-compatible
//! services.

use async_trait::async_trait;
use bytes::Bytes;
use futures::{StreamExt, TryStreamExt};
use object_store::aws::AmazonS3Builder;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStore, PutPayload};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::backend::{ByteStream, StorageBackend, compute_sha256, parse_digest};
use crate::error::StorageError;

/// S3 storage configuration
#[derive(Debug, Clone)]
pub struct S3Config {
    /// S3 bucket name
    pub bucket: String,
    /// S3 region (e.g., "us-east-1")
    pub region: String,
    /// S3 endpoint URL (for MinIO or other S3-compatible services)
    pub endpoint: Option<String>,
    /// AWS access key ID
    pub access_key_id: Option<String>,
    /// AWS secret access key
    pub secret_access_key: Option<String>,
    /// Prefix for all objects (optional)
    pub prefix: Option<String>,
    /// Allow HTTP (not HTTPS) connections
    pub allow_http: bool,
}

impl Default for S3Config {
    fn default() -> Self {
        Self {
            bucket: "harbor-cache".to_string(),
            region: "us-east-1".to_string(),
            endpoint: None,
            access_key_id: None,
            secret_access_key: None,
            prefix: None,
            allow_http: false,
        }
    }
}

/// S3 storage backend
///
/// Stores blobs in an S3-compatible bucket with content-addressable paths:
/// `<prefix>/blobs/<algorithm>/<first 2 chars>/<digest>`
pub struct S3Storage {
    store: Arc<dyn ObjectStore>,
    prefix: String,
}

impl S3Storage {
    /// Create a new S3 storage backend
    pub async fn new(config: S3Config) -> Result<Self, StorageError> {
        let mut builder = AmazonS3Builder::new()
            .with_bucket_name(&config.bucket)
            .with_region(&config.region);

        // Set endpoint for MinIO or other S3-compatible services
        if let Some(endpoint) = &config.endpoint {
            builder = builder.with_endpoint(endpoint);
        }

        // Set credentials
        if let Some(access_key) = &config.access_key_id {
            builder = builder.with_access_key_id(access_key);
        }
        if let Some(secret_key) = &config.secret_access_key {
            builder = builder.with_secret_access_key(secret_key);
        }

        // Allow HTTP for local development (MinIO)
        if config.allow_http {
            builder = builder.with_allow_http(true);
        }

        let store = builder.build().map_err(|e| {
            StorageError::Configuration(format!("Failed to create S3 client: {}", e))
        })?;

        let prefix = config.prefix.unwrap_or_default();

        info!(
            "Initialized S3 storage: bucket={}, region={}, endpoint={:?}, prefix={}",
            config.bucket, config.region, config.endpoint, prefix
        );

        Ok(Self {
            store: Arc::new(store),
            prefix,
        })
    }

    /// Get the object path for a blob digest
    fn blob_path(&self, digest: &str) -> Result<ObjectPath, StorageError> {
        let (algorithm, hash) = parse_digest(digest)?;

        if hash.len() < 2 {
            return Err(StorageError::InvalidDigest(format!(
                "Hash too short: {}",
                digest
            )));
        }

        // Use first 2 characters for sharding
        let shard = &hash[..2];
        let path = if self.prefix.is_empty() {
            format!("blobs/{}/{}/{}", algorithm, shard, hash)
        } else {
            format!("{}/blobs/{}/{}/{}", self.prefix, algorithm, shard, hash)
        };

        ObjectPath::parse(&path)
            .map_err(|e| StorageError::InvalidDigest(format!("Invalid path: {}", e)))
    }

    /// Get the object path for an upload session
    fn upload_path(&self, session_id: &str) -> ObjectPath {
        let path = if self.prefix.is_empty() {
            format!("uploads/{}", session_id)
        } else {
            format!("{}/uploads/{}", self.prefix, session_id)
        };

        ObjectPath::from(path)
    }
}

#[async_trait]
impl StorageBackend for S3Storage {
    async fn exists(&self, digest: &str) -> Result<bool, StorageError> {
        let path = self.blob_path(digest)?;

        match self.store.head(&path).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(StorageError::S3(e.to_string())),
        }
    }

    async fn size(&self, digest: &str) -> Result<u64, StorageError> {
        let path = self.blob_path(digest)?;

        let meta = self.store.head(&path).await.map_err(|e| match e {
            object_store::Error::NotFound { .. } => StorageError::NotFound(digest.to_string()),
            _ => StorageError::S3(e.to_string()),
        })?;

        Ok(meta.size as u64)
    }

    async fn read(&self, digest: &str) -> Result<Bytes, StorageError> {
        let path = self.blob_path(digest)?;
        debug!("Reading blob from S3: {:?}", path);

        let result = self.store.get(&path).await.map_err(|e| match e {
            object_store::Error::NotFound { .. } => StorageError::NotFound(digest.to_string()),
            _ => StorageError::S3(e.to_string()),
        })?;

        let bytes = result
            .bytes()
            .await
            .map_err(|e| StorageError::S3(format!("Failed to read bytes: {}", e)))?;

        Ok(bytes)
    }

    async fn read_range(&self, digest: &str, start: u64, end: u64) -> Result<Bytes, StorageError> {
        let path = self.blob_path(digest)?;
        debug!("Reading blob range {}-{} from S3: {:?}", start, end, path);

        let range = std::ops::Range {
            start: start as usize,
            end: (end + 1) as usize,
        };

        let result = self
            .store
            .get_range(&path, range)
            .await
            .map_err(|e| match e {
                object_store::Error::NotFound { .. } => StorageError::NotFound(digest.to_string()),
                _ => StorageError::S3(e.to_string()),
            })?;

        Ok(result)
    }

    async fn stream(&self, digest: &str) -> Result<ByteStream, StorageError> {
        let path = self.blob_path(digest)?;
        debug!("Streaming blob from S3: {:?}", path);

        let result = self.store.get(&path).await.map_err(|e| match e {
            object_store::Error::NotFound { .. } => StorageError::NotFound(digest.to_string()),
            _ => StorageError::S3(e.to_string()),
        })?;

        let stream = result
            .into_stream()
            .map_err(|e| StorageError::S3(format!("Stream error: {}", e)));

        Ok(Box::pin(stream))
    }

    async fn write(&self, digest: &str, data: Bytes) -> Result<String, StorageError> {
        // Verify digest
        let computed = compute_sha256(&data);
        if computed != digest {
            return Err(StorageError::DigestMismatch {
                expected: digest.to_string(),
                actual: computed,
            });
        }

        let path = self.blob_path(digest)?;
        debug!("Writing blob to S3: {:?}", path);

        self.store
            .put(&path, PutPayload::from(data))
            .await
            .map_err(|e| StorageError::S3(e.to_string()))?;

        Ok(path.to_string())
    }

    async fn write_stream(
        &self,
        digest: &str,
        mut stream: ByteStream,
        _expected_size: Option<u64>,
    ) -> Result<String, StorageError> {
        let path = self.blob_path(digest)?;
        debug!("Writing blob stream to S3: {:?}", path);

        // Use S3 multipart upload to avoid buffering entire blob in memory
        let mut upload = self
            .store
            .put_multipart(&path)
            .await
            .map_err(|e| StorageError::S3(format!("Failed to start multipart upload: {}", e)))?;

        let mut hasher = Sha256::new();
        let mut buffer = Vec::with_capacity(5 * 1024 * 1024); // 5MB minimum part size for S3

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            hasher.update(&chunk);
            buffer.extend_from_slice(&chunk);

            // Upload part when buffer reaches minimum size (5MB)
            // Last part can be smaller
            if buffer.len() >= 5 * 1024 * 1024 {
                upload
                    .put_part(PutPayload::from(Bytes::from(std::mem::take(&mut buffer))))
                    .await
                    .map_err(|e| StorageError::S3(format!("Failed to upload part: {}", e)))?;
                buffer = Vec::with_capacity(5 * 1024 * 1024);
            }
        }

        // Upload remaining data as final part
        if !buffer.is_empty() {
            upload
                .put_part(PutPayload::from(Bytes::from(buffer)))
                .await
                .map_err(|e| StorageError::S3(format!("Failed to upload final part: {}", e)))?;
        }

        // Complete multipart upload
        upload
            .complete()
            .await
            .map_err(|e| StorageError::S3(format!("Failed to complete multipart upload: {}", e)))?;

        // Verify digest
        let computed = format!("sha256:{}", hex::encode(hasher.finalize()));
        if computed != digest {
            // Clean up the uploaded object, log failure if cleanup fails
            if let Err(e) = self.store.delete(&path).await {
                warn!(
                    "Failed to clean up S3 object after digest mismatch (path: {:?}): {}",
                    path, e
                );
            }
            return Err(StorageError::DigestMismatch {
                expected: digest.to_string(),
                actual: computed,
            });
        }

        Ok(path.to_string())
    }

    async fn delete(&self, digest: &str) -> Result<bool, StorageError> {
        let path = self.blob_path(digest)?;
        debug!("Deleting blob from S3: {:?}", path);

        // Check if exists first
        let exists = self.exists(digest).await?;
        if !exists {
            return Ok(false);
        }

        self.store
            .delete(&path)
            .await
            .map_err(|e| StorageError::S3(e.to_string()))?;

        Ok(true)
    }

    fn storage_path(&self, digest: &str) -> String {
        self.blob_path(digest)
            .map(|p| format!("s3://{}", p))
            .unwrap_or_default()
    }

    async fn init_chunked_upload(&self, session_id: &str) -> Result<String, StorageError> {
        let path = self.upload_path(session_id);
        debug!("Initializing chunked upload at S3: {:?}", path);

        // Create empty object to mark upload session
        self.store
            .put(&path, PutPayload::from(Bytes::new()))
            .await
            .map_err(|e| StorageError::S3(e.to_string()))?;

        Ok(path.to_string())
    }

    async fn append_chunk(&self, session_id: &str, data: Bytes) -> Result<u64, StorageError> {
        let path = self.upload_path(session_id);
        debug!("Appending {} bytes to S3 upload: {:?}", data.len(), path);

        // S3 doesn't support append, so we need to read, append, and write back
        // This is inefficient but works for compatibility
        // For production, consider using S3 multipart uploads directly

        let existing = match self.store.get(&path).await {
            Ok(result) => result
                .bytes()
                .await
                .map_err(|e| StorageError::S3(format!("Failed to read existing data: {}", e)))?,
            Err(object_store::Error::NotFound { .. }) => Bytes::new(),
            Err(e) => return Err(StorageError::S3(e.to_string())),
        };

        let mut combined = existing.to_vec();
        combined.extend_from_slice(&data);
        let new_size = combined.len() as u64;

        self.store
            .put(&path, PutPayload::from(Bytes::from(combined)))
            .await
            .map_err(|e| StorageError::S3(e.to_string()))?;

        Ok(new_size)
    }

    async fn complete_chunked_upload(
        &self,
        session_id: &str,
        digest: &str,
    ) -> Result<String, StorageError> {
        let upload_path = self.upload_path(session_id);
        let blob_path = self.blob_path(digest)?;

        debug!(
            "Completing S3 chunked upload {:?} -> {:?}",
            upload_path, blob_path
        );

        // Stream uploaded data to compute digest without buffering entire blob
        let result = self.store.get(&upload_path).await.map_err(|e| match e {
            object_store::Error::NotFound { .. } => {
                StorageError::NotFound(format!("Upload session: {}", session_id))
            }
            _ => StorageError::S3(e.to_string()),
        })?;

        let mut stream = result.into_stream();
        let mut hasher = Sha256::new();

        // Stream through data to compute digest without buffering
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result
                .map_err(|e| StorageError::S3(format!("Failed to read chunk: {}", e)))?;
            hasher.update(&chunk);
        }

        // Verify digest
        let computed = format!("sha256:{}", hex::encode(hasher.finalize()));
        if computed != digest {
            // Clean up, log failure if cleanup fails
            if let Err(e) = self.store.delete(&upload_path).await {
                warn!(
                    "Failed to clean up S3 upload after digest mismatch (path: {:?}): {}",
                    upload_path, e
                );
            }
            return Err(StorageError::DigestMismatch {
                expected: digest.to_string(),
                actual: computed,
            });
        }

        // Use S3 copy operation to move to final location without re-downloading
        self.store
            .copy(&upload_path, &blob_path)
            .await
            .map_err(|e| StorageError::S3(format!("Failed to copy to final location: {}", e)))?;

        // Delete upload file
        if let Err(e) = self.store.delete(&upload_path).await {
            warn!(
                "Failed to delete S3 upload temp file after completion (path: {:?}): {}",
                upload_path, e
            );
        }

        Ok(blob_path.to_string())
    }

    async fn cancel_chunked_upload(&self, session_id: &str) -> Result<(), StorageError> {
        let path = self.upload_path(session_id);
        debug!("Canceling S3 chunked upload: {:?}", path);

        match self.store.delete(&path).await {
            Ok(()) => Ok(()),
            Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(e) => Err(StorageError::S3(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blob_path() {
        // This would require a mock S3 setup
        // For now, just test path generation logic
        let digest = "sha256:abc123def456";
        let (algo, hash) = parse_digest(digest).unwrap();
        assert_eq!(algo, "sha256");
        assert_eq!(hash, "abc123def456");
    }
}
