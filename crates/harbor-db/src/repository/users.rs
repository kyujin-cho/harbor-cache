//! User operations

use chrono::Utc;
use sqlx::Row;

use crate::error::DbError;
use crate::models::{NewUser, User, UserRole};

use super::Database;

impl Database {
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

        Ok(result.map(|row| User {
            id: row.get("id"),
            username: row.get("username"),
            password_hash: row.get("password_hash"),
            role: UserRole::from_str(row.get("role")).unwrap_or(UserRole::ReadOnly),
            created_at: chrono::DateTime::parse_from_rfc3339(row.get("created_at"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(row.get("updated_at"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        }))
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

        Ok(result.map(|row| User {
            id: row.get("id"),
            username: row.get("username"),
            password_hash: row.get("password_hash"),
            role: UserRole::from_str(row.get("role")).unwrap_or(UserRole::ReadOnly),
            created_at: chrono::DateTime::parse_from_rfc3339(row.get("created_at"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(row.get("updated_at"))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        }))
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

        Ok(rows
            .into_iter()
            .map(|row| User {
                id: row.get("id"),
                username: row.get("username"),
                password_hash: row.get("password_hash"),
                role: UserRole::from_str(row.get("role")).unwrap_or(UserRole::ReadOnly),
                created_at: chrono::DateTime::parse_from_rfc3339(row.get("created_at"))
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: chrono::DateTime::parse_from_rfc3339(row.get("updated_at"))
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })
            .collect())
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
}
