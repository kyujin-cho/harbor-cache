//! Database models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
