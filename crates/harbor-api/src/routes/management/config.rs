//! Configuration management routes

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
};
use std::path::Path as StdPath;
use tracing::{debug, info, warn};

use crate::error::ApiError;
use crate::state::AppState;

use super::auth::RequireAdmin;
use super::types::{
    ConfigEntryResponse, ConfigFileResponse, ConfigGroup, ConfigOption, ConfigSchemaField,
    ConfigSchemaResponse, UpdateConfigFileRequest, UpdateConfigRequest,
};

/// Maximum allowed size for config file content (1 MB)
const MAX_CONFIG_CONTENT_SIZE: usize = 1024 * 1024;

/// Validates that a config file path is safe to access.
///
/// Returns Ok(canonical_path) if safe, or Err with an ApiError if unsafe.
fn validate_config_path(path: &str) -> Result<std::path::PathBuf, ApiError> {
    let path_obj = StdPath::new(path);

    // Check that the path is not empty
    if path.is_empty() {
        return Err(ApiError::BadRequest("Config path is empty".to_string()));
    }

    // Check for path traversal attempts
    if path.contains("..") {
        warn!("Path traversal attempt detected in config path: {}", path);
        return Err(ApiError::BadRequest("Invalid config path".to_string()));
    }

    // Check that the file has a .toml extension
    if path_obj.extension().and_then(|e| e.to_str()) != Some("toml") {
        return Err(ApiError::BadRequest(
            "Config file must have .toml extension".to_string(),
        ));
    }

    // Try to canonicalize the parent directory (it must exist)
    if let Some(parent) = path_obj.parent() {
        if !parent.as_os_str().is_empty() {
            match parent.canonicalize() {
                Ok(_) => {}
                Err(_) => {
                    return Err(ApiError::BadRequest(
                        "Config file parent directory does not exist".to_string(),
                    ));
                }
            }
        }
    }

    Ok(path_obj.to_path_buf())
}

/// Validates the semantic content of a TOML configuration.
///
/// This performs basic validation of known configuration fields to prevent
/// obviously invalid values from being saved.
fn validate_config_semantics(content: &toml::Value) -> Result<(), String> {
    // Validate server.port if present
    if let Some(server) = content.get("server") {
        if let Some(port) = server.get("port") {
            if let Some(port_num) = port.as_integer() {
                if port_num < 1 || port_num > 65535 {
                    return Err(format!(
                        "server.port must be between 1 and 65535, got {}",
                        port_num
                    ));
                }
            }
        }
    }

    // Validate cache.max_size if present
    if let Some(cache) = content.get("cache") {
        if let Some(max_size) = cache.get("max_size") {
            if let Some(size) = max_size.as_integer() {
                if size < 0 {
                    return Err("cache.max_size must be non-negative".to_string());
                }
                // Cap at 100 TB to prevent overflow issues
                if size > 100 * 1024 * 1024 * 1024 * 1024_i64 {
                    return Err("cache.max_size exceeds maximum allowed value".to_string());
                }
            }
        }
        if let Some(retention_days) = cache.get("retention_days") {
            if let Some(days) = retention_days.as_integer() {
                if days < 1 {
                    return Err("cache.retention_days must be at least 1".to_string());
                }
                if days > 3650 {
                    return Err("cache.retention_days cannot exceed 3650 (10 years)".to_string());
                }
            }
        }
        if let Some(eviction_policy) = cache.get("eviction_policy") {
            if let Some(policy) = eviction_policy.as_str() {
                let valid_policies = ["lru", "lfu", "fifo"];
                if !valid_policies.contains(&policy) {
                    return Err(format!(
                        "cache.eviction_policy must be one of {:?}, got '{}'",
                        valid_policies, policy
                    ));
                }
            }
        }
    }

    // Validate logging.level if present
    if let Some(logging) = content.get("logging") {
        if let Some(level) = logging.get("level") {
            if let Some(level_str) = level.as_str() {
                let valid_levels = ["trace", "debug", "info", "warn", "error"];
                if !valid_levels.contains(&level_str) {
                    return Err(format!(
                        "logging.level must be one of {:?}, got '{}'",
                        valid_levels, level_str
                    ));
                }
            }
        }
        if let Some(format) = logging.get("format") {
            if let Some(format_str) = format.as_str() {
                let valid_formats = ["pretty", "json"];
                if !valid_formats.contains(&format_str) {
                    return Err(format!(
                        "logging.format must be one of {:?}, got '{}'",
                        valid_formats, format_str
                    ));
                }
            }
        }
    }

    // Validate storage.backend if present
    if let Some(storage) = content.get("storage") {
        if let Some(backend) = storage.get("backend") {
            if let Some(backend_str) = backend.as_str() {
                let valid_backends = ["local", "s3"];
                if !valid_backends.contains(&backend_str) {
                    return Err(format!(
                        "storage.backend must be one of {:?}, got '{}'",
                        valid_backends, backend_str
                    ));
                }
            }
        }
    }

    Ok(())
}

// ==================== Config Schema Definition ====================

fn build_config_schema() -> ConfigSchemaResponse {
    let groups = vec![
        ConfigGroup {
            id: "server".to_string(),
            label: "Server".to_string(),
            description: "Server bind address and port settings".to_string(),
        },
        ConfigGroup {
            id: "cache".to_string(),
            label: "Cache".to_string(),
            description: "Cache storage limits and eviction policies".to_string(),
        },
        ConfigGroup {
            id: "upstream".to_string(),
            label: "Upstream".to_string(),
            description: "Upstream Harbor registry connection settings".to_string(),
        },
        ConfigGroup {
            id: "storage".to_string(),
            label: "Storage".to_string(),
            description: "Storage backend configuration (local or S3)".to_string(),
        },
        ConfigGroup {
            id: "database".to_string(),
            label: "Database".to_string(),
            description: "SQLite database settings".to_string(),
        },
        ConfigGroup {
            id: "auth".to_string(),
            label: "Authentication".to_string(),
            description: "Authentication and authorization settings".to_string(),
        },
        ConfigGroup {
            id: "logging".to_string(),
            label: "Logging".to_string(),
            description: "Logging level and format settings".to_string(),
        },
        ConfigGroup {
            id: "tls".to_string(),
            label: "TLS".to_string(),
            description: "TLS/HTTPS configuration".to_string(),
        },
    ];

    let fields = vec![
        // Server
        ConfigSchemaField {
            key: "server.bind_address".to_string(),
            label: "Bind Address".to_string(),
            description: "IP address to bind the server to".to_string(),
            field_type: "string".to_string(),
            default_value: Some("0.0.0.0".to_string()),
            required: true,
            options: None,
            group: "server".to_string(),
        },
        ConfigSchemaField {
            key: "server.port".to_string(),
            label: "Port".to_string(),
            description: "Port number to listen on".to_string(),
            field_type: "number".to_string(),
            default_value: Some("5001".to_string()),
            required: true,
            options: None,
            group: "server".to_string(),
        },
        // Cache
        ConfigSchemaField {
            key: "cache.max_size".to_string(),
            label: "Maximum Size (bytes)".to_string(),
            description: "Maximum cache size in bytes".to_string(),
            field_type: "number".to_string(),
            default_value: Some("10737418240".to_string()),
            required: true,
            options: None,
            group: "cache".to_string(),
        },
        ConfigSchemaField {
            key: "cache.retention_days".to_string(),
            label: "Retention Days".to_string(),
            description: "Number of days to retain cached artifacts".to_string(),
            field_type: "number".to_string(),
            default_value: Some("30".to_string()),
            required: true,
            options: None,
            group: "cache".to_string(),
        },
        ConfigSchemaField {
            key: "cache.eviction_policy".to_string(),
            label: "Eviction Policy".to_string(),
            description: "Cache eviction policy when limit is reached".to_string(),
            field_type: "select".to_string(),
            default_value: Some("lru".to_string()),
            required: true,
            options: Some(vec![
                ConfigOption {
                    value: "lru".to_string(),
                    label: "LRU (Least Recently Used)".to_string(),
                },
                ConfigOption {
                    value: "lfu".to_string(),
                    label: "LFU (Least Frequently Used)".to_string(),
                },
                ConfigOption {
                    value: "fifo".to_string(),
                    label: "FIFO (First In First Out)".to_string(),
                },
            ]),
            group: "cache".to_string(),
        },
        // Upstream
        ConfigSchemaField {
            key: "upstream.url".to_string(),
            label: "URL".to_string(),
            description: "URL of the upstream Harbor registry".to_string(),
            field_type: "string".to_string(),
            default_value: Some("http://localhost:8880".to_string()),
            required: true,
            options: None,
            group: "upstream".to_string(),
        },
        ConfigSchemaField {
            key: "upstream.registry".to_string(),
            label: "Registry Name".to_string(),
            description: "Registry/project name to proxy".to_string(),
            field_type: "string".to_string(),
            default_value: Some("library".to_string()),
            required: true,
            options: None,
            group: "upstream".to_string(),
        },
        ConfigSchemaField {
            key: "upstream.username".to_string(),
            label: "Username".to_string(),
            description: "Username for upstream authentication".to_string(),
            field_type: "string".to_string(),
            default_value: None,
            required: false,
            options: None,
            group: "upstream".to_string(),
        },
        ConfigSchemaField {
            key: "upstream.password".to_string(),
            label: "Password".to_string(),
            description: "Password for upstream authentication".to_string(),
            field_type: "password".to_string(),
            default_value: None,
            required: false,
            options: None,
            group: "upstream".to_string(),
        },
        ConfigSchemaField {
            key: "upstream.skip_tls_verify".to_string(),
            label: "Skip TLS Verification".to_string(),
            description: "Skip TLS certificate verification (for self-signed certs)".to_string(),
            field_type: "boolean".to_string(),
            default_value: Some("false".to_string()),
            required: false,
            options: None,
            group: "upstream".to_string(),
        },
        // Storage
        ConfigSchemaField {
            key: "storage.backend".to_string(),
            label: "Backend".to_string(),
            description: "Storage backend type".to_string(),
            field_type: "select".to_string(),
            default_value: Some("local".to_string()),
            required: true,
            options: Some(vec![
                ConfigOption {
                    value: "local".to_string(),
                    label: "Local Disk".to_string(),
                },
                ConfigOption {
                    value: "s3".to_string(),
                    label: "S3 Compatible".to_string(),
                },
            ]),
            group: "storage".to_string(),
        },
        ConfigSchemaField {
            key: "storage.local.path".to_string(),
            label: "Local Path".to_string(),
            description: "Path for local cache storage".to_string(),
            field_type: "string".to_string(),
            default_value: Some("./data/cache".to_string()),
            required: false,
            options: None,
            group: "storage".to_string(),
        },
        ConfigSchemaField {
            key: "storage.s3.bucket".to_string(),
            label: "S3 Bucket".to_string(),
            description: "S3 bucket name".to_string(),
            field_type: "string".to_string(),
            default_value: None,
            required: false,
            options: None,
            group: "storage".to_string(),
        },
        ConfigSchemaField {
            key: "storage.s3.region".to_string(),
            label: "S3 Region".to_string(),
            description: "AWS region for the S3 bucket".to_string(),
            field_type: "string".to_string(),
            default_value: Some("us-east-1".to_string()),
            required: false,
            options: None,
            group: "storage".to_string(),
        },
        ConfigSchemaField {
            key: "storage.s3.endpoint".to_string(),
            label: "S3 Endpoint".to_string(),
            description: "Custom S3 endpoint (for MinIO or other S3-compatible services)"
                .to_string(),
            field_type: "string".to_string(),
            default_value: None,
            required: false,
            options: None,
            group: "storage".to_string(),
        },
        ConfigSchemaField {
            key: "storage.s3.access_key".to_string(),
            label: "S3 Access Key".to_string(),
            description: "AWS access key ID".to_string(),
            field_type: "string".to_string(),
            default_value: None,
            required: false,
            options: None,
            group: "storage".to_string(),
        },
        ConfigSchemaField {
            key: "storage.s3.secret_key".to_string(),
            label: "S3 Secret Key".to_string(),
            description: "AWS secret access key".to_string(),
            field_type: "password".to_string(),
            default_value: None,
            required: false,
            options: None,
            group: "storage".to_string(),
        },
        ConfigSchemaField {
            key: "storage.s3.prefix".to_string(),
            label: "S3 Prefix".to_string(),
            description: "Optional prefix for all objects".to_string(),
            field_type: "string".to_string(),
            default_value: None,
            required: false,
            options: None,
            group: "storage".to_string(),
        },
        ConfigSchemaField {
            key: "storage.s3.allow_http".to_string(),
            label: "Allow HTTP".to_string(),
            description: "Allow HTTP (not HTTPS) for S3 connections".to_string(),
            field_type: "boolean".to_string(),
            default_value: Some("false".to_string()),
            required: false,
            options: None,
            group: "storage".to_string(),
        },
        // Database
        ConfigSchemaField {
            key: "database.path".to_string(),
            label: "Database Path".to_string(),
            description: "Path to SQLite database file".to_string(),
            field_type: "string".to_string(),
            default_value: Some("./data/harbor-cache.db".to_string()),
            required: true,
            options: None,
            group: "database".to_string(),
        },
        // Auth
        ConfigSchemaField {
            key: "auth.jwt_secret".to_string(),
            label: "JWT Secret".to_string(),
            description: "Secret key for JWT token signing".to_string(),
            field_type: "password".to_string(),
            default_value: None,
            required: true,
            options: None,
            group: "auth".to_string(),
        },
        ConfigSchemaField {
            key: "auth.enabled".to_string(),
            label: "Enable Authentication".to_string(),
            description: "Enable authentication for API endpoints".to_string(),
            field_type: "boolean".to_string(),
            default_value: Some("true".to_string()),
            required: false,
            options: None,
            group: "auth".to_string(),
        },
        // Logging
        ConfigSchemaField {
            key: "logging.level".to_string(),
            label: "Log Level".to_string(),
            description: "Logging verbosity level".to_string(),
            field_type: "select".to_string(),
            default_value: Some("info".to_string()),
            required: false,
            options: Some(vec![
                ConfigOption {
                    value: "trace".to_string(),
                    label: "Trace".to_string(),
                },
                ConfigOption {
                    value: "debug".to_string(),
                    label: "Debug".to_string(),
                },
                ConfigOption {
                    value: "info".to_string(),
                    label: "Info".to_string(),
                },
                ConfigOption {
                    value: "warn".to_string(),
                    label: "Warning".to_string(),
                },
                ConfigOption {
                    value: "error".to_string(),
                    label: "Error".to_string(),
                },
            ]),
            group: "logging".to_string(),
        },
        ConfigSchemaField {
            key: "logging.format".to_string(),
            label: "Log Format".to_string(),
            description: "Log output format".to_string(),
            field_type: "select".to_string(),
            default_value: Some("pretty".to_string()),
            required: false,
            options: Some(vec![
                ConfigOption {
                    value: "pretty".to_string(),
                    label: "Pretty".to_string(),
                },
                ConfigOption {
                    value: "json".to_string(),
                    label: "JSON".to_string(),
                },
            ]),
            group: "logging".to_string(),
        },
        // TLS
        ConfigSchemaField {
            key: "tls.enabled".to_string(),
            label: "Enable TLS".to_string(),
            description: "Enable TLS/HTTPS".to_string(),
            field_type: "boolean".to_string(),
            default_value: Some("false".to_string()),
            required: false,
            options: None,
            group: "tls".to_string(),
        },
        ConfigSchemaField {
            key: "tls.cert_path".to_string(),
            label: "Certificate Path".to_string(),
            description: "Path to TLS certificate file (PEM format)".to_string(),
            field_type: "string".to_string(),
            default_value: None,
            required: false,
            options: None,
            group: "tls".to_string(),
        },
        ConfigSchemaField {
            key: "tls.key_path".to_string(),
            label: "Key Path".to_string(),
            description: "Path to TLS private key file (PEM format)".to_string(),
            field_type: "string".to_string(),
            default_value: None,
            required: false,
            options: None,
            group: "tls".to_string(),
        },
    ];

    ConfigSchemaResponse { fields, groups }
}

// ==================== Config Routes ====================

/// GET /api/v1/config (Admin only)
async fn get_config(
    _admin: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<Vec<ConfigEntryResponse>>, ApiError> {
    let entries = state.db.list_config().await?;

    Ok(Json(
        entries
            .into_iter()
            .map(|e| ConfigEntryResponse {
                key: e.key,
                value: e.value,
                updated_at: e.updated_at.to_rfc3339(),
            })
            .collect(),
    ))
}

/// PUT /api/v1/config (Admin only)
async fn update_config(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Json(request): Json<UpdateConfigRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    info!("Updating {} config entries", request.entries.len());

    for entry in &request.entries {
        state.db.set_config(&entry.key, &entry.value).await?;
    }

    Ok(Json(serde_json::json!({
        "updated": request.entries.len()
    })))
}

/// GET /api/v1/config/:key (Admin only)
async fn get_config_key(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<ConfigEntryResponse>, ApiError> {
    let entries = state.db.list_config().await?;

    let entry = entries
        .into_iter()
        .find(|e| e.key == key)
        .ok_or_else(|| ApiError::NotFound(format!("Config key: {}", key)))?;

    Ok(Json(ConfigEntryResponse {
        key: entry.key,
        value: entry.value,
        updated_at: entry.updated_at.to_rfc3339(),
    }))
}

/// DELETE /api/v1/config/:key (Admin only)
async fn delete_config_key(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<StatusCode, ApiError> {
    debug!("Deleting config key: {}", key);

    let deleted = state.db.delete_config(&key).await?;

    if deleted {
        info!("Deleted config key: {}", key);
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound(format!("Config key: {}", key)))
    }
}

/// GET /api/v1/config/schema (Admin only)
async fn get_config_schema(_admin: RequireAdmin) -> Result<Json<ConfigSchemaResponse>, ApiError> {
    Ok(Json(build_config_schema()))
}

/// GET /api/v1/config/file (Admin only)
async fn get_config_file(
    _admin: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<ConfigFileResponse>, ApiError> {
    let config_path = state
        .config_path
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("Config path not available".to_string()))?;

    let path = config_path.read().await;

    // Validate the path before reading
    validate_config_path(&path)?;

    let content = tokio::fs::read_to_string(path.as_str())
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to read config file: {}", e)))?;

    // Check file size to prevent memory issues with large files
    if content.len() > MAX_CONFIG_CONTENT_SIZE {
        return Err(ApiError::Internal(
            "Config file exceeds maximum allowed size".to_string(),
        ));
    }

    Ok(Json(ConfigFileResponse {
        content,
        format: "toml".to_string(),
    }))
}

/// PUT /api/v1/config/file (Admin only)
async fn update_config_file(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Json(request): Json<UpdateConfigFileRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Check content size limit first to prevent memory abuse
    if request.content.len() > MAX_CONFIG_CONTENT_SIZE {
        return Err(ApiError::BadRequest(format!(
            "Config content exceeds maximum allowed size of {} bytes",
            MAX_CONFIG_CONTENT_SIZE
        )));
    }

    let config_path = state
        .config_path
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("Config path not available".to_string()))?;

    // Validate TOML syntax
    let parsed_config = toml::from_str::<toml::Value>(&request.content)
        .map_err(|e| ApiError::BadRequest(format!("Invalid TOML syntax: {}", e)))?;

    // Validate semantic content
    validate_config_semantics(&parsed_config)
        .map_err(|e| ApiError::BadRequest(format!("Invalid configuration: {}", e)))?;

    let path = config_path.read().await;

    // Validate the path before writing
    validate_config_path(&path)?;

    info!("Updating config file: {}", path);

    tokio::fs::write(path.as_str(), &request.content)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to write config file: {}", e)))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Configuration file updated. Restart the server to apply changes."
    })))
}

/// POST /api/v1/config/validate (Admin only)
async fn validate_config(
    _admin: RequireAdmin,
    Json(request): Json<UpdateConfigFileRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Check content size limit first
    if request.content.len() > MAX_CONFIG_CONTENT_SIZE {
        return Ok(Json(serde_json::json!({
            "valid": false,
            "message": format!("Config content exceeds maximum allowed size of {} bytes", MAX_CONFIG_CONTENT_SIZE)
        })));
    }

    // Validate TOML syntax
    match toml::from_str::<toml::Value>(&request.content) {
        Ok(parsed_config) => {
            // Also validate semantic content
            match validate_config_semantics(&parsed_config) {
                Ok(_) => Ok(Json(serde_json::json!({
                    "valid": true,
                    "message": "Configuration is valid"
                }))),
                Err(e) => Ok(Json(serde_json::json!({
                    "valid": false,
                    "message": e
                }))),
            }
        }
        Err(e) => Ok(Json(serde_json::json!({
            "valid": false,
            "message": format!("Invalid TOML syntax: {}", e)
        }))),
    }
}

/// Create config management routes
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/v1/config", get(get_config))
        .route("/api/v1/config", put(update_config))
        .route("/api/v1/config/schema", get(get_config_schema))
        .route("/api/v1/config/file", get(get_config_file))
        .route("/api/v1/config/file", put(update_config_file))
        .route("/api/v1/config/validate", post(validate_config))
        .route("/api/v1/config/{key}", get(get_config_key))
        .route("/api/v1/config/{key}", delete(delete_config_key))
}
