//! Upstream manager for handling multiple Harbor clients
//!
//! The UpstreamManager is responsible for:
//! - Creating and managing HarborClient instances for each upstream
//! - Route-based upstream selection
//! - Health monitoring
//! - Dynamic upstream configuration updates

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use harbor_proxy::{HarborClient, HarborClientConfig};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use super::router::RouteMatcher;
use crate::config::{UpstreamConfig, UpstreamConfigProvider, UpstreamRouteConfig};
use crate::error::CoreError;

/// Health status for an upstream
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamHealth {
    pub upstream_name: String,
    pub name: String,
    pub healthy: bool,
    pub last_check: DateTime<Utc>,
    pub last_error: Option<String>,
    pub consecutive_failures: u32,
}

/// Information about a resolved upstream
#[derive(Clone)]
pub struct UpstreamInfo {
    /// The upstream configuration
    pub config: UpstreamConfig,
    /// The HarborClient for this upstream
    pub client: Arc<HarborClient>,
    /// How this upstream was selected
    pub match_reason: MatchReason,
}

impl std::fmt::Debug for UpstreamInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpstreamInfo")
            .field("config", &self.config)
            .field("match_reason", &self.match_reason)
            .finish_non_exhaustive()
    }
}

/// Reason for selecting a particular upstream
#[derive(Debug, Clone)]
pub enum MatchReason {
    /// Matched a specific route pattern
    RouteMatch { pattern: String, priority: i32 },
    /// Used as the default fallback
    DefaultFallback,
    /// Explicitly specified by name
    ExplicitName(String),
}

/// Internal state for each upstream
struct UpstreamState {
    config: UpstreamConfig,
    client: Arc<HarborClient>,
    health: UpstreamHealth,
}

/// Manages multiple upstream Harbor registries
pub struct UpstreamManager {
    config_provider: Arc<dyn UpstreamConfigProvider>,
    /// Map of upstream name to state
    upstreams: RwLock<HashMap<String, UpstreamState>>,
    /// Route matcher for path-based routing
    route_matcher: RwLock<RouteMatcher>,
    /// Default upstream name (if any)
    default_upstream_name: RwLock<Option<String>>,
}

impl UpstreamManager {
    /// Create a new UpstreamManager with a config provider
    pub fn new(config_provider: Arc<dyn UpstreamConfigProvider>) -> Result<Self, CoreError> {
        let manager = Self {
            config_provider,
            upstreams: RwLock::new(HashMap::new()),
            route_matcher: RwLock::new(RouteMatcher::new(vec![])),
            default_upstream_name: RwLock::new(None),
        };

        // Load initial configuration
        manager.reload()?;

        Ok(manager)
    }

    /// Reload upstream configuration from the config provider
    pub fn reload(&self) -> Result<(), CoreError> {
        info!("Reloading upstream configuration from config provider");

        // Load all upstreams from config
        let upstream_configs = self.config_provider.get_upstreams();

        let mut new_upstreams = HashMap::new();
        let mut default_name = None;
        let mut all_routes: Vec<(String, UpstreamRouteConfig)> = Vec::new();

        for upstream_config in upstream_configs {
            if !upstream_config.enabled {
                debug!("Skipping disabled upstream: {}", upstream_config.name);
                continue;
            }

            match Self::create_client(&upstream_config) {
                Ok(client) => {
                    let health = UpstreamHealth {
                        upstream_name: upstream_config.name.clone(),
                        name: upstream_config.display_name().to_string(),
                        healthy: true, // Assume healthy until proven otherwise
                        last_check: Utc::now(),
                        last_error: None,
                        consecutive_failures: 0,
                    };

                    if upstream_config.is_default {
                        default_name = Some(upstream_config.name.clone());
                    }

                    // Collect routes
                    for route in &upstream_config.routes {
                        all_routes.push((upstream_config.name.clone(), route.clone()));
                    }

                    info!(
                        "Loaded upstream: {} -> {} (registry: {})",
                        upstream_config.name, upstream_config.url, upstream_config.registry
                    );

                    new_upstreams.insert(
                        upstream_config.name.clone(),
                        UpstreamState {
                            config: upstream_config,
                            client: Arc::new(client),
                            health,
                        },
                    );
                }
                Err(e) => {
                    error!(
                        "Failed to create client for upstream {}: {}",
                        upstream_config.name, e
                    );
                }
            }
        }

        // Convert routes to the format needed by the router
        let routes: Vec<harbor_db::UpstreamRoute> = all_routes
            .into_iter()
            .enumerate()
            .map(|(idx, (_upstream_name, route))| harbor_db::UpstreamRoute {
                id: idx as i64,
                upstream_id: 0, // Not used in the new approach
                pattern: route.pattern,
                priority: route.priority,
                created_at: Utc::now(),
            })
            .collect();

        // Update state
        {
            let mut upstreams_guard = self.upstreams.write();
            *upstreams_guard = new_upstreams;
        }

        {
            let mut matcher_guard = self.route_matcher.write();
            *matcher_guard = RouteMatcher::new(routes);
        }

        {
            let mut default_guard = self.default_upstream_name.write();
            *default_guard = default_name;
        }

        info!("Upstream configuration reloaded");
        Ok(())
    }

    /// Create a HarborClient from an Upstream configuration
    fn create_client(config: &UpstreamConfig) -> Result<HarborClient, CoreError> {
        let client_config = HarborClientConfig {
            url: config.url.clone(),
            registry: config.registry.clone(),
            username: config.username.clone(),
            password: config.password.clone(),
            skip_tls_verify: config.skip_tls_verify,
        };

        HarborClient::new(client_config).map_err(CoreError::Proxy)
    }

    /// Find the appropriate upstream for a repository path
    pub fn find_upstream(&self, repository: &str) -> Option<UpstreamInfo> {
        let upstreams = self.upstreams.read();

        // First, try route matching
        let route_matcher = self.route_matcher.read();
        if let Some(route_match) = route_matcher.find_match(repository) {
            // Find the upstream that has this route pattern
            for state in upstreams.values() {
                if state
                    .config
                    .routes
                    .iter()
                    .any(|r| r.pattern == route_match.pattern)
                    && (state.health.healthy || state.health.consecutive_failures < 3)
                {
                    return Some(UpstreamInfo {
                        config: state.config.clone(),
                        client: state.client.clone(),
                        match_reason: MatchReason::RouteMatch {
                            pattern: route_match.pattern,
                            priority: route_match.priority,
                        },
                    });
                }
            }
        }

        // Fall back to default upstream
        let default_name = self.default_upstream_name.read().clone();
        if let Some(name) = default_name
            && let Some(state) = upstreams.get(&name)
        {
            return Some(UpstreamInfo {
                config: state.config.clone(),
                client: state.client.clone(),
                match_reason: MatchReason::DefaultFallback,
            });
        }

        // If no default, try first available healthy upstream
        for state in upstreams.values() {
            if state.health.healthy || state.health.consecutive_failures < 3 {
                return Some(UpstreamInfo {
                    config: state.config.clone(),
                    client: state.client.clone(),
                    match_reason: MatchReason::DefaultFallback,
                });
            }
        }

        None
    }

    /// Get an upstream by name
    pub fn get_upstream_by_name(&self, name: &str) -> Option<UpstreamInfo> {
        let upstreams = self.upstreams.read();

        upstreams.get(name).map(|state| UpstreamInfo {
            config: state.config.clone(),
            client: state.client.clone(),
            match_reason: MatchReason::ExplicitName(name.to_string()),
        })
    }

    /// Get the default upstream
    pub fn get_default_upstream(&self) -> Option<UpstreamInfo> {
        let default_name = self.default_upstream_name.read().clone();
        default_name.and_then(|name| self.get_upstream_by_name(&name))
    }

    /// List all upstreams
    pub fn list_upstreams(&self) -> Vec<UpstreamConfig> {
        let upstreams = self.upstreams.read();
        upstreams.values().map(|s| s.config.clone()).collect()
    }

    /// Get health status for all upstreams
    pub fn get_health_status(&self) -> Vec<UpstreamHealth> {
        let upstreams = self.upstreams.read();
        upstreams.values().map(|s| s.health.clone()).collect()
    }

    /// Get health status for a specific upstream
    pub fn get_upstream_health(&self, name: &str) -> Option<UpstreamHealth> {
        let upstreams = self.upstreams.read();
        upstreams.get(name).map(|s| s.health.clone())
    }

    /// Check health of a specific upstream
    pub async fn check_upstream_health(&self, name: &str) -> Result<UpstreamHealth, CoreError> {
        let client = {
            let upstreams = self.upstreams.read();
            upstreams.get(name).map(|s| s.client.clone())
        };

        let client = client.ok_or_else(|| CoreError::NotFound(format!("Upstream {}", name)))?;

        let (healthy, error) = match client.ping().await {
            Ok(true) => (true, None),
            Ok(false) => (false, Some("Ping returned false".to_string())),
            Err(e) => (false, Some(e.to_string())),
        };

        let now = Utc::now();

        // Update health status
        let mut upstreams = self.upstreams.write();
        if let Some(state) = upstreams.get_mut(name) {
            state.health.healthy = healthy;
            state.health.last_check = now;
            state.health.last_error = error.clone();

            if healthy {
                state.health.consecutive_failures = 0;
            } else {
                state.health.consecutive_failures += 1;
            }

            Ok(state.health.clone())
        } else {
            Err(CoreError::NotFound(format!("Upstream {}", name)))
        }
    }

    /// Check health of all upstreams
    pub async fn check_all_health(&self) -> Vec<UpstreamHealth> {
        let upstream_names: Vec<String> = {
            let upstreams = self.upstreams.read();
            upstreams.keys().cloned().collect()
        };

        let mut results = Vec::new();
        for name in upstream_names {
            match self.check_upstream_health(&name).await {
                Ok(health) => results.push(health),
                Err(e) => {
                    warn!("Failed to check health for upstream {}: {}", name, e);
                }
            }
        }

        results
    }

    /// Mark an upstream as unhealthy after a failure
    pub fn mark_unhealthy(&self, name: &str, error: &str) {
        let mut upstreams = self.upstreams.write();
        if let Some(state) = upstreams.get_mut(name) {
            state.health.healthy = false;
            state.health.last_error = Some(error.to_string());
            state.health.consecutive_failures += 1;
            debug!(
                "Marked upstream {} as unhealthy: {} (failures: {})",
                name, error, state.health.consecutive_failures
            );
        }
    }

    /// Mark an upstream as healthy after a successful operation
    pub fn mark_healthy(&self, name: &str) {
        let mut upstreams = self.upstreams.write();
        if let Some(state) = upstreams.get_mut(name) {
            if !state.health.healthy {
                info!("Upstream {} recovered", name);
            }
            state.health.healthy = true;
            state.health.last_error = None;
            state.health.consecutive_failures = 0;
        }
    }

    /// Get the number of configured upstreams
    pub fn upstream_count(&self) -> usize {
        self.upstreams.read().len()
    }

    /// Check if a specific upstream uses isolated caching
    pub fn uses_isolated_cache(&self, name: &str) -> bool {
        let upstreams = self.upstreams.read();
        upstreams
            .get(name)
            .map(|s| s.config.uses_isolated_cache())
            .unwrap_or(false)
    }

    /// Get upstream name for cache operations (None if shared caching)
    pub fn get_cache_upstream_name(&self, name: &str) -> Option<String> {
        if self.uses_isolated_cache(name) {
            Some(name.to_string())
        } else {
            None
        }
    }

    /// Get the config provider
    pub fn config_provider(&self) -> &Arc<dyn UpstreamConfigProvider> {
        &self.config_provider
    }
}
