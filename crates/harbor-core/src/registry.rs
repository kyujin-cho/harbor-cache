//! Registry service for OCI Distribution API operations

use bytes::Bytes;
use harbor_db::{Database, EntryType, NewUploadSession, UploadSession};
use harbor_proxy::HarborClient;
use harbor_storage::StorageBackend;
use std::sync::Arc;
use tracing::{debug, info};
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

    /// Get a blob (cache-aside pattern)
    pub async fn get_blob(&self, repository: &str, digest: &str) -> Result<Bytes, CoreError> {
        debug!("Getting blob: {}", digest);

        // Check cache first
        if let Some((data, _entry)) = self.cache.get(digest).await? {
            info!("Cache hit for blob: {}", digest);
            return Ok(data);
        }

        // Cache miss - fetch from upstream
        info!("Cache miss for blob: {}, fetching from upstream", digest);

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

    /// Check if a blob exists (HEAD request)
    pub async fn blob_exists(
        &self,
        repository: &str,
        digest: &str,
    ) -> Result<Option<i64>, CoreError> {
        // Check cache first
        if let Some(entry) = self.cache.get_metadata(digest).await? {
            return Ok(Some(entry.size));
        }

        // Check upstream
        if self.upstream.blob_exists(repository, digest).await? {
            // Fetch and cache it
            match self.get_blob(repository, digest).await {
                Ok(data) => Ok(Some(data.len() as i64)),
                Err(_) => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    // ==================== Upload Operations ====================

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
        Ok(self.db.get_upload_session(session_id).await?)
    }

    /// Append data to an upload session
    pub async fn append_upload(&self, session_id: &str, data: Bytes) -> Result<i64, CoreError> {
        debug!("Appending {} bytes to upload: {}", data.len(), session_id);

        let new_size = self.storage.append_chunk(session_id, data).await?;
        self.db
            .update_upload_session(session_id, new_size as i64)
            .await?;

        Ok(new_size as i64)
    }

    /// Complete an upload session
    pub async fn complete_upload(
        &self,
        repository: &str,
        session_id: &str,
        digest: &str,
    ) -> Result<(), CoreError> {
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

        // Read the data for pushing to upstream
        let data = self.storage.read(digest).await?;

        // Push to upstream
        self.upstream
            .push_blob(repository, digest, data.clone())
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
        debug!("Canceling upload: {}", session_id);

        self.storage.cancel_chunked_upload(session_id).await?;
        self.db.delete_upload_session(session_id).await?;

        Ok(())
    }

    /// Mount a blob from another repository (if it exists in cache)
    pub async fn mount_blob(
        &self,
        repository: &str,
        digest: &str,
        from: &str,
    ) -> Result<bool, CoreError> {
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
            // Fetch from source and cache
            let (data, _size) = self.upstream.get_blob(from, digest).await?;

            self.cache
                .put(
                    EntryType::Blob,
                    Some(repository.to_string()),
                    None,
                    digest,
                    "application/octet-stream",
                    data,
                )
                .await?;

            info!("Blob {} mounted from {} to {}", digest, from, repository);
            return Ok(true);
        }

        Ok(false)
    }
}
