//! Upload session operations

use chrono::Utc;
use sqlx::Row;

use crate::error::DbError;
use crate::models::{NewUploadSession, UploadSession};

use super::Database;

impl Database {
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

        Ok(result.map(|row| UploadSession {
            id: row.get("id"),
            repository: row.get("repository"),
            started_at: chrono::DateTime::parse_from_rfc3339(row.get("started_at"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            last_chunk_at: chrono::DateTime::parse_from_rfc3339(row.get("last_chunk_at"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            bytes_received: row.get("bytes_received"),
            temp_path: row.get("temp_path"),
        }))
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
}
