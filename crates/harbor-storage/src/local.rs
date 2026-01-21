//! Local disk storage backend

use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, info};

use crate::backend::{compute_sha256, parse_digest, ByteStream, StorageBackend};
use crate::error::StorageError;

/// Local disk storage backend
///
/// Stores blobs in a content-addressable directory structure:
/// `<base_path>/blobs/<algorithm>/<first 2 chars>/<digest>`
pub struct LocalStorage {
    base_path: PathBuf,
    uploads_path: PathBuf,
}

impl LocalStorage {
    /// Create a new local storage backend
    pub async fn new(base_path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let base_path = base_path.as_ref().to_path_buf();
        let uploads_path = base_path.join("uploads");

        // Create directories
        fs::create_dir_all(&base_path).await?;
        fs::create_dir_all(&uploads_path).await?;
        fs::create_dir_all(base_path.join("blobs")).await?;

        info!("Initialized local storage at {:?}", base_path);

        Ok(Self {
            base_path,
            uploads_path,
        })
    }

    /// Get the file path for a digest
    fn blob_path(&self, digest: &str) -> Result<PathBuf, StorageError> {
        let (algorithm, hash) = parse_digest(digest)?;

        if hash.len() < 2 {
            return Err(StorageError::InvalidDigest(format!(
                "Hash too short: {}",
                digest
            )));
        }

        // Use first 2 characters for sharding
        let shard = &hash[..2];
        Ok(self
            .base_path
            .join("blobs")
            .join(algorithm)
            .join(shard)
            .join(hash))
    }

    /// Get the temp file path for an upload session
    fn upload_path(&self, session_id: &str) -> PathBuf {
        self.uploads_path.join(session_id)
    }
}

#[async_trait]
impl StorageBackend for LocalStorage {
    async fn exists(&self, digest: &str) -> Result<bool, StorageError> {
        let path = self.blob_path(digest)?;
        Ok(path.exists())
    }

    async fn size(&self, digest: &str) -> Result<u64, StorageError> {
        let path = self.blob_path(digest)?;
        let metadata = fs::metadata(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(digest.to_string())
            } else {
                StorageError::Io(e)
            }
        })?;
        Ok(metadata.len())
    }

    async fn read(&self, digest: &str) -> Result<Bytes, StorageError> {
        let path = self.blob_path(digest)?;
        debug!("Reading blob from {:?}", path);

        let data = fs::read(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(digest.to_string())
            } else {
                StorageError::Io(e)
            }
        })?;

        Ok(Bytes::from(data))
    }

    async fn read_range(&self, digest: &str, start: u64, end: u64) -> Result<Bytes, StorageError> {
        let path = self.blob_path(digest)?;
        debug!("Reading blob range {}-{} from {:?}", start, end, path);

        let mut file = File::open(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(digest.to_string())
            } else {
                StorageError::Io(e)
            }
        })?;

        use tokio::io::AsyncSeekExt;
        file.seek(std::io::SeekFrom::Start(start)).await?;

        let len = (end - start + 1) as usize;
        let mut buffer = vec![0u8; len];
        file.read_exact(&mut buffer).await?;

        Ok(Bytes::from(buffer))
    }

    async fn stream(&self, digest: &str) -> Result<ByteStream, StorageError> {
        let path = self.blob_path(digest)?;
        debug!("Streaming blob from {:?}", path);

        let file = File::open(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(digest.to_string())
            } else {
                StorageError::Io(e)
            }
        })?;

        let reader = BufReader::new(file);
        let stream = tokio_util::io::ReaderStream::new(reader);

        Ok(Box::pin(stream.map(|result| {
            result.map_err(StorageError::Io)
        })))
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
        debug!("Writing blob to {:?}", path);

        // Create parent directories
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Write atomically using a temp file
        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, &data).await?;
        fs::rename(&temp_path, &path).await?;

        Ok(path.to_string_lossy().to_string())
    }

    async fn write_stream(
        &self,
        digest: &str,
        mut stream: ByteStream,
        _expected_size: Option<u64>,
    ) -> Result<String, StorageError> {
        let path = self.blob_path(digest)?;
        debug!("Writing blob stream to {:?}", path);

        // Create parent directories
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Write to temp file while computing digest
        let temp_path = path.with_extension("tmp");
        let mut file = File::create(&temp_path).await?;
        let mut hasher = Sha256::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
        }

        file.flush().await?;
        drop(file);

        // Verify digest
        let computed = format!("sha256:{}", hex::encode(hasher.finalize()));
        if computed != digest {
            fs::remove_file(&temp_path).await?;
            return Err(StorageError::DigestMismatch {
                expected: digest.to_string(),
                actual: computed,
            });
        }

        // Move to final location
        fs::rename(&temp_path, &path).await?;

        Ok(path.to_string_lossy().to_string())
    }

    async fn delete(&self, digest: &str) -> Result<bool, StorageError> {
        let path = self.blob_path(digest)?;
        debug!("Deleting blob at {:?}", path);

        match fs::remove_file(&path).await {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(StorageError::Io(e)),
        }
    }

    fn storage_path(&self, digest: &str) -> String {
        self.blob_path(digest)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    async fn init_chunked_upload(&self, session_id: &str) -> Result<String, StorageError> {
        let path = self.upload_path(session_id);
        debug!("Initializing chunked upload at {:?}", path);

        // Create empty file
        File::create(&path).await?;

        Ok(path.to_string_lossy().to_string())
    }

    async fn append_chunk(&self, session_id: &str, data: Bytes) -> Result<u64, StorageError> {
        let path = self.upload_path(session_id);
        debug!("Appending {} bytes to upload {:?}", data.len(), path);

        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    StorageError::NotFound(format!("Upload session: {}", session_id))
                } else {
                    StorageError::Io(e)
                }
            })?;

        file.write_all(&data).await?;
        file.flush().await?;

        // Return new total size
        let metadata = fs::metadata(&path).await?;
        Ok(metadata.len())
    }

    async fn complete_chunked_upload(
        &self,
        session_id: &str,
        digest: &str,
    ) -> Result<String, StorageError> {
        let upload_path = self.upload_path(session_id);
        let blob_path = self.blob_path(digest)?;

        debug!(
            "Completing chunked upload {:?} -> {:?}",
            upload_path, blob_path
        );

        // Read and verify digest
        let data = fs::read(&upload_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(format!("Upload session: {}", session_id))
            } else {
                StorageError::Io(e)
            }
        })?;

        let computed = compute_sha256(&data);
        if computed != digest {
            // Clean up
            let _ = fs::remove_file(&upload_path).await;
            return Err(StorageError::DigestMismatch {
                expected: digest.to_string(),
                actual: computed,
            });
        }

        // Create parent directories
        if let Some(parent) = blob_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Move to final location
        fs::rename(&upload_path, &blob_path).await?;

        Ok(blob_path.to_string_lossy().to_string())
    }

    async fn cancel_chunked_upload(&self, session_id: &str) -> Result<(), StorageError> {
        let path = self.upload_path(session_id);
        debug!("Canceling chunked upload at {:?}", path);

        match fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StorageError::Io(e)),
        }
    }
}
