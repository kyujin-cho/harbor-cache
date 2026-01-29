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
    /// Optional upstream ID for cache isolation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_id: Option<i64>,
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
    /// Optional upstream ID for cache isolation
    pub upstream_id: Option<i64>,
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

/// Cache isolation mode for upstreams
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CacheIsolation {
    /// Share cache across all upstreams (deduplicate by digest)
    Shared,
    /// Isolate cache per upstream
    Isolated,
}

impl Default for CacheIsolation {
    fn default() -> Self {
        CacheIsolation::Shared
    }
}

impl CacheIsolation {
    pub fn as_str(&self) -> &'static str {
        match self {
            CacheIsolation::Shared => "shared",
            CacheIsolation::Isolated => "isolated",
        }
    }
}

impl FromStr for CacheIsolation {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "shared" => Ok(CacheIsolation::Shared),
            "isolated" => Ok(CacheIsolation::Isolated),
            _ => Err(ParseError::InvalidEntryType(s.to_string())),
        }
    }
}

/// Upstream registry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Upstream {
    pub id: i64,
    /// Unique identifier for the upstream (used in API)
    pub name: String,
    /// Display name for UI
    pub display_name: String,
    /// URL of the upstream Harbor registry
    pub url: String,
    /// Registry/project name
    pub registry: String,
    /// Username for authentication
    #[serde(skip_serializing)]
    pub username: Option<String>,
    /// Password for authentication (never serialized)
    #[serde(skip_serializing)]
    pub password: Option<String>,
    /// Skip TLS certificate verification
    pub skip_tls_verify: bool,
    /// Priority for route matching (lower = higher priority)
    pub priority: i32,
    /// Whether this upstream is enabled
    pub enabled: bool,
    /// Cache isolation mode
    pub cache_isolation: CacheIsolation,
    /// Whether this is the default upstream (fallback)
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// New upstream (for insertion)
#[derive(Debug, Clone)]
pub struct NewUpstream {
    pub name: String,
    pub display_name: String,
    pub url: String,
    pub registry: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub skip_tls_verify: bool,
    pub priority: i32,
    pub enabled: bool,
    pub cache_isolation: CacheIsolation,
    pub is_default: bool,
}

/// Update upstream (for partial updates)
#[derive(Debug, Clone, Default)]
pub struct UpdateUpstream {
    pub display_name: Option<String>,
    pub url: Option<String>,
    pub registry: Option<String>,
    pub username: Option<Option<String>>,
    pub password: Option<Option<String>>,
    pub skip_tls_verify: Option<bool>,
    pub priority: Option<i32>,
    pub enabled: Option<bool>,
    pub cache_isolation: Option<CacheIsolation>,
    pub is_default: Option<bool>,
}

/// Upstream route pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamRoute {
    pub id: i64,
    pub upstream_id: i64,
    /// Pattern to match repository paths (supports glob patterns)
    pub pattern: String,
    /// Priority for this route (lower = higher priority)
    pub priority: i32,
    pub created_at: DateTime<Utc>,
}

/// New upstream route (for insertion)
#[derive(Debug, Clone)]
pub struct NewUpstreamRoute {
    pub upstream_id: i64,
    pub pattern: String,
    pub priority: i32,
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
            upstream_id: row.try_get("upstream_id").ok(),
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

impl TryFrom<&sqlx::sqlite::SqliteRow> for Upstream {
    type Error = sqlx::Error;

    fn try_from(row: &sqlx::sqlite::SqliteRow) -> Result<Self, Self::Error> {
        let cache_isolation_str: String = row.try_get("cache_isolation")?;
        Ok(Upstream {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            display_name: row.try_get("display_name")?,
            url: row.try_get("url")?,
            registry: row.try_get("registry")?,
            username: row.try_get("username")?,
            password: row.try_get("password")?,
            skip_tls_verify: row.try_get("skip_tls_verify")?,
            priority: row.try_get("priority")?,
            enabled: row.try_get("enabled")?,
            cache_isolation: CacheIsolation::from_str(&cache_isolation_str)
                .unwrap_or(CacheIsolation::Shared),
            is_default: row.try_get("is_default")?,
            created_at: parse_datetime_or_now(&row.try_get::<String, _>("created_at")?),
            updated_at: parse_datetime_or_now(&row.try_get::<String, _>("updated_at")?),
        })
    }
}

impl TryFrom<&sqlx::sqlite::SqliteRow> for UpstreamRoute {
    type Error = sqlx::Error;

    fn try_from(row: &sqlx::sqlite::SqliteRow) -> Result<Self, Self::Error> {
        Ok(UpstreamRoute {
            id: row.try_get("id")?,
            upstream_id: row.try_get("upstream_id")?,
            pattern: row.try_get("pattern")?,
            priority: row.try_get("priority")?,
            created_at: parse_datetime_or_now(&row.try_get::<String, _>("created_at")?),
        })
    }
}
