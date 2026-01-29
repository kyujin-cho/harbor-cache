//! Configuration management routes

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
};
use tracing::{debug, info};

use crate::error::ApiError;
use crate::state::AppState;

use super::auth::RequireAdmin;
use super::types::{
    ConfigEntryResponse, ConfigFileResponse, ConfigGroup, ConfigOption, ConfigSchemaField,
    ConfigSchemaResponse, UpdateConfigFileRequest, UpdateConfigRequest,
};

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
async fn get_config_schema(
    _admin: RequireAdmin,
) -> Result<Json<ConfigSchemaResponse>, ApiError> {
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

    let content = tokio::fs::read_to_string(path.as_str())
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to read config file: {}", e)))?;

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
    let config_path = state
        .config_path
        .as_ref()
        .ok_or_else(|| ApiError::BadRequest("Config path not available".to_string()))?;

    // Validate TOML syntax
    toml::from_str::<toml::Value>(&request.content)
        .map_err(|e| ApiError::BadRequest(format!("Invalid TOML syntax: {}", e)))?;

    let path = config_path.read().await;

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
    // Validate TOML syntax
    match toml::from_str::<toml::Value>(&request.content) {
        Ok(_) => Ok(Json(serde_json::json!({
            "valid": true,
            "message": "Configuration is valid"
        }))),
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
