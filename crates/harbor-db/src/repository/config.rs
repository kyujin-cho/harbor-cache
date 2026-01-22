//! Configuration operations

use chrono::Utc;
use sqlx::Row;

use crate::error::DbError;
use crate::models::ConfigEntry;

use super::Database;

impl Database {
    /// Get a config value
    pub async fn get_config(&self, key: &str) -> Result<Option<String>, DbError> {
        let result = sqlx::query("SELECT value FROM config WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(result.map(|row| row.get("value")))
    }

    /// Get a config entry with metadata
    pub async fn get_config_entry(&self, key: &str) -> Result<Option<ConfigEntry>, DbError> {
        let result = sqlx::query("SELECT key, value, updated_at FROM config WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;

        Ok(result.map(|row| ConfigEntry {
            key: row.get("key"),
            value: row.get("value"),
            updated_at: chrono::DateTime::parse_from_rfc3339(row.get("updated_at"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        }))
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

        Ok(rows
            .into_iter()
            .map(|row| ConfigEntry {
                key: row.get("key"),
                value: row.get("value"),
                updated_at: chrono::DateTime::parse_from_rfc3339(row.get("updated_at"))
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
            .collect())
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
