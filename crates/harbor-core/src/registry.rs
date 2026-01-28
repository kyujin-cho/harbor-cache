//! Registry service for OCI Distribution API operations

use bytes::Bytes;
use harbor_db::{Database, EntryType, NewUploadSession, UploadSession};
use harbor_proxy::HarborClient;
use harbor_storage::StorageBackend;
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::cache::CacheManager;
use crate::error::CoreError;

/// Registry service handling OCI Distribution API operations
pub struct RegistryService {
    cache: Arc<CacheManager>,
    upstream: Arc<HarborClient>,
    db: Database,
    storage: Arc<dyn StorageBackend>,
}

impl RegistryService {
    /// Create a new registry service
    pub fn new(
        cache: Arc<CacheManager>,
        upstream: Arc<HarborClient>,
        db: Database,
        storage: Arc<dyn StorageBackend>,
    ) -> Self {
        Self {
            cache,
            upstream,
            db,
            storage,
        }
    }

    // ==================== Manifest Operations ====================

    /// Get a manifest (cache-aside pattern)
    pub async fn get_manifest(
        &self,
        repository: &str,
        reference: &str,
    ) -> Result<(Bytes, String, String), CoreError> {
        // Validate digest format at service boundary to prevent path traversal
        // when reference is a digest (starts with sha256: or sha512:)
        if reference.starts_with("sha256:") || reference.starts_with("sha512:") {
            harbor_storage::backend::validate_digest(reference)?;
        }

        // First, check if reference is a digest
        let _cache_key = if reference.starts_with("sha256:") {
            reference.to_string()
        } else {
            // For tags, we need to check upstream to get the digest
            // But first try cache with tag as reference
            format!("{}:{}", repository, reference)
        };

        debug!("Getting manifest: {}:{}", repository, reference);

        // Check cache first (by digest if available)
        if reference.starts_with("sha256:")
            && let Some((data, entry)) = self.cache.get(reference).await?
        {
            info!("Cache hit for manifest: {}", reference);
            return Ok((data, entry.content_type, reference.to_string()));
        }

        // Cache miss - fetch from upstream
        info!(
            "Cache miss for manifest: {}:{}, fetching from upstream",
            repository, reference
        );

        let (data, content_type, digest) = self
            .upstream
            .get_manifest(repository, reference)
            .await
            .map_err(|e| {
                if matches!(e, harbor_proxy::ProxyError::NotFound(_)) {
                    CoreError::NotFound(format!("{}:{}", repository, reference))
                } else {
                    CoreError::Proxy(e)
                }
            })?;

        // Compute digest if not provided
        let digest = if digest.is_empty() {
            harbor_storage::backend::compute_sha256(&data)
        } else {
            digest
        };

        // Store in cache
        self.cache
            .put(
                EntryType::Manifest,
                Some(repository.to_string()),
                Some(reference.to_string()),
                &digest,
                &content_type,
                data.clone(),
            )
            .await?;

        Ok((data, content_type, digest))
    }

    /// Check if a manifest exists (HEAD request)
    pub async fn manifest_exists(
        &self,
        repository: &str,
        reference: &str,
    ) -> Result<Option<(String, String, i64)>, CoreError> {
        // Validate digest format at service boundary to prevent path traversal
        if reference.starts_with("sha256:") || reference.starts_with("sha512:") {
            harbor_storage::backend::validate_digest(reference)?;
        }

        // Check cache first if reference is a digest
        if reference.starts_with("sha256:")
            && let Some(entry) = self.cache.get_metadata(reference).await?
        {
            return Ok(Some((
                entry.content_type,
                reference.to_string(),
                entry.size,
            )));
        }

        // Try to get from upstream (this will cache it)
        match self.get_manifest(repository, reference).await {
            Ok((data, content_type, digest)) => Ok(Some((content_type, digest, data.len() as i64))),
            Err(CoreError::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Push a manifest
    pub async fn put_manifest(
        &self,
        repository: &str,
        reference: &str,
        content_type: &str,
        data: Bytes,
    ) -> Result<String, CoreError> {
        // Validate digest format at service boundary to prevent path traversal
        // when reference is a digest (starts with sha256: or sha512:)
        if reference.starts_with("sha256:") || reference.starts_with("sha512:") {
            harbor_storage::backend::validate_digest(reference)?;
        }

        debug!("Pushing manifest: {}:{}", repository, reference);

        // Compute digest
        let digest = harbor_storage::backend::compute_sha256(&data);

        // Push to upstream first
        let upstream_digest = self
            .upstream
            .push_manifest(repository, reference, data.clone(), content_type)
            .await?;

        // Verify digest matches
        let final_digest = if upstream_digest.is_empty() {
            digest.clone()
        } else {
            upstream_digest
        };

        // Store in cache
        self.cache
            .put(
                EntryType::Manifest,
                Some(repository.to_string()),
                Some(reference.to_string()),
                &final_digest,
                content_type,
                data,
            )
            .await?;

        info!(
            "Pushed manifest: {}:{} -> {}",
            repository, reference, final_digest
        );
        Ok(final_digest)
    }

    // ==================== Blob Operations ====================

    /// Get a blob as a stream (cache-aside pattern with tee for simultaneous caching and serving)
    pub async fn get_blob(
        &self,
        repository: &str,
        digest: &str,
    ) -> Result<(harbor_storage::backend::ByteStream, u64), CoreError> {
        // Validate digest format at service boundary to prevent path traversal
        harbor_storage::backend::validate_digest(digest)?;
        debug!("Getting blob stream: {}", digest);

        // Check cache first
        if let Some((stream, entry)) = self.cache.get_stream(digest).await? {
            info!("Cache hit for blob: {}", digest);
            return Ok((stream, entry.size as u64));
        }

        // Cache miss - fetch from upstream with streaming
        info!("Cache miss for blob: {}, fetching from upstream", digest);

        let (stream, size) = self
            .upstream
            .get_blob_stream(repository, digest)
            .await
            .map_err(|e| {
                if matches!(e, harbor_proxy::ProxyError::NotFound(_)) {
                    CoreError::NotFound(digest.to_string())
                } else {
                    CoreError::Proxy(e)
                }
            })?;

        // Convert ProxyError stream to StorageError stream for caching
        use futures::StreamExt;
        let storage_stream: harbor_storage::backend::ByteStream = Box::pin(stream.map(|result| {
            result.map_err(|e| harbor_storage::StorageError::Io(
                std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
            ))
        }));

        // Tee the stream: one copy to cache, one copy to return
        let (client_stream, cache_handle) = self
            .cache
            .tee_and_cache_stream(
                EntryType::Blob,
                Some(repository.to_string()),
                None,
                digest,
                "application/octet-stream",
                storage_stream,
                Some(size),
            )
            .await?;

        // Spawn a wrapper task that awaits the cache handle and logs errors.
        // We don't block the client response on caching completion.
        let digest_for_log = digest.to_string();
        tokio::spawn(async move {
            match cache_handle.await {
                Ok(Ok(_entry)) => {
                    debug!("Background cache write succeeded for {}", digest_for_log);
                }
                Ok(Err(e)) => {
                    warn!(
                        "Background cache write failed for {}: {}",
                        digest_for_log, e
                    );
                }
                Err(e) => {
                    warn!(
                        "Background cache task panicked for {}: {:?}",
                        digest_for_log, e
                    );
                }
            }
        });

        Ok((client_stream, size))
    }

    /// Get a blob fully buffered (for cases that need in-memory data)
    #[allow(dead_code)]
    pub async fn get_blob_buffered(&self, repository: &str, digest: &str) -> Result<Bytes, CoreError> {
        // Validate digest format at service boundary to prevent path traversal
        harbor_storage::backend::validate_digest(digest)?;
        debug!("Getting blob buffered: {}", digest);

        // Check cache first
        if let Some((data, _entry)) = self.cache.get(digest).await? {
            info!("Cache hit for blob: {}", digest);
            return Ok(data);
        }

        // Cache miss - fetch from upstream
        info!("Cache miss for blob: {}, fetching from upstream", digest);

        #[allow(deprecated)]
        let (data, _size) = self
            .upstream
            .get_blob(repository, digest)
            .await
            .map_err(|e| {
                if matches!(e, harbor_proxy::ProxyError::NotFound(_)) {
                    CoreError::NotFound(digest.to_string())
                } else {
                    CoreError::Proxy(e)
                }
            })?;

        // Store in cache
        self.cache
            .put(
                EntryType::Blob,
                Some(repository.to_string()),
                None,
                digest,
                "application/octet-stream",
                data.clone(),
            )
            .await?;

        Ok(data)
    }

    /// Check if a blob exists (HEAD request - no download)
    pub async fn blob_exists(
        &self,
        repository: &str,
        digest: &str,
    ) -> Result<Option<i64>, CoreError> {
        // Validate digest format at service boundary to prevent path traversal
        harbor_storage::backend::validate_digest(digest)?;
        // Check cache first
        if let Some(entry) = self.cache.get_metadata(digest).await? {
            return Ok(Some(entry.size));
        }

        // Check upstream with HEAD request only (no download)
        match self.upstream.get_blob_size(repository, digest).await {
            Ok((size, _content_type)) => {
                // Optionally trigger background cache warm-up
                // For now, just return the size without downloading
                Ok(Some(size as i64))
            }
            Err(harbor_proxy::ProxyError::NotFound(_)) => Ok(None),
            Err(e) => Err(CoreError::Proxy(e)),
        }
    }

    // ==================== Upload Operations ====================

    /// Validate session ID format to prevent path traversal attacks.
    /// Session IDs must be valid UUIDs (lowercase hex with dashes).
    fn validate_session_id(session_id: &str) -> Result<(), CoreError> {
        // UUID format: 8-4-4-4-12 lowercase hex characters with dashes
        // e.g., "550e8400-e29b-41d4-a716-446655440000"
        if session_id.len() != 36 {
            return Err(CoreError::BadRequest(format!(
                "Invalid session ID format: {}",
                session_id
            )));
        }

        // Check UUID format with dashes at correct positions
        let parts: Vec<&str> = session_id.split('-').collect();
        if parts.len() != 5 {
            return Err(CoreError::BadRequest(format!(
                "Invalid session ID format: {}",
                session_id
            )));
        }

        let expected_lens = [8, 4, 4, 4, 12];
        for (part, &expected_len) in parts.iter().zip(expected_lens.iter()) {
            if part.len() != expected_len {
                return Err(CoreError::BadRequest(format!(
                    "Invalid session ID format: {}",
                    session_id
                )));
            }
            // Must be lowercase hex only
            if !part.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
                return Err(CoreError::BadRequest(format!(
                    "Invalid session ID format (must be lowercase hex): {}",
                    session_id
                )));
            }
        }

        Ok(())
    }

    /// Start a blob upload session
    pub async fn start_upload(&self, repository: &str) -> Result<String, CoreError> {
        let session_id = Uuid::new_v4().to_string();
        let temp_path = self.storage.init_chunked_upload(&session_id).await?;

        debug!("Starting upload session: {} for {}", session_id, repository);

        self.db
            .create_upload_session(NewUploadSession {
                id: session_id.clone(),
                repository: repository.to_string(),
                temp_path,
            })
            .await?;

        Ok(session_id)
    }

    /// Get upload session info
    pub async fn get_upload_session(
        &self,
        session_id: &str,
    ) -> Result<Option<UploadSession>, CoreError> {
        // Validate session ID format to prevent path traversal
        Self::validate_session_id(session_id)?;
        Ok(self.db.get_upload_session(session_id).await?)
    }

    /// Append data to an upload session
    pub async fn append_upload(&self, session_id: &str, data: Bytes) -> Result<i64, CoreError> {
        // Validate session ID format to prevent path traversal
        Self::validate_session_id(session_id)?;
        debug!("Appending {} bytes to upload: {}", data.len(), session_id);

        let new_size = self.storage.append_chunk(session_id, data).await?;
        self.db
            .update_upload_session(session_id, new_size as i64)
            .await?;

        Ok(new_size as i64)
    }

    /// Complete an upload session (with streaming push to upstream)
    pub async fn complete_upload(
        &self,
        repository: &str,
        session_id: &str,
        digest: &str,
    ) -> Result<(), CoreError> {
        // Validate session ID format to prevent path traversal
        Self::validate_session_id(session_id)?;
        // Validate digest format at service boundary to prevent path traversal
        harbor_storage::backend::validate_digest(digest)?;
        debug!("Completing upload: {} -> {}", session_id, digest);

        // Get session info
        let _session = self
            .db
            .get_upload_session(session_id)
            .await?
            .ok_or_else(|| CoreError::NotFound(format!("Upload session: {}", session_id)))?;

        // Complete the chunked upload (validates digest)
        let storage_path = self
            .storage
            .complete_chunked_upload(session_id, digest)
            .await?;

        // Get the size
        let size = self.storage.size(digest).await?;

        // Stream the data for pushing to upstream (avoid buffering in memory)
        let storage_stream = self.storage.stream(digest).await?;

        // Convert StorageError stream to ProxyError stream for upstream
        use futures::StreamExt;
        let proxy_stream: harbor_proxy::client::ByteStream = Box::pin(storage_stream.map(|result| {
            result.map_err(|e| harbor_proxy::ProxyError::InvalidResponse(e.to_string()))
        }));

        // Push to upstream with streaming
        self.upstream
            .push_blob_stream(repository, digest, proxy_stream, size)
            .await?;

        // Create cache entry
        self.db
            .insert_cache_entry(harbor_db::NewCacheEntry {
                entry_type: EntryType::Blob,
                repository: Some(repository.to_string()),
                reference: None,
                digest: digest.to_string(),
                content_type: "application/octet-stream".to_string(),
                size: size as i64,
                storage_path,
            })
            .await?;

        // Delete upload session
        self.db.delete_upload_session(session_id).await?;

        info!("Completed upload: {} -> {}", session_id, digest);
        Ok(())
    }

    /// Cancel an upload session
    pub async fn cancel_upload(&self, session_id: &str) -> Result<(), CoreError> {
        // Validate session ID format to prevent path traversal
        Self::validate_session_id(session_id)?;
        debug!("Canceling upload: {}", session_id);

        self.storage.cancel_chunked_upload(session_id).await?;
        self.db.delete_upload_session(session_id).await?;

        Ok(())
    }

    /// Mount a blob from another repository (if it exists in cache) with streaming
    pub async fn mount_blob(
        &self,
        repository: &str,
        digest: &str,
        from: &str,
    ) -> Result<bool, CoreError> {
        // Validate digest format at service boundary to prevent path traversal
        harbor_storage::backend::validate_digest(digest)?;
        debug!(
            "Attempting to mount blob {} from {} to {}",
            digest, from, repository
        );

        // Check if blob exists in cache
        if self.cache.exists(digest).await? {
            info!("Blob {} found in cache, mount successful", digest);
            return Ok(true);
        }

        // Check if it exists in upstream (from source)
        if self.upstream.blob_exists(from, digest).await? {
            // Fetch from source and cache with streaming
            let (proxy_stream, size) = self.upstream.get_blob_stream(from, digest).await?;

            // Convert ProxyError stream to StorageError stream for caching
            use futures::StreamExt;
            let storage_stream: harbor_storage::backend::ByteStream = Box::pin(proxy_stream.map(|result| {
                result.map_err(|e| harbor_storage::StorageError::Io(
                    std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
                ))
            }));

            self.cache
                .put_stream(
                    EntryType::Blob,
                    Some(repository.to_string()),
                    None,
                    digest,
                    "application/octet-stream",
                    storage_stream,
                    Some(size),
                )
                .await?;

            info!("Blob {} mounted from {} to {}", digest, from, repository);
            return Ok(true);
        }

        Ok(false)
    }
}
