//! Request/Response DTOs for management API

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
    pub total_size: i64,
    pub total_size_human: String,
    pub entry_count: i64,
    pub manifest_count: i64,
    pub blob_count: i64,
    pub hit_count: i64,
    pub miss_count: i64,
    pub hit_rate: f64,
}

/// Cache entry response
#[derive(Serialize)]
pub struct CacheEntryResponse {
    pub id: i64,
    pub entry_type: String,
    pub repository: Option<String>,
    pub reference: Option<String>,
    pub digest: String,
    pub content_type: String,
    pub size: i64,
    pub size_human: String,
    pub created_at: String,
    pub last_accessed_at: String,
    pub access_count: i64,
}

/// Paginated cache entries response
#[derive(Serialize)]
pub struct CacheEntriesListResponse {
    pub entries: Vec<CacheEntryResponse>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

/// Cache entries query parameters
#[derive(Deserialize, Default)]
pub struct CacheEntriesQuery {
    #[serde(default)]
    pub entry_type: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default = "default_offset")]
    pub offset: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub sort_by: Option<String>,
    #[serde(default)]
    pub sort_order: Option<String>,
}

fn default_offset() -> i64 {
    0
}

fn default_limit() -> i64 {
    50
}

/// List of cached repositories
#[derive(Serialize)]
pub struct CachedRepositoriesResponse {
    pub repositories: Vec<String>,
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

/// Configuration schema field
#[derive(Serialize, Clone)]
pub struct ConfigSchemaField {
    pub key: String,
    pub label: String,
    pub description: String,
    pub field_type: String,
    pub default_value: Option<String>,
    pub required: bool,
    pub options: Option<Vec<ConfigOption>>,
    pub group: String,
}

/// Configuration option for select fields
#[derive(Serialize, Clone)]
pub struct ConfigOption {
    pub value: String,
    pub label: String,
}

/// Configuration schema response
#[derive(Serialize)]
pub struct ConfigSchemaResponse {
    pub fields: Vec<ConfigSchemaField>,
    pub groups: Vec<ConfigGroup>,
}

/// Configuration group
#[derive(Serialize, Clone)]
pub struct ConfigGroup {
    pub id: String,
    pub label: String,
    pub description: String,
}

/// Full configuration response (TOML format)
#[derive(Serialize)]
pub struct ConfigFileResponse {
    pub content: String,
    pub format: String,
}

/// Update configuration file request
#[derive(Deserialize)]
pub struct UpdateConfigFileRequest {
    pub content: String,
}

// ==================== Activity Log Types ====================

/// Activity log entry response
#[derive(Serialize)]
pub struct ActivityLogResponse {
    pub id: i64,
    pub timestamp: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub user_id: Option<i64>,
    pub username: Option<String>,
    pub details: Option<String>,
    pub ip_address: Option<String>,
}

/// Paginated activity logs response
#[derive(Serialize)]
pub struct ActivityLogsListResponse {
    pub logs: Vec<ActivityLogResponse>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

/// Activity logs query parameters
#[derive(Deserialize, Default)]
pub struct ActivityLogsQuery {
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub resource_type: Option<String>,
    #[serde(default)]
    pub user_id: Option<i64>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default = "default_offset")]
    pub offset: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

// ==================== Upstream Types ====================

/// Project configuration response
#[derive(Serialize, Clone)]
pub struct UpstreamProjectResponse {
    /// Project/registry name in Harbor (e.g., "library", "team-a")
    pub name: String,
    /// Pattern to match repository paths for this project
    pub pattern: Option<String>,
    /// Effective pattern (computed if pattern is None)
    pub effective_pattern: String,
    /// Priority for this project route (lower = higher priority)
    pub priority: i32,
    /// Whether this is the default project for this upstream
    pub is_default: bool,
}

/// Upstream response
#[derive(Serialize)]
pub struct UpstreamResponse {
    pub id: i64,
    pub name: String,
    pub display_name: String,
    pub url: String,
    /// Registry/project name (legacy single-project mode)
    pub registry: String,
    /// Multiple projects configuration (multi-project mode)
    pub projects: Vec<UpstreamProjectResponse>,
    /// Whether this upstream uses multi-project mode
    pub uses_multi_project: bool,
    pub skip_tls_verify: bool,
    pub priority: i32,
    pub enabled: bool,
    pub cache_isolation: String,
    pub is_default: bool,
    pub has_credentials: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Create upstream request
#[derive(Deserialize)]
pub struct CreateUpstreamRequest {
    pub name: String,
    pub display_name: String,
    pub url: String,
    pub registry: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub skip_tls_verify: bool,
    #[serde(default = "default_priority")]
    pub priority: i32,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_cache_isolation")]
    pub cache_isolation: String,
    #[serde(default)]
    pub is_default: bool,
    /// Route patterns for this upstream
    #[serde(default)]
    pub routes: Vec<CreateRouteRequest>,
}

fn default_priority() -> i32 {
    100
}

fn default_enabled() -> bool {
    true
}

fn default_cache_isolation() -> String {
    "shared".to_string()
}

/// Create route request
#[derive(Deserialize)]
pub struct CreateRouteRequest {
    pub pattern: String,
    #[serde(default = "default_priority")]
    pub priority: i32,
}

/// Update upstream request
#[derive(Deserialize)]
pub struct UpdateUpstreamRequest {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub registry: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub skip_tls_verify: Option<bool>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub cache_isolation: Option<String>,
    #[serde(default)]
    pub is_default: Option<bool>,
}

/// Upstream health response
#[derive(Serialize)]
pub struct UpstreamHealthResponse {
    pub upstream_id: i64,
    pub name: String,
    pub healthy: bool,
    pub last_check: String,
    pub last_error: Option<String>,
    pub consecutive_failures: u32,
}

/// Upstream route response
#[derive(Serialize)]
pub struct UpstreamRouteResponse {
    pub id: i64,
    pub upstream_id: i64,
    pub pattern: String,
    pub priority: i32,
    pub created_at: String,
}

/// Test upstream connection request
#[derive(Deserialize)]
pub struct TestUpstreamRequest {
    pub url: String,
    pub registry: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub skip_tls_verify: bool,
}

/// Test upstream connection response
#[derive(Serialize)]
pub struct TestUpstreamResponse {
    pub success: bool,
    pub message: String,
}
