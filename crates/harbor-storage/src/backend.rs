//! Storage backend trait

use async_trait::async_trait;
use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;

use crate::error::StorageError;

/// Type alias for a boxed stream of bytes
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, StorageError>> + Send>>;

/// Storage backend trait
///
/// Implementations of this trait provide content-addressable storage
/// for blobs and manifests.
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Check if a blob exists
    async fn exists(&self, digest: &str) -> Result<bool, StorageError>;

    /// Get the size of a blob
    async fn size(&self, digest: &str) -> Result<u64, StorageError>;

    /// Read a blob fully into memory
    async fn read(&self, digest: &str) -> Result<Bytes, StorageError>;

    /// Read a range of bytes from a blob
    async fn read_range(&self, digest: &str, start: u64, end: u64) -> Result<Bytes, StorageError>;

    /// Stream a blob
    async fn stream(&self, digest: &str) -> Result<ByteStream, StorageError>;

    /// Write a blob (verifies digest after writing)
    async fn write(&self, digest: &str, data: Bytes) -> Result<String, StorageError>;

    /// Write a blob from a stream
    async fn write_stream(
        &self,
        digest: &str,
        stream: ByteStream,
        expected_size: Option<u64>,
    ) -> Result<String, StorageError>;

    /// Delete a blob
    async fn delete(&self, digest: &str) -> Result<bool, StorageError>;

    /// Get the storage path for a digest (for metadata tracking)
    fn storage_path(&self, digest: &str) -> String;

    /// Initialize a chunked upload session, returns temp file path
    async fn init_chunked_upload(&self, session_id: &str) -> Result<String, StorageError>;

    /// Append data to a chunked upload
    async fn append_chunk(&self, session_id: &str, data: Bytes) -> Result<u64, StorageError>;

    /// Complete a chunked upload, verify digest, move to final location
    async fn complete_chunked_upload(
        &self,
        session_id: &str,
        digest: &str,
    ) -> Result<String, StorageError>;

    /// Cancel a chunked upload
    async fn cancel_chunked_upload(&self, session_id: &str) -> Result<(), StorageError>;
}

/// Parse a digest string (e.g., "sha256:abc123...")
pub fn parse_digest(digest: &str) -> Result<(&str, &str), StorageError> {
    let parts: Vec<&str> = digest.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(StorageError::InvalidDigest(format!(
            "Invalid digest format: {}",
            digest
        )));
    }
    Ok((parts[0], parts[1]))
}

/// Compute SHA256 digest of data
pub fn compute_sha256(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    format!("sha256:{}", hex::encode(result))
}
