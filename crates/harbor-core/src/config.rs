//! Shared configuration types for upstream management
//!
//! These types are shared across crates to avoid circular dependencies.
//! The main config loading is done in harbor-cache, but these types
//! define the upstream configuration structure used by harbor-core.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Upstream route pattern configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamRouteConfig {
    /// Pattern to match repository paths (supports glob patterns)
    pub pattern: String,
    /// Priority for this route (lower = higher priority)
    #[serde(default = "default_priority")]
    pub priority: i32,
}

/// Upstream configuration for a single Harbor registry
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

    /// Check if this upstream uses isolated caching
    pub fn uses_isolated_cache(&self) -> bool {
        self.cache_isolation == "isolated"
    }
}

fn default_priority() -> i32 {
    100
}

fn default_enabled() -> bool {
    true
}

fn default_registry() -> String {
    "library".to_string()
}

fn default_cache_isolation() -> String {
    "shared".to_string()
}

/// Trait for providing upstream configuration
/// This allows the config to be managed externally (e.g., by harbor-cache)
/// while harbor-core can use it for upstream management
pub trait UpstreamConfigProvider: Send + Sync {
    /// Get all upstreams
    fn get_upstreams(&self) -> Vec<UpstreamConfig>;

    /// Get an upstream by name
    fn get_upstream_by_name(&self, name: &str) -> Option<UpstreamConfig>;

    /// Get the default upstream
    fn get_default_upstream(&self) -> Option<UpstreamConfig>;

    /// Add a new upstream (persists to config file)
    fn add_upstream(&self, upstream: UpstreamConfig) -> anyhow::Result<()>;

    /// Update an existing upstream (persists to config file)
    fn update_upstream(&self, name: &str, updated: UpstreamConfig) -> anyhow::Result<()>;

    /// Remove an upstream (persists to config file)
    fn remove_upstream(&self, name: &str) -> anyhow::Result<UpstreamConfig>;

    /// Get the config file path
    fn get_config_path(&self) -> String;
}

/// A simple in-memory implementation of UpstreamConfigProvider for testing
/// or when no persistence is needed
pub struct InMemoryConfigProvider {
    upstreams: Arc<RwLock<Vec<UpstreamConfig>>>,
}

impl InMemoryConfigProvider {
    pub fn new(upstreams: Vec<UpstreamConfig>) -> Self {
        Self {
            upstreams: Arc::new(RwLock::new(upstreams)),
        }
    }
}

impl UpstreamConfigProvider for InMemoryConfigProvider {
    fn get_upstreams(&self) -> Vec<UpstreamConfig> {
        self.upstreams.read().clone()
    }

    fn get_upstream_by_name(&self, name: &str) -> Option<UpstreamConfig> {
        self.upstreams
            .read()
            .iter()
            .find(|u| u.name == name)
            .cloned()
    }

    fn get_default_upstream(&self) -> Option<UpstreamConfig> {
        let upstreams = self.upstreams.read();
        upstreams
            .iter()
            .find(|u| u.is_default && u.enabled)
            .or_else(|| upstreams.iter().find(|u| u.enabled))
            .cloned()
    }

    fn add_upstream(&self, upstream: UpstreamConfig) -> anyhow::Result<()> {
        let mut upstreams = self.upstreams.write();
        if upstreams.iter().any(|u| u.name == upstream.name) {
            anyhow::bail!("Upstream with name '{}' already exists", upstream.name);
        }
        upstreams.push(upstream);
        Ok(())
    }

    fn update_upstream(&self, name: &str, updated: UpstreamConfig) -> anyhow::Result<()> {
        let mut upstreams = self.upstreams.write();
        let idx = upstreams
            .iter()
            .position(|u| u.name == name)
            .ok_or_else(|| anyhow::anyhow!("Upstream '{}' not found", name))?;
        upstreams[idx] = updated;
        Ok(())
    }

    fn remove_upstream(&self, name: &str) -> anyhow::Result<UpstreamConfig> {
        let mut upstreams = self.upstreams.write();
        let idx = upstreams
            .iter()
            .position(|u| u.name == name)
            .ok_or_else(|| anyhow::anyhow!("Upstream '{}' not found", name))?;
        Ok(upstreams.remove(idx))
    }

    fn get_config_path(&self) -> String {
        String::new()
    }
}
