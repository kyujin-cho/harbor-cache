//! Configuration loading and management

use anyhow::{Context, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

/// Main configuration structure
/// Supports both old single [upstream] and new [[upstreams]] array format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub cache: CacheConfig,
    /// Legacy single upstream configuration (for backwards compatibility)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<LegacyUpstreamConfig>,
    /// New multi-upstream configuration
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upstreams: Vec<UpstreamConfig>,
    pub storage: StorageConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub tls: TlsConfig,
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

/// Legacy upstream Harbor configuration (for backwards compatibility)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyUpstreamConfig {
    pub url: String,
    #[serde(default = "default_registry")]
    pub registry: String,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default)]
    pub skip_tls_verify: bool,
}

/// Upstream route pattern configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamRouteConfig {
    /// Pattern to match repository paths (supports glob patterns)
    pub pattern: String,
    /// Priority for this route (lower = higher priority)
    #[serde(default = "default_priority")]
    pub priority: i32,
}

/// New upstream Harbor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamConfig {
    /// Unique identifier for the upstream
    pub name: String,
    /// Display name for UI (defaults to name if not set)
    #[serde(default)]
    pub display_name: Option<String>,
    /// URL of the upstream Harbor registry
    pub url: String,
    /// Registry/project name
    #[serde(default = "default_registry")]
    pub registry: String,
    /// Username for authentication
    #[serde(default)]
    pub username: Option<String>,
    /// Password for authentication
    #[serde(default)]
    pub password: Option<String>,
    /// Skip TLS certificate verification
    #[serde(default)]
    pub skip_tls_verify: bool,
    /// Priority for route matching (lower = higher priority)
    #[serde(default = "default_priority")]
    pub priority: i32,
    /// Whether this upstream is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Cache isolation mode: "shared" or "isolated"
    #[serde(default = "default_cache_isolation")]
    pub cache_isolation: String,
    /// Whether this is the default upstream (fallback)
    #[serde(default)]
    pub is_default: bool,
    /// Route patterns for this upstream
    #[serde(default)]
    pub routes: Vec<UpstreamRouteConfig>,
}

impl UpstreamConfig {
    /// Get the display name, falling back to name if not set
    pub fn display_name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }
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
    pub prefix: Option<String>,
    #[serde(default)]
    pub allow_http: bool,
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

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TlsConfig {
    /// Enable TLS/HTTPS
    #[serde(default)]
    pub enabled: bool,
    /// Path to TLS certificate file (PEM format)
    #[serde(default)]
    pub cert_path: Option<String>,
    /// Path to TLS private key file (PEM format)
    #[serde(default)]
    pub key_path: Option<String>,
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

fn default_priority() -> i32 {
    100
}

fn default_enabled() -> bool {
    true
}

fn default_cache_isolation() -> String {
    "shared".to_string()
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

        let mut config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path))?;

        // Migrate legacy upstream to new format if needed
        config.migrate_legacy_upstream();

        info!("Loaded configuration from {}", path);
        Ok(config)
    }

    /// Migrate legacy [upstream] to [[upstreams]] format
    fn migrate_legacy_upstream(&mut self) {
        if let Some(legacy) = self.upstream.take() {
            if self.upstreams.is_empty() {
                warn!("Migrating legacy [upstream] to [[upstreams]] format");
                self.upstreams.push(UpstreamConfig {
                    name: "default".to_string(),
                    display_name: Some("Default Upstream".to_string()),
                    url: legacy.url,
                    registry: legacy.registry,
                    username: legacy.username,
                    password: legacy.password,
                    skip_tls_verify: legacy.skip_tls_verify,
                    priority: default_priority(),
                    enabled: true,
                    cache_isolation: default_cache_isolation(),
                    is_default: true,
                    routes: vec![],
                });
            }
        }
    }

    /// Save configuration to a file
    pub fn save(&self, path: &str) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .with_context(|| "Failed to serialize configuration")?;

        std::fs::write(path, &content)
            .with_context(|| format!("Failed to write config file: {}", path))?;

        info!("Saved configuration to {}", path);
        Ok(())
    }

    /// Get all upstreams (returns references)
    pub fn get_upstreams(&self) -> &[UpstreamConfig] {
        &self.upstreams
    }

    /// Get an upstream by name
    pub fn get_upstream_by_name(&self, name: &str) -> Option<&UpstreamConfig> {
        self.upstreams.iter().find(|u| u.name == name)
    }

    /// Get the default upstream
    pub fn get_default_upstream(&self) -> Option<&UpstreamConfig> {
        self.upstreams
            .iter()
            .find(|u| u.is_default && u.enabled)
            .or_else(|| self.upstreams.iter().find(|u| u.enabled))
    }

    /// Add a new upstream
    pub fn add_upstream(&mut self, upstream: UpstreamConfig) -> Result<()> {
        // Check for duplicate name
        if self.upstreams.iter().any(|u| u.name == upstream.name) {
            anyhow::bail!("Upstream with name '{}' already exists", upstream.name);
        }

        // If this is marked as default, unmark others
        if upstream.is_default {
            for u in &mut self.upstreams {
                u.is_default = false;
            }
        }

        self.upstreams.push(upstream);
        Ok(())
    }

    /// Update an existing upstream by name
    pub fn update_upstream(&mut self, name: &str, updated: UpstreamConfig) -> Result<()> {
        let idx = self
            .upstreams
            .iter()
            .position(|u| u.name == name)
            .ok_or_else(|| anyhow::anyhow!("Upstream '{}' not found", name))?;

        // If the updated upstream is marked as default, unmark others
        if updated.is_default {
            for (i, u) in self.upstreams.iter_mut().enumerate() {
                if i != idx {
                    u.is_default = false;
                }
            }
        }

        self.upstreams[idx] = updated;
        Ok(())
    }

    /// Remove an upstream by name
    pub fn remove_upstream(&mut self, name: &str) -> Result<UpstreamConfig> {
        let idx = self
            .upstreams
            .iter()
            .position(|u| u.name == name)
            .ok_or_else(|| anyhow::anyhow!("Upstream '{}' not found", name))?;

        Ok(self.upstreams.remove(idx))
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
            upstream: None,
            upstreams: vec![UpstreamConfig {
                name: "default".to_string(),
                display_name: Some("Default Upstream".to_string()),
                url: "http://localhost:8880".to_string(),
                registry: default_registry(),
                username: Some("admin".to_string()),
                password: Some("Harbor12345".to_string()),
                skip_tls_verify: false,
                priority: default_priority(),
                enabled: true,
                cache_isolation: default_cache_isolation(),
                is_default: true,
                routes: vec![],
            }],
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
            tls: TlsConfig::default(),
        }
    }
}

/// Thread-safe configuration manager for runtime updates
#[derive(Clone)]
pub struct ConfigManager {
    config: Arc<RwLock<Config>>,
    path: Arc<RwLock<String>>,
}

impl ConfigManager {
    /// Create a new config manager
    pub fn new(config: Config, path: String) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            path: Arc::new(RwLock::new(path)),
        }
    }

    /// Get a clone of the current configuration
    pub fn get_config(&self) -> Config {
        self.config.read().clone()
    }

    /// Get upstreams configuration
    pub fn get_upstreams(&self) -> Vec<UpstreamConfig> {
        self.config.read().upstreams.clone()
    }

    /// Get an upstream by name
    pub fn get_upstream_by_name(&self, name: &str) -> Option<UpstreamConfig> {
        self.config
            .read()
            .upstreams
            .iter()
            .find(|u| u.name == name)
            .cloned()
    }

    /// Get the default upstream
    pub fn get_default_upstream(&self) -> Option<UpstreamConfig> {
        let config = self.config.read();
        config
            .upstreams
            .iter()
            .find(|u| u.is_default && u.enabled)
            .or_else(|| config.upstreams.iter().find(|u| u.enabled))
            .cloned()
    }

    /// Add a new upstream and save to file
    pub fn add_upstream(&self, upstream: UpstreamConfig) -> Result<()> {
        let mut config = self.config.write();
        config.add_upstream(upstream)?;
        let path = self.path.read().clone();
        config.save(&path)?;
        Ok(())
    }

    /// Update an existing upstream and save to file
    pub fn update_upstream(&self, name: &str, updated: UpstreamConfig) -> Result<()> {
        let mut config = self.config.write();
        config.update_upstream(name, updated)?;
        let path = self.path.read().clone();
        config.save(&path)?;
        Ok(())
    }

    /// Remove an upstream and save to file
    pub fn remove_upstream(&self, name: &str) -> Result<UpstreamConfig> {
        let mut config = self.config.write();
        let removed = config.remove_upstream(name)?;
        let path = self.path.read().clone();
        config.save(&path)?;
        Ok(removed)
    }

    /// Reload configuration from file
    pub fn reload(&self) -> Result<()> {
        let path = self.path.read().clone();
        let new_config = Config::load(&path)?;
        let mut config = self.config.write();
        *config = new_config;
        info!("Configuration reloaded from {}", path);
        Ok(())
    }

    /// Get the config file path
    pub fn get_path(&self) -> String {
        self.path.read().clone()
    }

    /// Update the config file path
    pub fn set_path(&self, path: String) {
        let mut p = self.path.write();
        *p = path;
    }
}
