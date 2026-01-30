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

/// Project configuration within an upstream
///
/// Allows multiple projects to be configured per upstream Harbor instance,
/// reducing configuration duplication when accessing multiple projects
/// from the same Harbor server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamProjectConfig {
    /// Project/registry name in Harbor (e.g., "library", "team-a")
    pub name: String,
    /// Pattern to match repository paths for this project (supports glob patterns)
    /// If not specified, defaults to "{project_name}/*"
    #[serde(default)]
    pub pattern: Option<String>,
    /// Priority for this project route (lower = higher priority)
    #[serde(default = "default_priority")]
    pub priority: i32,
    /// Whether this is the default project for this upstream
    #[serde(default)]
    pub is_default: bool,
}

impl UpstreamProjectConfig {
    /// Get the effective pattern for this project
    pub fn effective_pattern(&self) -> String {
        self.pattern
            .clone()
            .unwrap_or_else(|| format!("{}/*", self.name))
    }
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
    /// Registry/project name (legacy single-project mode)
    /// Used when `projects` is empty for backward compatibility
    #[serde(default = "default_registry")]
    pub registry: String,
    /// Multiple projects configuration (new multi-project mode)
    /// When non-empty, takes precedence over `registry`
    #[serde(default)]
    pub projects: Vec<UpstreamProjectConfig>,
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

    /// Check if this upstream uses multi-project mode
    pub fn uses_multi_project(&self) -> bool {
        !self.projects.is_empty()
    }

    /// Get all effective project names for this upstream
    pub fn get_project_names(&self) -> Vec<&str> {
        if self.projects.is_empty() {
            vec![&self.registry]
        } else {
            self.projects.iter().map(|p| p.name.as_str()).collect()
        }
    }

    /// Get the default project for this upstream
    /// Returns the first project marked as default, or the first project,
    /// or falls back to the `registry` field
    pub fn get_default_project(&self) -> &str {
        if self.projects.is_empty() {
            &self.registry
        } else {
            self.projects
                .iter()
                .find(|p| p.is_default)
                .or_else(|| self.projects.first())
                .map(|p| p.name.as_str())
                .unwrap_or(&self.registry)
        }
    }

    /// Find the matching project for a given repository path
    /// Returns the project name if a match is found
    pub fn find_matching_project(&self, repository: &str) -> Option<&str> {
        if self.projects.is_empty() {
            // Legacy mode: always use the registry field
            return Some(&self.registry);
        }

        // Sort by priority and find the first matching project
        let mut projects: Vec<_> = self.projects.iter().collect();
        projects.sort_by_key(|p| p.priority);

        for project in projects {
            let pattern = project.effective_pattern();
            if matches_glob_pattern(&pattern, repository) {
                return Some(&project.name);
            }
        }

        // If no pattern matches, check for a default project
        self.projects
            .iter()
            .find(|p| p.is_default)
            .map(|p| p.name.as_str())
    }
}

/// Simple glob pattern matching for project routing
/// Supports * (single segment) and ** (multi-segment) wildcards
fn matches_glob_pattern(pattern: &str, path: &str) -> bool {
    let parts = compile_pattern(pattern);
    match_pattern(&parts, path, 0, 0)
}

#[derive(Debug, Clone)]
enum PatternPart {
    Literal(String),
    SingleWildcard,
    MultiWildcard,
}

fn compile_pattern(pattern: &str) -> Vec<PatternPart> {
    let mut parts = Vec::new();
    let mut current = String::new();

    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        if ch == '*' {
            if !current.is_empty() {
                parts.push(PatternPart::Literal(current.clone()));
                current.clear();
            }

            if i + 1 < chars.len() && chars[i + 1] == '*' {
                parts.push(PatternPart::MultiWildcard);
                i += 2;
            } else {
                parts.push(PatternPart::SingleWildcard);
                i += 1;
            }
        } else {
            current.push(ch);
            i += 1;
        }
    }

    if !current.is_empty() {
        parts.push(PatternPart::Literal(current));
    }

    parts
}

fn match_pattern(parts: &[PatternPart], path: &str, part_idx: usize, path_pos: usize) -> bool {
    if part_idx >= parts.len() {
        return path_pos >= path.len();
    }

    let path_remaining = &path[path_pos..];

    match &parts[part_idx] {
        PatternPart::Literal(lit) => {
            if path_remaining.starts_with(lit) {
                match_pattern(parts, path, part_idx + 1, path_pos + lit.len())
            } else {
                false
            }
        }
        PatternPart::SingleWildcard => {
            if let Some(slash_pos) = path_remaining.find('/') {
                match_pattern(parts, path, part_idx + 1, path_pos + slash_pos)
            } else {
                match_pattern(parts, path, part_idx + 1, path.len())
            }
        }
        PatternPart::MultiWildcard => {
            let remaining_parts = &parts[part_idx + 1..];

            if remaining_parts.is_empty() {
                return true;
            }

            for i in 0..=path_remaining.len() {
                if match_pattern(parts, path, part_idx + 1, path_pos + i) {
                    return true;
                }
            }
            false
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_project(name: &str, pattern: Option<&str>, priority: i32, is_default: bool) -> UpstreamProjectConfig {
        UpstreamProjectConfig {
            name: name.to_string(),
            pattern: pattern.map(|p| p.to_string()),
            priority,
            is_default,
        }
    }

    fn create_test_upstream(projects: Vec<UpstreamProjectConfig>) -> UpstreamConfig {
        UpstreamConfig {
            name: "test".to_string(),
            display_name: None,
            url: "http://localhost:8880".to_string(),
            registry: "library".to_string(),
            projects,
            username: None,
            password: None,
            skip_tls_verify: false,
            priority: 100,
            enabled: true,
            cache_isolation: "shared".to_string(),
            is_default: true,
            routes: vec![],
        }
    }

    #[test]
    fn test_uses_multi_project() {
        let upstream_single = create_test_upstream(vec![]);
        assert!(!upstream_single.uses_multi_project());

        let upstream_multi = create_test_upstream(vec![
            create_test_project("library", None, 100, true),
        ]);
        assert!(upstream_multi.uses_multi_project());
    }

    #[test]
    fn test_get_project_names() {
        // Single-project mode
        let upstream_single = create_test_upstream(vec![]);
        assert_eq!(upstream_single.get_project_names(), vec!["library"]);

        // Multi-project mode
        let upstream_multi = create_test_upstream(vec![
            create_test_project("library", None, 100, true),
            create_test_project("team-a", None, 50, false),
            create_test_project("team-b", None, 50, false),
        ]);
        assert_eq!(upstream_multi.get_project_names(), vec!["library", "team-a", "team-b"]);
    }

    #[test]
    fn test_get_default_project() {
        // Single-project mode: uses registry
        let upstream_single = create_test_upstream(vec![]);
        assert_eq!(upstream_single.get_default_project(), "library");

        // Multi-project mode: uses is_default
        let upstream_multi = create_test_upstream(vec![
            create_test_project("team-a", None, 50, false),
            create_test_project("library", None, 100, true),
        ]);
        assert_eq!(upstream_multi.get_default_project(), "library");

        // Multi-project mode: falls back to first if no default
        let upstream_no_default = create_test_upstream(vec![
            create_test_project("team-a", None, 50, false),
            create_test_project("team-b", None, 50, false),
        ]);
        assert_eq!(upstream_no_default.get_default_project(), "team-a");
    }

    #[test]
    fn test_effective_pattern() {
        let project_with_pattern = create_test_project("library", Some("lib/*"), 100, true);
        assert_eq!(project_with_pattern.effective_pattern(), "lib/*");

        let project_without_pattern = create_test_project("team-a", None, 50, false);
        assert_eq!(project_without_pattern.effective_pattern(), "team-a/*");
    }

    #[test]
    fn test_find_matching_project_single_mode() {
        let upstream = create_test_upstream(vec![]);

        // In single-project mode, always returns the registry
        assert_eq!(upstream.find_matching_project("library/alpine"), Some("library"));
        assert_eq!(upstream.find_matching_project("team-a/myimage"), Some("library"));
    }

    #[test]
    fn test_find_matching_project_multi_mode() {
        let upstream = create_test_upstream(vec![
            create_test_project("library", None, 100, true),
            create_test_project("team-a", None, 50, false),
            create_test_project("team-b", None, 50, false),
        ]);

        // Matches the correct project based on path
        assert_eq!(upstream.find_matching_project("library/alpine"), Some("library"));
        assert_eq!(upstream.find_matching_project("team-a/myimage"), Some("team-a"));
        assert_eq!(upstream.find_matching_project("team-b/nginx"), Some("team-b"));

        // Falls back to default project for unmatched paths
        assert_eq!(upstream.find_matching_project("unknown/image"), Some("library"));
    }

    #[test]
    fn test_find_matching_project_priority() {
        // Create upstream with overlapping patterns
        let upstream = create_test_upstream(vec![
            create_test_project("catch-all", Some("**"), 100, false),
            create_test_project("team-a", None, 50, true),
        ]);

        // Higher priority (lower number) should match first
        assert_eq!(upstream.find_matching_project("team-a/image"), Some("team-a"));
    }

    #[test]
    fn test_glob_pattern_matching() {
        // Test basic patterns
        assert!(matches_glob_pattern("library/*", "library/alpine"));
        assert!(matches_glob_pattern("library/*", "library/nginx"));
        assert!(!matches_glob_pattern("library/*", "team-a/alpine"));

        // Test multi-segment wildcards
        assert!(matches_glob_pattern("library/**", "library/alpine"));
        assert!(matches_glob_pattern("library/**", "library/nested/path/image"));

        // Test exact match
        assert!(matches_glob_pattern("library/alpine", "library/alpine"));
        assert!(!matches_glob_pattern("library/alpine", "library/nginx"));

        // Test pattern with multiple wildcards
        assert!(matches_glob_pattern("*/alpine", "library/alpine"));
        assert!(matches_glob_pattern("*/alpine", "team-a/alpine"));
    }
}
