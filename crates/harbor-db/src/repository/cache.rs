//! Cache entry operations

use chrono::Utc;
use sqlx::Row;

use crate::error::DbError;
use crate::models::{CacheEntry, NewCacheEntry};
use crate::repository::Database;

impl Database {
    // ==================== Cache Entry Operations ====================

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
    pub async fn get_cache_entry_by_digest(
        &self,
        digest: &str,
    ) -> Result<Option<CacheEntry>, DbError> {
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

        result
            .map(|row| CacheEntry::try_from(&row).map_err(DbError::from))
            .transpose()
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

        rows.iter()
            .map(|row| CacheEntry::try_from(row).map_err(DbError::from))
            .collect()
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

        let manifest_count: i64 = sqlx::query(
            "SELECT COUNT(*) as count FROM cache_entries WHERE entry_type = 'manifest'",
        )
        .fetch_one(&self.pool)
        .await?
        .get("count");

        let blob_count: i64 =
            sqlx::query("SELECT COUNT(*) as count FROM cache_entries WHERE entry_type = 'blob'")
                .fetch_one(&self.pool)
                .await?
                .get("count");

        Ok(CacheStats {
            total_size,
            entry_count,
            manifest_count,
            blob_count,
            hit_count: 0,
            miss_count: 0,
        })
    }
}

/// Cache statistics
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheStats {
    pub total_size: i64,
    pub entry_count: i64,
    pub manifest_count: i64,
    pub blob_count: i64,
    /// In-memory hit count (not persisted to database)
    #[serde(default)]
    pub hit_count: i64,
    /// In-memory miss count (not persisted to database)
    #[serde(default)]
    pub miss_count: i64,
}

/// Allowed sort fields for cache entries (whitelist to prevent SQL injection)
const ALLOWED_CACHE_SORT_FIELDS: &[&str] =
    &["last_accessed_at", "created_at", "size", "access_count"];

/// Query parameters for listing cache entries
#[derive(Debug, Clone, Default)]
pub struct CacheEntryQuery {
    /// Filter by entry type
    pub entry_type: Option<String>,
    /// Filter by repository (partial match)
    pub repository: Option<String>,
    /// Filter by digest (partial match)
    pub digest: Option<String>,
    /// Pagination offset (must be non-negative)
    pub offset: i64,
    /// Pagination limit (must be positive)
    pub limit: i64,
    /// Sort field: "last_accessed_at", "created_at", "size", "access_count"
    pub sort_by: Option<String>,
    /// Sort direction: "asc" or "desc"
    pub sort_order: Option<String>,
}

impl CacheEntryQuery {
    /// Validates and normalizes the query parameters
    pub fn validated(mut self) -> Self {
        // Ensure offset is non-negative
        if self.offset < 0 {
            self.offset = 0;
        }
        // Ensure limit is positive and capped
        if self.limit <= 0 {
            self.limit = 50;
        } else if self.limit > 100 {
            self.limit = 100;
        }
        // Validate sort_by against whitelist
        if let Some(ref sort_by) = self.sort_by {
            if !ALLOWED_CACHE_SORT_FIELDS.contains(&sort_by.as_str()) {
                self.sort_by = None; // Reset to default
            }
        }
        // Validate sort_order
        if let Some(ref sort_order) = self.sort_order {
            let lower = sort_order.to_lowercase();
            if lower != "asc" && lower != "desc" {
                self.sort_order = None; // Reset to default
            }
        }
        self
    }
}

impl Database {
    /// List cache entries with filtering and pagination
    ///
    /// Note: Query parameters should be validated via CacheEntryQuery::validated()
    /// before calling this method to ensure security.
    pub async fn list_cache_entries(
        &self,
        query: CacheEntryQuery,
    ) -> Result<(Vec<CacheEntry>, i64), DbError> {
        // Apply validation to ensure safe parameters
        let query = query.validated();

        let mut conditions = Vec::new();
        let mut params: Vec<String> = Vec::new();

        if let Some(entry_type) = &query.entry_type {
            conditions.push("entry_type = ?");
            params.push(entry_type.clone());
        }
        if let Some(repository) = &query.repository {
            conditions.push("repository LIKE ?");
            params.push(format!("%{}%", repository));
        }
        if let Some(digest) = &query.digest {
            conditions.push("digest LIKE ?");
            params.push(format!("%{}%", digest));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        // Get total count
        let count_sql = format!(
            "SELECT COUNT(*) as count FROM cache_entries {}",
            where_clause
        );
        let mut count_query = sqlx::query(&count_sql);
        for param in &params {
            count_query = count_query.bind(param);
        }
        let count_row = count_query.fetch_one(&self.pool).await?;
        let total: i64 = count_row.get("count");

        // Determine sort order using validated whitelist values only
        // The validated() call above ensures sort_by is in ALLOWED_CACHE_SORT_FIELDS
        let sort_field = match query.sort_by.as_deref() {
            Some("created_at") => "created_at",
            Some("size") => "size",
            Some("access_count") => "access_count",
            Some("last_accessed_at") | None => "last_accessed_at",
            // This branch should never be reached after validation, but be defensive
            Some(_) => "last_accessed_at",
        };
        let sort_dir = match query.sort_order.as_deref() {
            Some(s) if s.eq_ignore_ascii_case("asc") => "ASC",
            _ => "DESC",
        };

        // Get entries
        let sql = format!(
            r#"
            SELECT id, entry_type, repository, reference, digest, content_type, size,
                   created_at, last_accessed_at, access_count, storage_path
            FROM cache_entries
            {}
            ORDER BY {} {}
            LIMIT ? OFFSET ?
            "#,
            where_clause, sort_field, sort_dir
        );

        let mut entries_query = sqlx::query(&sql);
        for param in &params {
            entries_query = entries_query.bind(param);
        }
        entries_query = entries_query.bind(query.limit).bind(query.offset);

        let rows = entries_query.fetch_all(&self.pool).await?;
        let entries: Result<Vec<CacheEntry>, _> = rows
            .iter()
            .map(|row| CacheEntry::try_from(row).map_err(DbError::from))
            .collect();

        Ok((entries?, total))
    }

    /// Get top accessed cache entries
    pub async fn get_top_accessed_entries(&self, limit: i64) -> Result<Vec<CacheEntry>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT id, entry_type, repository, reference, digest, content_type, size,
                   created_at, last_accessed_at, access_count, storage_path
            FROM cache_entries
            ORDER BY access_count DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| CacheEntry::try_from(row).map_err(DbError::from))
            .collect()
    }

    /// Get distinct repositories from cache entries
    ///
    /// Returns up to 1000 repositories to prevent unbounded queries.
    pub async fn get_cached_repositories(&self) -> Result<Vec<String>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT repository
            FROM cache_entries
            WHERE repository IS NOT NULL
            ORDER BY repository
            LIMIT 1000
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|row| row.get("repository")).collect())
    }
}
