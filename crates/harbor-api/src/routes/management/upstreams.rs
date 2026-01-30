//! Upstream management routes
//!
//! These endpoints manage upstream Harbor registries through the TOML config file.
//! Changes are persisted to the config file and reloaded at runtime.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
};
use harbor_core::{
    validate_pattern, validate_project_name, UpstreamConfig, UpstreamProjectConfig,
    UpstreamRouteConfig, MAX_PROJECTS_PER_UPSTREAM,
};
use harbor_proxy::{HarborClient, HarborClientConfig};
use std::net::{IpAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, info, warn};
use url::Url;

use crate::error::ApiError;
use crate::state::AppState;

use super::auth::RequireAdmin;
use super::types::{
    CreateUpstreamRequest, TestUpstreamRequest, TestUpstreamResponse, UpdateUpstreamRequest,
    UpstreamHealthResponse, UpstreamResponse, UpstreamRouteResponse,
};

// ==================== Rate Limiting ====================

/// Simple rate limiter for reload operations
/// Allows at most one reload per RELOAD_COOLDOWN_SECS seconds
static LAST_RELOAD_TIME: AtomicU64 = AtomicU64::new(0);
const RELOAD_COOLDOWN_SECS: u64 = 5;

// ==================== Input Validation ====================

/// Maximum length for upstream name
const MAX_NAME_LENGTH: usize = 64;
/// Minimum length for upstream name
const MIN_NAME_LENGTH: usize = 1;
/// Maximum length for display name
const MAX_DISPLAY_NAME_LENGTH: usize = 128;
/// Maximum length for URL
const MAX_URL_LENGTH: usize = 2048;
/// Maximum length for registry name
const MAX_REGISTRY_LENGTH: usize = 256;
/// Maximum length for route pattern
const MAX_PATTERN_LENGTH: usize = 512;
/// Maximum number of wildcards in a pattern
const MAX_WILDCARDS_IN_PATTERN: usize = 10;

/// Validate upstream URL to prevent SSRF attacks.
/// Only allows HTTP/HTTPS URLs to external hosts.
fn validate_upstream_url(url_str: &str) -> Result<(), ApiError> {
    // Check length first
    if url_str.len() > MAX_URL_LENGTH {
        return Err(ApiError::BadRequest(format!(
            "URL exceeds maximum length of {} characters",
            MAX_URL_LENGTH
        )));
    }

    // Parse the URL
    let url = Url::parse(url_str)
        .map_err(|e| ApiError::BadRequest(format!("Invalid URL format: {}", e)))?;

    // Only allow HTTP and HTTPS schemes
    match url.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(ApiError::BadRequest(format!(
                "URL scheme '{}' is not allowed. Only http and https are permitted",
                scheme
            )));
        }
    }

    // Get the host
    let host = url
        .host_str()
        .ok_or_else(|| ApiError::BadRequest("URL must have a host".to_string()))?;

    // Block localhost and loopback addresses
    if host == "localhost" || host == "127.0.0.1" || host == "::1" {
        return Err(ApiError::BadRequest(
            "Localhost URLs are not allowed for security reasons".to_string(),
        ));
    }

    // Block common internal hostnames
    let lower_host = host.to_lowercase();
    if lower_host == "metadata"
        || lower_host == "metadata.google.internal"
        || lower_host.ends_with(".internal")
        || lower_host.ends_with(".local")
    {
        return Err(ApiError::BadRequest(
            "Internal hostnames are not allowed for security reasons".to_string(),
        ));
    }

    // Try to parse as IP address and block private/internal ranges
    if let Ok(ip) = host.parse::<IpAddr>()
        && is_private_or_reserved_ip(&ip)
    {
        return Err(ApiError::BadRequest(
            "Private or reserved IP addresses are not allowed for security reasons".to_string(),
        ));
    }

    Ok(())
}

/// Validate upstream URL with DNS resolution to prevent DNS rebinding attacks.
/// This performs actual DNS resolution to verify the hostname doesn't resolve to internal IPs.
async fn validate_upstream_url_with_dns(url_str: &str) -> Result<(), ApiError> {
    // First, perform basic validation
    validate_upstream_url(url_str)?;

    // Parse URL to get host and port
    let url =
        Url::parse(url_str).map_err(|e| ApiError::BadRequest(format!("Invalid URL: {}", e)))?;

    let host = url
        .host_str()
        .ok_or_else(|| ApiError::BadRequest("URL must have a host".to_string()))?;

    // If it's already an IP, we already validated it above
    if host.parse::<IpAddr>().is_ok() {
        return Ok(());
    }

    // Resolve the hostname to IP addresses
    let port = url
        .port()
        .unwrap_or(if url.scheme() == "https" { 443 } else { 80 });
    let addr_str = format!("{}:{}", host, port);

    // Use spawn_blocking for DNS resolution to avoid blocking the async runtime
    let resolved = tokio::task::spawn_blocking(move || {
        addr_str
            .to_socket_addrs()
            .map(|addrs| addrs.collect::<Vec<_>>())
    })
    .await
    .map_err(|e| ApiError::Internal(format!("DNS resolution task failed: {}", e)))?
    .map_err(|e| ApiError::BadRequest(format!("Failed to resolve hostname '{}': {}", host, e)))?;

    if resolved.is_empty() {
        return Err(ApiError::BadRequest(format!(
            "Hostname '{}' did not resolve to any IP addresses",
            host
        )));
    }

    // Check all resolved IPs - reject if ANY resolve to private/reserved ranges
    for addr in &resolved {
        if is_private_or_reserved_ip(&addr.ip()) {
            warn!(
                "DNS rebinding protection: hostname '{}' resolves to private IP {}",
                host,
                addr.ip()
            );
            return Err(ApiError::BadRequest(format!(
                "Hostname '{}' resolves to a private or reserved IP address, which is not allowed for security reasons",
                host
            )));
        }
    }

    Ok(())
}

/// Check if an IP address is private, loopback, or otherwise reserved
fn is_private_or_reserved_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            ipv4.is_loopback()                    // 127.0.0.0/8
                || ipv4.is_private()              // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                || ipv4.is_link_local()           // 169.254.0.0/16
                || ipv4.is_broadcast()            // 255.255.255.255
                || ipv4.is_unspecified()          // 0.0.0.0
                || ipv4.octets()[0] == 169        // Cloud metadata (169.254.169.254)
                || ipv4.is_documentation() // 192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24
        }
        IpAddr::V6(ipv6) => {
            ipv6.is_loopback()                    // ::1
                || ipv6.is_unspecified()          // ::
                // IPv4-mapped IPv6 addresses
                || (ipv6.segments()[0..6] == [0, 0, 0, 0, 0, 0xFFFF]
                    && is_private_or_reserved_ip(&IpAddr::V4(std::net::Ipv4Addr::new(
                        (ipv6.segments()[6] >> 8) as u8,
                        (ipv6.segments()[6] & 0xFF) as u8,
                        (ipv6.segments()[7] >> 8) as u8,
                        (ipv6.segments()[7] & 0xFF) as u8,
                    ))))
        }
    }
}

/// Validate upstream name format and length
fn validate_upstream_name(name: &str) -> Result<(), ApiError> {
    if name.len() < MIN_NAME_LENGTH {
        return Err(ApiError::BadRequest(
            "Upstream name cannot be empty".to_string(),
        ));
    }

    if name.len() > MAX_NAME_LENGTH {
        return Err(ApiError::BadRequest(format!(
            "Upstream name exceeds maximum length of {} characters",
            MAX_NAME_LENGTH
        )));
    }

    // Must contain only alphanumeric, dashes, and underscores
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ApiError::BadRequest(
            "Upstream name must contain only alphanumeric characters, dashes, and underscores"
                .to_string(),
        ));
    }

    // Must start with alphanumeric
    if let Some(first) = name.chars().next()
        && !first.is_ascii_alphanumeric()
    {
        return Err(ApiError::BadRequest(
            "Upstream name must start with an alphanumeric character".to_string(),
        ));
    }

    Ok(())
}

/// Validate display name format and length
fn validate_display_name(name: &str) -> Result<(), ApiError> {
    if name.is_empty() {
        return Err(ApiError::BadRequest(
            "Display name cannot be empty".to_string(),
        ));
    }

    if name.len() > MAX_DISPLAY_NAME_LENGTH {
        return Err(ApiError::BadRequest(format!(
            "Display name exceeds maximum length of {} characters",
            MAX_DISPLAY_NAME_LENGTH
        )));
    }

    // Block obvious script injection attempts
    let lower = name.to_lowercase();
    if lower.contains("<script") || lower.contains("javascript:") || lower.contains("data:") {
        return Err(ApiError::BadRequest(
            "Display name contains disallowed characters".to_string(),
        ));
    }

    Ok(())
}

/// Validate registry name
fn validate_registry_name(registry: &str) -> Result<(), ApiError> {
    if registry.is_empty() {
        return Err(ApiError::BadRequest(
            "Registry name cannot be empty".to_string(),
        ));
    }

    if registry.len() > MAX_REGISTRY_LENGTH {
        return Err(ApiError::BadRequest(format!(
            "Registry name exceeds maximum length of {} characters",
            MAX_REGISTRY_LENGTH
        )));
    }

    Ok(())
}

/// Validate route pattern to prevent ReDoS and other issues
fn validate_route_pattern(pattern: &str) -> Result<(), ApiError> {
    if pattern.is_empty() {
        return Err(ApiError::BadRequest(
            "Route pattern cannot be empty".to_string(),
        ));
    }

    if pattern.len() > MAX_PATTERN_LENGTH {
        return Err(ApiError::BadRequest(format!(
            "Route pattern exceeds maximum length of {} characters",
            MAX_PATTERN_LENGTH
        )));
    }

    // Count wildcards to prevent ReDoS
    let wildcard_count = pattern.matches('*').count();
    if wildcard_count > MAX_WILDCARDS_IN_PATTERN {
        return Err(ApiError::BadRequest(format!(
            "Route pattern contains too many wildcards (max {})",
            MAX_WILDCARDS_IN_PATTERN
        )));
    }

    // Block path traversal attempts
    if pattern.contains("..") {
        return Err(ApiError::BadRequest(
            "Route pattern cannot contain path traversal sequences".to_string(),
        ));
    }

    Ok(())
}

/// Validate projects array for update request
fn validate_projects(
    projects: &[super::types::UpdateUpstreamProjectRequest],
) -> Result<(), ApiError> {
    // Check project count limit
    if projects.len() > MAX_PROJECTS_PER_UPSTREAM {
        return Err(ApiError::BadRequest(format!(
            "Too many projects (max {})",
            MAX_PROJECTS_PER_UPSTREAM
        )));
    }

    // Check for duplicate project names
    let mut seen_names = std::collections::HashSet::new();
    for project in projects {
        if !seen_names.insert(&project.name) {
            return Err(ApiError::BadRequest(format!(
                "Duplicate project name: '{}'",
                project.name
            )));
        }
    }

    // Check for multiple default projects
    let default_count = projects.iter().filter(|p| p.is_default).count();
    if default_count > 1 {
        return Err(ApiError::BadRequest(
            "Only one project can be marked as default".to_string(),
        ));
    }

    // Validate each project
    for (idx, project) in projects.iter().enumerate() {
        // Validate project name using shared validation
        if let Err(e) = validate_project_name(&project.name) {
            return Err(ApiError::BadRequest(format!("Project #{}: {}", idx + 1, e)));
        }

        // Validate pattern if provided using shared validation
        if let Some(ref pattern) = project.pattern {
            if let Err(e) = validate_pattern(pattern) {
                return Err(ApiError::BadRequest(format!(
                    "Project '{}' pattern: {}",
                    project.name, e
                )));
            }
        }
    }

    Ok(())
}

// ==================== Helper Functions ====================

fn upstream_config_to_response(config: &UpstreamConfig, idx: usize) -> UpstreamResponse {
    let projects: Vec<super::types::UpstreamProjectResponse> = config
        .projects
        .iter()
        .map(|p| {
            let effective_pattern = p.pattern.clone().unwrap_or_else(|| format!("{}/*", p.name));
            super::types::UpstreamProjectResponse {
                name: p.name.clone(),
                pattern: p.pattern.clone(),
                effective_pattern,
                priority: p.priority,
                is_default: p.is_default,
            }
        })
        .collect();

    UpstreamResponse {
        id: idx as i64, // Use index as ID for compatibility
        name: config.name.clone(),
        display_name: config.display_name().to_string(),
        url: config.url.clone(),
        registry: config.registry.clone(),
        projects,
        uses_multi_project: config.uses_multi_project(),
        skip_tls_verify: config.skip_tls_verify,
        priority: config.priority,
        enabled: config.enabled,
        cache_isolation: config.cache_isolation.clone(),
        is_default: config.is_default,
        has_credentials: config.username.is_some(),
        created_at: chrono::Utc::now().to_rfc3339(), // Not tracked in config
        updated_at: chrono::Utc::now().to_rfc3339(), // Not tracked in config
    }
}

fn route_config_to_response(
    route: &UpstreamRouteConfig,
    _upstream_name: &str,
    idx: usize,
) -> UpstreamRouteResponse {
    UpstreamRouteResponse {
        id: idx as i64,
        upstream_id: 0, // Not used with config-based storage
        pattern: route.pattern.clone(),
        priority: route.priority,
        created_at: chrono::Utc::now().to_rfc3339(),
    }
}

// ==================== Upstream Routes ====================

/// GET /api/v1/upstreams (Admin only)
/// Returns all upstreams from the TOML config file
async fn list_upstreams(
    _admin: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<Vec<UpstreamResponse>>, ApiError> {
    let upstreams = state.config_provider.get_upstreams();

    Ok(Json(
        upstreams
            .iter()
            .enumerate()
            .map(|(idx, u)| upstream_config_to_response(u, idx))
            .collect(),
    ))
}

/// POST /api/v1/upstreams (Admin only)
/// Creates a new upstream and saves to TOML config file
async fn create_upstream(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Json(request): Json<CreateUpstreamRequest>,
) -> Result<(StatusCode, Json<UpstreamResponse>), ApiError> {
    debug!("Creating upstream: {}", request.name);

    // Validate all input fields
    validate_upstream_name(&request.name)?;
    validate_display_name(&request.display_name)?;
    // Use DNS-resolving validation to prevent DNS rebinding attacks
    validate_upstream_url_with_dns(&request.url).await?;
    validate_registry_name(&request.registry)?;

    // Validate routes if provided
    for route in &request.routes {
        validate_route_pattern(&route.pattern)?;
    }

    // Check for duplicate name
    if state
        .config_provider
        .get_upstream_by_name(&request.name)
        .is_some()
    {
        return Err(ApiError::BadRequest(format!(
            "Upstream with name '{}' already exists",
            request.name
        )));
    }

    // Create the upstream config
    let routes: Vec<UpstreamRouteConfig> = request
        .routes
        .iter()
        .map(|r| UpstreamRouteConfig {
            pattern: r.pattern.clone(),
            priority: r.priority,
        })
        .collect();

    let upstream_config = UpstreamConfig {
        name: request.name.clone(),
        display_name: Some(request.display_name),
        url: request.url,
        registry: request.registry,
        projects: vec![], // Projects managed via config file or separate API
        username: request.username,
        password: request.password,
        skip_tls_verify: request.skip_tls_verify,
        priority: request.priority,
        enabled: request.enabled,
        cache_isolation: request.cache_isolation,
        is_default: request.is_default,
        routes,
    };

    // Add to config and save
    state
        .config_provider
        .add_upstream(upstream_config.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Reload the upstream manager to pick up changes
    state
        .upstream_manager
        .reload()
        .map_err(|e| ApiError::Internal(format!("Failed to reload upstreams: {}", e)))?;

    info!("Created upstream: {}", request.name);

    let upstreams = state.config_provider.get_upstreams();
    let idx = upstreams.len().saturating_sub(1);

    Ok((
        StatusCode::CREATED,
        Json(upstream_config_to_response(&upstream_config, idx)),
    ))
}

/// GET /api/v1/upstreams/:name (Admin only)
/// Gets an upstream by name from the TOML config file
async fn get_upstream(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<UpstreamResponse>, ApiError> {
    let upstreams = state.config_provider.get_upstreams();

    let (idx, upstream) = upstreams
        .iter()
        .enumerate()
        .find(|(_, u)| u.name == name)
        .ok_or_else(|| ApiError::NotFound(format!("Upstream: {}", name)))?;

    Ok(Json(upstream_config_to_response(upstream, idx)))
}

/// PUT /api/v1/upstreams/:name (Admin only)
/// Updates an upstream and saves to TOML config file
async fn update_upstream(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(request): Json<UpdateUpstreamRequest>,
) -> Result<Json<UpstreamResponse>, ApiError> {
    debug!("Updating upstream: {}", name);

    // Validate updated fields if provided
    if let Some(ref display_name) = request.display_name {
        validate_display_name(display_name)?;
    }
    if let Some(ref url) = request.url {
        // Use DNS-resolving validation to prevent DNS rebinding attacks
        validate_upstream_url_with_dns(url).await?;
    }
    if let Some(ref registry) = request.registry {
        validate_registry_name(registry)?;
    }
    // Validate projects if provided
    if let Some(ref projects) = request.projects {
        validate_projects(projects)?;
    }

    // Get existing upstream
    let existing = state
        .config_provider
        .get_upstream_by_name(&name)
        .ok_or_else(|| ApiError::NotFound(format!("Upstream: {}", name)))?;

    // Convert projects from request to config format if provided
    let projects = if let Some(ref project_requests) = request.projects {
        project_requests
            .iter()
            .map(|p| UpstreamProjectConfig {
                name: p.name.clone(),
                pattern: p.pattern.clone(),
                priority: p.priority,
                is_default: p.is_default,
            })
            .collect()
    } else {
        existing.projects
    };

    // Build updated config
    let updated = UpstreamConfig {
        name: existing.name.clone(),
        display_name: request
            .display_name
            .or(existing.display_name.clone())
            .or(Some(existing.name.clone())),
        url: request.url.unwrap_or(existing.url),
        registry: request.registry.unwrap_or(existing.registry),
        projects,
        username: request.username.or(existing.username),
        password: request.password.or(existing.password),
        skip_tls_verify: request.skip_tls_verify.unwrap_or(existing.skip_tls_verify),
        priority: request.priority.unwrap_or(existing.priority),
        enabled: request.enabled.unwrap_or(existing.enabled),
        cache_isolation: request.cache_isolation.unwrap_or(existing.cache_isolation),
        is_default: request.is_default.unwrap_or(existing.is_default),
        routes: existing.routes, // Routes managed separately
    };

    // Update config and save
    state
        .config_provider
        .update_upstream(&name, updated.clone())
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Reload the upstream manager to pick up changes
    state
        .upstream_manager
        .reload()
        .map_err(|e| ApiError::Internal(format!("Failed to reload upstreams: {}", e)))?;

    info!("Updated upstream: {}", name);

    let upstreams = state.config_provider.get_upstreams();
    let idx = upstreams.iter().position(|u| u.name == name).unwrap_or(0);

    Ok(Json(upstream_config_to_response(&updated, idx)))
}

/// DELETE /api/v1/upstreams/:name (Admin only)
/// Deletes an upstream from the TOML config file
async fn delete_upstream(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    debug!("Deleting upstream: {}", name);

    // Remove from config and save
    state.config_provider.remove_upstream(&name).map_err(|e| {
        if e.to_string().contains("not found") {
            ApiError::NotFound(format!("Upstream: {}", name))
        } else {
            ApiError::Internal(e.to_string())
        }
    })?;

    // Reload the upstream manager to pick up changes
    state
        .upstream_manager
        .reload()
        .map_err(|e| ApiError::Internal(format!("Failed to reload upstreams: {}", e)))?;

    info!("Deleted upstream: {}", name);
    Ok(StatusCode::NO_CONTENT)
}

// ==================== Route Management ====================

/// GET /api/v1/upstreams/:name/routes (Admin only)
async fn list_upstream_routes(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Vec<UpstreamRouteResponse>>, ApiError> {
    let upstream = state
        .config_provider
        .get_upstream_by_name(&name)
        .ok_or_else(|| ApiError::NotFound(format!("Upstream: {}", name)))?;

    Ok(Json(
        upstream
            .routes
            .iter()
            .enumerate()
            .map(|(idx, r)| route_config_to_response(r, &name, idx))
            .collect(),
    ))
}

/// POST /api/v1/upstreams/:name/routes (Admin only)
async fn add_upstream_route(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(request): Json<super::types::CreateRouteRequest>,
) -> Result<(StatusCode, Json<UpstreamRouteResponse>), ApiError> {
    debug!("Adding route to upstream {}: {}", name, request.pattern);

    // Validate route pattern
    validate_route_pattern(&request.pattern)?;

    // Get existing upstream
    let mut upstream = state
        .config_provider
        .get_upstream_by_name(&name)
        .ok_or_else(|| ApiError::NotFound(format!("Upstream: {}", name)))?;

    // Add the new route
    let route = UpstreamRouteConfig {
        pattern: request.pattern.clone(),
        priority: request.priority,
    };
    upstream.routes.push(route.clone());

    // Update config and save
    state
        .config_provider
        .update_upstream(&name, upstream)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Reload the upstream manager
    state
        .upstream_manager
        .reload()
        .map_err(|e| ApiError::Internal(format!("Failed to reload upstreams: {}", e)))?;

    info!("Added route {} to upstream {}", request.pattern, name);

    let updated = state.config_provider.get_upstream_by_name(&name).unwrap();
    let idx = updated.routes.len().saturating_sub(1);

    Ok((
        StatusCode::CREATED,
        Json(route_config_to_response(&route, &name, idx)),
    ))
}

/// DELETE /api/v1/upstreams/:upstream_name/routes/:route_idx (Admin only)
async fn delete_upstream_route(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path((upstream_name, route_idx)): Path<(String, usize)>,
) -> Result<StatusCode, ApiError> {
    debug!(
        "Deleting route {} from upstream {}",
        route_idx, upstream_name
    );

    // Get existing upstream
    let mut upstream = state
        .config_provider
        .get_upstream_by_name(&upstream_name)
        .ok_or_else(|| ApiError::NotFound(format!("Upstream: {}", upstream_name)))?;

    // Check if route index is valid
    if route_idx >= upstream.routes.len() {
        return Err(ApiError::NotFound(format!("Route: {}", route_idx)));
    }

    // Remove the route
    upstream.routes.remove(route_idx);

    // Update config and save
    state
        .config_provider
        .update_upstream(&upstream_name, upstream)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Reload the upstream manager
    state
        .upstream_manager
        .reload()
        .map_err(|e| ApiError::Internal(format!("Failed to reload upstreams: {}", e)))?;

    info!(
        "Deleted route {} from upstream {}",
        route_idx, upstream_name
    );
    Ok(StatusCode::NO_CONTENT)
}

// ==================== Health & Testing ====================

/// GET /api/v1/upstreams/:name/health (Admin only)
///
/// Uses the cached HarborClient from UpstreamManager for connection pooling efficiency.
async fn get_upstream_health(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<UpstreamHealthResponse>, ApiError> {
    // Use UpstreamManager's cached client for better connection pooling
    let health = state
        .upstream_manager
        .check_upstream_health(&name)
        .await
        .map_err(|e| {
            if e.to_string().contains("not found") || e.to_string().contains("NotFound") {
                ApiError::NotFound(format!("Upstream: {}", name))
            } else {
                ApiError::Internal(e.to_string())
            }
        })?;

    Ok(Json(UpstreamHealthResponse {
        upstream_id: 0, // Not used with config-based storage
        name: health.name,
        healthy: health.healthy,
        last_check: health.last_check.to_rfc3339(),
        last_error: health.last_error,
        consecutive_failures: health.consecutive_failures,
    }))
}

/// GET /api/v1/upstreams/health (Admin only) - Get health for all upstreams
///
/// Uses UpstreamManager's cached clients for better connection pooling efficiency.
async fn get_all_upstreams_health(
    _admin: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<Vec<UpstreamHealthResponse>>, ApiError> {
    // Use UpstreamManager's check_all_health which uses cached clients
    let health_results = state.upstream_manager.check_all_health().await;

    let responses: Vec<UpstreamHealthResponse> = health_results
        .into_iter()
        .map(|health| UpstreamHealthResponse {
            upstream_id: 0,
            name: health.name,
            healthy: health.healthy,
            last_check: health.last_check.to_rfc3339(),
            last_error: health.last_error,
            consecutive_failures: health.consecutive_failures,
        })
        .collect();

    Ok(Json(responses))
}

/// POST /api/v1/upstreams/test (Admin only) - Test connection without saving
async fn test_upstream_connection(
    _admin: RequireAdmin,
    Json(request): Json<TestUpstreamRequest>,
) -> Result<Json<TestUpstreamResponse>, ApiError> {
    debug!("Testing upstream connection: {}", request.url);

    // Validate URL to prevent SSRF attacks (with DNS resolution check)
    validate_upstream_url_with_dns(&request.url).await?;
    validate_registry_name(&request.registry)?;

    let config = HarborClientConfig {
        url: request.url.clone(),
        registry: request.registry,
        username: request.username,
        password: request.password,
        skip_tls_verify: request.skip_tls_verify,
    };

    match HarborClient::new(config) {
        Ok(client) => match client.ping().await {
            Ok(true) => Ok(Json(TestUpstreamResponse {
                success: true,
                message: "Connection successful".to_string(),
            })),
            Ok(false) => Ok(Json(TestUpstreamResponse {
                success: false,
                message: "Ping returned false".to_string(),
            })),
            Err(e) => Ok(Json(TestUpstreamResponse {
                success: false,
                message: format!("Connection failed: {}", e),
            })),
        },
        Err(e) => Ok(Json(TestUpstreamResponse {
            success: false,
            message: format!("Failed to create client: {}", e),
        })),
    }
}

/// GET /api/v1/upstreams/:name/stats (Admin only) - Get cache statistics for an upstream
async fn get_upstream_stats(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<super::types::CacheStatsResponse>, ApiError> {
    // Verify upstream exists
    let _upstream = state
        .config_provider
        .get_upstream_by_name(&name)
        .ok_or_else(|| ApiError::NotFound(format!("Upstream: {}", name)))?;

    // For now, return empty stats since we don't have upstream-specific stats in the new model
    // TODO: Implement upstream-specific cache stats if needed
    Ok(Json(super::types::CacheStatsResponse {
        total_size: 0,
        total_size_human: "0 B".to_string(),
        entry_count: 0,
        manifest_count: 0,
        blob_count: 0,
        hit_count: 0,
        miss_count: 0,
        hit_rate: 0.0,
    }))
}

/// POST /api/v1/upstreams/reload (Admin only) - Reload upstream configuration from file
///
/// Rate limited to prevent abuse - only one reload allowed per RELOAD_COOLDOWN_SECS seconds.
async fn reload_upstreams(
    _admin: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    debug!("Reloading upstream configuration");

    // Rate limiting check using atomic timestamp
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let last_reload = LAST_RELOAD_TIME.load(Ordering::Relaxed);
    if now - last_reload < RELOAD_COOLDOWN_SECS {
        let wait_time = RELOAD_COOLDOWN_SECS - (now - last_reload);
        return Err(ApiError::BadRequest(format!(
            "Rate limit exceeded. Please wait {} seconds before reloading again.",
            wait_time
        )));
    }

    // Update the last reload time (simple CAS to handle concurrent requests)
    if LAST_RELOAD_TIME
        .compare_exchange(last_reload, now, Ordering::SeqCst, Ordering::Relaxed)
        .is_err()
    {
        return Err(ApiError::BadRequest(
            "Another reload operation is in progress. Please try again.".to_string(),
        ));
    }

    state
        .upstream_manager
        .reload()
        .map_err(|e| ApiError::Internal(format!("Failed to reload upstreams: {}", e)))?;

    info!("Upstream configuration reloaded");

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Upstream configuration reloaded successfully"
    })))
}

/// GET /api/v1/upstreams/config-path (Admin only) - Get the config file path
async fn get_config_path(
    _admin: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path = state.config_provider.get_config_path();

    Ok(Json(serde_json::json!({
        "path": path,
        "message": "Changes to upstreams are saved to this config file"
    })))
}

/// Create upstream management routes
pub fn routes() -> Router<AppState> {
    Router::new()
        // Upstream CRUD - use name as identifier instead of ID
        .route("/api/v1/upstreams", get(list_upstreams))
        .route("/api/v1/upstreams", post(create_upstream))
        .route("/api/v1/upstreams/health", get(get_all_upstreams_health))
        .route("/api/v1/upstreams/test", post(test_upstream_connection))
        .route("/api/v1/upstreams/reload", post(reload_upstreams))
        .route("/api/v1/upstreams/config-path", get(get_config_path))
        .route("/api/v1/upstreams/{name}", get(get_upstream))
        .route("/api/v1/upstreams/{name}", put(update_upstream))
        .route("/api/v1/upstreams/{name}", delete(delete_upstream))
        // Routes management
        .route("/api/v1/upstreams/{name}/routes", get(list_upstream_routes))
        .route("/api/v1/upstreams/{name}/routes", post(add_upstream_route))
        .route(
            "/api/v1/upstreams/{upstream_name}/routes/{route_idx}",
            delete(delete_upstream_route),
        )
        // Health & Stats
        .route("/api/v1/upstreams/{name}/health", get(get_upstream_health))
        .route("/api/v1/upstreams/{name}/stats", get(get_upstream_stats))
}
