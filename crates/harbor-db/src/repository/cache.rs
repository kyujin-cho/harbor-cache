//! Cache entry operations

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::error::DbError;
use crate::models::{CacheEntry, EntryType, NewCacheEntry};

use super::Database;

impl Database {
    /// Insert a new cache entry
    pub async fn insert_cache_entry(&self, entry: NewCacheEntry) -> Result<CacheEntry, DbError> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            INSERT INTO cache_entries (entry_type, repository, reference, digest, content_type, size, created_at, last_accessed_at, access_count, storage_path)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?)
            RETURNING id
            "#,
        )
        .bind(entry.entry_type.as_str())
        .bind(&entry.repository)
        .bind(&entry.reference)
        .bind(&entry.digest)
        .bind(&entry.content_type)
        .bind(entry.size)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(&entry.storage_path)
        .fetch_one(&self.pool)
        .await?;

        let id: i64 = result.get("id");

        Ok(CacheEntry {
            id,
            entry_type: entry.entry_type,
            repository: entry.repository,
            reference: entry.reference,
            digest: entry.digest,
            content_type: entry.content_type,
            size: entry.size,
            created_at: now,
            last_accessed_at: now,
            access_count: 1,
            storage_path: entry.storage_path,
        })
    }

    /// Get a cache entry by digest
    pub async fn get_cache_entry_by_digest(&self, digest: &str) -> Result<Option<CacheEntry>, DbError> {
        let result = sqlx::query(
            r#"
            SELECT id, entry_type, repository, reference, digest, content_type, size, created_at, last_accessed_at, access_count, storage_path
            FROM cache_entries
            WHERE digest = ?
            "#,
        )
        .bind(digest)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(|row| CacheEntry {
            id: row.get("id"),
            entry_type: EntryType::from_str(row.get("entry_type")).unwrap_or(EntryType::Blob),
            repository: row.get("repository"),
            reference: row.get("reference"),
            digest: row.get("digest"),
            content_type: row.get("content_type"),
            size: row.get("size"),
            created_at: chrono::DateTime::parse_from_rfc3339(row.get("created_at"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            last_accessed_at: chrono::DateTime::parse_from_rfc3339(row.get("last_accessed_at"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            access_count: row.get("access_count"),
            storage_path: row.get("storage_path"),
        }))
    }

    /// Update last accessed time and increment access count
    pub async fn touch_cache_entry(&self, digest: &str) -> Result<(), DbError> {
        let now = Utc::now();
        sqlx::query(
            r#"
            UPDATE cache_entries
            SET last_accessed_at = ?, access_count = access_count + 1
            WHERE digest = ?
            "#,
        )
        .bind(now.to_rfc3339())
        .bind(digest)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Delete a cache entry by digest
    pub async fn delete_cache_entry(&self, digest: &str) -> Result<bool, DbError> {
        let result = sqlx::query("DELETE FROM cache_entries WHERE digest = ?")
            .bind(digest)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Get all cache entries sorted by last accessed time (oldest first) for LRU eviction
    pub async fn get_cache_entries_lru(&self, limit: i64) -> Result<Vec<CacheEntry>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT id, entry_type, repository, reference, digest, content_type, size, created_at, last_accessed_at, access_count, storage_path
            FROM cache_entries
            ORDER BY last_accessed_at ASC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| CacheEntry {
                id: row.get("id"),
                entry_type: EntryType::from_str(row.get("entry_type")).unwrap_or(EntryType::Blob),
                repository: row.get("repository"),
                reference: row.get("reference"),
                digest: row.get("digest"),
                content_type: row.get("content_type"),
                size: row.get("size"),
                created_at: chrono::DateTime::parse_from_rfc3339(row.get("created_at"))
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                last_accessed_at: chrono::DateTime::parse_from_rfc3339(row.get("last_accessed_at"))
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                access_count: row.get("access_count"),
                storage_path: row.get("storage_path"),
            })
            .collect())
    }

    /// Get total cache size
    pub async fn get_total_cache_size(&self) -> Result<i64, DbError> {
        let result = sqlx::query("SELECT COALESCE(SUM(size), 0) as total FROM cache_entries")
            .fetch_one(&self.pool)
            .await?;
        Ok(result.get("total"))
    }

    /// Get cache entry count
    pub async fn get_cache_entry_count(&self) -> Result<i64, DbError> {
        let result = sqlx::query("SELECT COUNT(*) as count FROM cache_entries")
            .fetch_one(&self.pool)
            .await?;
        Ok(result.get("count"))
    }

    /// Get cache statistics
    pub async fn get_cache_stats(&self) -> Result<CacheStats, DbError> {
        let total_size = self.get_total_cache_size().await?;
        let entry_count = self.get_cache_entry_count().await?;

        let manifest_count: i64 = sqlx::query("SELECT COUNT(*) as count FROM cache_entries WHERE entry_type = 'manifest'")
            .fetch_one(&self.pool)
            .await?
            .get("count");

        let blob_count: i64 = sqlx::query("SELECT COUNT(*) as count FROM cache_entries WHERE entry_type = 'blob'")
            .fetch_one(&self.pool)
            .await?
            .get("count");

        Ok(CacheStats {
            total_size,
            entry_count,
            manifest_count,
            blob_count,
        })
    }
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_size: i64,
    pub entry_count: i64,
    pub manifest_count: i64,
    pub blob_count: i64,
}
