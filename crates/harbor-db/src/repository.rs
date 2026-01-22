//! Database repository implementation

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tracing::info;

use crate::error::DbError;
use crate::models::*;

/// Database connection and operations
#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Create a new database connection
    pub async fn new(database_url: &str) -> Result<Self, DbError> {
        info!("Connecting to database: {}", database_url);

        let pool = SqlitePool::connect(database_url).await?;
        let db = Self { pool };
        db.run_migrations().await?;
        Ok(db)
    }

    /// Get the underlying pool for advanced usage
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Run database migrations
    async fn run_migrations(&self) -> Result<(), DbError> {
        info!("Running database migrations");

        // Create tables if they don't exist
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS cache_entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                entry_type TEXT NOT NULL,
                repository TEXT,
                reference TEXT,
                digest TEXT NOT NULL UNIQUE,
                content_type TEXT NOT NULL,
                size INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                last_accessed_at TEXT NOT NULL,
                access_count INTEGER DEFAULT 1,
                storage_path TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_cache_entries_digest ON cache_entries(digest)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_cache_entries_last_accessed ON cache_entries(last_accessed_at)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL UNIQUE,
                password_hash TEXT NOT NULL,
                role TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS upload_sessions (
                id TEXT PRIMARY KEY,
                repository TEXT NOT NULL,
                started_at TEXT NOT NULL,
                last_chunk_at TEXT NOT NULL,
                bytes_received INTEGER DEFAULT 0,
                temp_path TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        info!("Database migrations completed");
        Ok(())
    }

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

result.map(|row| CacheEntry::try_from(&row).map_err(DbError::from)).transpose()
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
            hit_count: 0,
            miss_count: 0,
        })
    }

    // ==================== User Operations ====================

    /// Insert a new user
    pub async fn insert_user(&self, user: NewUser) -> Result<User, DbError> {
        let now = Utc::now();

        // Check if user already exists
        let existing = self.get_user_by_username(&user.username).await?;
        if existing.is_some() {
            return Err(DbError::Duplicate(format!("User '{}' already exists", user.username)));
        }

        let result = sqlx::query(
            r#"
            INSERT INTO users (username, password_hash, role, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?)
            RETURNING id
            "#,
        )
        .bind(&user.username)
        .bind(&user.password_hash)
        .bind(user.role.as_str())
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .fetch_one(&self.pool)
        .await?;

        let id: i64 = result.get("id");

        Ok(User {
            id,
            username: user.username,
            password_hash: user.password_hash,
            role: user.role,
            created_at: now,
            updated_at: now,
        })
    }

    /// Get a user by username
    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<User>, DbError> {
        let result = sqlx::query(
            r#"
            SELECT id, username, password_hash, role, created_at, updated_at
            FROM users
            WHERE username = ?
            "#,
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;

result.map(|row| User::try_from(&row).map_err(DbError::from)).transpose()
    }

    /// Get a user by ID
    pub async fn get_user_by_id(&self, id: i64) -> Result<Option<User>, DbError> {
        let result = sqlx::query(
            r#"
            SELECT id, username, password_hash, role, created_at, updated_at
            FROM users
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        result.map(|row| User::try_from(&row).map_err(DbError::from)).transpose()
    }

    /// List all users
    pub async fn list_users(&self) -> Result<Vec<User>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT id, username, password_hash, role, created_at, updated_at
            FROM users
            ORDER BY username
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| User::try_from(row).map_err(DbError::from))
            .collect()
    }

    /// Update user role
    pub async fn update_user_role(&self, id: i64, role: UserRole) -> Result<bool, DbError> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            UPDATE users
            SET role = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(role.as_str())
        .bind(now.to_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Update user password
    pub async fn update_user_password(&self, id: i64, password_hash: &str) -> Result<bool, DbError> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            UPDATE users
            SET password_hash = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(password_hash)
        .bind(now.to_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Delete a user
    pub async fn delete_user(&self, id: i64) -> Result<bool, DbError> {
        let result = sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Check if any users exist
    pub async fn has_users(&self) -> Result<bool, DbError> {
        let result = sqlx::query("SELECT COUNT(*) as count FROM users")
            .fetch_one(&self.pool)
            .await?;
        let count: i64 = result.get("count");
        Ok(count > 0)
    }

    // ==================== Upload Session Operations ====================

    /// Create a new upload session
    pub async fn create_upload_session(&self, session: NewUploadSession) -> Result<UploadSession, DbError> {
        let now = Utc::now();
        sqlx::query(
            r#"
            INSERT INTO upload_sessions (id, repository, started_at, last_chunk_at, bytes_received, temp_path)
            VALUES (?, ?, ?, ?, 0, ?)
            "#,
        )
        .bind(&session.id)
        .bind(&session.repository)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(&session.temp_path)
        .execute(&self.pool)
        .await?;

        Ok(UploadSession {
            id: session.id,
            repository: session.repository,
            started_at: now,
            last_chunk_at: now,
            bytes_received: 0,
            temp_path: session.temp_path,
        })
    }

    /// Get an upload session by ID
    pub async fn get_upload_session(&self, id: &str) -> Result<Option<UploadSession>, DbError> {
        let result = sqlx::query(
            r#"
            SELECT id, repository, started_at, last_chunk_at, bytes_received, temp_path
            FROM upload_sessions
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        result.map(|row| UploadSession::try_from(&row).map_err(DbError::from)).transpose()
    }

    /// Update upload session bytes received
    pub async fn update_upload_session(&self, id: &str, bytes_received: i64) -> Result<bool, DbError> {
        let now = Utc::now();
        let result = sqlx::query(
            r#"
            UPDATE upload_sessions
            SET bytes_received = ?, last_chunk_at = ?
            WHERE id = ?
            "#,
        )
        .bind(bytes_received)
        .bind(now.to_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Delete an upload session
    pub async fn delete_upload_session(&self, id: &str) -> Result<bool, DbError> {
        let result = sqlx::query("DELETE FROM upload_sessions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ==================== Config Operations ====================

    /// Get a config value
    pub async fn get_config(&self, key: &str) -> Result<Option<String>, DbError> {
        let result = sqlx::query("SELECT value FROM config WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(result.map(|row| row.get("value")))
    }

    /// Set a config value
    pub async fn set_config(&self, key: &str, value: &str) -> Result<(), DbError> {
        let now = Utc::now();
        sqlx::query(
            r#"
            INSERT INTO config (key, value, updated_at)
            VALUES (?, ?, ?)
            ON CONFLICT(key) DO UPDATE SET value = ?, updated_at = ?
            "#,
        )
        .bind(key)
        .bind(value)
        .bind(now.to_rfc3339())
        .bind(value)
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// List all config values
    pub async fn list_config(&self) -> Result<Vec<ConfigEntry>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT key, value, updated_at
            FROM config
            ORDER BY key
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| ConfigEntry::try_from(row).map_err(DbError::from))
            .collect()
    }

    /// Delete a config value
    pub async fn delete_config(&self, key: &str) -> Result<bool, DbError> {
        let result = sqlx::query("DELETE FROM config WHERE key = ?")
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

/// Cache statistics
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
