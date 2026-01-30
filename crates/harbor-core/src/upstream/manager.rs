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
    /// The resolved project name (for multi-project upstreams)
    pub project: String,
}

impl std::fmt::Debug for UpstreamInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpstreamInfo")
            .field("config", &self.config)
            .field("match_reason", &self.match_reason)
            .field("project", &self.project)
            .finish_non_exhaustive()
    }
}

/// Reason for selecting a particular upstream
#[derive(Debug, Clone)]
pub enum MatchReason {
    /// Matched a specific route pattern
    RouteMatch { pattern: String, priority: i32 },
    /// Matched a project pattern in multi-project mode
    ProjectMatch { project: String, pattern: String, priority: i32 },
    /// Used as the default fallback
    DefaultFallback,
    /// Explicitly specified by name
    ExplicitName(String),
}

/// Internal state for each upstream
struct UpstreamState {
    config: UpstreamConfig,
    /// Client for single-project mode (uses config.registry)
    /// For multi-project mode, this is a "template" client for the default project
    default_client: Arc<HarborClient>,
    /// Cached clients for multi-project mode, keyed by project name
    project_clients: HashMap<String, Arc<HarborClient>>,
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

            // Security: Validate upstream configuration before loading
            if let Err(e) = upstream_config.validate() {
                warn!(
                    "Skipping upstream '{}' due to validation error: {}",
                    upstream_config.name, e
                );
                continue;
            }

            // Create the default client
            let default_project = upstream_config.get_default_project().to_string();
            match Self::create_client_for_project(&upstream_config, &default_project) {
                Ok(default_client) => {
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

                    // Collect routes from upstream-level routes
                    for route in &upstream_config.routes {
                        all_routes.push((upstream_config.name.clone(), route.clone()));
                    }

                    // Create project clients for multi-project mode
                    let mut project_clients = HashMap::new();
                    if upstream_config.uses_multi_project() {
                        for project in &upstream_config.projects {
                            match Self::create_client_for_project(&upstream_config, &project.name) {
                                Ok(client) => {
                                    project_clients.insert(project.name.clone(), Arc::new(client));
                                    debug!(
                                        "Created client for project {} on upstream {}",
                                        project.name, upstream_config.name
                                    );
                                }
                                Err(e) => {
                                    error!(
                                        "Failed to create client for project {} on upstream {}: {}",
                                        project.name, upstream_config.name, e
                                    );
                                }
                            }
                        }
                        info!(
                            "Loaded upstream: {} -> {} (multi-project: {:?})",
                            upstream_config.name,
                            upstream_config.url,
                            upstream_config.get_project_names()
                        );
                    } else {
                        info!(
                            "Loaded upstream: {} -> {} (registry: {})",
                            upstream_config.name, upstream_config.url, upstream_config.registry
                        );
                    }

                    new_upstreams.insert(
                        upstream_config.name.clone(),
                        UpstreamState {
                            config: upstream_config,
                            default_client: Arc::new(default_client),
                            project_clients,
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

    /// Create a HarborClient for a specific project on an upstream
    fn create_client_for_project(
        config: &UpstreamConfig,
        project: &str,
    ) -> Result<HarborClient, CoreError> {
        let client_config = HarborClientConfig {
            url: config.url.clone(),
            registry: project.to_string(),
            username: config.username.clone(),
            password: config.password.clone(),
            skip_tls_verify: config.skip_tls_verify,
        };

        HarborClient::new(client_config).map_err(CoreError::Proxy)
    }

    /// Create a HarborClient from an Upstream configuration (uses default project)
    #[allow(dead_code)]
    fn create_client(config: &UpstreamConfig) -> Result<HarborClient, CoreError> {
        let project = config.get_default_project();
        Self::create_client_for_project(config, project)
    }

    /// Find the appropriate upstream for a repository path
    pub fn find_upstream(&self, repository: &str) -> Option<UpstreamInfo> {
        let upstreams = self.upstreams.read();

        // First, try route matching (upstream-level routes)
        let route_matcher = self.route_matcher.read();
        if let Some(route_match) = route_matcher.find_match(repository) {
            // Collect matching upstreams and sort by priority for deterministic order
            let mut matching_states: Vec<_> = upstreams
                .values()
                .filter(|state| {
                    state
                        .config
                        .routes
                        .iter()
                        .any(|r| r.pattern == route_match.pattern)
                        && (state.health.healthy || state.health.consecutive_failures < 3)
                })
                .collect();

            // Sort by upstream priority, then by name for deterministic behavior
            matching_states.sort_by(|a, b| {
                a.config
                    .priority
                    .cmp(&b.config.priority)
                    .then_with(|| a.config.name.cmp(&b.config.name))
            });

            if let Some(state) = matching_states.first() {
                // For multi-project upstreams, find the matching project
                let (client, project) = self.get_client_and_project(state, repository);
                return Some(UpstreamInfo {
                    config: state.config.clone(),
                    client,
                    match_reason: MatchReason::RouteMatch {
                        pattern: route_match.pattern.clone(),
                        priority: route_match.priority,
                    },
                    project,
                });
            }
        }

        // Second, try project-level pattern matching for multi-project upstreams
        // Sort upstreams by priority, then by project priority
        let mut upstream_matches: Vec<_> = upstreams
            .values()
            .filter(|state| state.health.healthy || state.health.consecutive_failures < 3)
            .filter_map(|state| {
                if state.config.uses_multi_project() {
                    // Find matching project for this upstream
                    if let Some((project, pattern, priority)) =
                        self.find_matching_project(&state.config, repository)
                    {
                        let client = state
                            .project_clients
                            .get(&project)
                            .cloned()
                            .unwrap_or_else(|| state.default_client.clone());
                        return Some((state, client, project, pattern, priority));
                    }
                }
                None
            })
            .collect();

        // Sort by project priority first, then upstream priority, then name for determinism
        upstream_matches.sort_by(|a, b| {
            a.4.cmp(&b.4)
                .then_with(|| a.0.config.priority.cmp(&b.0.config.priority))
                .then_with(|| a.0.config.name.cmp(&b.0.config.name))
        });

        if let Some((state, client, project, pattern, priority)) = upstream_matches.into_iter().next()
        {
            return Some(UpstreamInfo {
                config: state.config.clone(),
                client,
                match_reason: MatchReason::ProjectMatch {
                    project: project.clone(),
                    pattern,
                    priority,
                },
                project,
            });
        }

        // Fall back to default upstream
        let default_name = self.default_upstream_name.read().clone();
        if let Some(name) = default_name
            && let Some(state) = upstreams.get(&name)
        {
            let (client, project) = self.get_client_and_project(state, repository);
            return Some(UpstreamInfo {
                config: state.config.clone(),
                client,
                match_reason: MatchReason::DefaultFallback,
                project,
            });
        }

        // If no default, try first available healthy upstream (sorted for determinism)
        let mut available: Vec<_> = upstreams
            .values()
            .filter(|state| state.health.healthy || state.health.consecutive_failures < 3)
            .collect();

        // Sort by priority, then name for deterministic fallback behavior
        available.sort_by(|a, b| {
            a.config
                .priority
                .cmp(&b.config.priority)
                .then_with(|| a.config.name.cmp(&b.config.name))
        });

        if let Some(state) = available.first() {
            let (client, project) = self.get_client_and_project(state, repository);
            return Some(UpstreamInfo {
                config: state.config.clone(),
                client,
                match_reason: MatchReason::DefaultFallback,
                project,
            });
        }

        None
    }

    /// Get the appropriate client and project for a given upstream state and repository
    fn get_client_and_project(&self, state: &UpstreamState, repository: &str) -> (Arc<HarborClient>, String) {
        if state.config.uses_multi_project() {
            // Try to find a matching project
            if let Some(project) = state.config.find_matching_project(repository) {
                if let Some(client) = state.project_clients.get(project) {
                    return (client.clone(), project.to_string());
                }
            }
        }
        // Fall back to default
        (state.default_client.clone(), state.config.get_default_project().to_string())
    }

    /// Find the matching project for a repository in multi-project mode
    fn find_matching_project(&self, config: &UpstreamConfig, repository: &str) -> Option<(String, String, i32)> {
        if !config.uses_multi_project() {
            return None;
        }

        // Sort projects by priority and find the first match
        let mut projects: Vec<_> = config.projects.iter().collect();
        projects.sort_by_key(|p| p.priority);

        for project in projects {
            let pattern = project.effective_pattern();
            if config.find_matching_project(repository) == Some(&project.name) {
                return Some((project.name.clone(), pattern, project.priority));
            }
        }

        None
    }

    /// Get an upstream by name
    pub fn get_upstream_by_name(&self, name: &str) -> Option<UpstreamInfo> {
        let upstreams = self.upstreams.read();

        upstreams.get(name).map(|state| {
            let project = state.config.get_default_project().to_string();
            UpstreamInfo {
                config: state.config.clone(),
                client: state.default_client.clone(),
                match_reason: MatchReason::ExplicitName(name.to_string()),
                project,
            }
        })
    }

    /// Get an upstream by name with a specific project
    pub fn get_upstream_by_name_and_project(&self, name: &str, project: &str) -> Option<UpstreamInfo> {
        let upstreams = self.upstreams.read();

        upstreams.get(name).and_then(|state| {
            let client = state.project_clients.get(project)
                .cloned()
                .or_else(|| {
                    // If project matches the default/legacy registry, use default client
                    if project == state.config.registry || project == state.config.get_default_project() {
                        Some(state.default_client.clone())
                    } else {
                        None
                    }
                })?;

            Some(UpstreamInfo {
                config: state.config.clone(),
                client,
                match_reason: MatchReason::ExplicitName(name.to_string()),
                project: project.to_string(),
            })
        })
    }

    /// Get the default upstream
    pub fn get_default_upstream(&self) -> Option<UpstreamInfo> {
        let default_name = self.default_upstream_name.read().clone();
        default_name.and_then(|name| self.get_upstream_by_name(&name))
    }

    /// Get the default upstream with a specific project
    pub fn get_default_upstream_for_project(&self, project: &str) -> Option<UpstreamInfo> {
        let default_name = self.default_upstream_name.read().clone();
        default_name.and_then(|name| self.get_upstream_by_name_and_project(&name, project))
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
            upstreams.get(name).map(|s| s.default_client.clone())
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
