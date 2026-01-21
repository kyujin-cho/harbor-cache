//! Configuration loading and management

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::info;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub cache: CacheConfig,
    pub upstream: UpstreamConfig,
    pub storage: StorageConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_bind_address")]
    pub bind_address: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_max_size")]
    pub max_size: u64,
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
    #[serde(default = "default_eviction_policy")]
    pub eviction_policy: String,
}

/// Upstream Harbor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamConfig {
    pub url: String,
    #[serde(default = "default_registry")]
    pub registry: String,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default)]
    pub skip_tls_verify: bool,
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default)]
    pub local: LocalStorageConfig,
    #[serde(default)]
    pub s3: S3StorageConfig,
}

/// Local storage configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalStorageConfig {
    #[serde(default = "default_local_path")]
    pub path: String,
}

/// S3 storage configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct S3StorageConfig {
    pub bucket: Option<String>,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_path")]
    pub path: String,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: String,
    #[serde(default = "default_auth_enabled")]
    pub enabled: bool,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default)]
    pub format: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: "pretty".to_string(),
        }
    }
}

// Default value functions
fn default_bind_address() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    5000
}

fn default_max_size() -> u64 {
    10 * 1024 * 1024 * 1024 // 10 GB
}

fn default_retention_days() -> u32 {
    30
}

fn default_eviction_policy() -> String {
    "lru".to_string()
}

fn default_registry() -> String {
    "library".to_string()
}

fn default_backend() -> String {
    "local".to_string()
}

fn default_local_path() -> String {
    "./data/cache".to_string()
}

fn default_db_path() -> String {
    "./data/harbor-cache.db".to_string()
}

fn default_jwt_secret() -> String {
    "change-me-in-production".to_string()
}

fn default_auth_enabled() -> bool {
    true
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Config {
    /// Load configuration from a file
    pub fn load(path: &str) -> Result<Self> {
        let config_path = Path::new(path);

        // Check if config file exists
        if !config_path.exists() {
            info!("Config file not found at {}, using defaults", path);
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read config file: {}", path))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path))?;

        info!("Loaded configuration from {}", path);
        Ok(config)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                bind_address: default_bind_address(),
                port: default_port(),
            },
            cache: CacheConfig {
                max_size: default_max_size(),
                retention_days: default_retention_days(),
                eviction_policy: default_eviction_policy(),
            },
            upstream: UpstreamConfig {
                url: "http://localhost:8880".to_string(),
                registry: default_registry(),
                username: Some("admin".to_string()),
                password: Some("Harbor12345".to_string()),
                skip_tls_verify: false,
            },
            storage: StorageConfig {
                backend: default_backend(),
                local: LocalStorageConfig {
                    path: default_local_path(),
                },
                s3: S3StorageConfig::default(),
            },
            database: DatabaseConfig {
                path: default_db_path(),
            },
            auth: AuthConfig {
                jwt_secret: default_jwt_secret(),
                enabled: default_auth_enabled(),
            },
            logging: LoggingConfig::default(),
        }
    }
}
