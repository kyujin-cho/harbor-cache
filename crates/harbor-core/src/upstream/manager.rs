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
use harbor_db::{CacheIsolation, Database, Upstream};
use harbor_proxy::{HarborClient, HarborClientConfig};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use super::router::RouteMatcher;
use crate::error::CoreError;

/// Health status for an upstream
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamHealth {
    pub upstream_id: i64,
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
    pub upstream: Upstream,
    /// The HarborClient for this upstream
    pub client: Arc<HarborClient>,
    /// How this upstream was selected
    pub match_reason: MatchReason,
}

impl std::fmt::Debug for UpstreamInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpstreamInfo")
            .field("upstream", &self.upstream)
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
    upstream: Upstream,
    client: Arc<HarborClient>,
    health: UpstreamHealth,
}

/// Manages multiple upstream Harbor registries
pub struct UpstreamManager {
    db: Database,
    /// Map of upstream ID to state
    upstreams: RwLock<HashMap<i64, UpstreamState>>,
    /// Route matcher for path-based routing
    route_matcher: RwLock<RouteMatcher>,
    /// Default upstream ID (if any)
    default_upstream_id: RwLock<Option<i64>>,
}

impl UpstreamManager {
    /// Create a new UpstreamManager
    pub async fn new(db: Database) -> Result<Self, CoreError> {
        let manager = Self {
            db,
            upstreams: RwLock::new(HashMap::new()),
            route_matcher: RwLock::new(RouteMatcher::new(vec![])),
            default_upstream_id: RwLock::new(None),
        };

        // Load initial configuration
        manager.reload().await?;

        Ok(manager)
    }

    /// Reload upstream configuration from database
    pub async fn reload(&self) -> Result<(), CoreError> {
        info!("Reloading upstream configuration");

        // Load all enabled upstreams
        let upstreams = self.db.list_enabled_upstreams().await?;
        let routes = self.db.list_upstream_routes().await?;

        let mut new_upstreams = HashMap::new();
        let mut default_id = None;

        for upstream in upstreams {
            match Self::create_client(&upstream) {
                Ok(client) => {
                    let health = UpstreamHealth {
                        upstream_id: upstream.id,
                        name: upstream.name.clone(),
                        healthy: true, // Assume healthy until proven otherwise
                        last_check: Utc::now(),
                        last_error: None,
                        consecutive_failures: 0,
                    };

                    if upstream.is_default {
                        default_id = Some(upstream.id);
                    }

                    info!(
                        "Loaded upstream: {} -> {} (registry: {})",
                        upstream.name, upstream.url, upstream.registry
                    );

                    new_upstreams.insert(
                        upstream.id,
                        UpstreamState {
                            upstream,
                            client: Arc::new(client),
                            health,
                        },
                    );
                }
                Err(e) => {
                    error!("Failed to create client for upstream {}: {}", upstream.name, e);
                }
            }
        }

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
            let mut default_guard = self.default_upstream_id.write();
            *default_guard = default_id;
        }

        info!("Upstream configuration reloaded");
        Ok(())
    }

    /// Create a HarborClient from an Upstream configuration
    fn create_client(upstream: &Upstream) -> Result<HarborClient, CoreError> {
        let config = HarborClientConfig {
            url: upstream.url.clone(),
            registry: upstream.registry.clone(),
            username: upstream.username.clone(),
            password: upstream.password.clone(),
            skip_tls_verify: upstream.skip_tls_verify,
        };

        HarborClient::new(config).map_err(CoreError::Proxy)
    }

    /// Find the appropriate upstream for a repository path
    pub fn find_upstream(&self, repository: &str) -> Option<UpstreamInfo> {
        let upstreams = self.upstreams.read();

        // First, try route matching
        let route_matcher = self.route_matcher.read();
        if let Some(route_match) = route_matcher.find_match(repository) {
            if let Some(state) = upstreams.get(&route_match.upstream_id) {
                if state.health.healthy || state.health.consecutive_failures < 3 {
                    return Some(UpstreamInfo {
                        upstream: state.upstream.clone(),
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
        let default_id = *self.default_upstream_id.read();
        if let Some(id) = default_id {
            if let Some(state) = upstreams.get(&id) {
                return Some(UpstreamInfo {
                    upstream: state.upstream.clone(),
                    client: state.client.clone(),
                    match_reason: MatchReason::DefaultFallback,
                });
            }
        }

        // If no default, try first available healthy upstream
        for state in upstreams.values() {
            if state.health.healthy || state.health.consecutive_failures < 3 {
                return Some(UpstreamInfo {
                    upstream: state.upstream.clone(),
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

        for state in upstreams.values() {
            if state.upstream.name == name {
                return Some(UpstreamInfo {
                    upstream: state.upstream.clone(),
                    client: state.client.clone(),
                    match_reason: MatchReason::ExplicitName(name.to_string()),
                });
            }
        }

        None
    }

    /// Get an upstream by ID
    pub fn get_upstream_by_id(&self, id: i64) -> Option<UpstreamInfo> {
        let upstreams = self.upstreams.read();

        upstreams.get(&id).map(|state| UpstreamInfo {
            upstream: state.upstream.clone(),
            client: state.client.clone(),
            match_reason: MatchReason::ExplicitName(state.upstream.name.clone()),
        })
    }

    /// Get the default upstream
    pub fn get_default_upstream(&self) -> Option<UpstreamInfo> {
        let default_id = *self.default_upstream_id.read();
        default_id.and_then(|id| self.get_upstream_by_id(id))
    }

    /// List all upstreams
    pub fn list_upstreams(&self) -> Vec<Upstream> {
        let upstreams = self.upstreams.read();
        upstreams.values().map(|s| s.upstream.clone()).collect()
    }

    /// Get health status for all upstreams
    pub fn get_health_status(&self) -> Vec<UpstreamHealth> {
        let upstreams = self.upstreams.read();
        upstreams.values().map(|s| s.health.clone()).collect()
    }

    /// Get health status for a specific upstream
    pub fn get_upstream_health(&self, id: i64) -> Option<UpstreamHealth> {
        let upstreams = self.upstreams.read();
        upstreams.get(&id).map(|s| s.health.clone())
    }

    /// Check health of a specific upstream
    pub async fn check_upstream_health(&self, id: i64) -> Result<UpstreamHealth, CoreError> {
        let client = {
            let upstreams = self.upstreams.read();
            upstreams.get(&id).map(|s| s.client.clone())
        };

        let client = client.ok_or_else(|| CoreError::NotFound(format!("Upstream {}", id)))?;

        let (healthy, error) = match client.ping().await {
            Ok(true) => (true, None),
            Ok(false) => (false, Some("Ping returned false".to_string())),
            Err(e) => (false, Some(e.to_string())),
        };

        let now = Utc::now();

        // Update health status
        let mut upstreams = self.upstreams.write();
        if let Some(state) = upstreams.get_mut(&id) {
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
            Err(CoreError::NotFound(format!("Upstream {}", id)))
        }
    }

    /// Check health of all upstreams
    pub async fn check_all_health(&self) -> Vec<UpstreamHealth> {
        let upstream_ids: Vec<i64> = {
            let upstreams = self.upstreams.read();
            upstreams.keys().copied().collect()
        };

        let mut results = Vec::new();
        for id in upstream_ids {
            match self.check_upstream_health(id).await {
                Ok(health) => results.push(health),
                Err(e) => {
                    warn!("Failed to check health for upstream {}: {}", id, e);
                }
            }
        }

        results
    }

    /// Mark an upstream as unhealthy after a failure
    pub fn mark_unhealthy(&self, id: i64, error: &str) {
        let mut upstreams = self.upstreams.write();
        if let Some(state) = upstreams.get_mut(&id) {
            state.health.healthy = false;
            state.health.last_error = Some(error.to_string());
            state.health.consecutive_failures += 1;
            debug!(
                "Marked upstream {} as unhealthy: {} (failures: {})",
                id, error, state.health.consecutive_failures
            );
        }
    }

    /// Mark an upstream as healthy after a successful operation
    pub fn mark_healthy(&self, id: i64) {
        let mut upstreams = self.upstreams.write();
        if let Some(state) = upstreams.get_mut(&id) {
            if !state.health.healthy {
                info!("Upstream {} recovered", id);
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
    pub fn uses_isolated_cache(&self, id: i64) -> bool {
        let upstreams = self.upstreams.read();
        upstreams
            .get(&id)
            .map(|s| s.upstream.cache_isolation == CacheIsolation::Isolated)
            .unwrap_or(false)
    }

    /// Get upstream ID for cache operations (None if shared caching)
    pub fn get_cache_upstream_id(&self, id: i64) -> Option<i64> {
        if self.uses_isolated_cache(id) {
            Some(id)
        } else {
            None
        }
    }
}
