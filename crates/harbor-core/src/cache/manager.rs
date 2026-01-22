//! Cache manager implementation

use bytes::Bytes;
use chrono::{Duration, Utc};
use harbor_db::{CacheEntry, CacheStats, Database, EntryType, NewCacheEntry};
use harbor_storage::StorageBackend;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::policy::EvictionPolicy;
use crate::error::CoreError;

/// Configuration for the cache manager
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum cache size in bytes
    pub max_size: u64,
    /// Retention period in days
    pub retention_days: u32,
    /// Eviction policy
    pub eviction_policy: EvictionPolicy,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_size: 10 * 1024 * 1024 * 1024, // 10 GB
            retention_days: 30,
            eviction_policy: EvictionPolicy::Lru,
        }
    }
}

/// Cache manager for handling blob and manifest caching
pub struct CacheManager {
    db: Database,
    storage: Arc<dyn StorageBackend>,
    config: CacheConfig,
    stats: RwLock<CacheStats>,
}

impl CacheManager {
    /// Create a new cache manager
    pub fn new(db: Database, storage: Arc<dyn StorageBackend>, config: CacheConfig) -> Self {
        info!(
            "Initializing cache manager (max_size: {} bytes, retention: {} days, policy: {})",
            config.max_size,
            config.retention_days,
            config.eviction_policy.as_str()
        );

        Self {
            db,
            storage,
            config,
            stats: RwLock::new(CacheStats::default()),
        }
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let mut stats: CacheStats = self.stats.read().await.clone();

        // Update from database
        if let Ok(db_stats) = self.db.get_cache_stats().await {
            stats.total_size = db_stats.total_size;
            stats.entry_count = db_stats.entry_count;
            stats.manifest_count = db_stats.manifest_count;
            stats.blob_count = db_stats.blob_count;
        }

        stats
    }

    /// Check if a blob/manifest is cached
    pub async fn exists(&self, digest: &str) -> Result<bool, CoreError> {
        let entry = self.db.get_cache_entry_by_digest(digest).await?;
        if entry.is_some() {
            // Also verify storage
            return Ok(self.storage.exists(digest).await?);
        }
        Ok(false)
    }

    /// Get a cached entry
    pub async fn get(&self, digest: &str) -> Result<Option<(Bytes, CacheEntry)>, CoreError> {
        let entry = match self.db.get_cache_entry_by_digest(digest).await? {
            Some(e) => e,
            None => {
                self.record_miss().await;
                return Ok(None);
            }
        };

        // Read from storage
        match self.storage.read(digest).await {
            Ok(data) => {
                // Update access time
                self.db.touch_cache_entry(digest).await?;
                self.record_hit().await;
                Ok(Some((data, entry)))
            }
            Err(harbor_storage::StorageError::NotFound(_)) => {
                // Storage doesn't have it, clean up database
                warn!("Cache entry in database but not in storage: {}", digest);
                self.db.delete_cache_entry(digest).await?;
                self.record_miss().await;
                Ok(None)
            }
            Err(e) => Err(CoreError::Storage(e)),
        }
    }

    /// Get a cached entry's metadata only
    pub async fn get_metadata(&self, digest: &str) -> Result<Option<CacheEntry>, CoreError> {
        Ok(self.db.get_cache_entry_by_digest(digest).await?)
    }

    /// Store a blob/manifest in the cache
    pub async fn put(
        &self,
        entry_type: EntryType,
        repository: Option<String>,
        reference: Option<String>,
        digest: &str,
        content_type: &str,
        data: Bytes,
    ) -> Result<CacheEntry, CoreError> {
        let size = data.len() as i64;

        debug!(
            "Caching {} {} ({} bytes)",
            entry_type.as_str(),
            digest,
            size
        );

        // Check if already cached
        if let Some(entry) = self.db.get_cache_entry_by_digest(digest).await? {
            debug!("Entry already cached: {}", digest);
            self.db.touch_cache_entry(digest).await?;
            return Ok(entry);
        }

        // Ensure we have space
        self.ensure_space(size as u64).await?;

        // Write to storage
        let storage_path = self.storage.write(digest, data).await?;

        // Create database entry
        let entry = self
            .db
            .insert_cache_entry(NewCacheEntry {
                entry_type,
                repository,
                reference,
                digest: digest.to_string(),
                content_type: content_type.to_string(),
                size,
                storage_path,
            })
            .await?;

        debug!("Cached entry: {}", digest);
        Ok(entry)
    }

    /// Delete a cached entry
    pub async fn delete(&self, digest: &str) -> Result<bool, CoreError> {
        debug!("Deleting cache entry: {}", digest);

        // Delete from storage first
        self.storage.delete(digest).await?;

        // Delete from database
        let deleted = self.db.delete_cache_entry(digest).await?;
        Ok(deleted)
    }

    /// Clear all cache entries
    pub async fn clear(&self) -> Result<u64, CoreError> {
        info!("Clearing all cache entries");

        let entries = self.db.get_cache_entries_lru(10000).await?;
        let count = entries.len() as u64;

        for entry in entries {
            if let Err(e) = self.storage.delete(&entry.digest).await {
                warn!("Failed to delete storage for {}: {}", entry.digest, e);
            }
            if let Err(e) = self.db.delete_cache_entry(&entry.digest).await {
                warn!("Failed to delete db entry for {}: {}", entry.digest, e);
            }
        }

        info!("Cleared {} cache entries", count);
        Ok(count)
    }

    /// Ensure there's enough space for a new entry
    async fn ensure_space(&self, required: u64) -> Result<(), CoreError> {
        let current_size = self.db.get_total_cache_size().await? as u64;

        if current_size + required <= self.config.max_size {
            return Ok(());
        }

        let to_free = current_size + required - self.config.max_size;
        info!("Cache size limit reached, need to free {} bytes", to_free);

        self.evict(to_free).await
    }

    /// Evict entries to free up space
    async fn evict(&self, bytes_to_free: u64) -> Result<(), CoreError> {
        let mut freed = 0u64;

        // Get entries to evict based on policy
        let entries = match self.config.eviction_policy {
            EvictionPolicy::Lru => self.db.get_cache_entries_lru(100).await?,
            EvictionPolicy::Lfu => {
                // For LFU, we'd need a different query sorted by access_count
                // For now, use LRU as fallback
                self.db.get_cache_entries_lru(100).await?
            }
            EvictionPolicy::Fifo => {
                // For FIFO, we'd need a query sorted by created_at
                // For now, use LRU as fallback
                self.db.get_cache_entries_lru(100).await?
            }
        };

        for entry in entries {
            if freed >= bytes_to_free {
                break;
            }

            debug!("Evicting cache entry: {}", entry.digest);

            if let Err(e) = self.storage.delete(&entry.digest).await {
                warn!("Failed to delete storage for {}: {}", entry.digest, e);
            }

            if let Err(e) = self.db.delete_cache_entry(&entry.digest).await {
                warn!("Failed to delete db entry for {}: {}", entry.digest, e);
            }

            freed += entry.size as u64;
        }

        info!("Evicted {} bytes from cache", freed);
        Ok(())
    }

    /// Run cleanup of expired entries
    pub async fn cleanup_expired(&self) -> Result<u64, CoreError> {
        let cutoff = Utc::now() - Duration::days(self.config.retention_days as i64);
        info!("Cleaning up entries older than {:?}", cutoff);

        let entries = self.db.get_cache_entries_lru(10000).await?;
        let mut cleaned = 0u64;

        for entry in entries {
            if entry.last_accessed_at < cutoff {
                debug!("Cleaning expired entry: {}", entry.digest);

                if let Err(e) = self.storage.delete(&entry.digest).await {
                    warn!("Failed to delete storage for {}: {}", entry.digest, e);
                }

                if let Err(e) = self.db.delete_cache_entry(&entry.digest).await {
                    warn!("Failed to delete db entry for {}: {}", entry.digest, e);
                }

                cleaned += 1;
            }
        }

        info!("Cleaned up {} expired entries", cleaned);
        Ok(cleaned)
    }

    /// Record a cache hit
    async fn record_hit(&self) {
        let mut stats = self.stats.write().await;
        stats.hit_count += 1;
    }

    /// Record a cache miss
    async fn record_miss(&self) {
        let mut stats = self.stats.write().await;
        stats.miss_count += 1;
    }

    /// Run size enforcement to ensure cache is within limits
    pub async fn enforce_size_limit(&self) -> Result<u64, CoreError> {
        let current_size = self.db.get_total_cache_size().await? as u64;

        if current_size <= self.config.max_size {
            return Ok(0);
        }

        let to_free = current_size - self.config.max_size;
        info!(
            "Cache size {} exceeds limit {}, freeing {} bytes",
            current_size, self.config.max_size, to_free
        );

        self.evict(to_free).await?;
        Ok(to_free)
    }

    /// Run full maintenance: cleanup expired entries and enforce size limits
    pub async fn run_maintenance(&self) -> Result<(u64, u64), CoreError> {
        info!("Running cache maintenance");

        // First, clean up expired entries
        let expired = self.cleanup_expired().await?;

        // Then, enforce size limits
        let freed = self.enforce_size_limit().await?;

        info!(
            "Maintenance complete: {} expired entries removed, {} bytes freed",
            expired, freed
        );

        Ok((expired, freed))
    }
}

/// Spawn a background task that runs cache maintenance periodically
pub fn spawn_cleanup_task(
    cache: Arc<CacheManager>,
    interval_hours: u64,
) -> tokio::task::JoinHandle<()> {
    use tokio::time::{Duration, interval};

    info!(
        "Starting background cache cleanup task (interval: {} hours)",
        interval_hours
    );

    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(interval_hours * 3600));

        // Skip the first tick (which fires immediately)
        ticker.tick().await;

        loop {
            ticker.tick().await;
            info!("Running scheduled cache maintenance");

            match cache.run_maintenance().await {
                Ok((expired, freed)) => {
                    if expired > 0 || freed > 0 {
                        info!(
                            "Scheduled maintenance: {} expired removed, {} bytes freed",
                            expired, freed
                        );
                    }
                }
                Err(e) => {
                    warn!("Error during scheduled maintenance: {}", e);
                }
            }
        }
    })
}
