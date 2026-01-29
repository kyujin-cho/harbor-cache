//! Database repository implementation

use sqlx::{Row, SqlitePool};
use tracing::info;

use crate::error::DbError;

// Submodules
mod activity_logs;
mod cache;
mod config;
mod sessions;
mod upstreams;
mod users;

// Re-export CacheStats and CacheEntryQuery
pub use activity_logs::ActivityLogQuery;
pub use cache::{CacheEntryQuery, CacheStats};

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

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS activity_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                action TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                resource_id TEXT,
                user_id INTEGER,
                username TEXT,
                details TEXT,
                ip_address TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_activity_logs_timestamp ON activity_logs(timestamp)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_activity_logs_action ON activity_logs(action)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_activity_logs_user_id ON activity_logs(user_id)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_activity_logs_resource_type ON activity_logs(resource_type)
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create upstreams table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS upstreams (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                url TEXT NOT NULL,
                registry TEXT NOT NULL,
                username TEXT,
                password TEXT,
                skip_tls_verify INTEGER DEFAULT 0,
                priority INTEGER DEFAULT 100,
                enabled INTEGER DEFAULT 1,
                cache_isolation TEXT DEFAULT 'shared',
                is_default INTEGER DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_upstreams_name ON upstreams(name)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_upstreams_priority ON upstreams(priority)
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create upstream routes table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS upstream_routes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                upstream_id INTEGER NOT NULL,
                pattern TEXT NOT NULL,
                priority INTEGER DEFAULT 100,
                created_at TEXT NOT NULL,
                FOREIGN KEY (upstream_id) REFERENCES upstreams(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_upstream_routes_upstream_id ON upstream_routes(upstream_id)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_upstream_routes_priority ON upstream_routes(priority)
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Add upstream_id to cache_entries for isolated caching (optional column)
        // Check if column exists first
        let column_exists: bool = sqlx::query(
            "SELECT COUNT(*) as count FROM pragma_table_info('cache_entries') WHERE name = 'upstream_id'"
        )
        .fetch_one(&self.pool)
        .await
        .map(|row| row.get::<i64, _>("count") > 0)
        .unwrap_or(false);

        if !column_exists {
            sqlx::query("ALTER TABLE cache_entries ADD COLUMN upstream_id INTEGER")
                .execute(&self.pool)
                .await?;

            sqlx::query(
                r#"
                CREATE INDEX IF NOT EXISTS idx_cache_entries_upstream_id ON cache_entries(upstream_id)
                "#,
            )
            .execute(&self.pool)
            .await?;
        }

        info!("Database migrations completed");
        Ok(())
    }
}
