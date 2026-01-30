//! Shared configuration types for upstream management
//!
//! These types are shared across crates to avoid circular dependencies.
//! The main config loading is done in harbor-cache, but these types
//! define the upstream configuration structure used by harbor-core.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ==================== Validation Constants ====================

/// Maximum length for project name
const MAX_PROJECT_NAME_LENGTH: usize = 256;
/// Maximum length for pattern
const MAX_PATTERN_LENGTH_VALIDATION: usize = 512;
/// Maximum number of projects per upstream to prevent memory exhaustion
pub const MAX_PROJECTS_PER_UPSTREAM: usize = 100;

// ==================== Validation Functions ====================

/// Validate a project name for security
/// Returns Ok(()) if valid, Err with message if invalid
pub fn validate_project_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Project name cannot be empty".to_string());
    }

    if name.len() > MAX_PROJECT_NAME_LENGTH {
        return Err(format!(
            "Project name exceeds maximum length of {} characters",
            MAX_PROJECT_NAME_LENGTH
        ));
    }

    // Block path traversal attempts
    if name.contains("..") {
        return Err("Project name cannot contain path traversal sequences (..)".to_string());
    }

    // Block null bytes which could cause issues in file paths
    if name.contains('\0') {
        return Err("Project name cannot contain null bytes".to_string());
    }

    // Only allow safe characters: alphanumeric, dash, underscore, dot, forward slash
    // Forward slash is allowed for nested projects like "team-a/subproject"
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/')
    {
        return Err(
            "Project name must contain only alphanumeric characters, dashes, underscores, dots, and forward slashes"
                .to_string(),
        );
    }

    // Must start with alphanumeric
    if let Some(first) = name.chars().next() {
        if !first.is_ascii_alphanumeric() {
            return Err("Project name must start with an alphanumeric character".to_string());
        }
    }

    Ok(())
}

/// Validate a pattern for security
/// Returns Ok(()) if valid, Err with message if invalid
pub fn validate_pattern(pattern: &str) -> Result<(), String> {
    if pattern.is_empty() {
        return Err("Pattern cannot be empty".to_string());
    }

    if pattern.len() > MAX_PATTERN_LENGTH_VALIDATION {
        return Err(format!(
            "Pattern exceeds maximum length of {} characters",
            MAX_PATTERN_LENGTH_VALIDATION
        ));
    }

    // Block path traversal attempts
    if pattern.contains("..") {
        return Err("Pattern cannot contain path traversal sequences (..)".to_string());
    }

    // Block null bytes
    if pattern.contains('\0') {
        return Err("Pattern cannot contain null bytes".to_string());
    }

    // Count wildcards to prevent ReDoS
    let wildcard_count = pattern.matches('*').count();
    if wildcard_count > 10 {
        return Err(format!(
            "Pattern contains {} wildcards, maximum allowed is 10",
            wildcard_count
        ));
    }

    Ok(())
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

impl UpstreamRouteConfig {
    /// Validate this route configuration
    pub fn validate(&self) -> Result<(), String> {
        validate_pattern(&self.pattern)
    }
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

    /// Validate this project configuration
    pub fn validate(&self) -> Result<(), String> {
        validate_project_name(&self.name)?;
        if let Some(ref pattern) = self.pattern {
            validate_pattern(pattern)?;
        }
        Ok(())
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

    /// Validate this upstream configuration
    /// Returns Ok(()) if valid, Err with message if invalid
    pub fn validate(&self) -> Result<(), String> {
        // Validate project count to prevent memory exhaustion
        if self.projects.len() > MAX_PROJECTS_PER_UPSTREAM {
            return Err(format!(
                "Upstream '{}' has {} projects, exceeding maximum of {}",
                self.name,
                self.projects.len(),
                MAX_PROJECTS_PER_UPSTREAM
            ));
        }

        // Validate each project
        for (idx, project) in self.projects.iter().enumerate() {
            if let Err(e) = project.validate() {
                return Err(format!(
                    "Upstream '{}' project #{} ('{}'): {}",
                    self.name, idx, project.name, e
                ));
            }
        }

        // Validate each route
        for (idx, route) in self.routes.iter().enumerate() {
            if let Err(e) = route.validate() {
                return Err(format!(
                    "Upstream '{}' route #{}: {}",
                    self.name, idx, e
                ));
            }
        }

        // Validate registry name if using single-project mode
        if self.projects.is_empty() {
            if let Err(e) = validate_project_name(&self.registry) {
                return Err(format!(
                    "Upstream '{}' registry name invalid: {}",
                    self.name, e
                ));
            }
        }

        Ok(())
    }
}

/// Maximum number of wildcards allowed in a pattern to prevent ReDoS
const MAX_WILDCARDS: usize = 10;
/// Maximum pattern length to prevent excessive memory usage
const MAX_PATTERN_LENGTH: usize = 512;
/// Maximum path length to prevent excessive matching time
const MAX_PATH_LENGTH: usize = 1024;
/// Maximum recursion depth to prevent stack overflow
const MAX_RECURSION_DEPTH: usize = 100;

/// Simple glob pattern matching for project routing
/// Supports * (single segment) and ** (multi-segment) wildcards
///
/// Security: This function includes protections against ReDoS attacks:
/// - Pattern length limit
/// - Wildcard count limit
/// - Path length limit
/// - Recursion depth limit
fn matches_glob_pattern(pattern: &str, path: &str) -> bool {
    // Security: Limit pattern length to prevent excessive memory usage
    if pattern.len() > MAX_PATTERN_LENGTH {
        tracing::warn!(
            "Pattern exceeds maximum length of {} characters, rejecting",
            MAX_PATTERN_LENGTH
        );
        return false;
    }

    // Security: Limit path length to prevent excessive matching time
    if path.len() > MAX_PATH_LENGTH {
        tracing::warn!(
            "Path exceeds maximum length of {} characters, rejecting",
            MAX_PATH_LENGTH
        );
        return false;
    }

    let parts = compile_pattern(pattern);

    // Security: Limit number of wildcards to prevent exponential matching
    let wildcard_count = parts
        .iter()
        .filter(|p| matches!(p, PatternPart::SingleWildcard | PatternPart::MultiWildcard))
        .count();
    if wildcard_count > MAX_WILDCARDS {
        tracing::warn!(
            "Pattern contains {} wildcards, exceeding maximum of {}",
            wildcard_count,
            MAX_WILDCARDS
        );
        return false;
    }

    match_pattern(&parts, path, 0, 0, 0)
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
                // Coalesce consecutive ** into a single MultiWildcard
                parts.push(PatternPart::MultiWildcard);
                i += 2;
                // Skip any additional consecutive *
                while i < chars.len() && chars[i] == '*' {
                    i += 1;
                }
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

    // Optimization: Merge consecutive wildcards to reduce complexity
    merge_consecutive_wildcards(parts)
}

/// Merge consecutive wildcards to reduce matching complexity
/// Multiple consecutive ** are equivalent to a single **
fn merge_consecutive_wildcards(parts: Vec<PatternPart>) -> Vec<PatternPart> {
    let mut result = Vec::with_capacity(parts.len());
    let mut prev_was_multi = false;

    for part in parts {
        match &part {
            PatternPart::MultiWildcard => {
                if !prev_was_multi {
                    result.push(part);
                    prev_was_multi = true;
                }
                // Skip consecutive MultiWildcards
            }
            _ => {
                result.push(part);
                prev_was_multi = false;
            }
        }
    }

    result
}

fn match_pattern(
    parts: &[PatternPart],
    path: &str,
    part_idx: usize,
    path_pos: usize,
    depth: usize,
) -> bool {
    // Security: Prevent stack overflow with deep recursion
    if depth > MAX_RECURSION_DEPTH {
        tracing::warn!("Pattern matching exceeded maximum recursion depth");
        return false;
    }

    if part_idx >= parts.len() {
        return path_pos >= path.len();
    }

    let path_remaining = &path[path_pos..];

    match &parts[part_idx] {
        PatternPart::Literal(lit) => {
            if path_remaining.starts_with(lit) {
                match_pattern(parts, path, part_idx + 1, path_pos + lit.len(), depth + 1)
            } else {
                false
            }
        }
        PatternPart::SingleWildcard => {
            if let Some(slash_pos) = path_remaining.find('/') {
                match_pattern(parts, path, part_idx + 1, path_pos + slash_pos, depth + 1)
            } else {
                match_pattern(parts, path, part_idx + 1, path.len(), depth + 1)
            }
        }
        PatternPart::MultiWildcard => {
            let remaining_parts = &parts[part_idx + 1..];

            if remaining_parts.is_empty() {
                return true;
            }

            // Optimization: If next part is a literal, find it directly instead of
            // trying all positions (reduces O(n^m) to O(n*m) for common cases)
            if let Some(PatternPart::Literal(next_lit)) = remaining_parts.first() {
                // Find all occurrences of the literal and try from there
                let mut search_start = 0;
                while search_start <= path_remaining.len() {
                    if let Some(pos) = path_remaining[search_start..].find(next_lit.as_str()) {
                        let abs_pos = search_start + pos;
                        if match_pattern(parts, path, part_idx + 1, path_pos + abs_pos, depth + 1) {
                            return true;
                        }
                        // Move past the found position to search for next occurrence
                        search_start = abs_pos + 1;
                    } else {
                        break;
                    }
                }
                false
            } else {
                // Fallback to character-by-character matching for non-literal following parts
                // Limit iterations to prevent excessive CPU usage
                let max_iterations = path_remaining.len().min(MAX_PATH_LENGTH);
                for i in 0..=max_iterations {
                    if match_pattern(parts, path, part_idx + 1, path_pos + i, depth + 1) {
                        return true;
                    }
                }
                false
            }
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

    // ==================== Security Validation Tests ====================

    #[test]
    fn test_validate_project_name_valid() {
        assert!(validate_project_name("library").is_ok());
        assert!(validate_project_name("team-a").is_ok());
        assert!(validate_project_name("my_project").is_ok());
        assert!(validate_project_name("project.v1").is_ok());
        assert!(validate_project_name("team/subproject").is_ok());
    }

    #[test]
    fn test_validate_project_name_empty() {
        assert!(validate_project_name("").is_err());
    }

    #[test]
    fn test_validate_project_name_path_traversal() {
        assert!(validate_project_name("..").is_err());
        assert!(validate_project_name("../admin").is_err());
        assert!(validate_project_name("project/../secret").is_err());
    }

    #[test]
    fn test_validate_project_name_null_byte() {
        assert!(validate_project_name("project\0name").is_err());
    }

    #[test]
    fn test_validate_project_name_invalid_chars() {
        assert!(validate_project_name("project<script>").is_err());
        assert!(validate_project_name("project;rm -rf").is_err());
        assert!(validate_project_name("project|cat").is_err());
    }

    #[test]
    fn test_validate_project_name_must_start_alphanumeric() {
        assert!(validate_project_name("-project").is_err());
        assert!(validate_project_name("_project").is_err());
        assert!(validate_project_name(".project").is_err());
    }

    #[test]
    fn test_validate_pattern_valid() {
        assert!(validate_pattern("library/*").is_ok());
        assert!(validate_pattern("team-a/**").is_ok());
        assert!(validate_pattern("*/alpine").is_ok());
    }

    #[test]
    fn test_validate_pattern_empty() {
        assert!(validate_pattern("").is_err());
    }

    #[test]
    fn test_validate_pattern_path_traversal() {
        assert!(validate_pattern("../admin/*").is_err());
        assert!(validate_pattern("project/../secret/*").is_err());
    }

    #[test]
    fn test_validate_pattern_too_many_wildcards() {
        // 12 wildcards should fail (max is 10)
        let pattern = "a*b*c*d*e*f*g*h*i*j*k*l";
        assert!(validate_pattern(pattern).is_err());
    }

    #[test]
    fn test_validate_pattern_acceptable_wildcards() {
        // 10 wildcards should pass
        let pattern = "a*b*c*d*e*f*g*h*i*j*";
        assert!(validate_pattern(pattern).is_ok());
    }

    #[test]
    fn test_upstream_validate_too_many_projects() {
        let mut projects = Vec::new();
        for i in 0..=MAX_PROJECTS_PER_UPSTREAM {
            projects.push(create_test_project(&format!("project{}", i), None, 100, false));
        }
        let upstream = create_test_upstream(projects);
        assert!(upstream.validate().is_err());
    }

    #[test]
    fn test_upstream_validate_invalid_project() {
        let upstream = create_test_upstream(vec![
            create_test_project("valid-project", None, 100, true),
            create_test_project("../invalid", None, 50, false),
        ]);
        assert!(upstream.validate().is_err());
    }

    #[test]
    fn test_upstream_validate_valid() {
        let upstream = create_test_upstream(vec![
            create_test_project("library", None, 100, true),
            create_test_project("team-a", Some("team-a/*"), 50, false),
        ]);
        assert!(upstream.validate().is_ok());
    }

    // ==================== ReDoS Protection Tests ====================

    #[test]
    fn test_pattern_matching_rejects_long_pattern() {
        let long_pattern = "a".repeat(MAX_PATTERN_LENGTH + 1);
        // Should return false (reject) for patterns exceeding max length
        assert!(!matches_glob_pattern(&long_pattern, "test"));
    }

    #[test]
    fn test_pattern_matching_rejects_long_path() {
        let long_path = "a".repeat(MAX_PATH_LENGTH + 1);
        // Should return false (reject) for paths exceeding max length
        assert!(!matches_glob_pattern("**", &long_path));
    }

    #[test]
    fn test_pattern_matching_handles_many_wildcards_gracefully() {
        // This would cause exponential time without our protections
        // With protections, it should complete quickly and return false
        let pattern = "**a**b**c**d**e**";
        let path = "xyzxyzxyzxyzxyzxyz";
        // Should complete without hanging (the pattern won't match anyway)
        let _result = matches_glob_pattern(pattern, path);
        // Test passes if we get here without timeout
    }

    #[test]
    fn test_consecutive_wildcards_merged() {
        // ****** should be treated as single **
        assert!(matches_glob_pattern("library/******", "library/nested/deep/path"));
    }
}
