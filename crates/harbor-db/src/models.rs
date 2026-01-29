//! Database models

use crate::utils::parse_datetime_or_now;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::fmt;
use std::str::FromStr;

/// Error type for parsing models from strings
#[derive(Debug, Clone)]
pub enum ParseError {
    InvalidEntryType(String),
    InvalidUserRole(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::InvalidEntryType(s) => write!(f, "Invalid entry type: {}", s),
            ParseError::InvalidUserRole(s) => write!(f, "Invalid user role: {}", s),
        }
    }
}

impl std::error::Error for ParseError {}

/// Cache entry type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EntryType {
    Manifest,
    Blob,
}

impl EntryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntryType::Manifest => "manifest",
            EntryType::Blob => "blob",
        }
    }
}

impl FromStr for EntryType {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "manifest" => Ok(EntryType::Manifest),
            "blob" => Ok(EntryType::Blob),
            _ => Err(ParseError::InvalidEntryType(s.to_string())),
        }
    }
}

/// Cache entry model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub id: i64,
    pub entry_type: EntryType,
    pub repository: Option<String>,
    pub reference: Option<String>,
    pub digest: String,
    pub content_type: String,
    pub size: i64,
    pub created_at: DateTime<Utc>,
    pub last_accessed_at: DateTime<Utc>,
    pub access_count: i64,
    pub storage_path: String,
}

/// User role
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum UserRole {
    Admin,
    ReadWrite,
    ReadOnly,
}

impl UserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserRole::Admin => "admin",
            UserRole::ReadWrite => "read-write",
            UserRole::ReadOnly => "read-only",
        }
    }

    pub fn can_write(&self) -> bool {
        matches!(self, UserRole::Admin | UserRole::ReadWrite)
    }

    pub fn is_admin(&self) -> bool {
        matches!(self, UserRole::Admin)
    }
}

impl FromStr for UserRole {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "admin" => Ok(UserRole::Admin),
            "read-write" => Ok(UserRole::ReadWrite),
            "read-only" => Ok(UserRole::ReadOnly),
            _ => Err(ParseError::InvalidUserRole(s.to_string())),
        }
    }
}

/// User model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Configuration entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    pub key: String,
    pub value: String,
    pub updated_at: DateTime<Utc>,
}

/// Upload session model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadSession {
    pub id: String,
    pub repository: String,
    pub started_at: DateTime<Utc>,
    pub last_chunk_at: DateTime<Utc>,
    pub bytes_received: i64,
    pub temp_path: String,
}

/// New cache entry (for insertion)
#[derive(Debug, Clone)]
pub struct NewCacheEntry {
    pub entry_type: EntryType,
    pub repository: Option<String>,
    pub reference: Option<String>,
    pub digest: String,
    pub content_type: String,
    pub size: i64,
    pub storage_path: String,
}

/// New user (for insertion)
#[derive(Debug, Clone)]
pub struct NewUser {
    pub username: String,
    pub password_hash: String,
    pub role: UserRole,
}

/// New upload session (for insertion)
#[derive(Debug, Clone)]
pub struct NewUploadSession {
    pub id: String,
    pub repository: String,
    pub temp_path: String,
}

/// Activity log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityLog {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub user_id: Option<i64>,
    pub username: Option<String>,
    pub details: Option<String>,
    pub ip_address: Option<String>,
}

/// New activity log entry (for insertion)
#[derive(Debug, Clone)]
pub struct NewActivityLog {
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub user_id: Option<i64>,
    pub username: Option<String>,
    pub details: Option<String>,
    pub ip_address: Option<String>,
}

// ==================== TryFrom Implementations ====================

impl TryFrom<&sqlx::sqlite::SqliteRow> for CacheEntry {
    type Error = sqlx::Error;

    fn try_from(row: &sqlx::sqlite::SqliteRow) -> Result<Self, Self::Error> {
        let entry_type_str: String = row.try_get("entry_type")?;
        Ok(CacheEntry {
            id: row.try_get("id")?,
            entry_type: EntryType::from_str(&entry_type_str).unwrap_or(EntryType::Blob),
            repository: row.try_get("repository")?,
            reference: row.try_get("reference")?,
            digest: row.try_get("digest")?,
            content_type: row.try_get("content_type")?,
            size: row.try_get("size")?,
            created_at: parse_datetime_or_now(&row.try_get::<String, _>("created_at")?),
            last_accessed_at: parse_datetime_or_now(&row.try_get::<String, _>("last_accessed_at")?),
            access_count: row.try_get("access_count")?,
            storage_path: row.try_get("storage_path")?,
        })
    }
}

impl TryFrom<&sqlx::sqlite::SqliteRow> for User {
    type Error = sqlx::Error;

    fn try_from(row: &sqlx::sqlite::SqliteRow) -> Result<Self, Self::Error> {
        let role_str: String = row.try_get("role")?;
        Ok(User {
            id: row.try_get("id")?,
            username: row.try_get("username")?,
            password_hash: row.try_get("password_hash")?,
            role: UserRole::from_str(&role_str).unwrap_or(UserRole::ReadOnly),
            created_at: parse_datetime_or_now(&row.try_get::<String, _>("created_at")?),
            updated_at: parse_datetime_or_now(&row.try_get::<String, _>("updated_at")?),
        })
    }
}

impl TryFrom<&sqlx::sqlite::SqliteRow> for UploadSession {
    type Error = sqlx::Error;

    fn try_from(row: &sqlx::sqlite::SqliteRow) -> Result<Self, Self::Error> {
        Ok(UploadSession {
            id: row.try_get("id")?,
            repository: row.try_get("repository")?,
            started_at: parse_datetime_or_now(&row.try_get::<String, _>("started_at")?),
            last_chunk_at: parse_datetime_or_now(&row.try_get::<String, _>("last_chunk_at")?),
            bytes_received: row.try_get("bytes_received")?,
            temp_path: row.try_get("temp_path")?,
        })
    }
}

impl TryFrom<&sqlx::sqlite::SqliteRow> for ConfigEntry {
    type Error = sqlx::Error;

    fn try_from(row: &sqlx::sqlite::SqliteRow) -> Result<Self, Self::Error> {
        Ok(ConfigEntry {
            key: row.try_get("key")?,
            value: row.try_get("value")?,
            updated_at: parse_datetime_or_now(&row.try_get::<String, _>("updated_at")?),
        })
    }
}

impl TryFrom<&sqlx::sqlite::SqliteRow> for ActivityLog {
    type Error = sqlx::Error;

    fn try_from(row: &sqlx::sqlite::SqliteRow) -> Result<Self, Self::Error> {
        Ok(ActivityLog {
            id: row.try_get("id")?,
            timestamp: parse_datetime_or_now(&row.try_get::<String, _>("timestamp")?),
            action: row.try_get("action")?,
            resource_type: row.try_get("resource_type")?,
            resource_id: row.try_get("resource_id")?,
            user_id: row.try_get("user_id")?,
            username: row.try_get("username")?,
            details: row.try_get("details")?,
            ip_address: row.try_get("ip_address")?,
        })
    }
}
