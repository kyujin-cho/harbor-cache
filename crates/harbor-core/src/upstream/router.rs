//! Route matching for multi-upstream support
//!
//! Provides pattern-based routing to determine which upstream
//! should handle a given repository path.

use harbor_db::UpstreamRoute;

/// Result of a route match
#[derive(Debug, Clone)]
pub struct RouteMatch {
    /// The upstream ID that matched
    pub upstream_id: i64,
    /// The pattern that matched
    pub pattern: String,
    /// The priority of the match
    pub priority: i32,
}

/// Maximum iterations allowed for pattern matching to prevent ReDoS
const MAX_MATCH_ITERATIONS: usize = 10000;

/// Route matcher for upstream selection
pub struct RouteMatcher {
    routes: Vec<CompiledRoute>,
}

#[derive(Debug, Clone)]
struct CompiledRoute {
    upstream_id: i64,
    pattern: String,
    priority: i32,
    /// Pre-compiled pattern parts for matching
    parts: Vec<PatternPart>,
}

#[derive(Debug, Clone)]
enum PatternPart {
    /// Literal text that must match exactly
    Literal(String),
    /// Single path segment wildcard (*)
    SingleWildcard,
    /// Multi-segment wildcard (**)
    MultiWildcard,
}

impl RouteMatcher {
    /// Create a new route matcher from a list of routes
    pub fn new(routes: Vec<UpstreamRoute>) -> Self {
        let mut compiled: Vec<CompiledRoute> = routes
            .into_iter()
            .map(|r| CompiledRoute {
                upstream_id: r.upstream_id,
                pattern: r.pattern.clone(),
                priority: r.priority,
                parts: Self::compile_pattern(&r.pattern),
            })
            .collect();

        // Sort by priority (lower = higher priority)
        compiled.sort_by_key(|r| r.priority);

        Self { routes: compiled }
    }

    /// Compile a glob-like pattern into parts
    fn compile_pattern(pattern: &str) -> Vec<PatternPart> {
        let mut parts = Vec::new();
        let mut current = String::new();

        let chars: Vec<char> = pattern.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let ch = chars[i];

            if ch == '*' {
                // Flush current literal
                if !current.is_empty() {
                    parts.push(PatternPart::Literal(current.clone()));
                    current.clear();
                }

                // Check for **
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

        // Flush remaining literal
        if !current.is_empty() {
            parts.push(PatternPart::Literal(current));
        }

        parts
    }

    /// Find the best matching route for a repository path
    pub fn find_match(&self, repository: &str) -> Option<RouteMatch> {
        for route in &self.routes {
            if Self::matches_pattern(&route.parts, repository) {
                return Some(RouteMatch {
                    upstream_id: route.upstream_id,
                    pattern: route.pattern.clone(),
                    priority: route.priority,
                });
            }
        }
        None
    }

    /// Check if a pattern matches a repository path
    fn matches_pattern(parts: &[PatternPart], path: &str) -> bool {
        let mut iterations = 0;
        Self::match_recursive(parts, path, 0, 0, &mut iterations)
    }

    fn match_recursive(
        parts: &[PatternPart],
        path: &str,
        part_idx: usize,
        path_pos: usize,
        iterations: &mut usize,
    ) -> bool {
        // Prevent ReDoS by limiting iterations
        *iterations += 1;
        if *iterations > MAX_MATCH_ITERATIONS {
            tracing::warn!(
                "Pattern matching exceeded {} iterations, aborting",
                MAX_MATCH_ITERATIONS
            );
            return false;
        }

        // Base cases
        if part_idx >= parts.len() {
            // Pattern exhausted, check if path is also exhausted
            return path_pos >= path.len();
        }

        let path_remaining = &path[path_pos..];

        match &parts[part_idx] {
            PatternPart::Literal(lit) => {
                if path_remaining.starts_with(lit) {
                    Self::match_recursive(parts, path, part_idx + 1, path_pos + lit.len(), iterations)
                } else {
                    false
                }
            }
            PatternPart::SingleWildcard => {
                // Match any characters until the next '/' or end
                if let Some(slash_pos) = path_remaining.find('/') {
                    // There's a slash, wildcard matches up to it
                    Self::match_recursive(parts, path, part_idx + 1, path_pos + slash_pos, iterations)
                } else {
                    // No slash, wildcard matches to end
                    Self::match_recursive(parts, path, part_idx + 1, path.len(), iterations)
                }
            }
            PatternPart::MultiWildcard => {
                // ** matches zero or more path segments
                // Try matching empty string, then progressively more
                let remaining_parts = &parts[part_idx + 1..];

                if remaining_parts.is_empty() {
                    // ** at end matches everything
                    return true;
                }

                // Try matching at each position
                for i in 0..=path_remaining.len() {
                    if Self::match_recursive(parts, path, part_idx + 1, path_pos + i, iterations) {
                        return true;
                    }
                }
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_route(upstream_id: i64, pattern: &str, priority: i32) -> UpstreamRoute {
        UpstreamRoute {
            id: 0,
            upstream_id,
            pattern: pattern.to_string(),
            priority,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_exact_match() {
        let matcher = RouteMatcher::new(vec![
            make_route(1, "library/nginx", 100),
        ]);

        let result = matcher.find_match("library/nginx");
        assert!(result.is_some());
        assert_eq!(result.unwrap().upstream_id, 1);

        assert!(matcher.find_match("library/alpine").is_none());
    }

    #[test]
    fn test_single_wildcard() {
        let matcher = RouteMatcher::new(vec![
            make_route(1, "library/*", 100),
        ]);

        let result = matcher.find_match("library/nginx");
        assert!(result.is_some());
        assert_eq!(result.unwrap().upstream_id, 1);

        let result = matcher.find_match("library/alpine");
        assert!(result.is_some());

        // Should not match nested paths
        assert!(matcher.find_match("other/nginx").is_none());
    }

    #[test]
    fn test_multi_wildcard() {
        let matcher = RouteMatcher::new(vec![
            make_route(1, "team-a/**", 100),
        ]);

        let result = matcher.find_match("team-a/project/image");
        assert!(result.is_some());
        assert_eq!(result.unwrap().upstream_id, 1);

        let result = matcher.find_match("team-a/image");
        assert!(result.is_some());

        assert!(matcher.find_match("team-b/image").is_none());
    }

    #[test]
    fn test_priority_ordering() {
        let matcher = RouteMatcher::new(vec![
            make_route(1, "library/*", 100),
            make_route(2, "library/nginx", 50), // Higher priority (lower number)
        ]);

        // Specific match should win due to lower priority number
        let result = matcher.find_match("library/nginx");
        assert!(result.is_some());
        assert_eq!(result.unwrap().upstream_id, 2);

        // Generic match still works for other paths
        let result = matcher.find_match("library/alpine");
        assert!(result.is_some());
        assert_eq!(result.unwrap().upstream_id, 1);
    }
}
