//! Request and response types for management API

use serde::{Deserialize, Serialize};

// ==================== Auth Types ====================

/// Login request
#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Login response
#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires_in: i64,
}

// ==================== User Types ====================

/// Create user request
#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: String,
}

/// Update user request
#[derive(Deserialize)]
pub struct UpdateUserRequest {
    pub role: Option<String>,
    pub password: Option<String>,
}

/// User response (without password)
#[derive(Serialize)]
pub struct UserResponse {
    pub id: i64,
    pub username: String,
    pub role: String,
    pub created_at: String,
    pub updated_at: String,
}

// ==================== Cache Types ====================

/// Cache statistics response
#[derive(Serialize)]
pub struct CacheStatsResponse {
    pub total_size: u64,
    pub total_size_human: String,
    pub entry_count: u64,
    pub manifest_count: u64,
    pub blob_count: u64,
    pub hit_count: u64,
    pub miss_count: u64,
    pub hit_rate: f64,
}

// ==================== Config Types ====================

/// Config entry response
#[derive(Serialize)]
pub struct ConfigEntryResponse {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

/// Update config request
#[derive(Deserialize)]
pub struct UpdateConfigRequest {
    pub entries: Vec<ConfigUpdateEntry>,
}

/// Single config update entry
#[derive(Deserialize)]
pub struct ConfigUpdateEntry {
    pub key: String,
    pub value: String,
}
